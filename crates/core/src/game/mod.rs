//! R-TURN-01: game state. It changes only through [`GameState::apply`].

mod attack;
mod command;
mod economy;
mod hq;
mod moves;
mod phase;
mod transfer;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use command::{Command, Event, SetupWarning};

use crate::data::{ap_to_halves, GameData, ReadinessRules, UnitClass};
use crate::error::RuleError;
use crate::hex::Hex;
use crate::map::HexMap;
use crate::movement::{Occupancy, Occupant};
use crate::rng::Pcg32;
use crate::side::SideId;
use crate::supply::{
    compute_supply, HqPosition, SupplyInput, SupplyMap, SupplyParams, SupplySource,
};
use crate::tech::TechLevels;
use crate::unit::{fatigue_pct, readiness_pct};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(pub u32);

/// A ground unit on the map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub class: String,
    pub side: SideId,
    pub hex: Hex,
    pub strength: u8,
    pub morale_pct: u8,
    /// Experience in hundredths of a star; formulas use whole stars.
    pub exp: u16,
    pub entrenchment: u8,
    /// Unit supply with inertia (see docs/decisions.md).
    pub supply: u8,
    pub zero_supply_phases: u8,
    /// R-UNIT-01 value before fatigue, set at the start of the own phase.
    pub base_readiness_pct: u8,
    /// Fatigue counters, reset at the start of the own phase.
    pub attacks_made: u32,
    pub attacks_received: u32,
    pub ap_halves: u32,
    pub strikes_left: u32,
    pub can_move: bool,
    pub started_in_zoc: bool,
    /// Since the start of the own phase.
    pub moved: bool,
    pub fought: bool,
    /// Moved, attacked or fired this phase (no reinforcement, R-TURN-01).
    pub acted: bool,
    /// Steps added this phase (R-SUP-03 limit per phase).
    pub reinforced_steps: u8,
    pub hq_rating: Option<u8>,
    /// R-HQ-01: formation tag ("brigade of division N").
    pub formation: Option<String>,
    /// R-HQ-01: units attach only to HQs of the same nation.
    pub nation: Option<String>,
    /// R-HQ-01: optional manual pin.
    pub pinned_hq: Option<UnitId>,
    /// R-HQ-01: set at the start of each own phase.
    pub attached_hq: Option<UnitId>,
    /// R-HQ-02: HQ destroyed; stays unattached for the next own phase.
    pub hq_lost: bool,
    /// R-COMBAT-06: units that attacked this one in the current phase.
    pub attacked_by: Vec<UnitId>,
    /// R-MOVE-02: boat crossing penalty, until the next own phase.
    pub crossing_penalty_pct: u8,
}

impl Unit {
    pub fn exp_stars(&self) -> u8 {
        u8::try_from(self.exp / 100).unwrap_or(u8::MAX)
    }

    /// R-UNIT-01: readiness including fatigue.
    pub fn readiness_pct(&self, rules: &ReadinessRules) -> u8 {
        let fatigue = fatigue_pct(rules, self.attacks_made, self.attacks_received);
        let crossing = i32::from(self.crossing_penalty_pct);
        (i32::from(self.base_readiness_pct) + fatigue - crossing).clamp(0, 100) as u8
    }

    fn lose_steps(&mut self, steps: u8, morale_per_step: u8) {
        self.strength = self.strength.saturating_sub(steps);
        self.morale_pct = self
            .morale_pct
            .saturating_sub(steps.saturating_mul(morale_per_step));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideState {
    pub id: SideId,
    pub tech: TechLevels,
    /// R-NM-01: caps unit morale.
    pub national_morale_pct: u8,
    /// MPP on hand.
    pub mpp: u32,
    /// R-ECO-01: scenario base income per phase.
    pub base_income: u32,
    /// R-ECO-03: how many units of each class may be bought ("above the schedule").
    #[serde(default)]
    pub purchase_limits: BTreeMap<String, u32>,
    /// Bought so far, destroyed ones included.
    #[serde(default)]
    pub purchased: BTreeMap<String, u32>,
}

/// R-ECO-01: a city's MPP value; income = value × owner's supply there / max.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CityValue {
    pub hex: Hex,
    pub value: u32,
}

/// R-ECO-01: scenario income event (airlift, emergency deliveries), per phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledIncome {
    pub side: SideId,
    pub from_turn: u32,
    /// Inclusive; `None` = until the end.
    pub until_turn: Option<u32>,
    pub amount: u32,
}

/// R-ECO-02: a bought unit on its way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arrival {
    pub side: SideId,
    pub class: String,
    pub turn: u32,
}

/// R-SUP-01: source with base value and current (captured) value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceState {
    pub base: u8,
    pub current: u8,
}

/// R-COMBAT-06: a vacated hex the attacker may occupy for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingAdvance {
    pub unit: UnitId,
    pub hex: Hex,
    /// R-MOVE-02: across a canal without a bridge (infantry by boat).
    pub by_boat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitSetup {
    pub class: String,
    pub side: SideId,
    pub hex: Hex,
    pub strength: u8,
    pub morale_pct: u8,
    /// Hundredths of a star.
    pub exp: u16,
    pub entrenchment: u8,
    /// Starting unit supply; `None` takes the hex supply.
    pub supply: Option<u8>,
    pub hq_rating: Option<u8>,
    pub formation: Option<String>,
    pub nation: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioSetup {
    /// R-SCALE-01; `None` uses the default hex size.
    pub hex_km: Option<f64>,
    /// R-SCALE-01 `supplyDecayAP`; `None` uses the default.
    pub supply_decay_ap: Option<f64>,
    pub seed: u64,
    pub start_turn: u32,
    /// In phase order: the first side moves first (R-TURN-01).
    pub sides: Vec<SideState>,
    pub map: HexMap,
    /// R-MOVE-06: initial control.
    pub control: BTreeMap<Hex, SideId>,
    /// R-SUP-01: base values; owner is the controller of the hex.
    pub sources: Vec<SupplySource>,
    /// Unit ids are assigned 1, 2, … in this order.
    pub units: Vec<UnitSetup>,
    pub cities: Vec<CityValue>,
    pub scheduled_income: Vec<ScheduledIncome>,
}

/// AP rules in half-points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ApParams {
    zoc_exit: u32,
    attack: u32,
    exploit_min: u32,
    night_penalty: u32,
    low_supply_penalty: u32,
    min: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameState {
    data: Arc<GameData>,
    hex_km: f64,
    supply_params: SupplyParams,
    ap: ApParams,
    map: HexMap,
    control: BTreeMap<Hex, SideId>,
    sources: BTreeMap<Hex, SourceState>,
    sides: Vec<SideState>,
    active: usize,
    turn: u32,
    units: BTreeMap<UnitId, Unit>,
    next_unit_id: u32,
    cities: Vec<CityValue>,
    scheduled_income: Vec<ScheduledIncome>,
    arrivals: Vec<Arrival>,
    pending_advance: Option<PendingAdvance>,
    rng: Pcg32,
}

fn invalid(msg: impl Into<String>) -> RuleError {
    RuleError::InvalidData(msg.into())
}

impl GameState {
    /// Builds the state and runs the start of the first phase.
    pub fn new(data: Arc<GameData>, setup: ScenarioSetup) -> Result<(Self, Vec<Event>), RuleError> {
        let hex_km = setup.hex_km.unwrap_or(data.rules.scale.default_hex_km);
        if hex_km <= 0.0 {
            return Err(invalid("hex_km must be > 0"));
        }
        let supply_params =
            SupplyParams::for_scale(&data.rules.supply, hex_km, setup.supply_decay_ap)?;
        let m = &data.rules.movement;
        let ap = ApParams {
            zoc_exit: ap_to_halves(m.zoc_exit_ap)?,
            attack: ap_to_halves(m.attack_ap)?,
            exploit_min: ap_to_halves(m.exploit_min_ap)?,
            night_penalty: ap_to_halves(m.night_ap_penalty)?,
            low_supply_penalty: ap_to_halves(m.low_supply_ap_penalty)?,
            min: ap_to_halves(m.min_ap)?,
        };
        if setup.start_turn == 0 {
            return Err(invalid("start_turn must be >= 1"));
        }
        let side_ids: BTreeSet<SideId> = setup.sides.iter().map(|s| s.id).collect();
        if setup.sides.len() < 2 || side_ids.len() != setup.sides.len() {
            return Err(invalid("need at least two sides with distinct ids"));
        }

        let mut sources = BTreeMap::new();
        for s in &setup.sources {
            if setup.map.cell(s.hex).is_none() || s.value > supply_params.max {
                return Err(invalid(format!("bad supply source at {:?}", s.hex)));
            }
            sources.insert(
                s.hex,
                SourceState {
                    base: s.value,
                    current: s.value,
                },
            );
        }

        let mut units = BTreeMap::new();
        let mut taken = BTreeSet::new();
        let mut supply_overrides = Vec::new();
        for (i, u) in setup.units.iter().enumerate() {
            let class = data.units.get(&u.class)?;
            if !class.target_class.is_ground() {
                return Err(RuleError::Unsupported(format!(
                    "`{}`: aircraft and ships in GameState (R-TURN-03)",
                    u.class
                )));
            }
            if !side_ids.contains(&u.side) {
                return Err(invalid(format!("unit {i}: unknown side {:?}", u.side)));
            }
            if setup.map.cell(u.hex).is_none() || !taken.insert(u.hex) {
                return Err(invalid(format!(
                    "unit {i}: hex {:?} off-map or occupied",
                    u.hex
                )));
            }
            if u.strength == 0 {
                return Err(invalid(format!("unit {i}: strength must be > 0")));
            }
            if class.has_tag(crate::data::UnitTag::Hq) != u.hq_rating.is_some() {
                return Err(invalid(format!(
                    "unit {i}: hq_rating is required for HQs only"
                )));
            }
            let id = UnitId(i as u32 + 1);
            supply_overrides.push((id, u.supply));
            units.insert(
                id,
                Unit {
                    id,
                    class: u.class.clone(),
                    side: u.side,
                    hex: u.hex,
                    strength: u.strength,
                    morale_pct: u.morale_pct.min(100),
                    exp: u.exp,
                    entrenchment: u.entrenchment,
                    supply: 0,
                    zero_supply_phases: 0,
                    base_readiness_pct: 0,
                    attacks_made: 0,
                    attacks_received: 0,
                    ap_halves: 0,
                    strikes_left: 0,
                    can_move: false,
                    started_in_zoc: false,
                    moved: false,
                    fought: false,
                    acted: false,
                    reinforced_steps: 0,
                    hq_rating: u.hq_rating,
                    formation: u.formation.clone(),
                    nation: u.nation.clone(),
                    pinned_hq: None,
                    attached_hq: None,
                    hq_lost: false,
                    attacked_by: Vec::new(),
                    crossing_penalty_pct: 0,
                },
            );
        }

        for c in &setup.cities {
            if setup.map.cell(c.hex).is_none() {
                return Err(invalid(format!("city {:?} is off-map", c.hex)));
            }
        }
        let next_unit_id = units.len() as u32 + 1;
        let mut state = Self {
            data,
            hex_km,
            supply_params,
            ap,
            map: setup.map,
            control: setup.control,
            sources,
            sides: setup.sides,
            active: 0,
            turn: setup.start_turn,
            units,
            next_unit_id,
            cities: setup.cities,
            scheduled_income: setup.scheduled_income,
            arrivals: Vec::new(),
            pending_advance: None,
            rng: Pcg32::new(setup.seed, 0),
        };

        // Initial supply and readiness for both sides (defenders need it too).
        let mut hex_supply = BTreeMap::new();
        for side in state.sides.iter().map(|s| s.id) {
            hex_supply.insert(side, state.hex_supply(side)?);
        }
        let data = Arc::clone(&state.data);
        for (id, over) in supply_overrides {
            if let Some(u) = state.units.get_mut(&id) {
                let from_hex = hex_supply.get(&u.side).map_or(0, |m| m.get(u.hex));
                u.supply = over.unwrap_or(from_hex).min(state.supply_params.max);
                u.base_readiness_pct = readiness_pct(
                    &data.rules.readiness,
                    u.supply,
                    u.morale_pct,
                    u.exp_stars(),
                    0,
                );
            }
        }
        // R-HQ-02: a side with line units but no HQ gets −10% everywhere.
        let mut events: Vec<Event> = state
            .sides
            .iter()
            .filter(|side| {
                let mine = || state.units.values().filter(|u| u.side == side.id);
                mine().any(|u| u.hq_rating.is_none()) && !mine().any(|u| u.hq_rating.is_some())
            })
            .map(|side| Event::SetupWarning {
                side: side.id,
                warning: SetupWarning::NoHq,
            })
            .collect();
        events.extend(state.start_phase()?);
        Ok((state, events))
    }

    /// Applies a command atomically: on error the state is unchanged.
    pub fn apply(&mut self, command: Command) -> Result<Vec<Event>, RuleError> {
        let keeps_pending = matches!(command, Command::Attack { .. } | Command::Advance { .. });
        let backup = self.clone();
        let result = match command {
            Command::Move { unit, path } => self.move_unit(unit, &path),
            Command::Attack { unit, target } => self.attack(unit, target),
            Command::Bombard { unit, target } => self.bombard(unit, target),
            Command::Advance { unit } => self.advance(unit),
            Command::Reinforce { unit, steps, elite } => self.reinforce(unit, steps, elite),
            Command::Purchase { class } => self.purchase(&class),
            Command::Transfer { unit, to } => self.transfer(unit, to),
            Command::PinToHq { unit, hq } => self.pin_to_hq(unit, hq),
            Command::EndPhase => self.end_phase(),
        };
        match result {
            Ok(events) => {
                if !keeps_pending {
                    self.pending_advance = None;
                }
                Ok(events)
            }
            Err(e) => {
                *self = backup;
                Err(e)
            }
        }
    }

    // ------------------------------------------------------------ queries

    pub fn data(&self) -> &GameData {
        &self.data
    }

    pub fn turn(&self) -> u32 {
        self.turn
    }

    pub fn active_side(&self) -> SideId {
        self.sides[self.active].id
    }

    /// R-TURN-02: odd turns are day, even turns are night.
    pub fn is_night(&self) -> bool {
        self.turn.is_multiple_of(2)
    }

    pub fn hex_km(&self) -> f64 {
        self.hex_km
    }

    pub fn map(&self) -> &HexMap {
        &self.map
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.units.get(&id)
    }

    pub fn units(&self) -> impl Iterator<Item = &Unit> {
        self.units.values()
    }

    pub fn unit_at(&self, hex: Hex) -> Option<&Unit> {
        self.units.values().find(|u| u.hex == hex)
    }

    /// R-MOVE-06.
    pub fn controller(&self, hex: Hex) -> Option<SideId> {
        self.control.get(&hex).copied()
    }

    pub fn source(&self, hex: Hex) -> Option<SourceState> {
        self.sources.get(&hex).copied()
    }

    pub fn side(&self, side: SideId) -> Option<&SideState> {
        self.sides.iter().find(|s| s.id == side)
    }

    pub fn arrivals(&self) -> &[Arrival] {
        &self.arrivals
    }

    pub fn pending_advance(&self) -> Option<PendingAdvance> {
        self.pending_advance
    }

    /// R-SUP-02 / R-SUP-04: current hex supply map of `side`.
    pub fn hex_supply(&self, side: SideId) -> Result<SupplyMap, RuleError> {
        let sources: Vec<SupplySource> = self
            .sources
            .iter()
            .filter(|(h, _)| self.control.get(h) == Some(&side))
            .map(|(&hex, s)| SupplySource {
                hex,
                value: s.current,
            })
            .collect();
        let hqs: Vec<HqPosition> = self
            .units
            .values()
            .filter(|u| u.side == side)
            .filter_map(|u| u.hq_rating.map(|rating| HqPosition { hex: u.hex, rating }))
            .collect();
        compute_supply(&SupplyInput {
            map: &self.map,
            terrain: &self.data.terrain,
            params: &self.supply_params,
            side,
            sources: &sources,
            occupancy: &self.occupancy(),
            control: &self.control,
            hqs: &hqs,
        })
    }

    // ------------------------------------------------------------ helpers

    fn class(&self, unit: &Unit) -> Result<&UnitClass, RuleError> {
        self.data.units.get(&unit.class)
    }

    fn side_state_mut(&mut self, side: SideId) -> Result<&mut SideState, RuleError> {
        self.sides
            .iter_mut()
            .find(|s| s.id == side)
            .ok_or_else(|| invalid(format!("unknown side {side:?}")))
    }

    fn side_state(&self, side: SideId) -> Result<&SideState, RuleError> {
        self.sides
            .iter()
            .find(|s| s.id == side)
            .ok_or_else(|| invalid(format!("unknown side {side:?}")))
    }

    fn get_unit(&self, id: UnitId) -> Result<&Unit, RuleError> {
        self.units.get(&id).ok_or(RuleError::UnknownUnit(id.0))
    }

    fn own_unit(&self, id: UnitId) -> Result<&Unit, RuleError> {
        let unit = self.get_unit(id)?;
        if unit.side != self.active_side() {
            return Err(RuleError::NotYourUnit(id.0));
        }
        Ok(unit)
    }

    /// Ground units for ZOC and supply (R-MOVE-03: no ZOC at strength below the threshold).
    fn occupancy(&self) -> Occupancy {
        let min = self.data.rules.movement.zoc_min_strength;
        self.units
            .values()
            .map(|u| {
                let exerts =
                    self.data.units.get(&u.class).is_ok_and(|c| c.exerts_zoc) && u.strength >= min;
                (
                    u.hex,
                    Occupant {
                        side: u.side,
                        exerts_zoc: exerts,
                    },
                )
            })
            .collect()
    }

    /// R-MOVE-06: a ground unit entering a hex takes control of it.
    /// A captured supply source drops to value / divisor (R-SUP-01).
    fn take_hex(&mut self, hex: Hex, side: SideId, events: &mut Vec<Event>) {
        let from = self.control.get(&hex).copied();
        if from == Some(side) {
            return;
        }
        self.control.insert(hex, side);
        if let Some(src) = self.sources.get_mut(&hex) {
            src.current = src.base / self.data.rules.supply.captured_source_divisor;
        }
        events.push(Event::HexCaptured { hex, side, from });
    }

    fn remove_destroyed(&mut self, events: &mut Vec<Event>) {
        let dead: Vec<UnitId> = self
            .units
            .values()
            .filter(|u| u.strength == 0)
            .map(|u| u.id)
            .collect();
        for id in dead {
            let Some(unit) = self.units.remove(&id) else {
                continue;
            };
            events.push(Event::UnitDestroyed { unit: id });
            if unit.hq_rating.is_some() {
                self.hq_destroyed(&unit, events);
            }
        }
    }
}
