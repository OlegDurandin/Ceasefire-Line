//! R-AI-01 «поиск котлов» (stage 4 step 7): where the enemy's supply can be
//! cut. Once per phase, at the operational level.
//!
//! 1. The enemy's supply tree (R-SUP-02, the core's own code) gives every
//!    enemy unit its best path from a source.
//! 2. The dependency of a hex D(h) is the sum over the enemy units whose path
//!    runs through h of strength × price / 10.
//! 3. Chokepoints are the free hexes with the largest D(h).

use std::collections::BTreeMap;

use ceasefire_core::error::RuleError;
use ceasefire_core::game::GameState;
use ceasefire_core::hex::Hex;
use ceasefire_core::side::SideId;

/// Weight of a unit in the pocket sums: strength × price / 10.
pub fn unit_weight(s: &GameState, class: &str, strength: u8) -> f64 {
    let price = s.data().units.get(class).map_or(0, |c| c.cost_mpp);
    f64::from(strength) * f64::from(price) / 10.0
}

/// Per hex of the enemy's supply paths: D(h), the units depending on it
/// (ids, sorted) and its path cost from the source.
struct Dependency {
    weight: f64,
    units: Vec<u32>,
    cost: u32,
    /// A supply source (no predecessor on the path).
    source: bool,
}

fn dependency(
    s: &GameState,
    enemy: SideId,
    held: &[(Hex, SideId)],
) -> Result<BTreeMap<Hex, Dependency>, RuleError> {
    let tree = s.supply_tree_if(enemy, held)?;
    let mut d: BTreeMap<Hex, Dependency> = BTreeMap::new();
    let mut units: Vec<_> = s.units().filter(|u| u.side == enemy).collect();
    units.sort_by_key(|u| u.id);
    for u in units {
        if !tree.cost.contains_key(&u.hex) {
            continue; // already cut off
        }
        let w = unit_weight(s, &u.class, u.strength);
        for h in tree.path_to(u.hex) {
            if h == u.hex {
                continue;
            }
            let e = d.entry(h).or_insert(Dependency {
                weight: 0.0,
                units: Vec::new(),
                cost: tree.cost[&h],
                source: tree.pred.get(&h).is_some_and(Option::is_none),
            });
            e.weight += w;
            e.units.push(u.id.0);
        }
    }
    Ok(d)
}

/// D(h) for the units of `enemy`: how much of it depends on each hex of
/// its supply paths (units' own hexes excluded).
pub fn supply_dependency(s: &GameState, enemy: SideId) -> Result<BTreeMap<Hex, f64>, RuleError> {
    Ok(dependency(s, enemy, &[])?
        .into_iter()
        .map(|(h, d)| (h, d.weight))
        .collect())
}

/// The `k` chokepoints: free hexes (no unit) with the largest D(h). Hexes
/// that cut the very same units (one trunk of the tree) give one candidate:
/// the narrowest — most impassable neighbours (a pass among mountains) —
/// then the farthest along the path. Largest D first; ties by hex order.
pub fn chokepoints(s: &GameState, enemy: SideId, k: usize) -> Result<Vec<(Hex, f64)>, RuleError> {
    chokepoints_if(s, enemy, &[], k)
}

/// [`chokepoints`] with hypothetical held hexes (the trial recount).
pub fn chokepoints_if(
    s: &GameState,
    enemy: SideId,
    held: &[(Hex, SideId)],
    k: usize,
) -> Result<Vec<(Hex, f64)>, RuleError> {
    let mut occupied: std::collections::BTreeSet<Hex> = s.units().map(|u| u.hex).collect();
    occupied.extend(held.iter().map(|h| h.0));
    ranked_chokepoints(s, enemy, held, k, &occupied, &[])
}

/// Step 7 item 5 «защита своих горловин»: the same chokepoints of the side's
/// own supply, seen from its side — hexes its own units stand on count (a
/// unit holding the neck is its defence); only enemy-held hexes do not.
/// Of a trunk the hex nearest to an enemy ground unit stands for it: the
/// enemy cuts the trunk there first.
pub fn own_chokepoints(s: &GameState, us: SideId, k: usize) -> Result<Vec<(Hex, f64)>, RuleError> {
    let occupied: std::collections::BTreeSet<Hex> =
        s.units().filter(|u| u.side != us).map(|u| u.hex).collect();
    let enemy: Vec<Hex> = s
        .units()
        .filter(|u| u.side != us && u.hq_rating.is_none())
        .filter(|u| {
            s.data()
                .units
                .get(&u.class)
                .is_ok_and(|c| c.target_class.is_ground())
        })
        .map(|u| u.hex)
        .collect();
    ranked_chokepoints(s, us, &[], k, &occupied, &enemy)
}

fn ranked_chokepoints(
    s: &GameState,
    enemy: SideId,
    held: &[(Hex, SideId)],
    k: usize,
    occupied: &std::collections::BTreeSet<Hex>,
    toward: &[Hex],
) -> Result<Vec<(Hex, f64)>, RuleError> {
    // Narrowness: neighbours whose terrain cannot be entered (mountains
    // without a pass, water); the map's edge and canal sides do not count.
    let narrowness = |h: Hex| -> usize {
        h.neighbors()
            .into_iter()
            .filter_map(|n| s.map().cell(n))
            .filter(|c| {
                s.data()
                    .terrain
                    .get(&c.terrain)
                    .is_ok_and(|t| t.def.impassable || (t.def.requires_pass && !c.pass))
            })
            .count()
    };
    // Per trunk: (hex, D, blocked neighbours, path cost).
    let mut by_units: BTreeMap<Vec<u32>, (Hex, f64, usize, u32)> = BTreeMap::new();
    for (h, d) in dependency(s, enemy, held)? {
        // Sources are not chokepoints: they lie in the enemy's rear.
        if d.weight <= 0.0 || occupied.contains(&h) || d.source {
            continue;
        }
        let n = narrowness(h);
        let e = by_units.entry(d.units).or_insert((h, d.weight, n, d.cost));
        let better = if toward.is_empty() {
            n > e.2 || (n == e.2 && d.cost > e.3)
        } else {
            // Own defence: nearest to the enemy, then the narrowest.
            let near = |x: Hex| {
                toward
                    .iter()
                    .map(|t| t.distance(x))
                    .min()
                    .unwrap_or(u32::MAX)
            };
            (near(h), std::cmp::Reverse(n), h) < (near(e.0), std::cmp::Reverse(e.2), e.0)
        };
        if better {
            *e = (h, d.weight, n, d.cost);
        }
    }
    let mut c: Vec<(Hex, f64)> = by_units.into_values().map(|(h, w, _, _)| (h, w)).collect();
    c.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    c.truncate(k);
    Ok(c)
}

/// A pocket the side can close: its chokepoints (1–2, pincers), its value
/// V and the enemy units it cuts off (projected unit supply ≤ the cut band).
#[derive(Debug, Clone, PartialEq)]
pub struct Pocket {
    pub hexes: Vec<Hex>,
    pub value: f64,
    pub cut: Vec<ceasefire_core::game::UnitId>,
}

/// V for `held` hexes: we hold them with a unit each; the enemy's supply is
/// recounted and each unit's supply projected `lookahead_phases` ahead with
/// the R-SUP-05 inertia. Σ strength × price / 10 over units at ≤ the cut
/// band, × `zero_supply_mult` for those at 0.
pub fn pocket_value(
    s: &GameState,
    us: SideId,
    enemy: SideId,
    held: &[Hex],
    w: &crate::params::PocketWeights,
) -> Result<(f64, Vec<ceasefire_core::game::UnitId>), RuleError> {
    let trial: Vec<(Hex, SideId)> = held.iter().map(|&h| (h, us)).collect();
    let map = s.hex_supply_if(enemy, &trial)?;
    let drop = s.data().rules.supply.max_drop_per_phase;
    let mut value = 0.0;
    let mut cut = Vec::new();
    let mut units: Vec<_> = s.units().filter(|u| u.side == enemy).collect();
    units.sort_by_key(|u| u.id);
    for u in units {
        let ahead = u
            .supply
            .saturating_sub(drop.saturating_mul(w.lookahead_phases));
        let projected = map.get(u.hex).max(ahead);
        if projected <= w.cut_supply_max {
            let mut v = unit_weight(s, &u.class, u.strength);
            if projected == 0 {
                v *= w.zero_supply_mult;
            }
            value += v;
            cut.push(u.id);
        }
    }
    Ok((value, cut))
}

/// The best pocket through chokepoint `c`: alone, or — if paths remain —
/// with a second chokepoint on the remaining paths (greedy, at most
/// `max_pincers` hexes in all).
pub fn best_pocket(
    s: &GameState,
    us: SideId,
    enemy: SideId,
    c: Hex,
    w: &crate::params::PocketWeights,
) -> Result<Pocket, RuleError> {
    let (value, cut) = pocket_value(s, us, enemy, &[c], w)?;
    let mut best = Pocket {
        hexes: vec![c],
        value,
        cut,
    };
    while best.hexes.len() < w.max_pincers.max(1) {
        let held: Vec<(Hex, SideId)> = best.hexes.iter().map(|&h| (h, us)).collect();
        let mut improved: Option<Pocket> = None;
        for (c2, _) in chokepoints_if(s, enemy, &held, w.candidates)? {
            let mut hexes = best.hexes.clone();
            hexes.push(c2);
            let (value, cut) = pocket_value(s, us, enemy, &hexes, w)?;
            if value > improved.as_ref().map_or(best.value, |p| p.value) + 1e-9 {
                improved = Some(Pocket { hexes, value, cut });
            }
        }
        match improved {
            Some(p) => best = p,
            None => break,
        }
    }
    Ok(best)
}

/// Enemy phases left in the scenario after a pocket closes `delay` own
/// phases from now (0: this phase). Without a last turn: `horizon_phases`.
pub fn enemy_phases_left(
    s: &GameState,
    enemy: SideId,
    delay: u32,
    w: &crate::params::PocketWeights,
) -> u32 {
    let Some(last) = s.last_turn() else {
        return w.horizon_phases.saturating_sub(delay);
    };
    let us = s.active_side();
    // Does the enemy still move this turn after us? (phase order = side order)
    let later =
        s.sides().iter().position(|x| x.id == enemy) > s.sides().iter().position(|x| x.id == us);
    let phases = last.saturating_sub(s.turn()) + u32::from(later);
    phases.saturating_sub(delay)
}

/// R-AI-01 common currency: expected victory points (R-NM-02) the pocket
/// `held` brings by the end of the scenario if it closes `delay` own phases
/// from now. Each cut unit's supply is projected phase by phase (R-SUP-05
/// inertia); at 0 it suffers attrition (R-SUP-03), desertion (variant) and our
/// finishing attacks (R-COMBAT-05 variant); its steps and, if it dies, the unit
/// itself cost its side NM (R-NM-01) — our margin grows by as much.
pub fn pocket_vp(
    s: &GameState,
    us: SideId,
    enemy: SideId,
    held: &[Hex],
    delay: u32,
    w: &crate::params::PocketWeights,
) -> Result<f64, RuleError> {
    let phases = enemy_phases_left(s, enemy, delay, w);
    if phases == 0 {
        return Ok(0.0);
    }
    let trial: Vec<(Hex, SideId)> = held.iter().map(|&h| (h, us)).collect();
    let map = s.hex_supply_if(enemy, &trial)?;
    let rules = &s.data().rules;
    let v = s.variants();
    let sensitivity = s.side(enemy).map_or(1.0, |x| f64::from(x.loss_sensitivity));
    let per_step = sensitivity / f64::from(rules.national_morale.steps_per_loss.max(1));
    let mut vp = 0.0;
    let mut units: Vec<_> = s.units().filter(|u| u.side == enemy).collect();
    units.sort_by_key(|u| u.id);
    for u in units {
        let level = map.get(u.hex);
        if level > 0 {
            continue; // still supplied: no steps lost to the pocket
        }
        let (mut supply, mut zero) = (u.supply, u32::from(u.zero_supply_phases));
        let mut lost = 0.0;
        for _ in 0..phases {
            supply = supply.saturating_sub(rules.supply.max_drop_per_phase);
            if supply > 0 {
                zero = 0;
                continue;
            }
            zero += 1;
            if zero >= u32::from(rules.supply.attrition_after_phases) {
                lost += f64::from(rules.supply.attrition_steps);
            }
            if v.pocket_desertion {
                lost += rules.supply.desertion_chance * f64::from(rules.supply.desertion_steps);
            }
            if v.unsupplied_target {
                lost += w.finish_steps_per_phase;
            }
        }
        let strength = f64::from(u.strength);
        let lost = lost.min(strength);
        vp += lost * per_step;
        if lost >= strength {
            vp += f64::from(s.unit_nm_loss(&u.class, true));
        }
    }
    Ok(vp)
}
