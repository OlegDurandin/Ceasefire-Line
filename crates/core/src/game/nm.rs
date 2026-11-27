//! R-NM-01, R-NM-02: national morale, objectives, victory.

use std::collections::BTreeMap;

use crate::error::RuleError;
use crate::side::SideId;

use super::{EndReason, Event, GameState, NmReason, Outcome, VictoryLevel};

impl GameState {
    /// R-NM-01: the highest unit morale the side's NM allows — NM itself, or
    /// 100 with the variant `nm_morale_multiplier` (NM then scales morale in
    /// readiness instead, see [`Self::readiness_morale_pct`]).
    pub(super) fn morale_cap(&self, side: SideId) -> Result<u8, RuleError> {
        if self.variants.nm_morale_multiplier {
            Ok(100)
        } else {
            Ok(self.side_state(side)?.national_morale_pct)
        }
    }

    /// R-NM-01, R-UNIT-01: unit morale as it enters readiness. With the variant
    /// `nm_morale_multiplier` it is scaled by the NM band of the side
    /// (`national_morale.morale_bands`), rounded half up; otherwise unchanged.
    pub fn readiness_morale_pct(&self, side: SideId, morale_pct: u8) -> u8 {
        scale_morale(morale_pct, self.readiness_morale_factor_pct(side))
    }

    /// R-NM-01 variant `nm_morale_multiplier`: the side's morale multiplier in
    /// percent (100 without the variant).
    pub(super) fn readiness_morale_factor_pct(&self, side: SideId) -> u8 {
        if !self.variants.nm_morale_multiplier {
            return 100;
        }
        let nm = self.side(side).map_or(100, |s| s.national_morale_pct);
        self.data.rules.national_morale.morale_factor_pct(nm)
    }

    /// R-NM-01: changes NM within 0..=100 and caps unit morale at the new NM
    /// (no cap with the variant `nm_morale_multiplier`).
    pub(super) fn change_nm(
        &mut self,
        side: SideId,
        delta: i32,
        reason: NmReason,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let multiplier = self.variants.nm_morale_multiplier;
        let state = self.side_state_mut(side)?;
        let old = i32::from(state.national_morale_pct);
        let value = (old + delta).clamp(0, 100) as u8;
        state.national_morale_pct = value;
        let cap = if multiplier { 100 } else { value };
        for u in self.units.values_mut().filter(|u| u.side == side) {
            u.morale_pct = u.morale_pct.min(cap);
        }
        events.push(Event::NationalMorale {
            side,
            change: i32::from(value) - old,
            value,
            reason,
        });
        Ok(())
    }

    /// R-NM-01: −sensitivity NM each time total losses cross a multiple of
    /// `steps_per_loss`.
    pub(super) fn charge_losses(
        &mut self,
        side: SideId,
        steps: u8,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        if steps == 0 {
            return Ok(());
        }
        let per = self.data.rules.national_morale.steps_per_loss;
        let state = self.side_state_mut(side)?;
        let before = state.steps_lost / per;
        state.steps_lost += u32::from(steps);
        let total = state.steps_lost;
        let crossed = total / per - before;
        let loss = crossed * u32::from(state.loss_sensitivity);
        if loss > 0 {
            let delta = -i32::try_from(loss).unwrap_or(i32::MAX);
            self.change_nm(side, delta, NmReason::StepsLost { total }, events)?;
        }
        Ok(())
    }

    /// R-TURN-01 step 3, R-NM-01, R-NM-02.
    pub(super) fn end_of_phase_checks(&mut self, events: &mut Vec<Event>) -> Result<(), RuleError> {
        let share = u32::from(self.data.rules.national_morale.objective_taken_share_pct);
        // Objectives that changed hands since the last check.
        for o in self.objectives.clone() {
            let now = self.control.get(&o.hex).copied();
            let before = self.objective_holders.get(&o.hex).copied().flatten();
            if now == before {
                continue;
            }
            self.objective_holders.insert(o.hex, now);
            events.push(Event::ObjectiveChanged {
                hex: o.hex,
                from: before,
                to: now,
            });
            if let Some(loser) = before {
                self.change_nm(
                    loser,
                    -i32::from(o.nm_value),
                    NmReason::ObjectiveLost { hex: o.hex },
                    events,
                )?;
            }
            if let Some(taker) = now {
                let gain = u32::from(o.nm_value) * share / 100;
                self.change_nm(
                    taker,
                    gain as i32,
                    NmReason::ObjectiveTaken { hex: o.hex },
                    events,
                )?;
            }
        }
        // Key hexes held by the side whose phase ends.
        let side = self.active_side();
        for k in self.key_hexes.clone() {
            if k.side == side && self.control.get(&k.hex) == Some(&side) {
                self.change_nm(
                    side,
                    i32::from(k.nm_per_turn),
                    NmReason::KeyHexHeld { hex: k.hex },
                    events,
                )?;
            }
        }
        self.outcome = self.sudden_death().or_else(|| self.by_date());
        Ok(())
    }

    /// R-NM-02: VP = objectives held + NM + steps taken out through exits +
    /// VP for the enemy's lost steps (scenario conditions).
    pub fn victory_points(&self) -> Vec<(SideId, i32)> {
        let mut vp: BTreeMap<SideId, i32> = self
            .sides
            .iter()
            .map(|s| (s.id, i32::from(s.national_morale_pct)))
            .collect();
        for o in &self.objectives {
            if let Some(holder) = self.control.get(&o.hex) {
                *vp.entry(*holder).or_insert(0) += o.vp;
            }
        }
        for (side, gained) in &self.exited_vp {
            *vp.entry(*side).or_insert(0) += gained;
        }
        for (side, per) in &self.conditions.vp_per_enemy_step {
            let lost: u32 = self
                .sides
                .iter()
                .filter(|s| s.id != *side)
                .map(|s| s.steps_lost)
                .sum();
            *vp.entry(*side).or_insert(0) += per.for_steps(lost);
        }
        self.sides.iter().map(|s| (s.id, vp[&s.id])).collect()
    }

    /// R-NM-02: NM at or below the threshold → the other side wins decisively;
    /// both at once → draw.
    fn sudden_death(&self) -> Option<Outcome> {
        let default = self.data.rules.national_morale.sudden_death_pct;
        // Per side (scenario conditions); `None` — this side never breaks.
        let limit = |side: SideId| -> Option<u8> {
            self.conditions
                .sudden_death
                .get(&side)
                .copied()
                .unwrap_or(Some(default))
        };
        let broken: Vec<SideId> = self
            .sides
            .iter()
            .filter(|s| limit(s.id).is_some_and(|l| s.national_morale_pct <= l))
            .map(|s| s.id)
            .collect();
        if broken.is_empty() {
            return None;
        }
        let standing: Vec<SideId> = self
            .sides
            .iter()
            .map(|s| s.id)
            .filter(|id| !broken.contains(id))
            .collect();
        let (winner, level) = match standing.as_slice() {
            [one] => (Some(*one), VictoryLevel::Decisive),
            _ => (None, VictoryLevel::Draw),
        };
        Some(Outcome {
            winner,
            level,
            reason: EndReason::SuddenDeath,
            vp: self.victory_points(),
        })
    }

    /// R-NM-02: after the last side's phase of `last_turn`: VP margin against
    /// the best opponent vs. the side's thresholds; otherwise a draw.
    fn by_date(&self) -> Option<Outcome> {
        let last = self.last_turn?;
        if self.turn < last || self.active + 1 < self.sides.len() {
            return None;
        }
        let vp = self.victory_points();
        let margin = |side: SideId| {
            let own = vp.iter().find(|(s, _)| *s == side).map_or(0, |v| v.1);
            let best_other = vp
                .iter()
                .filter(|(s, _)| *s != side)
                .map(|v| v.1)
                .max()
                .unwrap_or(0);
            own - best_other
        };
        let mut result = (None, VictoryLevel::Draw);
        for t in &self.victory {
            let m = margin(t.side);
            if m >= t.decisive {
                result = (Some(t.side), VictoryLevel::Decisive);
                break;
            }
            if m >= t.victory && result.0.is_none() {
                result = (Some(t.side), VictoryLevel::Victory);
            }
        }
        Some(Outcome {
            winner: result.0,
            level: result.1,
            reason: EndReason::Date,
            vp,
        })
    }
}

/// `morale × factor / 100`, rounded half up.
pub(super) fn scale_morale(morale_pct: u8, factor_pct: u8) -> u8 {
    let scaled = (u32::from(morale_pct) * u32::from(factor_pct) + 50) / 100;
    u8::try_from(scaled).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::*;
    use crate::data::{GameData, GameDataJson};
    use crate::game::{ScenarioSetup, SideState};
    use crate::hex::Hex;
    use crate::map::{HexCell, HexMap};
    use crate::tech::TechLevels;

    fn state(sensitivity: u8) -> GameState {
        let data = GameData::from_json(GameDataJson {
            rules: include_str!("../../../../data/rules.json"),
            terrain: include_str!("../../../../data/terrain.json"),
            units: include_str!("../../../../data/units.json"),
            tech: include_str!("../../../../data/tech.json"),
        })
        .unwrap();
        let side = |id| SideState {
            id,
            tech: TechLevels::default(),
            national_morale_pct: 100,
            mpp: 0,
            base_income: 0,
            purchase_limits: BTreeMap::new(),
            purchase_alt: Default::default(),
            purchased: BTreeMap::new(),
            loss_sensitivity: sensitivity,
            steps_lost: 0,
            night_combat: Vec::new(),
        };
        let mut map = HexMap::default();
        map.insert(Hex::new(0, 0), HexCell::new("plain"));
        let setup = ScenarioSetup {
            hex_km: None,
            supply_decay_ap: None,
            seed: 1,
            start_turn: 1,
            sides: vec![side(SideId(0)), side(SideId(1))],
            map,
            control: BTreeMap::new(),
            sources: vec![],
            units: vec![],
            cities: vec![],
            scheduled_income: vec![],
            objectives: vec![],
            key_hexes: vec![],
            last_turn: None,
            victory: vec![],
            events: vec![],
            tech_boosts: vec![],
            neutral: vec![],
            offmap_bases: vec![],
            variants: BTreeMap::new(),
            conditions: Default::default(),
        };
        GameState::new(Arc::new(data), setup).unwrap().0
    }

    #[test]
    fn r_nm_01_every_ten_steps() {
        let mut s = state(2);
        let mut events = Vec::new();
        let side = SideId(0);
        s.charge_losses(side, 9, &mut events).unwrap();
        assert_eq!(s.sides[0].national_morale_pct, 100);
        s.charge_losses(side, 1, &mut events).unwrap();
        assert_eq!(
            s.sides[0].national_morale_pct, 98,
            "Israel-like sensitivity 2"
        );
        s.charge_losses(side, 25, &mut events).unwrap();
        assert_eq!(s.sides[0].national_morale_pct, 94, "35 steps: 3 multiples");
    }
}
