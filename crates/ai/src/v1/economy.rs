//! R-AI-01 п. 3 «экономика» (stage 4 step 7, designer task 2026-11-27): the
//! phase's MPP decisions in the common currency of expected victory points,
//! made after the operational plan and before any action.
//!
//! - Reinforcement: steps × the VP per step of the unit's role (operation,
//!   own chokepoint, objective defence or attack; a unit within reach of the
//!   enemy at least holds the line: a step is worth the NM its loss costs) × the share of the own
//!   phases left in which it will be in action (not this one). With
//!   `reinforce_locks` only units that do not act this phase by the plan.
//! - Purchase: arrives after the delay; worth nothing with fewer than
//!   `min_phases_after_arrival` own phases after it; else a full unit's steps
//!   × the VP per step of the neediest role × the share of phases it serves.
//!   Force limit R-ECO-03; a class without a limit is not bought. With
//!   `arrival_choice` it arrives at the qualifying source nearest that role.
//! - Boost (R-TECH-02): ready at least `boost_turns_before_end` turns before
//!   the end; worth the change of expected losses in the own attacks the
//!   side can make now, by the forecaster on the boosted values, per phase ×
//!   the own phases after it is ready.
//! - Transfer reserve (R-MOVE-05): with two sectors or more, the price of one
//!   transfer of the dearest own unit stays unspent.
//!
//! Greedy by VP per MPP while MPP lasts; every value gets the per-game seeded
//! spread (`economy.jitter`).

use std::collections::{BTreeMap, BTreeSet};

use ceasefire_core::game::{Command, GameState, Unit, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::side::SideId;

use super::{accepted, objective_margin, unit_float, AiV1, Plan};
use crate::operations::op_units;

/// What a unit does for the side: its VP per strength step and where.
#[derive(Debug, Clone, Copy)]
pub(super) struct Role {
    pub(super) vp_per_step: f64,
    hex: Hex,
    /// A pocket operation: wants tanks and mechs.
    operation: bool,
}

struct Candidate {
    value: f64,
    cost: u32,
    cmd: Command,
    /// Ties: kind (0 reinforce, 1 purchase, 2 boost), then the scenario order.
    tie: (u8, usize),
}

impl AiV1 {
    /// The phase's economy commands, best VP per MPP first.
    pub(super) fn economy_plan(&self, s: &GameState) -> Vec<Command> {
        let Some(plan) = self.plan.as_ref() else {
            return Vec::new();
        };
        let us = s.active_side();
        let e = &self.params.economy;
        let Some(side) = s.side(us) else {
            return Vec::new();
        };
        let own_phases = self.own_phases_left(s);
        if own_phases == 0 {
            return Vec::new();
        }
        let roles = self.roles(s, plan);
        // Units acting this phase by the plan are not reinforced when that
        // would stop them (R-ECO-02 variant `reinforce_locks`); without the
        // variant a reinforced unit still acts (interpretation, docs/decisions.md).
        let acting = if s.variants().reinforce_locks {
            self.acting(s, plan)
        } else {
            BTreeSet::new()
        };
        let spread = |salt: u64| -> f64 {
            1.0 + e.jitter * (unit_float(self.seed, 0x0ec0_0000 ^ salt) * 2.0 - 1.0)
        };
        let mut cands: Vec<Candidate> = Vec::new();

        // Reinforcement of units out of action this phase.
        let max = s.data().rules.unit.max_strength;
        let mut own: Vec<&Unit> = s.units().filter(|u| u.side == us).collect();
        own.sort_by_key(|u| u.id);
        for u in own
            .iter()
            .filter(|u| u.strength < max && !acting.contains(&u.id))
        {
            let Some(role) = roles.get(&u.id) else {
                continue;
            };
            let later = f64::from(own_phases - 1) / f64::from(own_phases);
            // Every affordable size: the greedy falls back to fewer steps.
            for k in (1..=max - u.strength).rev() {
                let cmd = Command::Reinforce {
                    unit: u.id,
                    steps: k,
                    elite: false,
                };
                let Ok(cost) = s.reinforce_cost(u.id, k, false) else {
                    continue;
                };
                if cost == 0 || !accepted(s, &cmd) {
                    continue;
                }
                cands.push(Candidate {
                    value: f64::from(k) * role.vp_per_step * later * spread(u64::from(u.id.0)),
                    cost,
                    cmd,
                    // Same unit: more steps first; the greedy takes one per unit.
                    tie: (0, u.id.0 as usize * 16 + usize::from(15 - k.min(15))),
                });
            }
        }

        // Purchases for the neediest role.
        let need = roles.values().copied().max_by(|a, b| {
            a.vp_per_step
                .total_cmp(&b.vp_per_step)
                .then(b.hex.cmp(&a.hex))
        });
        let delay = s.data().rules.economy.purchase_delay_turns;
        let after = self.own_phases_from(s, s.turn() + delay);
        if let Some(need) = need.filter(|_| after >= e.min_phases_after_arrival) {
            let at = s
                .arrival_choices(us)
                .into_iter()
                .min_by_key(|h| (h.distance(need.hex), *h));
            for (i, (class, limit)) in side.purchase_limits.iter().enumerate() {
                if side.purchased.get(class).copied().unwrap_or(0) >= *limit {
                    continue;
                }
                let Ok(c) = s.data().units.get(class) else {
                    continue;
                };
                let fit =
                    if need.operation && !c.has_tag(ceasefire_core::data::UnitTag::Exploitation) {
                        e.non_mobile_op_fit
                    } else {
                        1.0
                    };
                let share = f64::from(after.min(own_phases)) / f64::from(own_phases);
                cands.push(Candidate {
                    value: f64::from(max)
                        * need.vp_per_step
                        * fit
                        * share
                        * spread(1000 + i as u64),
                    cost: c.cost_mpp,
                    cmd: Command::Purchase {
                        class: class.clone(),
                        at,
                    },
                    tie: (1, i),
                });
            }
        }

        // Tech boosts on the forecaster's boosted values.
        for (i, b) in s
            .tech_boosts()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.side == us)
        {
            if s.boost_bought(&b.id) {
                continue;
            }
            let ready = s.turn() + b.turns;
            if s.last_turn()
                .is_some_and(|last| ready + e.boost_turns_before_end > last)
            {
                continue;
            }
            let cmd = Command::BuyTechBoost { id: b.id.clone() };
            if !accepted(s, &cmd) {
                continue;
            }
            let per_phase = self.boost_gain(s, us, b.branch);
            let phases = f64::from(self.own_phases_from(s, ready));
            if per_phase > 0.0 {
                cands.push(Candidate {
                    value: per_phase * phases * spread(2000 + i as u64),
                    cost: b.cost,
                    cmd,
                    tie: (2, i),
                });
            }
        }

        // Transfer reserve with two sectors or more (R-MOVE-05); not kept when
        // the MPP do not cover it — no transfer this phase anyway (test C).
        let reserve = if self.hints.sectors.len() >= 2 {
            own.iter()
                .filter_map(|u| s.transfer_cost(u.id).ok())
                .max()
                .unwrap_or(0)
        } else {
            0
        };
        let reserve = if reserve > side.mpp { 0 } else { reserve };

        cands.retain(|c| c.value > 0.0 && c.cost > 0);
        cands.sort_by(|a, b| {
            (b.value / f64::from(b.cost))
                .total_cmp(&(a.value / f64::from(a.cost)))
                .then(a.tie.cmp(&b.tie))
        });
        let mut mpp = side.mpp.saturating_sub(reserve);
        let mut out = Vec::new();
        let mut reinforced: BTreeSet<UnitId> = BTreeSet::new();
        for c in cands {
            if c.cost > mpp {
                continue;
            }
            if let Command::Reinforce { unit, .. } = &c.cmd {
                if !reinforced.insert(*unit) {
                    continue;
                }
            }
            mpp -= c.cost;
            out.push(c.cmd);
        }
        out
    }

    /// Own phases left including this one (the scenario's last turn, else the horizon).
    pub(super) fn own_phases_left(&self, s: &GameState) -> u32 {
        self.own_phases_from(s, s.turn())
    }

    /// Own phases from the start of turn `turn` to the end (this side moves once a turn).
    fn own_phases_from(&self, s: &GameState, turn: u32) -> u32 {
        match s.last_turn() {
            Some(last) => (last + 1).saturating_sub(turn),
            None => self
                .params
                .pockets
                .horizon_phases
                .saturating_sub(turn.saturating_sub(s.turn())),
        }
    }

    /// Units acting this phase by the plan: sent somewhere by an operation,
    /// a chokepoint or a withdrawal, or with an attack or bombardment worth
    /// making now. Holders of a chokepoint stay put and do not count.
    fn acting(&self, s: &GameState, plan: &Plan) -> BTreeSet<UnitId> {
        let mut acting: BTreeSet<UnitId> = plan
            .envelop
            .keys()
            .chain(plan.pin.keys())
            .chain(plan.guard.keys())
            .chain(plan.withdraw.keys())
            .chain(plan.exit.keys())
            .copied()
            .collect();
        for op in &plan.ops {
            if !op.breaches.is_empty() {
                acting.extend(op_units(op).into_keys());
            }
        }
        let b = self.board(s);
        for (_, _, cmd) in self.strike_options(&b) {
            if let Command::Attack { unit, .. } | Command::Bombard { unit, .. } = cmd {
                acting.insert(unit);
            }
        }
        acting
    }

    /// VP per strength step of each own unit's role; the best role counts.
    pub(super) fn roles(&self, s: &GameState, plan: &Plan) -> BTreeMap<UnitId, Role> {
        let us = s.active_side();
        let m = &self.params.moves;
        let mut roles: BTreeMap<UnitId, Role> = BTreeMap::new();
        let mut give = |ids: &[UnitId], vp: f64, hex: Hex, operation: bool| {
            let steps: f64 = ids
                .iter()
                .filter_map(|id| s.unit(*id))
                .map(|u| f64::from(u.strength.max(1)))
                .sum();
            if vp <= 0.0 || steps <= 0.0 {
                return;
            }
            let r = Role {
                vp_per_step: vp / steps,
                hex,
                operation,
            };
            for id in ids {
                let better = roles.get(id).is_none_or(|x| x.vp_per_step < r.vp_per_step);
                if better {
                    roles.insert(*id, r);
                }
            }
        };
        // Pocket operations: their tanks and holders.
        for op in &plan.ops {
            let mut ids: Vec<UnitId> = op_units(op).into_keys().collect();
            for (c, unit) in &op.roles {
                if unit.is_none() {
                    if let Some(u) = s.unit_at(*c).filter(|u| u.side == us) {
                        ids.push(u.id);
                    }
                }
            }
            let hex = op.roles.first().map_or(Hex::new(0, 0), |r| r.0);
            give(&ids, op.utility, hex, true);
        }
        // Own chokepoints: their guards and holders.
        for (c, vp) in &plan.guard_vp {
            let mut ids: Vec<UnitId> = plan
                .guard
                .iter()
                .filter(|(_, h)| *h == c)
                .map(|(id, _)| *id)
                .collect();
            if let Some(u) = s.unit_at(*c).filter(|u| u.side == us) {
                ids.push(u.id);
            }
            give(&ids, *vp, *c, false);
        }
        let ground = |u: &&Unit| u.side == us && u.hq_rating.is_none() && !super::is_air(s, u);
        // Threatened own objectives: the units near them.
        for o in s.objectives() {
            if s.controller(o.hex) != Some(us) {
                continue;
            }
            let threatened = s
                .units()
                .any(|e| e.side != us && e.hex.distance(o.hex) <= m.defend_radius);
            if !threatened {
                continue;
            }
            let ids: Vec<UnitId> = s
                .units()
                .filter(ground)
                .filter(|u| u.hex.distance(o.hex) <= m.defend_radius)
                .map(|u| u.id)
                .collect();
            give(&ids, objective_margin(s, o), o.hex, false);
        }
        // «Линия»: a ground unit within reach of the enemy holds the line; a
        // step of it is worth the NM its loss costs (R-NM-01) in VP.
        let step_vp = s.side(us).map_or(1.0, |x| f64::from(x.loss_sensitivity))
            / f64::from(s.data().rules.national_morale.steps_per_loss.max(1));
        let enemy: Vec<Hex> = s
            .units()
            .filter(|e| e.side != us && e.hq_rating.is_none() && !super::is_air(s, e))
            .map(|e| e.hex)
            .collect();
        for u in s.units().filter(ground) {
            if enemy
                .iter()
                .any(|h| h.distance(u.hex) <= m.influence_radius)
            {
                let steps = f64::from(u.strength.max(1));
                give(&[u.id], step_vp * steps, u.hex, false);
            }
        }
        // The objective the side attacks: the units within reach of it.
        let vp = self.objective_vp(s, us);
        if let Some(t) = self.attack_target(s, us).filter(|_| vp > 0.0) {
            let ids: Vec<UnitId> = s
                .units()
                .filter(ground)
                .filter(|u| u.hex.distance(t) <= self.params.economy.attack_radius)
                .map(|u| u.id)
                .collect();
            give(&ids, vp, t, false);
        }
        roles
    }

    /// Expected VP per phase a boost of `branch` adds to the side's attacks now
    /// possible: for each own unit its best attack, the change of the enemy's
    /// expected losses and of its own, in VP (R-NM-01 losses).
    fn boost_gain(
        &self,
        s: &GameState,
        us: SideId,
        branch: ceasefire_core::tech::TechBranch,
    ) -> f64 {
        let Some(side) = s.side(us) else { return 0.0 };
        let Ok(level) = side.tech.level(branch) else {
            return 0.0;
        };
        let Ok(boosted) = s.with_tech_level(us, branch, (level + 1).min(3)) else {
            return 0.0;
        };
        let steps_per = f64::from(s.data().rules.national_morale.steps_per_loss.max(1));
        let step_vp =
            |side: SideId| s.side(side).map_or(1.0, |x| f64::from(x.loss_sensitivity)) / steps_per;
        let mut total = 0.0;
        for u in s.units().filter(|u| u.side == us) {
            let best = s
                .units()
                .filter(|e| e.side != us && e.hex.distance(u.hex) == 1)
                .filter_map(|e| {
                    let now = s.forecast_attack(u.id, e.id).ok()?;
                    let then = boosted.forecast_attack(u.id, e.id).ok()?;
                    Some(
                        (then.defender_losses.expected - now.defender_losses.expected)
                            * step_vp(e.side)
                            + (now.attacker_losses.expected - then.attacker_losses.expected)
                                * step_vp(us),
                    )
                })
                .fold(0.0, f64::max);
            total += best;
        }
        total
    }

    /// The objective the side attacks: the next of its sequence not held, else
    /// the most valuable not held.
    fn attack_target(&self, s: &GameState, us: SideId) -> Option<Hex> {
        let next = self.hints.sides.get(&us).and_then(|h| {
            h.sequence
                .iter()
                .find(|hex| s.controller(**hex) != Some(us))
                .copied()
        });
        next.or_else(|| {
            s.objectives()
                .iter()
                .filter(|o| s.controller(o.hex) != Some(us))
                .max_by_key(|o| (o.vp, std::cmp::Reverse(o.hex)))
                .map(|o| o.hex)
        })
    }
}
