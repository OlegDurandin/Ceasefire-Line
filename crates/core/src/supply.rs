//! R-SUP-02: supply propagation (gradient 0..max).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use crate::data::{ap_to_halves, SupplyRules, TerrainCatalog};
use crate::error::RuleError;
use crate::hex::Hex;
use crate::map::HexMap;
use crate::movement::{enemy_zoc, entry_cost_halves, Mobility, Occupancy};
use crate::scale::range_in_hexes;
use crate::side::SideId;

/// Supply rules converted to the scenario scale (R-SCALE-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupplyParams {
    pub max: u8,
    /// `supplyDecayAP` in half-AP: supply drops by 1 per this much path cost.
    pub decay_halves: u32,
    pub bridge_penalty: u8,
    pub hq_radius: u32,
    pub hq_min_supply: u8,
    pub hq_bonus: u8,
    pub hq_bonus_high: u8,
    pub hq_high_rating: u8,
    /// R-SUP-06: HQs work as supply nodes instead of the R-SUP-02 bonus.
    pub hq_node: bool,
    pub hq_node_min_supply: u8,
    pub hq_node_bonus: u8,
    pub hq_node_max: u8,
    pub hq_node_strength_divisor: u8,
    /// R-HQ-02 variant `hq_strength_scaling`: the HQ bonus needs this strength.
    pub hq_bonus_min_strength: Option<u8>,
}

impl SupplyParams {
    /// `decay_ap` overrides the default `supplyDecayAP` (scenario parameter).
    pub fn for_scale(
        rules: &SupplyRules,
        hex_km: f64,
        decay_ap: Option<f64>,
    ) -> Result<Self, RuleError> {
        let decay_halves = ap_to_halves(decay_ap.unwrap_or(rules.decay_ap))?;
        if decay_halves == 0 {
            return Err(RuleError::InvalidData("supplyDecayAP must be > 0".into()));
        }
        Ok(Self {
            max: rules.max,
            decay_halves,
            bridge_penalty: rules.bridge_penalty,
            hq_radius: range_in_hexes(rules.hq_bonus_radius_km, hex_km),
            hq_min_supply: rules.hq_min_supply,
            hq_bonus: rules.hq_bonus,
            hq_bonus_high: rules.hq_bonus_high,
            hq_high_rating: rules.hq_high_rating,
            hq_node: false,
            hq_node_min_supply: rules.hq_node_min_supply,
            hq_node_bonus: rules.hq_node_bonus,
            hq_node_max: rules.hq_node_max,
            hq_node_strength_divisor: rules.hq_node_strength_divisor,
            hq_bonus_min_strength: None,
        })
    }
}

/// R-SUP-01: a supply source with its current value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupplySource {
    pub hex: Hex,
    pub value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HqPosition {
    pub hex: Hex,
    pub rating: u8,
    /// R-SUP-06: a weak HQ is a weaker node.
    pub strength: u8,
}

pub struct SupplyInput<'a> {
    pub map: &'a HexMap,
    pub terrain: &'a TerrainCatalog,
    pub params: &'a SupplyParams,
    pub side: SideId,
    pub sources: &'a [SupplySource],
    pub occupancy: &'a Occupancy,
    /// R-MOVE-06: hex control; supply runs only through `side`'s hexes.
    pub control: &'a BTreeMap<Hex, SideId>,
    /// HQs of `side`.
    pub hqs: &'a [HqPosition],
}

/// Supply level per hex; hexes not listed have 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplyMap(BTreeMap<Hex, u8>);

impl SupplyMap {
    pub fn get(&self, hex: Hex) -> u8 {
        self.0.get(&hex).copied().unwrap_or(0)
    }

    /// Hexes with supply above 0, in hex order.
    pub fn iter(&self) -> impl Iterator<Item = (Hex, u8)> + '_ {
        self.0.iter().map(|(h, v)| (*h, *v))
    }
}

/// R-SUP-02: `Supply = max over sources (Source − floor(path AP / decay)) + HQ bonus`;
/// with `hq_node` (R-SUP-06) HQs are nodes instead of the bonus.
pub fn compute_supply(input: &SupplyInput<'_>) -> Result<SupplyMap, RuleError> {
    let p = input.params;
    let base = spread(input, input.sources, None)?;
    if !p.hq_node {
        return Ok(SupplyMap(apply_hq_bonus(&base, p, input.hqs)));
    }
    let mut out = base.clone();
    for hq in input.hqs {
        let Some(value) = hq_node_value(p, base.get(&hq.hex).copied().unwrap_or(0), hq.strength)
        else {
            continue;
        };
        let node = [SupplySource { hex: hq.hex, value }];
        for (h, level) in spread(input, &node, Some((hq.hex, p.hq_radius)))? {
            let e = out.entry(h).or_insert(0);
            *e = (*e).max(level);
        }
    }
    Ok(SupplyMap(out))
}

/// R-SUP-06: the value of an HQ node from the HQ's own supply `s` (without
/// any HQ's contribution, so nodes do not chain) and its strength; `None`
/// when the HQ is not a node.
pub fn hq_node_value(p: &SupplyParams, s: u8, strength: u8) -> Option<u8> {
    if s < p.hq_node_min_supply {
        return None;
    }
    let full = s
        .saturating_add(p.hq_node_bonus)
        .min(p.hq_node_max)
        .min(p.max);
    // floor(min(8, S + 2) × strength / 10), at any strength (designer 2026-11-26).
    let v = (u16::from(full) * u16::from(strength) / u16::from(p.hq_node_strength_divisor))
        .min(u16::from(p.max)) as u8;
    (v > 0).then_some(v)
}

/// R-SUP-02 paths from `sources`: for every reached hex its path cost in
/// half-AP (with each source's offset `(max − value)·decay`, so
/// `max − floor(cost / decay)` is the per-source formula) and its
/// predecessor on the best path (`None` at a source).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplyTree {
    pub cost: BTreeMap<Hex, u32>,
    pub pred: BTreeMap<Hex, Option<Hex>>,
}

impl SupplyTree {
    /// The best path from its source to `hex`, source first.
    pub fn path_to(&self, hex: Hex) -> Vec<Hex> {
        let mut out = Vec::new();
        let mut cur = Some(hex);
        while let Some(h) = cur {
            if !self.cost.contains_key(&h) || out.len() > self.cost.len() {
                break;
            }
            out.push(h);
            cur = self.pred.get(&h).copied().flatten();
        }
        out.reverse();
        out
    }
}

/// Dijkstra over the R-SUP-02 rules, without a cost limit.
/// A bridge adds `bridge_penalty·decay`, i.e. −`bridge_penalty` supply.
/// Paths run only through hexes controlled by `side` (R-MOVE-06) and never
/// enter enemy-occupied hexes; a hex in enemy ZOC is reached
/// but passed through only if a friendly unit stands in it; canals only by
/// bridge, closed neutral hexes never (`entry_cost_halves`). `within` keeps
/// the spread inside a radius of a hex (R-SUP-06 nodes).
pub fn supply_tree(
    input: &SupplyInput<'_>,
    sources: &[SupplySource],
    within: Option<(Hex, u32)>,
) -> Result<SupplyTree, RuleError> {
    let p = input.params;
    let side = input.side;
    let zoc = enemy_zoc(input.map, input.occupancy, side);
    let enemy_at = |h: Hex| input.occupancy.get(&h).is_some_and(|o| o.side != side);
    let friendly_at = |h: Hex| input.occupancy.get(&h).is_some_and(|o| o.side == side);
    let own = |h: Hex| input.control.get(&h) == Some(&side);
    let inside = |h: Hex| within.is_none_or(|(c, r)| h.distance(c) <= r);

    let mut tree = SupplyTree::default();
    let mut heap = BinaryHeap::new();
    for source in sources {
        if source.value > p.max {
            return Err(RuleError::InvalidData(format!(
                "supply source {:?} value {} exceeds max {}",
                source.hex, source.value, p.max
            )));
        }
        if input.map.cell(source.hex).is_none() || enemy_at(source.hex) || !own(source.hex) {
            continue;
        }
        let cost = u32::from(p.max - source.value) * p.decay_halves;
        if tree.cost.get(&source.hex).is_none_or(|&b| cost < b) {
            tree.cost.insert(source.hex, cost);
            tree.pred.insert(source.hex, None);
            heap.push(Reverse((cost, source.hex)));
        }
    }

    while let Some(Reverse((cost, hex))) = heap.pop() {
        if tree.cost.get(&hex).is_some_and(|&b| cost > b) {
            continue;
        }
        if zoc.contains(&hex) && !friendly_at(hex) {
            continue;
        }
        for next in hex.neighbors() {
            if enemy_at(next) || !own(next) || !inside(next) {
                continue;
            }
            let Some(step) = entry_cost_halves(
                input.map,
                input.terrain,
                hex,
                next,
                Mobility::Tracked,
                zoc.contains(&next),
            )?
            else {
                continue;
            };
            let bridge = if input.map.has_bridge(hex, next) {
                u32::from(p.bridge_penalty) * p.decay_halves
            } else {
                0
            };
            let next_cost = cost.saturating_add(step).saturating_add(bridge);
            if tree.cost.get(&next).is_none_or(|&b| next_cost < b) {
                tree.cost.insert(next, next_cost);
                tree.pred.insert(next, Some(hex));
                heap.push(Reverse((next_cost, next)));
            }
        }
    }
    Ok(tree)
}

/// Supply levels from `sources` (levels ≤ 0 dropped).
fn spread(
    input: &SupplyInput<'_>,
    sources: &[SupplySource],
    within: Option<(Hex, u32)>,
) -> Result<BTreeMap<Hex, u8>, RuleError> {
    let p = input.params;
    Ok(supply_tree(input, sources, within)?
        .cost
        .into_iter()
        .map(|(h, cost)| {
            let level = u32::from(p.max).saturating_sub(cost / p.decay_halves);
            (h, level as u8)
        })
        .filter(|&(_, level)| level > 0)
        .collect())
}

/// R-SUP-02: +bonus within the HQ radius if the HQ itself has enough supply.
/// Bonuses of several HQs do not stack; hexes without supply get nothing.
fn apply_hq_bonus(
    base: &BTreeMap<Hex, u8>,
    p: &SupplyParams,
    hqs: &[HqPosition],
) -> BTreeMap<Hex, u8> {
    let mut bonus: BTreeMap<Hex, u8> = BTreeMap::new();
    for hq in hqs {
        if base.get(&hq.hex).copied().unwrap_or(0) < p.hq_min_supply {
            continue;
        }
        if p.hq_bonus_min_strength.is_some_and(|m| hq.strength < m) {
            continue;
        }
        let b = if hq.rating >= p.hq_high_rating {
            p.hq_bonus_high
        } else {
            p.hq_bonus
        };
        for &h in base.keys().filter(|h| h.distance(hq.hex) <= p.hq_radius) {
            let e = bonus.entry(h).or_insert(0);
            *e = (*e).max(b);
        }
    }
    base.iter()
        .map(|(&h, &level)| {
            let b = bonus.get(&h).copied().unwrap_or(0);
            (h, level.saturating_add(b).min(p.max))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Rules;
    use crate::map::HexCell;
    use crate::movement::Occupant;

    const US: SideId = SideId(0);
    const THEM: SideId = SideId(1);

    fn terrain() -> TerrainCatalog {
        TerrainCatalog::from_json(include_str!("../../../data/terrain.json")).unwrap()
    }

    fn params(decay_ap: Option<f64>) -> SupplyParams {
        let rules = Rules::from_json(include_str!("../../../data/rules.json")).unwrap();
        SupplyParams::for_scale(&rules.supply, 10.0, decay_ap).unwrap()
    }

    /// Straight line of hexes (0,0)..(len,0), source 10 at (0,0).
    fn line(terrain_id: &str, len: i32) -> HexMap {
        let mut map = HexMap::default();
        for q in 0..=len {
            map.insert(Hex::new(q, 0), HexCell::new(terrain_id));
        }
        map
    }

    fn all_ours(map: &HexMap) -> BTreeMap<Hex, SideId> {
        map.hexes().map(|h| (h, US)).collect()
    }

    fn run(map: &HexMap, p: &SupplyParams, occupancy: &Occupancy) -> SupplyMap {
        let t = terrain();
        let control = all_ours(map);
        compute_supply(&SupplyInput {
            map,
            terrain: &t,
            params: p,
            side: US,
            sources: &[SupplySource {
                hex: Hex::new(0, 0),
                value: 10,
            }],
            occupancy,
            control: &control,
            hqs: &[],
        })
        .unwrap()
    }

    #[test]
    fn r_sup_02_roads_reach_four_times_farther_than_dunes() {
        let p = params(None);
        let dunes = run(&line("dunes", 8), &p, &Occupancy::new());
        // 2 AP per dune hex: -1 per hex.
        assert_eq!(dunes.get(Hex::new(4, 0)), 6);
        let mut road = line("plain", 16);
        for q in 0..16 {
            road.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
        }
        let road = run(&road, &p, &Occupancy::new());
        assert_eq!(road.get(Hex::new(16, 0)), 6);
    }

    #[test]
    fn r_scale_01_supply_decay_parameter() {
        let map = line("plain", 8);
        assert_eq!(
            run(&map, &params(None), &Occupancy::new()).get(Hex::new(8, 0)),
            6
        );
        assert_eq!(
            run(&map, &params(Some(4.0)), &Occupancy::new()).get(Hex::new(8, 0)),
            8
        );
    }

    #[test]
    fn r_sup_02_enemy_unit_blocks_path() {
        let map = line("plain", 4);
        let mut occ = Occupancy::new();
        occ.insert(
            Hex::new(2, 0),
            Occupant {
                side: THEM,
                exerts_zoc: false,
            },
        );
        let s = run(&map, &params(None), &occ);
        assert_eq!(s.get(Hex::new(2, 0)), 0);
        assert_eq!(s.get(Hex::new(4, 0)), 0);
        assert_eq!(s.get(Hex::new(1, 0)), 10);
    }

    #[test]
    fn r_sup_02_enemy_zoc_needs_friendly_unit() {
        let mut map = line("plain", 4);
        // Enemy at (2,-1): its ZOC covers (1,0) and (2,0) on the line.
        map.insert(Hex::new(2, -1), HexCell::new("plain"));
        let mut occ = Occupancy::new();
        occ.insert(
            Hex::new(2, -1),
            Occupant {
                side: THEM,
                exerts_zoc: true,
            },
        );
        let blocked = run(&map, &params(None), &occ);
        assert!(blocked.get(Hex::new(1, 0)) > 0, "ZOC hex itself is reached");
        assert_eq!(blocked.get(Hex::new(2, 0)), 0, "but not passed through");
        assert_eq!(blocked.get(Hex::new(4, 0)), 0);

        occ.insert(
            Hex::new(1, 0),
            Occupant {
                side: US,
                exerts_zoc: true,
            },
        );
        occ.insert(
            Hex::new(2, 0),
            Occupant {
                side: US,
                exerts_zoc: true,
            },
        );
        let held = run(&map, &params(None), &occ);
        assert!(
            held.get(Hex::new(4, 0)) > 0,
            "friendly units hold the corridor"
        );
    }

    #[test]
    fn r_move_06_supply_only_through_own_hexes() {
        let map = line("plain", 4);
        let t = terrain();
        let mut control = all_ours(&map);
        control.insert(Hex::new(2, 0), THEM);
        let s = compute_supply(&SupplyInput {
            map: &map,
            terrain: &t,
            params: &params(None),
            side: US,
            sources: &[SupplySource {
                hex: Hex::new(0, 0),
                value: 10,
            }],
            occupancy: &Occupancy::new(),
            control: &control,
            hqs: &[],
        })
        .unwrap();
        assert_eq!(s.get(Hex::new(1, 0)), 10);
        assert_eq!(s.get(Hex::new(2, 0)), 0);
        assert_eq!(s.get(Hex::new(3, 0)), 0);
    }

    #[test]
    fn r_move_02_canal_blocks_and_bridge_costs_two() {
        let mut map = line("plain", 4);
        map.add_canal(Hex::new(1, 0), Hex::new(2, 0)).unwrap();
        assert_eq!(
            run(&map, &params(None), &Occupancy::new()).get(Hex::new(2, 0)),
            0
        );
        map.add_bridge(Hex::new(1, 0), Hex::new(2, 0)).unwrap();
        // 2 AP → -1, bridge → -2.
        assert_eq!(
            run(&map, &params(None), &Occupancy::new()).get(Hex::new(2, 0)),
            7
        );
    }

    #[test]
    fn r_sup_02_hq_bonus_rules() {
        let map = line("plain", 12);
        let p = params(None);
        let t = terrain();
        let control = all_ours(&map);
        let at = |hqs: &[HqPosition], hex: Hex| {
            compute_supply(&SupplyInput {
                map: &map,
                terrain: &t,
                params: &p,
                side: US,
                sources: &[SupplySource {
                    hex: Hex::new(0, 0),
                    value: 10,
                }],
                occupancy: &Occupancy::new(),
                control: &control,
                hqs,
            })
            .unwrap()
            .get(hex)
        };
        let hq = |q, rating| HqPosition {
            hex: Hex::new(q, 0),
            rating,
            strength: 10,
        };
        // (6,0) base 7; HQ at (4,0) base 8 ≥ 5, within radius 2.
        assert_eq!(at(&[hq(4, 6)], Hex::new(6, 0)), 8);
        assert_eq!(at(&[hq(4, 8)], Hex::new(6, 0)), 9, "rating ≥ 8 gives +2");
        assert_eq!(at(&[hq(4, 6), hq(5, 6)], Hex::new(6, 0)), 8, "no stacking");
        assert_eq!(at(&[hq(4, 6)], Hex::new(7, 0)), 7, "outside radius");
        // HQ at (12,0) has base 4 < 5: no bonus.
        assert_eq!(at(&[hq(12, 9)], Hex::new(12, 0)), 4);
        assert_eq!(at(&[hq(1, 9)], Hex::new(0, 0)), 10, "capped at max");
    }

    /// R-SUP-06 (variant `hq_supply_node`): an HQ with supply S ≥ 4 is a source
    /// of floor(min(8, S + 2) × strength / 10) within 20 km; several HQs do
    /// not stack; nodes are computed from S without any HQ.
    #[test]
    fn r_sup_06_hq_as_a_supply_node() {
        let map = line("plain", 14);
        let mut p = params(None);
        p.hq_node = true;
        let t = terrain();
        let control = all_ours(&map);
        let at = |hqs: &[HqPosition], q: i32| {
            compute_supply(&SupplyInput {
                map: &map,
                terrain: &t,
                params: &p,
                side: US,
                sources: &[SupplySource {
                    hex: Hex::new(0, 0),
                    value: 10,
                }],
                occupancy: &Occupancy::new(),
                control: &control,
                hqs,
            })
            .unwrap()
            .get(Hex::new(q, 0))
        };
        let hq = |q, strength| HqPosition {
            hex: Hex::new(q, 0),
            rating: 5,
            strength,
        };
        // Plain: base 10 − floor(q / 2). HQ at (8,0): S = 6 → node 8.
        assert_eq!(at(&[], 8), 6);
        assert_eq!(at(&[hq(8, 10)], 8), 8);
        assert_eq!(at(&[hq(8, 10)], 10), 7, "radius 2 hexes, normal decay");
        assert_eq!(at(&[hq(8, 10)], 11), 5, "outside the radius: base");
        assert_eq!(at(&[hq(8, 10)], 4), 8, "max(base, node)");
        // Two HQs: the maximum; the second node counts from its base 5 → 7.
        assert_eq!(
            at(&[hq(8, 10), hq(10, 10)], 10),
            7,
            "no stacking, no chaining"
        );
        assert_eq!(at(&[hq(8, 10), hq(10, 10)], 12), 6);
        // Weak HQ: 8 × 4 / 10 = 3 < base — no gain.
        assert_eq!(at(&[hq(8, 4)], 8), 6);
        // S = 6: strength 10 → 8, 5 → 4, 4 → 3.
        assert_eq!(hq_node_value(&p, 6, 10), Some(8));
        assert_eq!(hq_node_value(&p, 6, 5), Some(4));
        assert_eq!(hq_node_value(&p, 6, 4), Some(3));
        // S < 4: not a node.
        assert_eq!(hq_node_value(&p, 3, 10), None);
        assert_eq!(at(&[hq(14, 10)], 14), 3);
        // The R-SUP-02 bonus is off with the variant.
        // Node 8 at (4,0) reaches (6,0) at 7 = base: no gain (the bonus would give 8).
        assert_eq!(at(&[hq(4, 10)], 6), 7);
    }
}
