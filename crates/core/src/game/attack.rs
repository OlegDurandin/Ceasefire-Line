//! R-MOVE-04 (attack costs), R-COMBAT-02..04, R-COMBAT-06.

use std::sync::Arc;

use crate::combat::{
    fire_params, forecast_fire, forecast_ground, ground_inputs, Combatant, ExchangeForecast,
    FireInput, FireParams, GroundAttack,
};
use crate::data::{GameData, UnitTag};
use crate::error::{AttackError, RuleError};
use crate::hex::Hex;
use crate::movement::{enemy_zoc, entry_cost_halves, Mobility};
use crate::prob::LossForecast;
use crate::scale::range_in_hexes;

use super::{DeclaredAttack, Event, GameState, PendingAdvance, Unit, UnitId};
use crate::prob::{hits_distribution, quantile};
use crate::tactical::CombatSource;

/// R-UI-04: what the rule variants add to one ground attack.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AttackModifiers {
    /// R-COMBAT-07: readiness points of the prepared attack (0: none).
    pub prepared_pct: u8,
    /// R-COMBAT-05: × attacker's A against a target without supply (1.0: none).
    pub unsupplied_target_mult: f64,
    /// R-COMBAT-02 variant: entrenchment levels the attack will strip.
    pub entrench_strip: u8,
}

fn illegal(unit: UnitId, reason: AttackError) -> RuleError {
    RuleError::IllegalAttack {
        unit: unit.0,
        reason,
    }
}

impl GameState {
    pub(super) fn combatant<'d>(
        &self,
        data: &'d GameData,
        unit: &Unit,
    ) -> Result<Combatant<'d>, RuleError> {
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

    /// R-MOVE-01 terrain defense multiplier of `defender`'s hex; variant
    /// `armor_city_penalty`: hard defenders take `hard_defense_mult` where set.
    pub(super) fn terrain_defense(&self, defender: &Unit) -> Result<f64, RuleError> {
        let terrain = self.data.terrain.get(&self.map_terrain(defender.hex)?)?;
        let hard = self.class(defender)?.target_class == crate::data::TargetClass::Hard;
        Ok(match terrain.def.hard_defense_mult {
            Some(m) if hard && self.variants.armor_city_penalty => m,
            _ => terrain.def.defense_mult,
        })
    }

    fn ground_context(&self, attacker: &Unit, defender: &Unit) -> Result<GroundAttack, RuleError> {
        Ok(GroundAttack {
            terrain_mult: self.terrain_defense(defender)?,
            night: self.is_night(),
            across_canal: self.map.has_canal(attacker.hex, defender.hex),
            attack_mult: self
                .modifiers_for(attacker, defender)
                .unsupplied_target_mult,
        })
    }

    /// R-COMBAT-07, R-COMBAT-05 variants of one ground attack.
    fn modifiers_for(&self, attacker: &Unit, defender: &Unit) -> AttackModifiers {
        let c = &self.data.rules.combat;
        let v = self.variants;
        AttackModifiers {
            prepared_pct: if v.prepared_attack && !attacker.moved {
                c.prepared_attack_readiness_pct
            } else {
                0
            },
            unsupplied_target_mult: if v.unsupplied_target && defender.supply == 0 {
                c.unsupplied_target_attack_mult
            } else {
                1.0
            },
            entrench_strip: self
                .attack_entrench_loss(&attacker.class)
                .min(defender.entrenchment),
        }
    }

    /// Both sides of a ground attack as they fight: the attacker with the
    /// R-COMBAT-07 bonus, and the context. Forecast, explanation and
    /// resolution all start here.
    fn ground_sides<'d>(
        &self,
        data: &'d GameData,
        a: &Unit,
        d: &Unit,
    ) -> Result<(Combatant<'d>, Combatant<'d>, GroundAttack), RuleError> {
        let mut att = self.combatant(data, a)?;
        let bonus = self.modifiers_for(a, d).prepared_pct;
        att.readiness_pct = att.readiness_pct.saturating_add(bonus).min(100);
        Ok((att, self.combatant(data, d)?, self.ground_context(a, d)?))
    }

    /// R-UI-04: rule variants that change this attack (for the forecast lines).
    pub fn attack_modifiers(
        &self,
        id: UnitId,
        target: UnitId,
    ) -> Result<AttackModifiers, RuleError> {
        let (a, d) = self.check_attack(id, target)?;
        Ok(self.modifiers_for(&a, &d))
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
        let (att, def, ctx) = self.ground_sides(&data, &a, &d)?;
        forecast_ground(&data.rules.combat, &att, &def, &ctx)
    }

    /// R-UI-04: inputs of the attack's two fires, `[to defender, to attacker]`,
    /// from the same code as the forecast and the resolution.
    pub fn explain_attack(&self, id: UnitId, target: UnitId) -> Result<[FireInput; 2], RuleError> {
        let (a, d) = self.check_attack(id, target)?;
        let data = Arc::clone(&self.data);
        let (att, def, ctx) = self.ground_sides(&data, &a, &d)?;
        ground_inputs(&data.rules.combat, &att, &def, &ctx)
    }

    /// R-UI-04: inputs of a bombardment.
    pub fn explain_bombard(&self, id: UnitId, target: UnitId) -> Result<FireInput, RuleError> {
        let (a, d) = self.check_bombard(id, target)?;
        let data = Arc::clone(&self.data);
        self.bombard_input(&data, &a, &d)
    }

    /// R-MOVE-04: 1 AP and a strike; non-mechanised units lose the rest of
    /// their AP, tank and mech brigades keep moving with ≥ 2 AP left.
    fn charge_attack(&mut self, id: UnitId, exploits: bool) {
        let attack_ap = self.ap.attack;
        let exploit_min = self.ap.exploit_min;
        if let Some(u) = self.units.get_mut(&id) {
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
    }

    /// Tactical battles: fixes the combat (forecast at this moment) and
    /// charges the attack; the losses come later from `conclude`.
    pub(super) fn declare(
        &mut self,
        id: UnitId,
        target: UnitId,
    ) -> Result<DeclaredAttack, RuleError> {
        let (a, d) = self.check_attack(id, target)?;
        let data = Arc::clone(&self.data);
        let (att, def, ctx) = self.ground_sides(&data, &a, &d)?;
        let forecast = forecast_ground(&data.rules.combat, &att, &def, &ctx)?;
        // R-COMBAT-06 + R-MOVE-02: across a canal without a bridge only
        // infantry may occupy, by boat.
        let by_boat = self.map.has_canal(a.hex, d.hex) && !self.map.has_bridge(a.hex, d.hex);
        let occupies = if by_boat {
            att.class.has_tag(UnitTag::BoatCrossing)
        } else {
            att.class.has_tag(UnitTag::OccupyHex)
        };
        let exploits = att.class.has_tag(UnitTag::Exploitation);
        let declared = DeclaredAttack {
            attacker: id,
            defender: target,
            forecast,
            defender_readiness_pct: def.readiness_pct,
            attacker_strength: a.strength,
            defender_strength: d.strength,
            defender_hex: d.hex,
            occupies,
            by_boat,
        };
        self.charge_attack(id, exploits);
        Ok(declared)
    }

    /// R-COMBAT-02, R-MOVE-04, R-COMBAT-06: declare and resolve at once.
    pub(super) fn attack(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, RuleError> {
        let declared = self.declare(id, target)?;
        self.conclude(&declared, CombatSource::Auto)
    }

    /// Losses for a declared combat: dice (defender first, then attacker) or
    /// the tactical outcome's quantile — defender losses at q, attacker
    /// losses at 1 − q of the forecast distributions (q = 1 / 0: the
    /// distribution's maximum / minimum).
    fn combat_losses(
        &mut self,
        c: &DeclaredAttack,
        source: CombatSource,
    ) -> Result<(u8, u8), RuleError> {
        let x = c.forecast.exchange;
        match source {
            CombatSource::Auto => {
                let d = capped(
                    self.hits(&x.vs_defender, Some(c.defender_strength)),
                    c.defender_strength,
                );
                let a = capped(
                    self.hits(&x.vs_attacker, Some(c.attacker_strength)),
                    c.attacker_strength,
                );
                Ok((a, d))
            }
            CombatSource::Tactical { outcome } => {
                let q = *self
                    .data
                    .rules
                    .tactical
                    .percentiles
                    .get(&outcome)
                    .ok_or_else(|| {
                        RuleError::InvalidData(format!("no percentile for {outcome:?}"))
                    })?;
                let dist =
                    |p: &FireParams, cap: u8| hits_distribution(&[(p.n, p.p)], u32::from(cap));
                let d = quantile(&dist(&x.vs_defender, c.defender_strength), q);
                let a = quantile(&dist(&x.vs_attacker, c.attacker_strength), 1.0 - q);
                Ok((
                    capped(a, c.attacker_strength),
                    capped(d, c.defender_strength),
                ))
            }
        }
    }

    /// Applies a declared combat's result: losses, fatigue, retreat or extra
    /// losses, experience, NM, destroyed units, advance.
    pub(super) fn conclude(
        &mut self,
        c: &DeclaredAttack,
        source: CombatSource,
    ) -> Result<Vec<Event>, RuleError> {
        let data = Arc::clone(&self.data);
        let rules = &data.rules;
        let (id, target) = (c.attacker, c.defender);
        let (attacker_losses, defender_losses) = self.combat_losses(c, source)?;
        let mut events = vec![Event::CombatResolved {
            attacker: id,
            defender: target,
            exchange: c.forecast.exchange,
            attacker_losses,
            defender_losses,
            source,
        }];
        let side_of = |s: &GameState, u: UnitId| s.units.get(&u).map(|u| u.side);
        let (a_side, d_side) = (side_of(self, id), side_of(self, target));

        // Read before the attacker's losses: it strips even if destroyed.
        let strip = self
            .units
            .get(&id)
            .map_or(0, |u| self.attack_entrench_loss(&u.class));
        if let Some(u) = self.units.get_mut(&id) {
            let lost = u.lose_steps(attacker_losses, rules.morale.loss_per_step);
            if let Some(side) = a_side {
                self.charge_losses(side, lost, &mut events)?;
            }
        }
        if let Some(u) = self.units.get_mut(&target) {
            let lost = u.lose_steps(defender_losses, rules.morale.loss_per_step);
            // R-COMBAT-02 variant: the attack strips entrenchment after the combat.
            u.entrenchment = u.entrenchment.saturating_sub(strip);
            u.attacks_received += 1;
            u.fought = true;
            u.attacked_by.push(id);
            if let Some(side) = d_side {
                self.charge_losses(side, lost, &mut events)?;
            }
        }

        // R-COMBAT-06: retreat or extra losses.
        let mut vacated = false;
        if let Some(defender) = self.units.get(&target).cloned() {
            if defender.strength == 0 {
                vacated = true;
            } else {
                let r = &rules.retreat;
                // R-COMBAT-06 variant `fortified_hold`: a city or fortified
                // area is held — no retreat and no «nowhere to go» losses.
                let holds = self.variants.fortified_hold
                    && data
                        .terrain
                        .get(&self.map_terrain(defender.hex)?)?
                        .def
                        .stronghold;
                let must_retreat = !holds
                    && (defender.strength <= r.max_strength
                        || (defender_losses >= r.heavy_loss_steps
                            && c.defender_readiness_pct < r.low_readiness_pct));
                if must_retreat {
                    match self.retreat_hex(&defender)? {
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
                                let lost = u.lose_steps(extra, rules.morale.loss_per_step);
                                vacated = u.strength == 0;
                                self.charge_losses(defender.side, lost, &mut events)?;
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
        self.remove_destroyed(&mut events)?;
        self.pending_advance =
            (vacated && attacker_alive && c.occupies).then_some(PendingAdvance {
                unit: id,
                hex: c.defender_hex,
                by_boat: c.by_boat,
            });
        Ok(events)
    }

    /// R-COMBAT-06: a free, passable neighbour outside enemy ZOC, at distance
    /// ≥ 2 from every unit that attacked this one in the current phase, not
    /// enemy-controlled. Preference: own hex over neutral, higher supply,
    /// higher terrain defense, lower (q, r).
    fn retreat_hex(&self, unit: &Unit) -> Result<Option<Hex>, RuleError> {
        let occupancy = self.occupancy();
        let zoc = enemy_zoc(&self.map, &occupancy, unit.side);
        let supply = self.hex_supply(unit.side)?;
        let attackers: Vec<Hex> = unit
            .attacked_by
            .iter()
            .filter_map(|id| self.units.get(id))
            .map(|u| u.hex)
            .collect();
        let mut candidates = Vec::new();
        for to in unit.hex.neighbors() {
            let owner = self.control.get(&to).copied();
            if owner.is_some_and(|s| s != unit.side)
                || attackers.iter().any(|&a| to.distance(a) < 2)
                || occupancy.contains_key(&to)
                || zoc.contains(&to)
            {
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
            let defense = self
                .data
                .terrain
                .get(&self.map_terrain(to)?)?
                .def
                .defense_mult;
            candidates.push((owner == Some(unit.side), supply.get(to), defense, to));
        }
        candidates.sort_by(|x, y| {
            y.0.cmp(&x.0)
                .then(y.1.cmp(&x.1))
                .then(y.2.total_cmp(&x.2))
                .then(x.3.cmp(&y.3))
        });
        Ok(candidates.first().map(|c| c.3))
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
        let in_zoc = enemy_zoc(&self.map, &occupancy, side).contains(&pending.hex);
        let boat_penalty = self.data.rules.movement.boat_crossing_readiness_pct;
        let mut events = vec![Event::Advanced {
            unit: id,
            to: pending.hex,
            by_boat: pending.by_boat,
        }];
        self.take_hex(pending.hex, side, &mut events);
        if let Some(u) = self.units.get_mut(&id) {
            u.hex = pending.hex;
            u.entrenchment = 0;
            u.moved = true;
            if in_zoc {
                u.can_move = false;
            }
            // R-MOVE-02: a boat crossing takes all AP and −30% readiness.
            if pending.by_boat {
                u.ap_halves = 0;
                u.can_move = false;
                u.crossing_penalty_pct = boat_penalty;
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
        Ok(fire_params(
            &data.rules.combat,
            &self.bombard_input(data, a, d)?,
        ))
    }

    /// R-COMBAT-04: inputs of a bombardment (shared by the explanation).
    fn bombard_input(&self, data: &GameData, a: &Unit, d: &Unit) -> Result<FireInput, RuleError> {
        let att = self.combatant(data, a)?;
        let def = self.combatant(data, d)?;
        let rules = &data.rules;
        let night_mult = if self.is_night() && !att.class.has_tag(UnitTag::NightCombat) {
            rules.combat.night_attack_mult
        } else {
            1.0
        };
        let terrain_mult = self.terrain_defense(d)?;
        Ok(FireInput {
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
            terrain_mult,
            entrenchment: def.entrenchment,
            defense_mult: 1.0,
            p_shift: 0.0,
        })
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
        let losses = u8::try_from(self.hits(&params, Some(d.strength)))
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
        let mut events = vec![Event::Bombarded {
            unit: id,
            target,
            params,
            losses,
        }];
        if let Some(u) = self.units.get_mut(&target) {
            let lost = u.lose_steps(losses, rules.morale.loss_per_step);
            u.entrenchment = u.entrenchment.saturating_sub(rules.bombard.entrench_loss);
            u.attacks_received += 1;
            u.fought = true;
            u.attacked_by.push(id);
            self.charge_losses(d.side, lost, &mut events)?;
        }
        self.remove_destroyed(&mut events)?;
        Ok(events)
    }
}

fn capped(hits: u32, strength: u8) -> u8 {
    u8::try_from(hits).unwrap_or(u8::MAX).min(strength)
}
