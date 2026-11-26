//! R-AI-01 «поиск котлов» (stage 4 step 7): pocket operations, how a
//! chokepoint behind the enemy's line is reached (breakthrough sector), and
//! how operations persist from phase to phase.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ceasefire_core::data::UnitTag;
use ceasefire_core::game::{Command, GameState, Unit, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::prob::hits_distribution;
use ceasefire_core::side::SideId;

use crate::params::PocketWeights;
use crate::pockets::Pocket;

/// A breakthrough sector: the enemy unit whose hex opens the way to the
/// chokepoint, the chance it leaves its hex this phase and the way stays
/// open (no ZOC-to-ZOC), the expected own losses and who attacks.
#[derive(Debug, Clone, PartialEq)]
pub struct Breach {
    pub defender: UnitId,
    pub hex: Hex,
    pub p: f64,
    pub losses: f64,
    pub attackers: Vec<UnitId>,
}

/// One pocket operation: its chokepoints, who goes to each (`None`: an own
/// unit already holds it), how they get there and what it is worth.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub pocket: Pocket,
    pub roles: Vec<(Hex, Option<UnitId>)>,
    /// Breakthrough sector per chokepoint that is not reachable freely.
    pub breaches: Vec<Breach>,
    pub utility: f64,
}

pub fn is_mobile(s: &GameState, u: &Unit) -> bool {
    s.data()
        .units
        .get(&u.class)
        .is_ok_and(|c| c.has_tag(UnitTag::Exploitation))
}

fn ap_hexes(s: &GameState, u: &Unit) -> u32 {
    s.data()
        .units
        .get(&u.class)
        .ok()
        .and_then(|c| c.ap)
        .unwrap_or(0.0) as u32
}

/// Hexes in the ZOC of `enemy`'s units (strength ≥ the ZOC threshold, units
/// that exert ZOC), leaving out `except`.
fn enemy_zoc(s: &GameState, enemy: SideId, except: Option<UnitId>) -> BTreeSet<Hex> {
    let min = s.data().rules.movement.zoc_min_strength;
    s.units()
        .filter(|e| e.side == enemy && Some(e.id) != except && e.strength >= min)
        .filter(|e| s.data().units.get(&e.class).is_ok_and(|c| c.exerts_zoc))
        .flat_map(|e| e.hex.neighbors())
        .collect()
}

/// A corridor free of enemy units and enemy ZOC from `u` to `c` within
/// `steps` hexes (the destination may be in ZOC): reachable over two phases
/// without fighting.
pub fn free_corridor(s: &GameState, u: &Unit, c: Hex, steps: u32, zoc: &BTreeSet<Hex>) -> bool {
    let occupied: BTreeSet<Hex> = s
        .units()
        .filter(|e| e.side != u.side)
        .map(|e| e.hex)
        .collect();
    let mut seen = BTreeSet::from([u.hex]);
    let mut q = VecDeque::from([(u.hex, 0u32)]);
    while let Some((h, d)) = q.pop_front() {
        if h == c {
            return true;
        }
        if d >= steps || (h != u.hex && zoc.contains(&h)) {
            continue;
        }
        for n in h.neighbors() {
            if s.map().cell(n).is_none() || occupied.contains(&n) || !seen.insert(n) {
                continue;
            }
            q.push_back((n, d + 1));
        }
    }
    false
}

/// (n, p) of the attacks the defender would take this phase from units
/// adjacent to it, or able to move next to it, and artillery in range;
/// tank and mech brigades strike twice. The expected attacker losses too.
fn strike_groups(
    s: &GameState,
    us: SideId,
    defender: &Unit,
    max_movers: usize,
) -> (Vec<(u32, f64)>, f64, Vec<UnitId>) {
    let mut groups = Vec::new();
    let mut losses = 0.0;
    let mut who = Vec::new();
    let rules = &s.data().rules;
    let mut own: Vec<&Unit> = s
        .units()
        .filter(|u| u.side == us && u.hq_rating.is_none())
        .collect();
    own.sort_by_key(|u| (u.hex.distance(defender.hex), u.id));
    let mut movers = 0;
    // Hexes next to the defender already taken (by own units or by movers).
    let mut slots: BTreeSet<Hex> = s.units().map(|u| u.hex).collect();
    for u in own {
        let strikes = s.data().units.get(&u.class).map_or(0, |c| c.strikes);
        if strikes == 0 || u.strikes_left == 0 {
            continue;
        }
        // Artillery: bombardment, no return fire.
        if let Ok(input) = s.explain_bombard(u.id, defender.id) {
            let f = ceasefire_core::combat::fire_params(&rules.combat, &input);
            groups.push((f.n, f.p));
            who.push(u.id);
            continue;
        }
        let forecast = if u.hex.is_adjacent(defender.hex) {
            s.forecast_attack(u.id, defender.id).ok()
        } else if movers < max_movers && u.hex.distance(defender.hex) <= 1 + ap_hexes(s, u) {
            // Could it move next to the defender? Try on a copy.
            let reach = s.reachable(u.id).unwrap_or_default();
            reach
                .keys()
                .filter(|h| h.is_adjacent(defender.hex) && !slots.contains(h))
                .min()
                .copied()
                .and_then(|to| {
                    slots.insert(to);
                    let mut trial = s.clone();
                    let path = trial.path_to(u.id, to).ok().flatten()?;
                    trial.apply(Command::Move { unit: u.id, path }).ok()?;
                    movers += 1;
                    trial.forecast_attack(u.id, defender.id).ok()
                })
        } else {
            None
        };
        if let Some(f) = forecast {
            let times = if is_mobile(s, u) {
                strikes.min(u.strikes_left)
            } else {
                1
            };
            for _ in 0..times {
                groups.push((f.exchange.vs_defender.n, f.exchange.vs_defender.p));
                losses += f.attacker_losses.expected;
            }
            who.push(u.id);
        }
    }
    (groups, losses, who)
}

/// R-AI-01 breakthrough sector for chokepoint `c`: among the enemy units on
/// the front (next to own units) within a tank's move of `c`, the one with
/// the best P(breach) × value / (expected losses + 1). P(breach) = P(the
/// defender is left at strength ≤ the retreat threshold, or 0 in a
/// stronghold with `fortified_hold`) by the exact loss distribution ×
/// P(the way on is not ZOC-to-ZOC).
pub fn best_breach(
    s: &GameState,
    us: SideId,
    enemy: SideId,
    c: Hex,
    value: f64,
    w: &PocketWeights,
) -> Option<Breach> {
    let rules = &s.data().rules;
    let tank_move = 5;
    let mut best: Option<(f64, Breach)> = None;
    let mut front: Vec<&Unit> = s
        .units()
        .filter(|e| e.side == enemy && e.hex.distance(c) <= tank_move + 1)
        .filter(|e| {
            s.units()
                .any(|u| u.side == us && u.hex.distance(e.hex) <= 2)
        })
        .collect();
    front.sort_by_key(|e| e.id);
    for e in front {
        let (groups, losses, attackers) = strike_groups(s, us, e, w.breach_movers);
        if groups.is_empty() {
            continue;
        }
        let dist = hits_distribution(&groups, u32::from(e.strength));
        let holds = s.variants().fortified_hold
            && s.map()
                .cell(e.hex)
                .and_then(|cell| s.data().terrain.get(&cell.terrain).ok())
                .is_some_and(|t| t.def.stronghold);
        let left_max = if holds { 0 } else { rules.retreat.max_strength };
        let p_vacate: f64 = dist
            .iter()
            .enumerate()
            .filter(|(k, _)| u32::from(e.strength).saturating_sub(*k as u32) <= u32::from(left_max))
            .map(|(_, p)| p)
            .sum();
        // The way on: out of the breach hex toward c without ZOC-to-ZOC.
        let zoc = enemy_zoc(s, enemy, Some(e.id));
        let open = !zoc.contains(&e.hex)
            || e.hex == c
            || e.hex.neighbors().iter().any(|n| {
                s.map().cell(*n).is_some()
                    && n.distance(c) < e.hex.distance(c)
                    && (!zoc.contains(n) || *n == c)
                    && s.unit_at(*n).is_none()
            });
        let p = p_vacate * if open { 1.0 } else { w.breach_closed_exit };
        let score = p * value / (losses + 1.0);
        if best.as_ref().is_none_or(|(b, _)| score > *b) {
            best = Some((
                score,
                Breach {
                    defender: e.id,
                    hex: e.hex,
                    p,
                    losses,
                    attackers,
                },
            ));
        }
    }
    best.map(|(_, b)| b).filter(|b| b.p >= w.breach_min_p)
}

/// How a unit gets to chokepoint `c`: (reachability factor, closes this
/// phase, breakthrough needed).
pub fn reach_to(
    s: &GameState,
    u: &Unit,
    c: Hex,
    enemy: SideId,
    value: f64,
    w: &PocketWeights,
    zoc: &BTreeSet<Hex>,
) -> Option<(f64, bool, Option<Breach>)> {
    if s.reachable(u.id).is_ok_and(|r| r.contains_key(&c)) {
        return Some((w.reach_now, true, None));
    }
    if free_corridor(s, u, c, 2 * ap_hexes(s, u), zoc) {
        return Some((w.reach_two_phases, false, None));
    }
    best_breach(s, u.side, enemy, c, value, w).map(|b| (w.reach_two_phases * b.p, false, Some(b)))
}

/// Carried operations: kept unless no longer feasible or a fresh one is worth
/// more than `switch_margin` above. A unit leaves an operation only at
/// supply ≤ `drop_supply` or strength ≤ `drop_strength`.
pub fn reconcile(
    s: &GameState,
    carried: &[Operation],
    fresh: Vec<Operation>,
    w: &PocketWeights,
) -> (Vec<Operation>, bool) {
    let alive_ok = |id: UnitId| {
        s.unit(id)
            .is_some_and(|u| u.supply > w.drop_supply && u.strength > w.drop_strength)
    };
    let enemy_on = |h: Hex| s.unit_at(h).is_some_and(|u| u.side != s.active_side());
    let still: Vec<&Operation> = carried
        .iter()
        .filter(|op| {
            op.pocket.hexes.iter().all(|h| !enemy_on(*h))
                && op.roles.iter().all(|(h, id)| match id {
                    Some(id) => alive_ok(*id),
                    None => s.unit_at(*h).is_some_and(|u| u.side == s.active_side()),
                })
        })
        .collect();
    let best_fresh = fresh.iter().map(|o| o.utility).fold(0.0, f64::max);
    let best_kept = still.iter().map(|o| o.utility).fold(0.0, f64::max);
    if still.is_empty() || best_fresh > best_kept * (1.0 + w.switch_margin) {
        return (fresh, false);
    }
    // Keep the carried ones; fresh ones that share no unit or hex fill up.
    let mut out: Vec<Operation> = still.into_iter().cloned().collect();
    let mut used: BTreeSet<UnitId> = out
        .iter()
        .flat_map(|o| o.roles.iter().filter_map(|r| r.1))
        .collect();
    let mut hexes: BTreeSet<Hex> = out
        .iter()
        .flat_map(|o| o.pocket.hexes.iter().copied())
        .collect();
    for op in fresh {
        if out.len() >= w.max_operations {
            break;
        }
        let units: Vec<UnitId> = op.roles.iter().filter_map(|r| r.1).collect();
        if units.iter().any(|u| used.contains(u))
            || op.pocket.hexes.iter().any(|h| hexes.contains(h))
        {
            continue;
        }
        used.extend(units);
        hexes.extend(op.pocket.hexes.iter().copied());
        out.push(op);
    }
    (out, true)
}

/// Statistics for `sim`: operations kept between phases, and breakthrough
/// forecasts against what happened (calibration).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpStats {
    /// Phases that had operations carried in from the side's previous phase.
    pub carried_phases: u32,
    /// Of those, phases where the operations were kept.
    pub kept_phases: u32,
    /// (P(breach) forecast, the defender left its hex during the phase).
    pub breaches: Vec<(f64, bool)>,
}

/// Units an operation's chokepoints and breaches concern.
pub fn op_units(op: &Operation) -> BTreeMap<UnitId, Hex> {
    op.roles
        .iter()
        .filter_map(|(h, id)| id.map(|id| (id, *h)))
        .collect()
}
