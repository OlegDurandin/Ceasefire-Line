//! R-AIR-01, R-AIR-02 (stage 4, no SAM umbrellas yet): aircraft on bases,
//! strikes on units, airfields and bridges, automatic interception and
//! escort, rebasing. Aircraft live apart from ground units, so occupancy,
//! ZOC, control and supply never see them.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::air::{air_fire_input, parked_strike_input, strike_input, strike_return_input};
use crate::combat::{fire_params, forecast_fire, Combatant, FireParams};
use crate::data::{GameData, TargetClass, UnitTag};
use crate::error::RuleError;
use crate::hex::Hex;
use crate::prob::LossForecast;
use crate::scale::range_in_hexes;
use crate::side::SideId;

use super::{Event, GameState, UnitId};

/// Where an aircraft is based: a hex with a city or an airfield, or a base
/// off the map (R-AIR-01, designer decision 2026-11-26).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AirBase {
    Hex { hex: Hex },
    OffMap { id: String },
}

/// Map edge an off-map base lies behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapEdge {
    North,
    South,
    East,
    West,
}

/// R-AIR-01: a base off the map; ranges count from the map edge plus
/// `distance_km`; it can be attacked only by a scenario event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OffMapBase {
    pub id: String,
    pub side: SideId,
    pub edge: MapEdge,
    pub distance_km: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirUnit {
    pub id: UnitId,
    pub class: String,
    pub side: SideId,
    pub base: AirBase,
    pub strength: u8,
    pub morale_pct: u8,
    /// Hundredths of a star.
    pub exp: u16,
    pub nation: Option<String>,
    /// R-UI-03: optional sprite model id; does not change unit statistics.
    #[serde(default)]
    pub model: Option<String>,
    pub name: Option<String>,
    /// R-AIR-01: −10 per sortie, +20 back after an own phase without one.
    pub fatigue_pct: u8,
    /// Sorties this own phase (strikes); escorts count too.
    pub sorties: u32,
    /// Flew since the start of its last own phase (no rest then).
    pub flew: bool,
    /// Rebased this phase: no sorties.
    pub rebased: bool,
    /// Intercepted in the current enemy phase.
    pub intercepted: bool,
}

impl AirUnit {
    pub fn exp_stars(&self) -> u8 {
        u8::try_from(self.exp / 100).unwrap_or(u8::MAX)
    }

    /// R-AIR-01: base readiness (full supply at a base) minus sortie fatigue.
    pub fn readiness_pct(&self, data: &GameData) -> u8 {
        let rules = &data.rules;
        let base = crate::unit::readiness_pct(
            &rules.readiness,
            rules.supply.max,
            self.morale_pct,
            self.exp_stars(),
            0,
        );
        base.saturating_sub(self.fatigue_pct)
    }
}

/// Why an air command is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AirError {
    NotAircraft,
    Night,
    AlreadyFlew,
    Rebased,
    OutOfRange,
    NotEnemy,
    /// Only strike aircraft hit airfields and bridges.
    NotInfrastructureStriker,
    NoParkedAircraft,
    NoBridge,
    NotABase,
    NoReadiness,
}

fn refuse(id: UnitId, reason: AirError) -> RuleError {
    RuleError::IllegalAir { unit: id.0, reason }
}

/// R-AIR-02: one air battle of the sortie and its expected losses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirDuel {
    /// The fighter that fired first in the pairing (escort or interceptor).
    pub a: UnitId,
    pub b: UnitId,
    pub a_fire: FireParams,
    pub b_fire: FireParams,
}

/// R-COMBAT-03 for air: what the sortie expects, with the code the resolution uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirStrikeForecast {
    pub escorts: Vec<UnitId>,
    pub interceptors: Vec<UnitId>,
    /// Escort i against interceptor i.
    pub escort_duels: Vec<AirDuel>,
    /// Interceptors left over against the strike group (`a` = interceptor).
    pub group_duels: Vec<AirDuel>,
    /// The strike itself at full strength (before any interception losses).
    pub strike: FireParams,
    pub strike_losses: LossForecast,
    /// The target's return fire (ground strikes only).
    pub return_fire: Option<FireParams>,
}

/// What a sortie aims at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aim {
    Unit(UnitId),
    Airfield(Hex),
    Bridge(Hex, Hex),
}

impl GameState {
    /// Aircraft, by id.
    pub fn air_units(&self) -> impl Iterator<Item = &AirUnit> {
        self.air.values()
    }

    pub fn air_unit(&self, id: UnitId) -> Option<&AirUnit> {
        self.air.get(&id)
    }

    pub fn offmap_bases(&self) -> &[OffMapBase] {
        &self.offmap_bases
    }

    fn air_combatant<'d>(
        &self,
        data: &'d GameData,
        a: &AirUnit,
    ) -> Result<Combatant<'d>, RuleError> {
        let class = data.units.get(&a.class)?;
        let tech = &self.side_state(a.side)?.tech;
        Ok(Combatant {
            class,
            stats: data.tech.unit_stats(class, tech)?,
            strength: a.strength,
            readiness_pct: a.readiness_pct(data),
            exp_stars: a.exp_stars(),
            entrenchment: 0,
        })
    }

    /// Distance in hexes from a base to a hex (off-map: from the edge, plus the base's distance).
    pub fn base_distance(&self, base: &AirBase, to: Hex) -> Option<u32> {
        match base {
            AirBase::Hex { hex } => Some(hex.distance(to)),
            AirBase::OffMap { id } => {
                let b = self.offmap_bases.iter().find(|b| &b.id == id)?;
                let edge = self.edge_hexes(b.edge);
                let to_edge = edge.iter().map(|h| h.distance(to)).min()?;
                Some(to_edge + range_in_hexes(b.distance_km, self.hex_km))
            }
        }
    }

    fn edge_hexes(&self, edge: MapEdge) -> Vec<Hex> {
        let offs: Vec<(Hex, [i32; 2])> = self
            .map
            .hexes()
            .map(|h| (h, crate::scenario::axial_to_offset(h)))
            .collect();
        let pick = |key: fn(&[i32; 2]) -> i32, max: bool| {
            let v = offs.iter().map(|(_, o)| key(o));
            let extreme = if max { v.max() } else { v.min() };
            offs.iter()
                .filter(|(_, o)| Some(key(o)) == extreme)
                .map(|(h, _)| *h)
                .collect::<Vec<_>>()
        };
        match edge {
            MapEdge::West => pick(|o| o[0], false),
            MapEdge::East => pick(|o| o[0], true),
            MapEdge::North => pick(|o| o[1], false),
            MapEdge::South => pick(|o| o[1], true),
        }
    }

    fn radius_hexes(&self, a: &AirUnit) -> Result<u32, RuleError> {
        let class = self.data.units.get(&a.class)?;
        let km = class
            .radius_km
            .ok_or_else(|| refuse(a.id, AirError::NotAircraft))?;
        Ok(range_in_hexes(km, self.hex_km))
    }

    /// A hex an aircraft of `side` may be based at: its own, with a city or an airfield.
    pub fn is_air_base(&self, side: SideId, hex: Hex) -> bool {
        self.control.get(&hex) == Some(&side)
            && (self.map.is_airfield(hex) || self.cities.iter().any(|c| c.hex == hex))
    }

    fn check_sortie(&self, id: UnitId, aim: Aim) -> Result<(AirUnit, Hex), RuleError> {
        let a = self
            .air
            .get(&id)
            .ok_or(RuleError::UnknownUnit(id.0))?
            .clone();
        if a.side != self.active_side() {
            return Err(RuleError::NotYourUnit(id.0));
        }
        if self.is_night() {
            return Err(refuse(id, AirError::Night));
        }
        if a.rebased {
            return Err(refuse(id, AirError::Rebased));
        }
        let class = self.data.units.get(&a.class)?;
        if a.sorties >= class.strikes {
            return Err(refuse(id, AirError::AlreadyFlew));
        }
        if a.readiness_pct(&self.data) == 0 {
            return Err(refuse(id, AirError::NoReadiness));
        }
        let target_hex = match aim {
            Aim::Unit(t) => {
                let enemy = self.get_unit(t)?;
                if enemy.side == a.side {
                    return Err(refuse(id, AirError::NotEnemy));
                }
                enemy.hex
            }
            Aim::Airfield(h) => {
                if !class.has_tag(UnitTag::StrikeInfrastructure) {
                    return Err(refuse(id, AirError::NotInfrastructureStriker));
                }
                let parked = self
                    .air
                    .values()
                    .any(|p| p.side != a.side && p.base == AirBase::Hex { hex: h });
                if !parked {
                    return Err(refuse(id, AirError::NoParkedAircraft));
                }
                h
            }
            Aim::Bridge(x, y) => {
                if !class.has_tag(UnitTag::StrikeInfrastructure) {
                    return Err(refuse(id, AirError::NotInfrastructureStriker));
                }
                if !self.map.has_crossing(x, y) {
                    return Err(refuse(id, AirError::NoBridge));
                }
                x
            }
        };
        let dist = self
            .base_distance(&a.base, target_hex)
            .ok_or_else(|| refuse(id, AirError::NotABase))?;
        if dist > self.radius_hexes(&a)? {
            return Err(refuse(id, AirError::OutOfRange));
        }
        Ok((a, target_hex))
    }

    /// Fighters that would escort (own) and intercept (enemy) a sortie at `target`.
    fn sortie_fighters(
        &self,
        striker: &AirUnit,
        target: Hex,
    ) -> Result<(Vec<UnitId>, Vec<UnitId>), RuleError> {
        let air = &self.data.rules.air;
        let min = air.intercept_min_readiness_pct;
        // SC п. 30: a worn fighter stays out of interception and escort.
        let min_strength = if self.variants.air_strength_threshold {
            air.intercept_min_strength
        } else {
            1
        };
        let intercept = range_in_hexes(air.intercept_radius_km, self.hex_km);
        let mut escorts = Vec::new();
        let mut interceptors = Vec::new();
        for f in self.air.values() {
            let class = self.data.units.get(&f.class)?;
            if f.id == striker.id || f.readiness_pct(&self.data) < min || f.strength < min_strength
            {
                continue;
            }
            let Some(d) = self.base_distance(&f.base, target) else {
                continue;
            };
            if f.side == striker.side {
                if class.has_tag(UnitTag::Escort)
                    && !f.rebased
                    && f.sorties < class.strikes
                    && d <= self.radius_hexes(f)?
                {
                    escorts.push((f.readiness_pct(&self.data), f.strength, f.id));
                }
            } else if class.has_tag(UnitTag::Intercept) && !f.intercepted && d <= intercept {
                interceptors.push((f.readiness_pct(&self.data), f.strength, f.id));
            }
        }
        // Best first: readiness, then strength, then the lower id.
        let order = |v: &mut Vec<(u8, u8, UnitId)>| {
            v.sort_by(|x, y| y.0.cmp(&x.0).then(y.1.cmp(&x.1)).then(x.2.cmp(&y.2)));
        };
        order(&mut escorts);
        order(&mut interceptors);
        // Designer decision 2026-11-26: at most `max_intercepts_per_strike` interceptors.
        interceptors.truncate(air.max_intercepts_per_strike);
        // One escort per interceptor at most: the rest stay home.
        escorts.truncate(interceptors.len());
        Ok((
            escorts.into_iter().map(|e| e.2).collect(),
            interceptors.into_iter().map(|i| i.2).collect(),
        ))
    }

    fn duel(&self, data: &GameData, a: UnitId, b: UnitId) -> Result<AirDuel, RuleError> {
        let (ua, ub) = (self.air[&a].clone(), self.air[&b].clone());
        let (ca, cb) = (
            self.air_combatant(data, &ua)?,
            self.air_combatant(data, &ub)?,
        );
        Ok(AirDuel {
            a,
            b,
            a_fire: fire_params(&data.rules.combat, &air_fire_input(&ca, &cb)),
            b_fire: fire_params(&data.rules.combat, &air_fire_input(&cb, &ca)),
        })
    }

    fn forecast_sortie(&self, id: UnitId, aim: Aim) -> Result<AirStrikeForecast, RuleError> {
        let (a, target) = self.check_sortie(id, aim)?;
        let data = Arc::clone(&self.data);
        let (escorts, interceptors) = self.sortie_fighters(&a, target)?;
        let escort_duels = escorts
            .iter()
            .zip(&interceptors)
            .map(|(e, i)| self.duel(&data, *e, *i))
            .collect::<Result<Vec<_>, _>>()?;
        let group_duels = interceptors
            .iter()
            .skip(escorts.len())
            .map(|i| self.duel(&data, *i, id))
            .collect::<Result<Vec<_>, _>>()?;
        let striker = self.air_combatant(&data, &a)?;
        let (strike, cap, return_fire) = match aim {
            Aim::Unit(t) => {
                let enemy = self.get_unit(t)?.clone();
                let target = self.combatant(&data, &enemy)?;
                (
                    fire_params(&data.rules.combat, &strike_input(&striker, &target)),
                    enemy.strength,
                    Some(fire_params(
                        &data.rules.combat,
                        &strike_return_input(&target, &striker),
                    )),
                )
            }
            Aim::Airfield(h) => {
                let parked: u8 = self
                    .air
                    .values()
                    .filter(|p| p.side != a.side && p.base == AirBase::Hex { hex: h })
                    .map(|p| p.strength)
                    .sum();
                (
                    fire_params(
                        &data.rules.combat,
                        &parked_strike_input(&striker, data.rules.air.parked_air_defense),
                    ),
                    parked,
                    None,
                )
            }
            Aim::Bridge(..) => (
                fire_params(&data.rules.combat, &parked_strike_input(&striker, 0.0)),
                0,
                None,
            ),
        };
        Ok(AirStrikeForecast {
            escorts,
            interceptors,
            escort_duels,
            group_duels,
            strike,
            strike_losses: forecast_fire(&data.rules.combat, &strike, cap),
            return_fire,
        })
    }

    /// R-AIR-02 forecast of a strike on a ground unit.
    pub fn forecast_air_strike(
        &self,
        id: UnitId,
        target: UnitId,
    ) -> Result<AirStrikeForecast, RuleError> {
        self.forecast_sortie(id, Aim::Unit(target))
    }

    /// R-AIR-02 forecast of a strike on aircraft parked at an airfield.
    pub fn forecast_airfield_strike(
        &self,
        id: UnitId,
        hex: Hex,
    ) -> Result<AirStrikeForecast, RuleError> {
        self.forecast_sortie(id, Aim::Airfield(hex))
    }

    /// Steps an aircraft loses; destroyed ones are removed; NM is charged.
    fn air_losses(
        &mut self,
        id: UnitId,
        hits: u32,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let Some(a) = self.air.get_mut(&id) else {
            return Ok(());
        };
        let lost = u8::try_from(hits).unwrap_or(u8::MAX).min(a.strength);
        a.strength -= lost;
        let (side, dead) = (a.side, a.strength == 0);
        self.charge_losses(side, lost, events)?;
        if dead {
            self.air.remove(&id);
            events.push(Event::AircraftDestroyed { unit: id });
        }
        Ok(())
    }

    fn fly(&mut self, id: UnitId) {
        let loss = self.data.rules.air.sortie_readiness_loss_pct;
        if let Some(a) = self.air.get_mut(&id) {
            a.fatigue_pct = a.fatigue_pct.saturating_add(loss).min(100);
            a.flew = true;
        }
    }

    /// Air battles before the strike; returns whether the striker survived.
    fn fight_for_sortie(
        &mut self,
        f: &AirStrikeForecast,
        striker: UnitId,
        events: &mut Vec<Event>,
    ) -> Result<bool, RuleError> {
        // Fatigue comes after the sortie (R-AIR-01): the battles fire at the
        // readiness the forecast shows.
        for e in &f.escorts {
            if let Some(a) = self.air.get_mut(e) {
                a.sorties += 1;
            }
        }
        for i in &f.interceptors {
            if let Some(a) = self.air.get_mut(i) {
                a.intercepted = true;
            }
        }
        for d in f.escort_duels.iter().chain(&f.group_duels) {
            let hit_b = self.hits(&d.a_fire, None);
            let hit_a = self.hits(&d.b_fire, None);
            events.push(Event::AirBattle {
                a: d.a,
                b: d.b,
                a_losses: u8::try_from(hit_a).unwrap_or(u8::MAX),
                b_losses: u8::try_from(hit_b).unwrap_or(u8::MAX),
            });
            self.air_losses(d.a, hit_a, events)?;
            self.air_losses(d.b, hit_b, events)?;
        }
        for id in f.escorts.iter().chain(&f.interceptors) {
            self.fly(*id);
        }
        Ok(self.air.contains_key(&striker))
    }

    fn sortie(&mut self, id: UnitId, aim: Aim) -> Result<Vec<Event>, RuleError> {
        let f = self.forecast_sortie(id, aim)?;
        let data = Arc::clone(&self.data);
        let mut events = Vec::new();
        if let Some(a) = self.air.get_mut(&id) {
            a.sorties += 1;
        }
        let survived = self.fight_for_sortie(&f, id, &mut events)?;
        if !survived {
            return Ok(events);
        }
        let events = self.strike_with(id, aim, &data, events)?;
        self.fly(id);
        Ok(events)
    }

    /// The strike itself, after the air battles.
    fn strike_with(
        &mut self,
        id: UnitId,
        aim: Aim,
        data: &GameData,
        mut events: Vec<Event>,
    ) -> Result<Vec<Event>, RuleError> {
        // The strike with what is left of the group (same parameter code as the forecast).
        let a = self.air[&id].clone();
        let striker = self.air_combatant(data, &a)?;
        match aim {
            Aim::Unit(t) => {
                let enemy = self.get_unit(t)?.clone();
                let target = self.combatant(data, &enemy)?;
                let hit = fire_params(&data.rules.combat, &strike_input(&striker, &target));
                let back = fire_params(&data.rules.combat, &strike_return_input(&target, &striker));
                let losses = u8::try_from(self.hits(&hit, Some(enemy.strength)))
                    .unwrap_or(u8::MAX)
                    .min(enemy.strength);
                let back_hits = self.hits(&back, Some(a.strength));
                events.push(Event::AirStrike {
                    unit: id,
                    target: t,
                    params: hit,
                    losses,
                    return_params: back,
                    return_losses: u8::try_from(back_hits).unwrap_or(u8::MAX).min(a.strength),
                });
                let loss = data.rules.air.ground_strike_readiness_loss_pct;
                let strip = self.attack_entrench_loss(&a.class);
                if let Some(u) = self.units.get_mut(&t) {
                    let lost = u.lose_steps(losses, data.rules.morale.loss_per_step);
                    // R-COMBAT-02 variant: a strike strips entrenchment too.
                    u.entrenchment = u.entrenchment.saturating_sub(strip);
                    u.event_penalty_pct = u.event_penalty_pct.saturating_add(loss).min(100);
                    u.attacks_received += 1;
                    self.charge_losses(enemy.side, lost, &mut events)?;
                }
                self.air_losses(id, back_hits, &mut events)?;
                self.remove_destroyed(&mut events)?;
            }
            Aim::Airfield(h) => {
                let hit = fire_params(
                    &data.rules.combat,
                    &parked_strike_input(&striker, data.rules.air.parked_air_defense),
                );
                let mut hits = self.hits(&hit, None);
                let mut parked: Vec<(u8, UnitId)> = self
                    .air
                    .values()
                    .filter(|p| p.side != a.side && p.base == AirBase::Hex { hex: h })
                    .map(|p| (p.strength, p.id))
                    .collect();
                parked.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
                events.push(Event::AirfieldStrike {
                    unit: id,
                    hex: h,
                    params: hit,
                });
                for (strength, pid) in parked {
                    let take = hits.min(u32::from(strength));
                    hits -= take;
                    self.air_losses(pid, take, &mut events)?;
                }
                let loss = data.rules.air.airfield_strike_readiness_loss_pct;
                for p in self
                    .air
                    .values_mut()
                    .filter(|p| p.base == AirBase::Hex { hex: h })
                {
                    p.fatigue_pct = p.fatigue_pct.saturating_add(loss).min(100);
                }
            }
            Aim::Bridge(x, y) => {
                let chance = data.rules.bridge.air_strike_destroy_chance;
                let destroyed = self.rng.chance(chance);
                if destroyed {
                    self.map.destroy_bridge(x, y)?;
                }
                events.push(Event::BridgeStrike {
                    unit: id,
                    a: x,
                    b: y,
                    chance,
                    destroyed,
                });
            }
        }
        Ok(events)
    }

    pub(super) fn air_strike(
        &mut self,
        id: UnitId,
        target: UnitId,
    ) -> Result<Vec<Event>, RuleError> {
        let class = self.get_unit(target)?.class.clone();
        if self.data.units.get(&class)?.target_class == TargetClass::Air {
            return Err(refuse(id, AirError::NotEnemy));
        }
        self.sortie(id, Aim::Unit(target))
    }

    pub(super) fn airfield_strike(
        &mut self,
        id: UnitId,
        hex: Hex,
    ) -> Result<Vec<Event>, RuleError> {
        self.sortie(id, Aim::Airfield(hex))
    }

    pub(super) fn air_bridge_strike(
        &mut self,
        id: UnitId,
        a: Hex,
        b: Hex,
    ) -> Result<Vec<Event>, RuleError> {
        self.sortie(id, Aim::Bridge(a, b))
    }

    /// R-AIR-01: rebasing takes the whole phase.
    pub(super) fn rebase(&mut self, id: UnitId, base: AirBase) -> Result<Vec<Event>, RuleError> {
        let a = self
            .air
            .get(&id)
            .ok_or(RuleError::UnknownUnit(id.0))?
            .clone();
        if a.side != self.active_side() {
            return Err(RuleError::NotYourUnit(id.0));
        }
        if a.sorties > 0 || a.rebased {
            return Err(refuse(id, AirError::AlreadyFlew));
        }
        let ok = match &base {
            AirBase::Hex { hex } => self.is_air_base(a.side, *hex),
            AirBase::OffMap { id: b } => self
                .offmap_bases
                .iter()
                .any(|x| &x.id == b && x.side == a.side),
        };
        if !ok || base == a.base {
            return Err(refuse(id, AirError::NotABase));
        }
        if let Some(u) = self.air.get_mut(&id) {
            u.base = base.clone();
            u.rebased = true;
        }
        Ok(vec![Event::Rebased { unit: id, base }])
    }

    /// R-AIR-01 at the start of a phase: the active side's aircraft rest or not,
    /// flags reset; aircraft whose base fell move to the nearest own base or are lost.
    pub(super) fn refresh_air(&mut self, events: &mut Vec<Event>) -> Result<(), RuleError> {
        let side = self.active_side();
        let rest = self.data.rules.air.rest_readiness_gain_pct;
        for a in self.air.values_mut() {
            a.intercepted = false;
            if a.side == side {
                if !a.flew {
                    a.fatigue_pct = a.fatigue_pct.saturating_sub(rest);
                }
                a.flew = false;
                a.sorties = 0;
                a.rebased = false;
            }
        }
        let lost: Vec<(UnitId, Hex)> = self
            .air
            .values()
            .filter(|a| a.side == side)
            .filter_map(|a| match a.base {
                AirBase::Hex { hex } if !self.is_air_base(side, hex) => Some((a.id, hex)),
                _ => None,
            })
            .collect();
        for (id, from) in lost {
            let mut bases: Vec<Hex> = self
                .control
                .iter()
                .filter(|(h, s)| **s == side && self.is_air_base(side, **h))
                .map(|(h, _)| *h)
                .collect();
            bases.sort_by_key(|h| (h.distance(from), *h));
            let next = bases.first().map(|h| AirBase::Hex { hex: *h }).or_else(|| {
                self.offmap_bases
                    .iter()
                    .find(|b| b.side == side)
                    .map(|b| AirBase::OffMap { id: b.id.clone() })
            });
            match next {
                Some(base) => {
                    let loss = self.data.rules.air.base_lost_readiness_loss_pct;
                    if let Some(a) = self.air.get_mut(&id) {
                        a.base = base.clone();
                        a.fatigue_pct = a.fatigue_pct.saturating_add(loss).min(100);
                    }
                    events.push(Event::Rebased { unit: id, base });
                }
                None => {
                    self.air.remove(&id);
                    events.push(Event::AircraftDestroyed { unit: id });
                }
            }
        }
        Ok(())
    }
}
