//! The tactical battle: state, commands and events (T-TURN-01/02,
//! T-LOS-02/03, T-COMBAT-01/02/03, T-SUP-01, T-RES-01/02/04). Deterministic
//! by seed: the command log replays the battle.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ceasefire_core::combat::{sample_hits, FireParams};
use ceasefire_core::hex::Hex;
use ceasefire_core::prob::hits_distribution;
use ceasefire_core::rng::Pcg32;
use ceasefire_core::tactical::{Role, TacticalOutcome};
use serde::{Deserialize, Serialize};

use crate::map::TacMap;
use crate::rules::TacRules;
use crate::TacError;

pub fn other(r: Role) -> Role {
    match r {
        Role::Attacker => Role::Defender,
        Role::Defender => Role::Attacker,
    }
}

fn ix(r: Role) -> usize {
    match r {
        Role::Attacker => 0,
        Role::Defender => 1,
    }
}

/// T-TURN-01 / open question 1: alternating activations (default) or
/// IGOUGO with overwatch (flag `igougo_overwatch`, for comparison).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnMode {
    Alternating,
    IgougoOverwatch,
}

/// A battalion or company on the tactical map (T-UNIT-01/02).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacUnit {
    pub id: u32,
    pub role: Role,
    pub kind: String,
    /// Attack against soft / hard targets, ground and air defense (strategic
    /// class with technology, T-UNIT-02).
    pub attack_soft: f64,
    pub attack_hard: f64,
    pub defense: f64,
    pub air_defense: f64,
    pub armor: bool,
    pub steps: u8,
    /// Damage carried: 2 damage = 1 step (T-COMBAT-01).
    pub damage: u8,
    pub readiness_pct: u8,
    pub exp_stars: u8,
    pub entrench: u8,
    pub hex: Hex,
    /// Off the map until this turn (reserves, T-SUP-01).
    pub arrives: Option<u32>,
    /// Visible to the enemy until this activation count (fired, T-LOS-02).
    pub revealed_until: u32,
    pub suppressed: bool,
    /// Hit this turn (suppression lifts at turn start otherwise).
    pub hit_this_turn: bool,
    pub moved_this_turn: bool,
    pub overwatch: bool,
    pub activated: bool,
}

impl TacUnit {
    pub fn on_map(&self, turn: u32) -> bool {
        self.steps > 0 && self.arrives.is_none_or(|t| t <= turn)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Support {
    pub artillery: u8,
    pub mortar: u8,
    pub air: u8,
    /// T-SUP-02: a rating ≥ 8 HQ: one call of any kind.
    pub any: u8,
    /// The enemy's SAM umbrella covers the battle (air strikes risk abort).
    pub enemy_sam: bool,
    /// Attack of the artillery and of the aircraft (with technology).
    pub artillery_soft: f64,
    pub artillery_hard: f64,
    pub air_soft: f64,
    pub air_hard: f64,
    pub mortar_soft: f64,
    pub mortar_hard: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SupportCall {
    Artillery { hex: Hex },
    Mortar { hex: Hex },
    Smoke { hex: Hex },
    Illumination { hex: Hex },
    Air { target: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Move {
        path: Vec<Hex>,
    },
    Fire {
        target: u32,
    },
    MoveFire {
        path: Vec<Hex>,
        target: u32,
    },
    Assault {
        target: u32,
    },
    Overwatch,
    Entrench,
    /// Engineer: a bridge over the canal side toward `toward`, ready next turn.
    Bridge {
        toward: Hex,
    },
    Wait,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TacCommand {
    Act {
        unit: u32,
        support: Option<SupportCall>,
        action: Action,
    },
    /// Defer the side's activation (once per turn, T-TURN-01).
    Pass,
    /// T-RES-04: surrender or abort — the result by the current score.
    Resign,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TacEvent {
    TurnStarted {
        turn: u32,
    },
    ReserveArrived {
        unit: u32,
        hex: Hex,
    },
    Moved {
        unit: u32,
        path: Vec<Hex>,
    },
    MineHit {
        unit: u32,
        steps: u8,
    },
    Fired {
        shooter: u32,
        target: u32,
        params: FireParams,
        hits: u32,
        ambush: bool,
        flank: bool,
    },
    Support {
        role: Role,
        call: SupportCall,
        aborted: bool,
    },
    StepsLost {
        unit: u32,
        steps: u8,
    },
    Suppressed {
        unit: u32,
    },
    Destroyed {
        unit: u32,
    },
    Withdrew {
        unit: u32,
        to: Hex,
    },
    KeyPoint {
        hex: Hex,
        role: Role,
    },
    BridgeBuilt {
        a: Hex,
        b: Hex,
    },
    Passed {
        role: Role,
    },
    BattleOver {
        result: TacResult,
    },
}

/// T-RES-01/02: the score and the outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacResult {
    pub attacker_points: i32,
    pub defender_points: i32,
    pub margin: i32,
    pub key_points_taken: usize,
    pub outcome: TacticalOutcome,
    pub percentile: f64,
    pub turns: u32,
}

/// A fire's expected damage and its spread (forecast, T-COMBAT-01).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FireForecast {
    pub params: FireParams,
    pub expected: f64,
    pub low: u32,
    pub high: u32,
    pub p_suppress: f64,
    pub ambush: bool,
}

#[derive(Debug, Clone)]
pub struct Battle {
    pub rules: Arc<TacRules>,
    pub map: TacMap,
    pub units: BTreeMap<u32, TacUnit>,
    pub turn: u32,
    pub max_turns: u32,
    pub night: bool,
    pub mode: TurnMode,
    pub active: Role,
    pub passed: [bool; 2],
    pub activations: u32,
    /// Hex → last turn it is smoked (T-LOS-01).
    pub smoke: BTreeMap<Hex, u32>,
    /// Hex → turn it is lit (night, T-SUP-01).
    pub lit: BTreeMap<Hex, u32>,
    /// Target → hex of the first unit that fired at it this turn (flank).
    pub fired_from: BTreeMap<u32, Hex>,
    pub control: BTreeMap<Hex, Role>,
    /// Steps lost by each role (index 0 attacker).
    pub steps_lost: [u32; 2],
    pub support: [Support; 2],
    pub pending_bridges: Vec<(Hex, Hex)>,
    pub rng: Pcg32,
    pub result: Option<TacResult>,
    pub log: Vec<TacCommand>,
}

fn illegal(why: &str) -> TacError {
    TacError::Illegal(why.into())
}

impl Battle {
    pub fn unit(&self, id: u32) -> Option<&TacUnit> {
        self.units.get(&id)
    }

    pub fn unit_at(&self, h: Hex) -> Option<&TacUnit> {
        self.units
            .values()
            .find(|u| u.hex == h && u.on_map(self.turn))
    }

    pub fn alive(&self, role: Role) -> impl Iterator<Item = &TacUnit> + '_ {
        self.units
            .values()
            .filter(move |u| u.role == role && u.on_map(self.turn))
    }

    fn smoke_set(&self) -> BTreeSet<Hex> {
        self.smoke
            .iter()
            .filter(|(_, t)| **t >= self.turn)
            .map(|(h, _)| *h)
            .collect()
    }

    /// Sight range of an observer (T-LOS-01/02): day 4, night 1; recon +1;
    /// from a hill +1.
    fn sight(&self, u: &TacUnit) -> u32 {
        let r = &self.rules;
        let mut s = if self.night { r.los.night } else { r.los.day };
        if r.units.get(&u.kind).is_some_and(|k| k.has("recon")) {
            s += r.los.recon_bonus;
        }
        if self.map.level(u.hex) > 0 {
            s += r.los.hill_bonus;
        }
        s
    }

    /// T-LOS-02: does `obs` see `t`? Range, line of sight, concealment in
    /// cover (infantry, AT, commandos: within 2, recon 3), wadi (adjacent only).
    pub fn sees_unit(&self, obs: &TacUnit, t: &TacUnit) -> bool {
        let d = obs.hex.distance(t.hex);
        let r = &self.rules;
        let lit = self.night && self.lit.get(&t.hex).is_some_and(|x| *x == self.turn);
        if d > self.sight(obs) && !lit {
            return false;
        }
        if d > 1 {
            let cell = self.map.cell(t.hex);
            let terrain = cell.and_then(|c| r.terrain.get(&c.terrain));
            if terrain.is_some_and(|t| t.hidden_but_adjacent) {
                return false;
            }
            let concealed = r.units.get(&t.kind).is_some_and(|k| k.has("concealed"))
                && (terrain.is_some_and(|t| t.cover) || t.entrench >= 2);
            let recon = r.units.get(&obs.kind).is_some_and(|k| k.has("recon"));
            let limit = if recon {
                r.los.concealed_recon
            } else {
                r.los.concealed_day
            };
            if concealed && d > limit {
                return false;
            }
        }
        self.map.sees(r, obs.hex, t.hex, &self.smoke_set())
    }

    /// Is `t` visible to the side `role` (any of its units sees it, or `t`
    /// revealed itself by firing, or it stands in a lit hex)?
    pub fn visible_to(&self, role: Role, t: &TacUnit) -> bool {
        if t.revealed_until > self.activations {
            return true;
        }
        if self.night && self.lit.get(&t.hex).is_some_and(|x| *x == self.turn) {
            return true;
        }
        self.alive(role).any(|o| self.sees_unit(o, t))
    }

    /// T-COMBAT-01: n, A, D, p of a fire.
    pub fn fire_params(
        &self,
        shooter: &TacUnit,
        target: &TacUnit,
        move_fire: bool,
        assault: bool,
    ) -> Option<(FireParams, bool, bool)> {
        let r = &self.rules;
        let c = &r.combat;
        let kind = r.units.get(&shooter.kind)?;
        let d = shooter.hex.distance(target.hex);
        let range_ok = if assault {
            d == 1
        } else {
            d >= 1 && d <= kind.range
        };
        if !range_ok {
            return None;
        }
        let attack = if target.armor {
            shooter.attack_hard
        } else {
            shooter.attack_soft
        };
        if attack <= 0.0 || (kind.has("armor_only") && !target.armor) {
            return None;
        }
        let range_mult = if kind.has("armor_only") && (2..=3).contains(&d) {
            c.at_range_mult
        } else {
            c.range_mult[(d as usize).min(c.range_mult.len() - 1)]
        };
        let ambush = !assault && !self.visible_to(target.role, shooter);
        let flank = self
            .fired_from
            .get(&target.id)
            .is_some_and(|h0| *h0 != shooter.hex && !h0.is_adjacent(shooter.hex));
        let mut a = attack * (1.0 + c.exp_bonus * f64::from(shooter.exp_stars)) * range_mult;
        if move_fire {
            a *= c.move_fire_mult;
        }
        if ambush {
            a *= c.ambush_mult;
        }
        if self.night && !kind.has("night") {
            a *= c.night_mult;
        }
        if flank {
            a *= c.flank_mult;
        }
        if shooter.suppressed {
            a *= c.suppressed_fire_mult;
        }
        if assault {
            let cover = self
                .map
                .cell(target.hex)
                .and_then(|cell| r.terrain.get(&cell.terrain))
                .is_some_and(|t| t.cover);
            let support = self.alive(shooter.role).any(|o| {
                o.id != shooter.id
                    && o.hex.is_adjacent(target.hex)
                    && r.units
                        .get(&o.kind)
                        .is_some_and(|k| k.has("infantry") || k.kind == "mech_bn")
            });
            if support {
                a *= c.combined_arms_mult;
            } else if kind.has("armor") && cover {
                a *= c.tank_assault_cover_mult;
            }
        }
        let terrain = self
            .map
            .cell(target.hex)
            .and_then(|cell| r.terrain.get(&cell.terrain))
            .map_or(1.0, |t| t.defense);
        let dd = target.defense
            * terrain
            * (1.0 + c.entrench_bonus * f64::from(target.entrench))
            * (1.0 + c.exp_bonus * f64::from(target.exp_stars));
        Some((
            self.params(shooter.steps, shooter.readiness_pct, a, dd),
            ambush,
            flank,
        ))
    }

    /// n = round(2 × steps × readiness), p = clamp(0.30 + 0.06 (A − D)).
    pub fn params(&self, steps: u8, readiness_pct: u8, a: f64, d: f64) -> FireParams {
        let c = &self.rules.combat;
        let n = (c.shots_per_step * f64::from(steps) * f64::from(readiness_pct) / 100.0 + 0.5)
            .floor() as u32;
        let p = (c.p_base + c.p_slope * (a - d)).clamp(c.p_min, c.p_max);
        FireParams { n, a, d, p }
    }

    /// Forecast of a fire (expected damage, 10–90% band, P(suppression)).
    pub fn forecast(
        &self,
        shooter: u32,
        target: u32,
        move_fire: bool,
        assault: bool,
    ) -> Option<FireForecast> {
        let s = self.unit(shooter)?;
        let t = self.unit(target)?;
        let (params, ambush, _) = self.fire_params(s, t, move_fire, assault)?;
        Some(forecast_of(&self.rules, params, ambush))
    }

    // ------------------------------------------------------------ commands

    pub fn apply(&mut self, cmd: TacCommand) -> Result<Vec<TacEvent>, TacError> {
        if self.result.is_some() {
            return Err(illegal("the battle is over"));
        }
        let mut events = Vec::new();
        match &cmd {
            TacCommand::Resign => {
                self.finish(&mut events);
            }
            TacCommand::Pass => {
                let i = ix(self.active);
                if self.mode == TurnMode::IgougoOverwatch {
                    // IGOUGO: «pass» ends the side's half-turn.
                    for u in self.units.values_mut().filter(|u| u.role == self.active) {
                        u.activated = true;
                    }
                } else {
                    if self.passed[i] || !self.has_ready(other(self.active)) {
                        return Err(illegal("no pass left"));
                    }
                    self.passed[i] = true;
                }
                events.push(TacEvent::Passed { role: self.active });
                self.next_side(&mut events);
            }
            TacCommand::Act {
                unit,
                support,
                action,
            } => {
                let u = self
                    .unit(*unit)
                    .ok_or_else(|| illegal("no such unit"))?
                    .clone();
                if u.role != self.active || !u.on_map(self.turn) || u.activated {
                    return Err(illegal("not a ready unit of the active side"));
                }
                self.check_action(&u, action)?;
                if let Some(call) = support {
                    self.support_call(u.role, call, &mut events)?;
                }
                let was_suppressed = self.units[unit].suppressed;
                if self.units.get(unit).is_some_and(|x| x.steps > 0) {
                    self.act(*unit, action, &mut events)?;
                }
                if let Some(x) = self.units.get_mut(unit) {
                    x.activated = true;
                    // T-COMBAT-02: suppression lasts to the end of the next activation.
                    if was_suppressed {
                        x.suppressed = false;
                    }
                }
                self.activations += 1;
                self.next_side(&mut events);
            }
        }
        self.log.push(cmd);
        Ok(events)
    }

    fn has_ready(&self, role: Role) -> bool {
        self.alive(role).any(|u| !u.activated)
    }

    /// T-TURN-01: the other side activates next if it has units left; the
    /// side with more units plays the rest in a row; then the turn ends.
    fn next_side(&mut self, events: &mut Vec<TacEvent>) {
        if self.result.is_some() {
            return;
        }
        if self.alive(Role::Attacker).next().is_none()
            || self.alive(Role::Defender).next().is_none()
        {
            self.finish(events);
            return;
        }
        let me = self.active;
        let next = match self.mode {
            TurnMode::Alternating => {
                if self.has_ready(other(me)) {
                    other(me)
                } else {
                    me
                }
            }
            TurnMode::IgougoOverwatch => {
                if self.has_ready(me) {
                    me
                } else {
                    other(me)
                }
            }
        };
        if self.has_ready(next) {
            self.active = next;
        } else {
            self.end_turn(events);
        }
    }

    fn end_turn(&mut self, events: &mut Vec<TacEvent>) {
        let r = Arc::clone(&self.rules);
        // Key points: a unit standing on one holds it.
        let points = self.map.key_points.clone();
        for h in points {
            if let Some(u) = self.unit_at(h) {
                let role = u.role;
                if self.control.get(&h) != Some(&role) {
                    self.control.insert(h, role);
                    events.push(TacEvent::KeyPoint { hex: h, role });
                }
            }
        }
        // Units that did not move all turn dig in by 1.
        let turn = self.turn;
        for u in self.units.values_mut().filter(|u| u.on_map(turn)) {
            if !u.moved_this_turn {
                u.entrench = (u.entrench + r.entrench.per_turn_still).min(r.entrench.max);
            }
        }
        if self.turn >= self.max_turns {
            self.finish(events);
            return;
        }
        self.turn += 1;
        self.start_turn(events);
    }

    pub fn start_turn(&mut self, events: &mut Vec<TacEvent>) {
        events.push(TacEvent::TurnStarted { turn: self.turn });
        let turn = self.turn;
        // Bridges started last turn are ready.
        for (a, b) in std::mem::take(&mut self.pending_bridges) {
            self.map.add_bridge(a, b);
            events.push(TacEvent::BridgeBuilt { a, b });
        }
        for u in self.units.values_mut() {
            u.activated = false;
            u.moved_this_turn = false;
            u.overwatch = false;
            if !u.hit_this_turn {
                u.suppressed = false;
            }
            u.hit_this_turn = false;
        }
        // Reserves arrive at their edge (a free hex nearest their spot).
        let arriving: Vec<u32> = self
            .units
            .values()
            .filter(|u| u.arrives == Some(turn) && u.steps > 0)
            .map(|u| u.id)
            .collect();
        for id in arriving {
            let spot = self.units[&id].hex;
            let free = if self.unit_at(spot).is_none() {
                Some(spot)
            } else {
                let mut cand: Vec<Hex> = self
                    .map
                    .cells
                    .keys()
                    .filter(|h| self.unit_at(**h).is_none())
                    .copied()
                    .collect();
                cand.sort_by_key(|h| (h.distance(spot), *h));
                cand.first().copied()
            };
            if let Some(h) = free {
                if let Some(u) = self.units.get_mut(&id) {
                    u.hex = h;
                }
                events.push(TacEvent::ReserveArrived { unit: id, hex: h });
            }
        }
        self.fired_from.clear();
        self.passed = [false; 2];
        self.active = Role::Attacker;
        if !self.has_ready(Role::Attacker) {
            self.active = Role::Defender;
        }
    }

    /// T-RES-01/02: score and outcome.
    pub fn score(&self) -> TacResult {
        let r = &self.rules.result;
        let held = |role: Role| {
            self.map
                .key_points
                .iter()
                .filter(|h| self.control.get(h) == Some(&role))
                .count()
        };
        let taken = held(Role::Attacker);
        let att = r.key_point * taken as i32 + r.step * self.steps_lost[1] as i32;
        let def = r.key_point * held(Role::Defender) as i32 + r.step * self.steps_lost[0] as i32;
        let m = att - def;
        let t = &r.thresholds;
        let outcome = if m >= t.decisive && taken > 0 {
            TacticalOutcome::AttackerDecisive
        } else if m >= t.victory && taken > 0 {
            TacticalOutcome::AttackerVictory
        } else if m <= t.rout {
            TacticalOutcome::AttackerRout
        } else if m <= t.defeat {
            TacticalOutcome::AttackerDefeat
        } else {
            TacticalOutcome::Draw
        };
        TacResult {
            attacker_points: att,
            defender_points: def,
            margin: m,
            key_points_taken: taken,
            outcome,
            percentile: percentile_of(&self.rules, outcome),
            turns: self.turn,
        }
    }

    fn finish(&mut self, events: &mut Vec<TacEvent>) {
        let result = self.score();
        events.push(TacEvent::BattleOver {
            result: result.clone(),
        });
        self.result = Some(result);
    }

    // ------------------------------------------------------------ actions

    fn check_action(&self, u: &TacUnit, a: &Action) -> Result<(), TacError> {
        let kind = self.rules.kind(&u.kind)?;
        match a {
            Action::Assault { .. } | Action::Overwatch if u.suppressed => {
                Err(illegal("suppressed"))
            }
            Action::MoveFire { .. } if !kind.has("move_fire") => {
                Err(illegal("cannot move and fire"))
            }
            Action::Bridge { .. } if !kind.has("engineer") => Err(illegal("not an engineer")),
            _ => Ok(()),
        }
    }

    fn act(
        &mut self,
        id: u32,
        action: &Action,
        events: &mut Vec<TacEvent>,
    ) -> Result<(), TacError> {
        match action {
            Action::Wait => Ok(()),
            Action::Overwatch => {
                if let Some(u) = self.units.get_mut(&id) {
                    u.overwatch = true;
                }
                Ok(())
            }
            Action::Entrench => {
                let r = Arc::clone(&self.rules);
                let fast = r.kind(&self.units[&id].kind)?.has("dig_fast");
                if let Some(u) = self.units.get_mut(&id) {
                    let dig = if fast {
                        r.entrench.dig_infantry
                    } else {
                        r.entrench.dig
                    };
                    u.entrench = (u.entrench + dig).min(r.entrench.max);
                }
                Ok(())
            }
            Action::Move { path } => {
                let mp = self.rules.kind(&self.units[&id].kind)?.move_points;
                self.do_move(id, path, mp, events)
            }
            Action::MoveFire { path, target } => {
                let mp = self.rules.kind(&self.units[&id].kind)?.move_points / 2;
                self.do_move(id, path, mp, events)?;
                if self
                    .units
                    .get(&id)
                    .is_some_and(|u| u.steps > 0 && !u.suppressed)
                {
                    self.fire(id, *target, true, events)?;
                }
                Ok(())
            }
            Action::Fire { target } => self.fire(id, *target, false, events),
            Action::Assault { target } => self.assault(id, *target, events),
            Action::Bridge { toward } => {
                let h = self.units[&id].hex;
                if !h.is_adjacent(*toward) || !self.map.has_canal(h, *toward) {
                    return Err(illegal("no canal there"));
                }
                self.pending_bridges.push((h, *toward));
                Ok(())
            }
        }
    }

    /// Movement: terrain costs, a canal only by bridge (infantry with all its
    /// move), mines, a stop next to a visible enemy, overwatch after each step;
    /// a suppressed unit only moves away from the enemy.
    fn do_move(
        &mut self,
        id: u32,
        path: &[Hex],
        mp: u32,
        events: &mut Vec<TacEvent>,
    ) -> Result<(), TacError> {
        let r = Arc::clone(&self.rules);
        let u = self.units[&id].clone();
        let kind = r.kind(&u.kind)?;
        let mut left = mp;
        let mut at = u.hex;
        let mut done = Vec::new();
        let enemy_near = |b: &Battle, h: Hex| {
            b.alive(other(u.role))
                .filter(|e| b.visible_to(u.role, e))
                .map(|e| e.hex.distance(h))
                .min()
        };
        for (i, &next) in path.iter().enumerate() {
            if !at.is_adjacent(next) {
                return Err(illegal("path not adjacent"));
            }
            let Some(cell) = self.map.cell(next).cloned() else {
                return Err(illegal("off the map"));
            };
            if self.unit_at(next).is_some() {
                return Err(illegal("occupied"));
            }
            if u.suppressed {
                if let (Some(a), Some(b)) = (enemy_near(self, at), enemy_near(self, next)) {
                    if b < a {
                        return Err(illegal("suppressed: only away from the enemy"));
                    }
                }
            }
            let mut cost = r.terrain(&cell.terrain)?.move_cost;
            if self.map.has_canal(at, next) && !self.map.has_bridge(at, next) {
                if !kind.has("infantry") || i > 0 || left < mp {
                    return Err(illegal("canal: bridge needed (infantry: whole move)"));
                }
                cost = left;
            }
            if cell.mine && cell.mine_known && !kind.has("engineer") {
                cost = left.max(1);
            }
            if cost > left {
                return Err(illegal("not enough move"));
            }
            left -= cost;
            at = next;
            done.push(next);
            if let Some(x) = self.units.get_mut(&id) {
                x.hex = next;
                x.moved_this_turn = true;
                x.entrench = 0;
            }
            // Mines (engineers clear them).
            if cell.mine {
                if kind.has("engineer") {
                    if let Some(c) = self.map.cells.get_mut(&next) {
                        c.mine = false;
                    }
                } else {
                    if let Some(c) = self.map.cells.get_mut(&next) {
                        c.mine_known = true;
                    }
                    if self.rng.chance(r.mines.loss_chance) {
                        self.lose_steps(id, r.mines.loss_steps, events);
                        events.push(TacEvent::MineHit {
                            unit: id,
                            steps: r.mines.loss_steps,
                        });
                    }
                    left = 0;
                }
            }
            self.capture(id, next, events);
            // Overwatch: the first enemy watcher that sees the mover fires once.
            self.overwatch_fire(id, events)?;
            let stopped = self
                .units
                .get(&id)
                .is_none_or(|x| x.steps == 0 || x.suppressed);
            let contact = enemy_near(self, next).is_some_and(|d| d == 1);
            if stopped || contact || left == 0 {
                if i + 1 < path.len() && !stopped && !contact && left == 0 {
                    return Err(illegal("not enough move"));
                }
                break;
            }
        }
        events.push(TacEvent::Moved {
            unit: id,
            path: done,
        });
        Ok(())
    }

    /// Key point taken by entering it; others than motorised infantry need
    /// no enemy next to it (night interpretation of «быстрее»).
    fn capture(&mut self, id: u32, h: Hex, events: &mut Vec<TacEvent>) {
        if !self.map.key_points.contains(&h) {
            return;
        }
        let u = self.units[&id].clone();
        let fast = self
            .rules
            .units
            .get(&u.kind)
            .is_some_and(|k| k.has("fast_capture"));
        let contested = self.alive(other(u.role)).any(|e| e.hex.is_adjacent(h));
        if (fast || !contested) && self.control.get(&h) != Some(&u.role) {
            self.control.insert(h, u.role);
            events.push(TacEvent::KeyPoint {
                hex: h,
                role: u.role,
            });
        }
    }

    fn overwatch_fire(&mut self, mover: u32, events: &mut Vec<TacEvent>) -> Result<(), TacError> {
        let m = self.units[&mover].clone();
        let mut watchers: Vec<&TacUnit> = self
            .alive(other(m.role))
            .filter(|w| w.overwatch && !w.suppressed)
            .filter(|w| self.sees_unit(w, &m))
            .filter(|w| self.fire_params(w, &m, false, false).is_some())
            .collect();
        watchers.sort_by_key(|w| (w.hex.distance(m.hex), w.id));
        if let Some(w) = watchers.first().map(|w| w.id) {
            if let Some(x) = self.units.get_mut(&w) {
                x.overwatch = false;
            }
            self.fire(w, mover, false, events)?;
        }
        Ok(())
    }

    /// T-COMBAT-01/02, T-LOS-03: a fire; adjacent fire draws return fire
    /// (not from an ambush), ranged fire does not.
    fn fire(
        &mut self,
        shooter: u32,
        target: u32,
        move_fire: bool,
        events: &mut Vec<TacEvent>,
    ) -> Result<(), TacError> {
        let s = self
            .units
            .get(&shooter)
            .ok_or_else(|| illegal("no shooter"))?
            .clone();
        let t = self
            .units
            .get(&target)
            .ok_or_else(|| illegal("no target"))?
            .clone();
        if t.role == s.role || !t.on_map(self.turn) {
            return Err(illegal("not an enemy"));
        }
        if !self.visible_to(s.role, &t) {
            return Err(illegal("target not visible"));
        }
        let (params, ambush, flank) = self
            .fire_params(&s, &t, move_fire, false)
            .ok_or_else(|| illegal("cannot fire at it"))?;
        let back = if s.hex.distance(t.hex) == 1 && !ambush {
            self.fire_params(&t, &s, false, false).map(|x| x.0)
        } else {
            None
        };
        let hits = sample_hits(&mut self.rng, &params);
        let back_hits = back.map(|p| sample_hits(&mut self.rng, &p));
        events.push(TacEvent::Fired {
            shooter,
            target,
            params,
            hits,
            ambush,
            flank,
        });
        self.fired_from.entry(target).or_insert(s.hex);
        let reveal = self.activations + 2;
        if let Some(x) = self.units.get_mut(&shooter) {
            x.revealed_until = x.revealed_until.max(reveal);
        }
        self.damage(target, hits, events);
        if let (Some(p), Some(h)) = (back, back_hits) {
            events.push(TacEvent::Fired {
                shooter: target,
                target: shooter,
                params: p,
                hits: h,
                ambush: false,
                flank: false,
            });
            if let Some(x) = self.units.get_mut(&target) {
                x.revealed_until = x.revealed_until.max(reveal);
            }
            self.damage(shooter, h, events);
        }
        Ok(())
    }

    /// Hits → damage; 2 damage = 1 step; any hit suppresses.
    fn damage(&mut self, id: u32, hits: u32, events: &mut Vec<TacEvent>) {
        if hits == 0 {
            return;
        }
        let per = self.rules.combat.damage_per_step.max(1);
        let Some(u) = self.units.get_mut(&id) else {
            return;
        };
        let total = u32::from(u.damage) + hits;
        let steps = (total / per).min(u32::from(u.steps)) as u8;
        u.damage = (total % per) as u8;
        u.suppressed = true;
        u.hit_this_turn = true;
        events.push(TacEvent::Suppressed { unit: id });
        if steps > 0 {
            self.lose_steps(id, steps, events);
        }
    }

    fn lose_steps(&mut self, id: u32, steps: u8, events: &mut Vec<TacEvent>) {
        let Some(u) = self.units.get_mut(&id) else {
            return;
        };
        let lost = steps.min(u.steps);
        u.steps -= lost;
        let role = u.role;
        let dead = u.steps == 0;
        self.steps_lost[ix(role)] += u32::from(lost);
        events.push(TacEvent::StepsLost {
            unit: id,
            steps: lost,
        });
        if dead {
            events.push(TacEvent::Destroyed { unit: id });
        }
    }

    /// T-COMBAT-03: both fire at once; the defender keeps terrain and
    /// entrenchment. It withdraws (or is destroyed with no way out) if it lost
    /// more steps or has one left; the attacker takes the hex. Otherwise the
    /// attacker stays and is suppressed.
    fn assault(
        &mut self,
        id: u32,
        target: u32,
        events: &mut Vec<TacEvent>,
    ) -> Result<(), TacError> {
        let a = self.units[&id].clone();
        let d = self
            .units
            .get(&target)
            .ok_or_else(|| illegal("no target"))?
            .clone();
        if d.role == a.role || !a.hex.is_adjacent(d.hex) {
            return Err(illegal("assault: an adjacent enemy"));
        }
        let pa = self.fire_params(&a, &d, false, true).map(|x| x.0);
        let pd = self.fire_params(&d, &a, false, true).map(|x| x.0);
        if pa.is_none() {
            return Err(illegal("cannot assault it"));
        }
        let (sa, sd) = (a.steps, d.steps);
        let ha = pa.map_or(0, |p| sample_hits(&mut self.rng, &p));
        let hd = pd.map_or(0, |p| sample_hits(&mut self.rng, &p));
        if let Some(p) = pa {
            events.push(TacEvent::Fired {
                shooter: id,
                target,
                params: p,
                hits: ha,
                ambush: false,
                flank: false,
            });
        }
        if let Some(p) = pd {
            events.push(TacEvent::Fired {
                shooter: target,
                target: id,
                params: p,
                hits: hd,
                ambush: false,
                flank: false,
            });
        }
        self.damage(target, ha, events);
        self.damage(id, hd, events);
        let a_now = self.units.get(&id).map_or(0, |u| u.steps);
        let d_now = self.units.get(&target).map_or(0, |u| u.steps);
        let (lost_a, lost_d) = (sa - a_now, sd - d_now);
        if a_now == 0 {
            return Ok(());
        }
        if d_now == 0 || lost_d > lost_a || d_now == 1 {
            if d_now > 0 {
                // Withdraw one hex away from the attacker, or be destroyed.
                let from = d.hex;
                let mut away: Vec<Hex> = self
                    .map
                    .neighbors(from)
                    .into_iter()
                    .filter(|h| {
                        self.unit_at(*h).is_none() && h.distance(a.hex) > from.distance(a.hex)
                    })
                    .filter(|h| !self.map.has_canal(from, *h) || self.map.has_bridge(from, *h))
                    .collect();
                away.sort();
                match away.first() {
                    Some(&to) => {
                        if let Some(x) = self.units.get_mut(&target) {
                            x.hex = to;
                            x.entrench = 0;
                        }
                        events.push(TacEvent::Withdrew { unit: target, to });
                    }
                    None => self.lose_steps(target, d_now, events),
                }
            }
            if let Some(x) = self.units.get_mut(&id) {
                x.hex = d.hex;
                x.moved_this_turn = true;
                x.entrench = 0;
            }
            self.capture(id, d.hex, events);
        } else if let Some(x) = self.units.get_mut(&id) {
            x.suppressed = true;
            x.hit_this_turn = true;
            events.push(TacEvent::Suppressed { unit: id });
        }
        Ok(())
    }

    // ------------------------------------------------------------ support

    /// Calls left of a side for a kind (its own pool, then «any»).
    pub fn calls_left(&self, role: Role, call: &SupportCall) -> u8 {
        let s = &self.support[ix(role)];
        let own = match call {
            SupportCall::Artillery { .. }
            | SupportCall::Smoke { .. }
            | SupportCall::Illumination { .. } => s.artillery,
            SupportCall::Mortar { .. } => s.mortar,
            SupportCall::Air { .. } => s.air,
        };
        own + s.any
    }

    fn spend(&mut self, role: Role, call: &SupportCall) -> Result<(), TacError> {
        let s = &mut self.support[ix(role)];
        let pool = match call {
            SupportCall::Artillery { .. }
            | SupportCall::Smoke { .. }
            | SupportCall::Illumination { .. } => &mut s.artillery,
            SupportCall::Mortar { .. } => &mut s.mortar,
            SupportCall::Air { .. } => &mut s.air,
        };
        if *pool > 0 {
            *pool -= 1;
        } else if s.any > 0 {
            s.any -= 1;
        } else {
            return Err(illegal("no calls of this kind left"));
        }
        Ok(())
    }

    /// T-SUP-01: artillery (salvo on a hex and its neighbours, −1 entrenchment,
    /// half strength on an unseen hex), smoke, illumination (night), air
    /// strike (day, a visible target, may abort under the enemy's SAMs).
    fn support_call(
        &mut self,
        role: Role,
        call: &SupportCall,
        events: &mut Vec<TacEvent>,
    ) -> Result<(), TacError> {
        let r = Arc::clone(&self.rules);
        if matches!(call, SupportCall::Air { .. }) && self.night {
            return Err(illegal("no aircraft at night"));
        }
        if matches!(call, SupportCall::Illumination { .. }) && !self.night {
            return Err(illegal("illumination is for the night"));
        }
        if let SupportCall::Air { target } = call {
            let t = self.unit(*target).ok_or_else(|| illegal("no target"))?;
            if t.role == role || !self.visible_to(role, t) {
                return Err(illegal("air: a visible enemy"));
            }
        }
        self.spend(role, call)?;
        let sup = self.support[ix(role)];
        match call {
            SupportCall::Smoke { hex } => {
                let until = self.turn + r.support.smoke_turns.saturating_sub(1);
                let mut hexes = vec![*hex];
                if r.support.smoke_radius > 0 {
                    hexes.extend(
                        self.map
                            .neighbors(*hex)
                            .into_iter()
                            .filter(|h| h.distance(*hex) <= r.support.smoke_radius)
                            .take(1),
                    );
                }
                for h in hexes {
                    self.smoke.insert(h, until);
                }
                events.push(TacEvent::Support {
                    role,
                    call: call.clone(),
                    aborted: false,
                });
            }
            SupportCall::Illumination { hex } => {
                let radius = r.los.illumination_radius;
                let lit: Vec<Hex> = self
                    .map
                    .cells
                    .keys()
                    .filter(|h| h.distance(*hex) <= radius)
                    .copied()
                    .collect();
                for h in lit {
                    self.lit.insert(h, self.turn);
                }
                events.push(TacEvent::Support {
                    role,
                    call: call.clone(),
                    aborted: false,
                });
            }
            SupportCall::Artillery { hex } | SupportCall::Mortar { hex } => {
                let (center, neigh, soft, hard) = match call {
                    SupportCall::Mortar { .. } => (
                        r.support.mortar.shots_center,
                        r.support.mortar.shots_neighbour,
                        sup.mortar_soft,
                        sup.mortar_hard,
                    ),
                    _ => (
                        r.support.artillery.shots_center,
                        r.support.artillery.shots_neighbour,
                        sup.artillery_soft,
                        sup.artillery_hard,
                    ),
                };
                events.push(TacEvent::Support {
                    role,
                    call: call.clone(),
                    aborted: false,
                });
                let seen = self.alive(role).any(|o| {
                    self.map.sees(&r, o.hex, *hex, &self.smoke_set())
                        && o.hex.distance(*hex) <= self.sight(o)
                });
                let mult = if seen {
                    1.0
                } else {
                    r.support.artillery.unseen_mult
                };
                let mut targets: Vec<(u32, u32)> = Vec::new();
                for u in self.alive(other(role)) {
                    let shots = if u.hex == *hex {
                        center
                    } else if u.hex.is_adjacent(*hex) {
                        neigh
                    } else {
                        continue;
                    };
                    targets.push((u.id, (f64::from(shots) * mult).round() as u32));
                }
                for (id, shots) in targets {
                    let t = self.units[&id].clone();
                    let a = if t.armor { hard } else { soft };
                    let terrain = self
                        .map
                        .cell(t.hex)
                        .and_then(|c| r.terrain.get(&c.terrain))
                        .map_or(1.0, |x| x.defense);
                    let d = t.defense
                        * terrain
                        * (1.0 + r.combat.entrench_bonus * f64::from(t.entrench));
                    let mut p = self.params(1, 100, a, d);
                    p.n = shots;
                    let hits = sample_hits(&mut self.rng, &p);
                    events.push(TacEvent::Fired {
                        shooter: 0,
                        target: id,
                        params: p,
                        hits,
                        ambush: false,
                        flank: false,
                    });
                    if t.hex == *hex {
                        if let Some(x) = self.units.get_mut(&id) {
                            x.entrench =
                                x.entrench.saturating_sub(r.support.artillery.entrench_loss);
                        }
                    }
                    self.damage(id, hits, events);
                }
            }
            SupportCall::Air { target } => {
                let aborted = sup.enemy_sam && self.rng.chance(r.support.air.sam_abort_chance);
                events.push(TacEvent::Support {
                    role,
                    call: call.clone(),
                    aborted,
                });
                if !aborted {
                    let t = self.units[target].clone();
                    let a = if t.armor { sup.air_hard } else { sup.air_soft };
                    let d = t.air_defense * (1.0 + r.combat.entrench_bonus * f64::from(t.entrench));
                    let mut p = self.params(1, 100, a, d);
                    p.n = r.support.air.shots;
                    let hits = sample_hits(&mut self.rng, &p);
                    events.push(TacEvent::Fired {
                        shooter: 0,
                        target: *target,
                        params: p,
                        hits,
                        ambush: false,
                        flank: false,
                    });
                    self.damage(*target, hits, events);
                }
            }
        }
        Ok(())
    }
}

/// Expected damage, its 10–90% band and P(at least one hit).
pub fn forecast_of(rules: &TacRules, params: FireParams, ambush: bool) -> FireForecast {
    let dist = hits_distribution(&[(params.n, params.p)], params.n);
    let expected = dist.iter().enumerate().map(|(k, w)| k as f64 * w).sum();
    let q = |x: f64| {
        let mut acc = 0.0;
        for (k, w) in dist.iter().enumerate() {
            acc += w;
            if acc + 1e-12 >= x {
                return k as u32;
            }
        }
        params.n
    };
    FireForecast {
        params,
        expected,
        low: q(rules.combat.forecast_low),
        high: q(rules.combat.forecast_high),
        p_suppress: 1.0 - dist.first().copied().unwrap_or(1.0),
        ambush,
    }
}

/// T-RES-02: the outcome's percentile for the campaign (R-TAC-02).
pub fn percentile_of(rules: &TacRules, o: TacticalOutcome) -> f64 {
    let key = match o {
        TacticalOutcome::AttackerDecisive => "attacker_decisive",
        TacticalOutcome::AttackerVictory => "attacker_victory",
        TacticalOutcome::Draw => "draw",
        TacticalOutcome::AttackerDefeat => "attacker_defeat",
        TacticalOutcome::AttackerRout => "attacker_rout",
    };
    rules.result.percentiles.get(key).copied().unwrap_or(50.0)
}
