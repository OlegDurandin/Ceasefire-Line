//! R-TURN-01: start and end of a phase.

use std::sync::Arc;

use crate::data::{ap_to_halves, UnitTag};
use crate::error::RuleError;
use crate::movement::enemy_zoc;
use crate::unit::readiness_pct;

use super::{Event, GameState, UnitId};

impl GameState {
    /// R-TURN-01 step 1 for the active side: source recovery, supply with
    /// inertia, morale, attrition, entrenchment, readiness, AP and strikes.
    pub(super) fn start_phase(&mut self) -> Result<Vec<Event>, RuleError> {
        let data = Arc::clone(&self.data);
        let rules = &data.rules;
        let side = self.active_side();
        let night = self.is_night();
        let nm = self.side_state(side)?.national_morale_pct;
        let mut events = vec![Event::PhaseStarted {
            turn: self.turn,
            side,
            night,
        }];

        // R-TURN-01 step 1 starts with events (R-EVT-01).
        self.run_events(&mut events)?;
        // R-AIR-01: sorties, rest and lost bases of the aircraft.
        self.refresh_air(&mut events)?;
        if self.outcome.is_some() {
            return Ok(events);
        }

        // R-COMBAT-06: "attacked this phase" is per phase, for both sides.
        for u in self.units.values_mut() {
            u.attacked_by.clear();
        }

        // R-SUP-01: captured sources recover +1 per turn for their owner.
        for (&hex, src) in self.sources.iter_mut() {
            if self.control.get(&hex) == Some(&side) && src.current < src.base {
                src.current = src
                    .current
                    .saturating_add(rules.supply.captured_recovery_per_turn)
                    .min(src.base);
                events.push(Event::SourceRecovered {
                    hex,
                    value: src.current,
                });
            }
        }

        // R-TECH-02: boosts due this turn.
        self.finish_boosts(side, &mut events)?;

        // R-MOVE-02: bridges started last phase are ready before supply is computed.
        self.finish_bridges(side, &mut events)?;

        // R-TURN-01: arrivals, then supply, then income by the new supply.
        self.deliver_arrivals(side, nm, &mut events);
        let supply = self.hex_supply(side)?;
        let income = self.income(side, &supply);
        let side_state = self.side_state_mut(side)?;
        side_state.mpp = side_state.mpp.saturating_add(income);
        events.push(Event::IncomeReceived {
            side,
            amount: income,
            total: side_state.mpp,
        });
        // R-HQ-01, R-HQ-02.
        let links = self.attach_hqs(side);
        events.push(Event::HqAttached { links });
        let hq_effects: std::collections::BTreeMap<UnitId, i32> = self
            .units
            .values()
            .filter(|u| u.side == side)
            .map(|u| (u.id, self.hq_effect(u)))
            .collect();
        let zoc = enemy_zoc(&self.map, &self.occupancy(), side);
        let ids: Vec<UnitId> = self
            .units
            .values()
            .filter(|u| u.side == side)
            .map(|u| u.id)
            .collect();

        let mut attrition = Vec::new();
        for id in ids {
            let Some(unit) = self.units.get(&id) else {
                continue;
            };
            let class = data.units.get(&unit.class)?;
            let terrain_def = &data.terrain.get(&self.map_terrain(unit.hex)?)?.def;
            let max_entrench = terrain_def.max_entrench;
            // R-MOVE-01 variant `fortified_hold`: entered an own city or fortified area.
            let stronghold = self.variants.fortified_hold
                && terrain_def.stronghold
                && self.control.get(&unit.hex) == Some(&side);
            let class_ap = ap_to_halves(
                class
                    .ap
                    .ok_or_else(|| RuleError::InvalidData(format!("`{}` has no AP", class.id)))?,
            )?;
            let in_zoc = zoc.contains(&unit.hex);
            let hq_readiness = hq_effects.get(&id).copied().unwrap_or(0);
            let hex_level = supply.get(unit.hex);
            let Some(unit) = self.units.get_mut(&id) else {
                continue;
            };
            let s = &rules.supply;

            // R-SUP-05: supply inertia, drops at most `max_drop_per_phase`, rises at once.
            unit.supply = hex_level.max(unit.supply.saturating_sub(s.max_drop_per_phase));

            // Morale by the new unit supply: 0 → −15 (R-SUP-03); ≥ rest_min_supply
            // without combat since the last own phase → +10; otherwise unchanged.
            if unit.supply == 0 {
                unit.zero_supply_phases = unit.zero_supply_phases.saturating_add(1);
                unit.morale_pct = unit.morale_pct.saturating_sub(rules.morale.no_supply_loss);
            } else {
                unit.zero_supply_phases = 0;
                if unit.supply >= rules.morale.rest_min_supply && !unit.fought {
                    unit.morale_pct = unit.morale_pct.saturating_add(rules.morale.rest_gain);
                }
            }
            unit.morale_pct = unit.morale_pct.min(nm).min(100);
            if unit.zero_supply_phases >= s.attrition_after_phases {
                let lost = unit.lose_steps(s.attrition_steps, rules.morale.loss_per_step);
                attrition.push(lost);
                events.push(Event::Attrition {
                    unit: id,
                    steps: lost,
                });
            }

            // Entrenchment grows without movement and with supply above the low band;
            // a unit that entered an own stronghold gets half its maximum at once.
            if unit.supply >= s.entrench_min_supply {
                if !unit.moved {
                    let gain = u8::try_from(class.entrench_per_turn).unwrap_or(u8::MAX);
                    unit.entrenchment = unit.entrenchment.saturating_add(gain).min(max_entrench);
                } else if stronghold {
                    unit.entrenchment = unit.entrenchment.max(max_entrench / 2);
                }
            }
            unit.entrenchment = unit.entrenchment.min(max_entrench);

            // R-UNIT-01 + R-HQ-02: readiness; fatigue counters reset.
            unit.attacks_made = 0;
            unit.attacks_received = 0;
            unit.crossing_penalty_pct = 0;
            unit.event_penalty_pct = 0;
            unit.base_readiness_pct = readiness_pct(
                &rules.readiness,
                unit.supply,
                unit.morale_pct,
                unit.exp_stars(),
                hq_readiness,
            );
            unit.readiness_basis = super::ReadinessBasis {
                supply: unit.supply,
                morale_pct: unit.morale_pct,
                exp_stars: unit.exp_stars(),
                hq_pct: hq_readiness,
            };

            // R-UNIT-02: AP penalties for low supply and night, minimum AP;
            // with the `zero_supply_ap` variant supply 0 costs more.
            let mut ap = class_ap;
            if unit.supply == 0 {
                ap = ap.saturating_sub(self.ap.zero_supply_penalty);
            } else if unit.supply <= s.low_supply_max {
                ap = ap.saturating_sub(self.ap.low_supply_penalty);
            }
            if night && !class.has_tag(UnitTag::NightCombat) {
                ap = ap.saturating_sub(self.ap.night_penalty);
            }
            unit.ap_halves = ap.max(self.ap.min);
            unit.strikes_left = class.strikes;
            unit.can_move = true;
            unit.started_in_zoc = in_zoc;
            unit.moved = false;
            unit.fought = false;
            unit.acted = false;
            unit.reinforced_steps = 0;

            events.push(Event::UnitRefreshed {
                unit: id,
                supply: unit.supply,
                morale_pct: unit.morale_pct,
                readiness_pct: unit.base_readiness_pct,
                entrenchment: unit.entrenchment,
                ap_halves: unit.ap_halves,
            });
        }
        let lost: u32 = attrition.iter().map(|&l| u32::from(l)).sum();
        self.charge_losses(side, u8::try_from(lost).unwrap_or(u8::MAX), &mut events)?;
        self.remove_destroyed(&mut events)?;
        Ok(events)
    }

    /// R-TURN-01 step 3: objectives, NM, victory; then hand-over.
    pub(super) fn end_phase(&mut self) -> Result<Vec<Event>, RuleError> {
        let mut events = Vec::new();
        self.end_of_phase_checks(&mut events)?;
        events.push(Event::PhaseEnded {
            turn: self.turn,
            side: self.active_side(),
        });
        if let Some(outcome) = self.outcome.clone() {
            events.push(Event::GameOver { outcome });
            return Ok(events);
        }
        self.active += 1;
        if self.active == self.sides.len() {
            self.active = 0;
            self.turn += 1;
        }
        events.extend(self.start_phase()?);
        Ok(events)
    }

    pub(super) fn map_terrain(&self, hex: crate::hex::Hex) -> Result<String, RuleError> {
        self.map
            .cell(hex)
            .map(|c| c.terrain.clone())
            .ok_or_else(|| RuleError::InvalidData(format!("hex {hex:?} is off-map")))
    }
}
