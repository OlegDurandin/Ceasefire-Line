//! R-AI-01 «сектора», R-MOVE-05 (stage 4 step 7, designer decisions
//! 2026-11-26): the operational level counts threat and forces by the
//! scenario's sectors and moves a free reserve by road to the sector whose
//! threat exceeds its forces.
//!
//! - Threat of a sector: power of enemy ground units within
//!   `transfer.threat_radius` hexes of its hexes; forces: power of own ground
//!   units in its hexes (HQs and aircraft do not count).
//! - A sector needs help when threat > forces × `need_ratio`; a donor has
//!   forces > threat. The donor's reserve: an own ground unit there, out of
//!   contact, in no operation or chokepoint role, whose departure keeps the
//!   donor's forces ≥ its threat; the strongest such.
//! - Destination: a free hex of the needy sector the transfer route reaches,
//!   nearest the enemy there. At most `max_per_phase` transfers a phase, when
//!   the MPP cover it.

use std::collections::BTreeSet;

use ceasefire_core::game::{Command, Unit, UnitId};
use ceasefire_core::hex::Hex;

use super::{is_air, power, AiV1, Board};

impl AiV1 {
    pub(super) fn transfer(&self, b: &Board) -> Option<Command> {
        let t = &self.params.transfer;
        if !t.enabled || self.hints.sectors.len() < 2 || self.transfers_done >= t.max_per_phase {
            return None;
        }
        let s = b.s;
        let ground = |u: &Unit| u.hq_rating.is_none() && !is_air(s, u);
        let enemy: Vec<&Unit> = b.enemy.iter().copied().filter(|u| ground(u)).collect();
        let sectors: Vec<(BTreeSet<Hex>, f64, f64)> = self
            .hints
            .sectors
            .iter()
            .map(|(_, hexes)| {
                let set: BTreeSet<Hex> = hexes.iter().copied().collect();
                let threat: f64 = enemy
                    .iter()
                    .filter(|e| set.iter().any(|h| h.distance(e.hex) <= t.threat_radius))
                    .map(|e| power(s, e))
                    .sum();
                let forces: f64 = b
                    .own
                    .iter()
                    .filter(|u| ground(u) && set.contains(&u.hex))
                    .map(|u| power(s, u))
                    .sum();
                (set, threat, forces)
            })
            .collect();
        // The neediest sector first.
        let mut needy: Vec<usize> = (0..sectors.len())
            .filter(|&i| sectors[i].1 > sectors[i].2 * t.need_ratio)
            .collect();
        needy.sort_by(|&a, &c| {
            let gap = |i: usize| sectors[i].1 - sectors[i].2;
            gap(c).total_cmp(&gap(a)).then(a.cmp(&c))
        });
        let mpp = s.side(b.side).map_or(0, |x| x.mpp);
        let busy: BTreeSet<UnitId> = self.plan.as_ref().map_or_else(BTreeSet::new, |p| {
            p.envelop
                .keys()
                .chain(p.guard.keys())
                .chain(p.withdraw.keys())
                .chain(p.pin.keys())
                .chain(p.hold.iter())
                .copied()
                .collect()
        });
        for n in needy {
            let target = &sectors[n].0;
            for (d, (donor, threat, forces)) in sectors.iter().enumerate() {
                if d == n || forces <= threat {
                    continue;
                }
                let mut reserves: Vec<&Unit> = b
                    .own
                    .iter()
                    .copied()
                    .filter(|u| ground(u) && donor.contains(&u.hex) && !busy.contains(&u.id))
                    .filter(|u| !u.acted && !enemy.iter().any(|e| e.hex.distance(u.hex) <= 1))
                    .filter(|u| forces - power(s, u) >= *threat)
                    .collect();
                reserves.sort_by(|a, c| power(s, c).total_cmp(&power(s, a)).then(a.id.cmp(&c.id)));
                for u in reserves {
                    let Ok(cost) = s.transfer_cost(u.id) else {
                        continue;
                    };
                    if cost > mpp {
                        continue;
                    }
                    let near_enemy = |h: Hex| {
                        enemy
                            .iter()
                            .map(|e| e.hex.distance(h))
                            .min()
                            .unwrap_or(u32::MAX)
                    };
                    let dest = target
                        .iter()
                        .copied()
                        .filter(|h| s.unit_at(*h).is_none() && s.transfer_route(u.id, *h).is_ok())
                        .min_by_key(|h| (near_enemy(*h), *h));
                    if let Some(to) = dest {
                        return Some(Command::Transfer { unit: u.id, to });
                    }
                }
            }
        }
        None
    }
}
