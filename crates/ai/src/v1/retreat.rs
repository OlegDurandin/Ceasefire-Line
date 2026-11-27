//! R-AI-01 «Отход» (designer decision 2026-11-27): when its side's exits are
//! open, a unit compares the value of holding its hex with the victory points
//! its steps bring through an exit (R-NM-02), in the common currency.
//!
//! - Exit value: steps × the exit's VP per step × `per_phase` for every own
//!   phase beyond this one it needs to get there (by distance and AP) ×
//!   `cut_way` when its way passes hexes a closing pocket would cut; none if
//!   it cannot get there before the end.
//! - Hold value: the VP per step of its role (economy roles: operation, own
//!   chokepoint, objective, the line) × steps, minus — when its hex is at risk
//!   of a cut — the VP its loss would cost (the enemy's VP per our lost step
//!   plus our NM per step).
//! - Rearguards: a unit near the way of withdrawing units adds
//!   `rearguard_share` of their exit VP to its hold value — the time it wins.
//!
//! Units in a pocket operation or guarding an own chokepoint keep their task.

use std::collections::BTreeMap;

use ceasefire_core::game::{GameState, Unit, UnitId};
use ceasefire_core::hex::Hex;

use super::{is_air, AiV1, Plan};
use crate::operations::ap_hexes;

/// Hexes of a straight way from `a` to `b` (each step to the neighbour
/// nearest `b`, lower hex on ties), `b` included.
fn way(a: Hex, b: Hex) -> Vec<Hex> {
    let mut out = Vec::new();
    let mut cur = a;
    while cur != b && out.len() < 64 {
        cur = cur
            .neighbors()
            .into_iter()
            .min_by_key(|n| (n.distance(b), *n))
            .unwrap_or(b);
        out.push(cur);
    }
    out
}

struct Cand {
    id: UnitId,
    hex: Hex,
    exit: Hex,
    exit_vp: f64,
    hold_vp: f64,
    way: Vec<Hex>,
}

impl AiV1 {
    pub(super) fn retreat(&self, s: &GameState, plan: &mut Plan) {
        let r = &self.params.retreat;
        if !r.enabled {
            return;
        }
        let us = s.active_side();
        let exits: Vec<(Hex, i32)> = s
            .conditions()
            .exits
            .iter()
            .filter(|z| z.open && z.side == us)
            .flat_map(|z| z.hexes.iter().map(move |h| (*h, z.vp_per_step)))
            .collect();
        if exits.is_empty() {
            return;
        }
        let phases_left = self.own_phases_left(s);
        let roles = self.roles(s, plan);
        let steps_per = f64::from(s.data().rules.national_morale.steps_per_loss.max(1));
        let own_step_vp = s.side(us).map_or(1.0, |x| f64::from(x.loss_sensitivity)) / steps_per;
        let enemy_step_vp: f64 = s
            .conditions()
            .vp_per_enemy_step
            .iter()
            .filter(|(side, _)| **side != us)
            .map(|(_, v)| v.per_step())
            .sum();
        let band = self.params.pockets.cut_supply_max;
        let mut own: Vec<&Unit> = s
            .units()
            .filter(|u| u.side == us && !is_air(s, u))
            .collect();
        own.sort_by_key(|u| u.id);
        let mut cands: Vec<Cand> = Vec::new();
        for u in own {
            if plan.envelop.contains_key(&u.id)
                || plan.hold.contains(&u.id)
                || plan.guard.contains_key(&u.id)
            {
                continue;
            }
            let Some(&(exit, per)) = exits.iter().min_by_key(|(h, _)| (h.distance(u.hex), *h))
            else {
                continue;
            };
            let now = s.reachable(u.id).is_ok_and(|m| m.contains_key(&exit)) || u.hex == exit;
            let ap = ap_hexes(s, u).max(1);
            let extra = if now {
                0
            } else {
                u.hex.distance(exit).div_ceil(ap)
            };
            if extra + 1 > phases_left {
                continue;
            }
            let path = way(u.hex, exit);
            // Plain products, not powi: the same result native and in WASM (R-AI-02).
            let mut reach = (0..extra).fold(1.0, |acc, _| acc * r.per_phase);
            if path.iter().any(|h| plan.risk.contains(h)) {
                reach *= r.cut_way;
            }
            let steps = f64::from(u.strength);
            let role = roles.get(&u.id).map_or(0.0, |x| x.vp_per_step) * steps;
            let at_risk = plan.risk.contains(&u.hex) || u.supply <= band;
            let loss = if at_risk {
                steps * (enemy_step_vp + own_step_vp)
            } else {
                0.0
            };
            cands.push(Cand {
                id: u.id,
                hex: u.hex,
                exit,
                exit_vp: steps * f64::from(per) * reach,
                hold_vp: role - loss,
                way: path,
            });
        }
        // First pass: who would rather leave; then the rearguards near their ways.
        let leaving: Vec<usize> = (0..cands.len())
            .filter(|&i| cands[i].exit_vp > cands[i].hold_vp)
            .collect();
        let mut bonus: BTreeMap<UnitId, f64> = BTreeMap::new();
        for c in &cands {
            let covered: f64 = leaving
                .iter()
                .map(|&j| &cands[j])
                .filter(|o| {
                    o.id != c.id
                        && o.way
                            .iter()
                            .any(|h| h.distance(c.hex) <= r.rearguard_radius)
                })
                .map(|o| o.exit_vp)
                .sum();
            if covered > 0.0 {
                bonus.insert(c.id, r.rearguard_share * covered);
            }
        }
        for c in &cands {
            let b = bonus.get(&c.id).copied().unwrap_or(0.0);
            if c.exit_vp > c.hold_vp + b {
                plan.exit.insert(c.id, c.exit);
                plan.chokes.insert(c.exit); // the «no stranding» rule lets it go there
                plan.pin.remove(&c.id);
                plan.withdraw.remove(&c.id);
            } else if b > 0.0 && c.exit_vp > c.hold_vp {
                plan.rearguard.insert(c.id);
            }
        }
    }
}
