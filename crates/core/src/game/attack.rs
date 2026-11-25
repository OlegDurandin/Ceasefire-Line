//! R-MOVE-04 (attack costs), R-COMBAT-02..04, R-COMBAT-06.

use std::sync::Arc;

use crate::combat::{
    fire_params, forecast_fire, forecast_ground, resolve_ground, sample_hits, Combatant,
    ExchangeForecast, FireInput, FireParams, GroundAttack,
};
use crate::data::{GameData, UnitTag};
use crate::error::{AttackError, RuleError};
use crate::hex::Hex;
use crate::movement::{enemy_zoc, entry_cost_halves, Mobility};
use crate::prob::LossForecast;
use crate::scale::range_in_hexes;

use super::{Event, GameState, PendingAdvance, Unit, UnitId};

fn illegal(unit: UnitId, reason: AttackError) -> RuleError {
    RuleError::IllegalAttack {
        unit: unit.0,
        reason,
    }
}

impl GameState {
    fn combatant<'d>(&self, data: &'d GameData, unit: &Unit) -> Result<Combatant<'d>, RuleError> {
        let class = data.units.get(&unit.class)?;
        let tech = &self.side_state(unit.side)?.tech;
        Ok(Combatant {
            class,
            stats: data.tech.unit_stats(class, tech)?,
            strength: unit.strength,
            readiness_pct: unit.readiness_pct(&data.rules.readiness),
            exp_stars: unit.exp_stars(),
            entrenchment: unit.entrenchment,
        })
    }

    /// Common checks for attack and bombardment: own unit, strike and AP left,
    /// enemy target.
    fn check_strike(&self, id: UnitId, target: UnitId) -> Result<(Unit, Unit), RuleError> {
        let unit = self.own_unit(id)?.clone();
        if unit.strikes_left == 0 {
            return Err(illegal(id, AttackError::NoStrikes));
        }
        if unit.ap_halves < self.ap.attack {
            return Err(illegal(id, AttackError::NoAp));
        }
        let enemy = self.get_unit(target)?.clone();
        if enemy.side == unit.side {
            return Err(illegal(id, AttackError::NotEnemy));
        }
        if !self.class(&enemy)?.target_class.is_ground() {
            return Err(illegal(id, AttackError::NotGroundTarget));
        }
        Ok((unit, enemy))
    }

    fn check_attack(&self, id: UnitId, target: UnitId) -> Result<(Unit, Unit), RuleError> {
        let (unit, enemy) = self.check_strike(id, target)?;
        if self.class(&unit)?.has_tag(UnitTag::Bombard) {
            return Err(illegal(id, AttackError::UseBombard));
        }
        if !unit.hex.is_adjacent(enemy.hex) {
            return Err(illegal(id, AttackError::NotAdjacent));
        }
        Ok((unit, enemy))
    }

    fn ground_context(&self, attacker: &Unit, defender: &Unit) -> Result<GroundAttack, RuleError> {
        let terrain = self.data.terrain.get(&self.map_terrain(defender.hex)?)?;
        Ok(GroundAttack {
            terrain_mult: terrain.def.defense_mult,
            night: self.is_night(),
            across_canal: self.map.has_canal(attacker.hex, defender.hex),
        })
    }

    /// R-COMBAT-03: forecast with the same code path as [`Command::Attack`].
    ///
    /// [`Command::Attack`]: super::Command::Attack
    pub fn forecast_attack(
        &self,
        id: UnitId,
        target: UnitId,
    ) -> Result<ExchangeForecast, RuleError> {
        let (a, d) = self.check_attack(id, target)?;
        let data = Arc::clone(&self.data);
        let ctx = self.ground_context(&a, &d)?;
        forecast_ground(
            &data.rules.combat,
            &self.combatant(&data, &a)?,
            &self.combatant(&data, &d)?,
            &ctx,
        )
    }

    /// R-COMBAT-02, R-MOVE-04, R-COMBAT-06.
    pub(super) fn attack(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, RuleError> {
        let (a, d) = self.check_attack(id, target)?;
        let data = Arc::clone(&self.data);
        let rules = &data.rules;
        let ctx = self.ground_context(&a, &d)?;
        let att = self.combatant(&data, &a)?;
        let def = self.combatant(&data, &d)?;
        let outcome = resolve_ground(&rules.combat, &att, &def, &ctx, &mut self.rng)?;
        let exploits = att.class.has_tag(UnitTag::Exploitation);
        let occupies = att.class.has_tag(UnitTag::OccupyHex);
        let mut events = vec![Event::CombatResolved {
            attacker: id,
            defender: target,
            exchange: outcome.exchange,
            attacker_losses: outcome.attacker_losses,
            defender_losses: outcome.defender_losses,
        }];

        // Attacker: losses, fatigue, R-MOVE-04 costs.
        let attack_ap = self.ap.attack;
        let exploit_min = self.ap.exploit_min;
        if let Some(u) = self.units.get_mut(&id) {
            u.lose_steps(outcome.attacker_losses, rules.morale.loss_per_step);
            u.attacks_made += 1;
            u.fought = true;
            u.acted = true;
            u.strikes_left -= 1;
            u.ap_halves = u.ap_halves.saturating_sub(attack_ap);
            if exploits {
                u.can_move = u.ap_halves >= exploit_min;
            } else {
                u.ap_halves = 0;
                u.can_move = false;
            }
        }
        // Defender: losses, fatigue.
        if let Some(u) = self.units.get_mut(&target) {
            u.lose_steps(outcome.defender_losses, rules.morale.loss_per_step);
            u.attacks_received += 1;
            u.fought = true;
        }

        // R-COMBAT-06: retreat or extra losses.
        let mut vacated = false;
        if let Some(defender) = self.units.get(&target).cloned() {
            if defender.strength == 0 {
                vacated = true;
            } else {
                let r = &rules.retreat;
                let must_retreat = defender.strength <= r.max_strength
                    || (outcome.defender_losses >= r.heavy_loss_steps
                        && def.readiness_pct < r.low_readiness_pct);
                if must_retreat {
                    match self.retreat_hex(&defender, a.hex)? {
                        Some(to) => {
                            self.take_hex(to, defender.side, &mut events);
                            if let Some(u) = self.units.get_mut(&target) {
                                u.hex = to;
                                u.entrenchment = 0;
                                u.moved = true;
                            }
                            events.push(Event::Retreated {
                                unit: target,
                                from: defender.hex,
                                to,
                            });
                            vacated = true;
                        }
                        None => {
                            let extra = r.blocked_extra_losses.min(defender.strength);
                            if let Some(u) = self.units.get_mut(&target) {
                                u.lose_steps(extra, rules.morale.loss_per_step);
                                vacated = u.strength == 0;
                            }
                            events.push(Event::RetreatBlocked {
                                unit: target,
                                extra_losses: extra,
                            });
                        }
                    }
                }
            }
        }

        // Experience: +per_combat each; the attacker gets `win` instead when
        // the defender left the hex. R-HQ-02: ×fast_exp_pct under a strong HQ.
        let e = &rules.experience;
        let attacker_gain = if vacated { e.win } else { e.per_combat };
        for (uid, gain) in [(id, attacker_gain), (target, e.per_combat)] {
            let gain = self.exp_gain(uid, gain);
            if let Some(u) = self.units.get_mut(&uid) {
                u.exp = u.exp.saturating_add(gain).min(e.max);
            }
        }

        let attacker_alive = self.units.get(&id).is_some_and(|u| u.strength > 0);
        self.remove_destroyed(&mut events);
        self.pending_advance = (vacated && attacker_alive && occupies).then_some(PendingAdvance {
            unit: id,
            hex: d.hex,
        });
        Ok(events)
    }

    /// R-COMBAT-06: free hex one step farther from the attacker, passable,
    /// outside enemy ZOC. Prefers own hexes, then the lowest coordinates.
    fn retreat_hex(&self, unit: &Unit, attacker_hex: Hex) -> Result<Option<Hex>, RuleError> {
        let occupancy = self.occupancy();
        let zoc = enemy_zoc(&occupancy, unit.side);
        let mut best: Option<(bool, Hex)> = None;
        for to in unit.hex.neighbors() {
            if to.distance(attacker_hex) != 2 || occupancy.contains_key(&to) || zoc.contains(&to) {
                continue;
            }
            if entry_cost_halves(
                &self.map,
                &self.data.terrain,
                unit.hex,
                to,
                Mobility::Tracked,
                false,
            )?
            .is_none()
            {
                continue;
            }
            let key = (self.control.get(&to) != Some(&unit.side), to);
            if best.is_none_or(|b| key < b) {
                best = Some(key);
            }
        }
        Ok(best.map(|(_, h)| h))
    }

    /// R-COMBAT-06: occupy the vacated hex for 0 AP.
    pub(super) fn advance(&mut self, id: UnitId) -> Result<Vec<Event>, RuleError> {
        let pending = self
            .pending_advance
            .filter(|p| p.unit == id)
            .ok_or(RuleError::NoPendingAdvance(id.0))?;
        let side = self.own_unit(id)?.side;
        let occupancy = self.occupancy();
        if occupancy.contains_key(&pending.hex) {
            return Err(RuleError::NoPendingAdvance(id.0));
        }
        let in_zoc = enemy_zoc(&occupancy, side).contains(&pending.hex);
        let mut events = vec![Event::Advanced {
            unit: id,
            to: pending.hex,
        }];
        self.take_hex(pending.hex, side, &mut events);
        if let Some(u) = self.units.get_mut(&id) {
            u.hex = pending.hex;
            u.entrenchment = 0;
            u.moved = true;
            if in_zoc {
                u.can_move = false;
            }
        }
        self.pending_advance = None;
        Ok(events)
    }

    fn check_bombard(&self, id: UnitId, target: UnitId) -> Result<(Unit, Unit), RuleError> {
        let (unit, enemy) = self.check_strike(id, target)?;
        let class = self.class(&unit)?;
        let range_km = class
            .range_km
            .filter(|_| class.has_tag(UnitTag::Bombard))
            .ok_or_else(|| illegal(id, AttackError::CannotBombard))?;
        let dist = unit.hex.distance(enemy.hex);
        if dist == 0 || dist > range_in_hexes(range_km, self.hex_km) {
            return Err(illegal(id, AttackError::OutOfRange));
        }
        Ok((unit, enemy))
    }

    fn bombard_params(&self, data: &GameData, a: &Unit, d: &Unit) -> Result<FireParams, RuleError> {
        let att = self.combatant(data, a)?;
        let def = self.combatant(data, d)?;
        let rules = &data.rules;
        let night_mult = if self.is_night() && !att.class.has_tag(UnitTag::NightCombat) {
            rules.combat.night_attack_mult
        } else {
            1.0
        };
        let terrain = data.terrain.get(&self.map_terrain(d.hex)?)?;
        Ok(fire_params(
            &rules.combat,
            &FireInput {
                strength: att.strength,
                readiness_pct: att.readiness_pct,
                shot_fraction: rules.bombard.shot_fraction,
                attack: att.stats.attack.against(def.class.target_class),
                shooter_exp: att.exp_stars,
                attack_mult: night_mult,
                defense: def
                    .stats
                    .defense
                    .ground
                    .ok_or_else(|| RuleError::NoGroundDefense(def.class.id.clone()))?,
                target_exp: def.exp_stars,
                terrain_mult: terrain.def.defense_mult,
                entrenchment: def.entrenchment,
                defense_mult: 1.0,
                p_shift: 0.0,
            },
        ))
    }

    /// R-COMBAT-03 for bombardment.
    pub fn forecast_bombard(&self, id: UnitId, target: UnitId) -> Result<LossForecast, RuleError> {
        let (a, d) = self.check_bombard(id, target)?;
        let data = Arc::clone(&self.data);
        let params = self.bombard_params(&data, &a, &d)?;
        Ok(forecast_fire(&data.rules.combat, &params, d.strength))
    }

    /// R-COMBAT-04: no return fire, half shots, −1 entrenchment, no retreat.
    pub(super) fn bombard(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, RuleError> {
        let (a, d) = self.check_bombard(id, target)?;
        let data = Arc::clone(&self.data);
        let rules = &data.rules;
        let params = self.bombard_params(&data, &a, &d)?;
        let losses = u8::try_from(sample_hits(&mut self.rng, &params))
            .unwrap_or(u8::MAX)
            .min(d.strength);
        if let Some(u) = self.units.get_mut(&id) {
            u.attacks_made += 1;
            u.fought = true;
            u.acted = true;
            u.strikes_left -= 1;
            u.ap_halves = 0;
            u.can_move = false;
        }
        if let Some(u) = self.units.get_mut(&target) {
            u.lose_steps(losses, rules.morale.loss_per_step);
            u.entrenchment = u.entrenchment.saturating_sub(rules.bombard.entrench_loss);
            u.attacks_received += 1;
            u.fought = true;
        }
        let mut events = vec![Event::Bombarded {
            unit: id,
            target,
            params,
            losses,
        }];
        self.remove_destroyed(&mut events);
        Ok(events)
    }
}
