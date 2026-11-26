//! R-TURN-01: game state. It changes only through [`GameState::apply`].

mod attack;
pub use attack::AttackModifiers;
mod aviation;
mod boosts;
mod bridge;
mod combats;
mod command;
mod economy;
mod events;
mod hq;
mod moves;
mod nm;
mod phase;
mod transfer;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use aviation::{AirBase, AirDuel, AirError, AirStrikeForecast, AirUnit, MapEdge, OffMapBase};
pub use boosts::{PendingBoost, TechBoost};
pub use combats::{DeclaredAttack, PendingCombat};
pub use command::{Command, EndReason, Event, NmReason, Outcome, SetupWarning, VictoryLevel};
pub use events::{Effect, Predicate, ScenarioEvent};

use crate::data::{ap_to_halves, GameData, ReadinessRules, UnitClass, Variants};
use crate::error::RuleError;
use crate::hex::Hex;
use crate::map::HexMap;
use crate::movement::{Occupancy, Occupant};
use crate::rng::Pcg32;
use crate::side::SideId;
use crate::supply::{
    compute_supply, supply_tree, HqPosition, SupplyInput, SupplyMap, SupplyParams, SupplySource,
    SupplyTree,
};
use crate::tech::TechLevels;
use crate::unit::{fatigue_pct, readiness_pct};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(pub u32);

/// R-UNIT-01: what the phase-start readiness was computed from: supply,
/// morale and experience then, and the HQ modifier (R-HQ-02) in points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReadinessBasis {
    pub supply: u8,
    pub morale_pct: u8,
    pub exp_stars: u8,
    pub hq_pct: i32,
}

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
    /// R-UNIT-01: inputs of `base_readiness_pct` (for the UI's explanation).
    pub readiness_basis: ReadinessBasis,
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
    /// R-UI-04: the formation's name (a locale key); the rules ignore it.
    pub name: Option<String>,
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
    /// R-EVT-01: readiness penalty from a scenario event, until the next own phase.
    pub event_penalty_pct: u8,
}

impl Unit {
    /// A fresh unit from scenario data, placed at `hex`; phase-start refresh
    /// fills in supply, readiness and AP.
    pub fn from_setup(id: UnitId, u: &UnitSetup, hex: Hex) -> Self {
        Self {
            id,
            class: u.class.clone(),
            side: u.side,
            hex,
            strength: u.strength,
            morale_pct: u.morale_pct.min(100),
            exp: u.exp,
            entrenchment: u.entrenchment,
            supply: 0,
            zero_supply_phases: 0,
            base_readiness_pct: 0,
            readiness_basis: ReadinessBasis::default(),
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
            name: u.name.clone(),
            pinned_hq: None,
            attached_hq: None,
            hq_lost: false,
            attacked_by: Vec::new(),
            crossing_penalty_pct: 0,
            event_penalty_pct: 0,
        }
    }

    pub fn exp_stars(&self) -> u8 {
        u8::try_from(self.exp / 100).unwrap_or(u8::MAX)
    }

    /// R-UNIT-01: readiness including fatigue.
    pub fn readiness_pct(&self, rules: &ReadinessRules) -> u8 {
        let fatigue = fatigue_pct(rules, self.attacks_made, self.attacks_received);
        let crossing = i32::from(self.crossing_penalty_pct) + i32::from(self.event_penalty_pct);
        (i32::from(self.base_readiness_pct) + fatigue - crossing).clamp(0, 100) as u8
    }

    /// Returns the steps actually lost.
    fn lose_steps(&mut self, steps: u8, morale_per_step: u8) -> u8 {
        let lost = steps.min(self.strength);
        self.strength -= lost;
        self.morale_pct = self
            .morale_pct
            .saturating_sub(lost.saturating_mul(morale_per_step));
        lost
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
    /// R-NM-01: NM lost per `steps_per_loss` steps (1; Israel 2).
    pub loss_sensitivity: u8,
    /// Steps lost so far.
    #[serde(default)]
    pub steps_lost: u32,
}

/// R-NM-01, R-NM-02: objective city.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Objective {
    pub hex: Hex,
    /// NM lost by the side that loses it (5..15).
    pub nm_value: u8,
    /// Victory points for the side holding it at the end.
    pub vp: i32,
}

/// R-NM-01: "+1 per turn for holding a bridgehead or key hex".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyHex {
    pub hex: Hex,
    pub side: SideId,
    pub nm_per_turn: u8,
}

/// R-NM-02: a side wins by date if its VP margin reaches these values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VictoryThreshold {
    pub side: SideId,
    pub decisive: i32,
    pub victory: i32,
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

/// R-MOVE-02: a bridge under construction; ready at the builder side's next phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingBridge {
    pub engineer: UnitId,
    pub side: SideId,
    pub a: Hex,
    pub b: Hex,
}

/// R-COMBAT-06: a vacated hex the attacker may occupy for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingAdvance {
    pub unit: UnitId,
    pub hex: Hex,
    /// R-MOVE-02: across a canal without a bridge (infantry by boat).
    pub by_boat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// R-UI-04: the formation's name (a locale key).
    pub name: Option<String>,
    /// R-AIR-01: aircraft based off the map (id of an `OffMapBase`).
    pub offmap_base: Option<String>,
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
    pub objectives: Vec<Objective>,
    pub key_hexes: Vec<KeyHex>,
    /// R-NM-02: the game ends after this turn (ceasefire date).
    pub last_turn: Option<u32>,
    pub victory: Vec<VictoryThreshold>,
    /// R-EVT-01.
    pub events: Vec<ScenarioEvent>,
    /// R-TECH-02.
    pub tech_boosts: Vec<TechBoost>,
    /// R-MOVE-06: neutral closed territory (by country): ground units do not
    /// enter, supply does not pass; air is not restricted.
    pub neutral: Vec<NeutralZone>,
    /// R-AIR-01: bases off the map.
    pub offmap_bases: Vec<OffMapBase>,
    /// Rule variants switched on or off for this scenario (over `rules.variants`).
    pub variants: BTreeMap<String, bool>,
}

/// R-MOVE-06: a neutral country's hexes in a scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeutralZone {
    pub id: String,
    pub hexes: Vec<Hex>,
}

/// AP rules in half-points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ApParams {
    zoc_exit: u32,
    attack: u32,
    exploit_min: u32,
    night_penalty: u32,
    low_supply_penalty: u32,
    /// R-UNIT-02 variant `zero_supply_ap`; equals `low_supply_penalty` when off.
    zero_supply_penalty: u32,
    min: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameState {
    data: Arc<GameData>,
    /// `rules.variants` with the scenario's overrides.
    variants: Variants,
    /// Loss draws: binomial in the game; `sim` may switch it (П10).
    variance: crate::combat::Variance,
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
    /// R-AIR-01: aircraft, apart from ground units.
    air: BTreeMap<UnitId, AirUnit>,
    offmap_bases: Vec<OffMapBase>,
    next_unit_id: u32,
    cities: Vec<CityValue>,
    scheduled_income: Vec<ScheduledIncome>,
    arrivals: Vec<Arrival>,
    objectives: Vec<Objective>,
    objective_holders: BTreeMap<Hex, Option<SideId>>,
    key_hexes: Vec<KeyHex>,
    last_turn: Option<u32>,
    victory: Vec<VictoryThreshold>,
    outcome: Option<Outcome>,
    events: Vec<ScenarioEvent>,
    fired_events: BTreeSet<String>,
    tech_boosts: Vec<TechBoost>,
    /// R-TECH-02: tech levels at the start of the scenario (the cap base).
    start_tech: BTreeMap<SideId, TechLevels>,
    bought_boosts: BTreeSet<String>,
    pending_boosts: Vec<PendingBoost>,
    pending_bridges: Vec<PendingBridge>,
    pending_combats: Vec<PendingCombat>,
    next_combat_id: u32,
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
        let variants = data.rules.variants.with_overrides(&setup.variants)?;
        let mut supply_params =
            SupplyParams::for_scale(&data.rules.supply, hex_km, setup.supply_decay_ap)?;
        supply_params.hq_node = variants.hq_supply_node;
        supply_params.hq_bonus_min_strength = variants
            .hq_strength_scaling
            .then_some(data.rules.hq.supply_bonus_min_strength);
        let m = &data.rules.movement;
        let ap = ApParams {
            zero_supply_penalty: ap_to_halves(if variants.zero_supply_ap {
                m.zero_supply_ap_penalty
            } else {
                m.low_supply_ap_penalty
            })?,
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
        let mut air = BTreeMap::new();
        let mut taken = BTreeSet::new();
        let mut supply_overrides = Vec::new();
        for (i, u) in setup.units.iter().enumerate() {
            let class = data.units.get(&u.class)?;
            if class.target_class == crate::data::TargetClass::Air {
                // R-AIR-01: aircraft live apart, on a base.
                let base = match &u.offmap_base {
                    Some(b)
                        if setup
                            .offmap_bases
                            .iter()
                            .any(|x| &x.id == b && x.side == u.side) =>
                    {
                        AirBase::OffMap { id: b.clone() }
                    }
                    Some(b) => {
                        return Err(invalid(format!(
                            "unit {i}: unknown off-map base `{b}` of its side"
                        )))
                    }
                    None => {
                        let own = setup.control.get(&u.hex) == Some(&u.side);
                        let base = setup.map.is_airfield(u.hex)
                            || setup.cities.iter().any(|c| c.hex == u.hex);
                        if !own || !base {
                            return Err(invalid(format!("unit {i}: aircraft need an own city or airfield, or an off-map base")));
                        }
                        AirBase::Hex { hex: u.hex }
                    }
                };
                let id = UnitId(i as u32 + 1);
                air.insert(
                    id,
                    AirUnit {
                        id,
                        class: u.class.clone(),
                        side: u.side,
                        base,
                        strength: u.strength,
                        morale_pct: u.morale_pct.min(100),
                        exp: u.exp,
                        nation: u.nation.clone(),
                        name: u.name.clone(),
                        fatigue_pct: 0,
                        sorties: 0,
                        flew: false,
                        rebased: false,
                        intercepted: false,
                    },
                );
                continue;
            }
            if !class.target_class.is_ground() {
                return Err(RuleError::Unsupported(format!(
                    "`{}`: ships in GameState (R-TURN-03)",
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
            units.insert(id, Unit::from_setup(id, u, u.hex));
        }

        for c in &setup.cities {
            if setup.map.cell(c.hex).is_none() {
                return Err(invalid(format!("city {:?} is off-map", c.hex)));
            }
        }
        for o in &setup.objectives {
            if setup.map.cell(o.hex).is_none() {
                return Err(invalid(format!("objective {:?} is off-map", o.hex)));
            }
        }
        if setup.last_turn.is_some_and(|t| t < setup.start_turn) {
            return Err(invalid("last_turn is before start_turn"));
        }
        let objective_holders = setup
            .objectives
            .iter()
            .map(|o| (o.hex, setup.control.get(&o.hex).copied()))
            .collect();
        let start_tech = setup.sides.iter().map(|s| (s.id, s.tech.clone())).collect();
        for b in &setup.tech_boosts {
            if !side_ids.contains(&b.side) {
                return Err(invalid(format!("tech boost `{}`: unknown side", b.id)));
            }
        }
        // Ids follow the scenario's unit list, aircraft included.
        let next_unit_id = setup.units.len() as u32 + 1;
        let mut state = Self {
            variance: crate::combat::Variance::default(),
            variants,
            air,
            offmap_bases: setup.offmap_bases.clone(),
            data,
            hex_km,
            supply_params,
            ap,
            map: {
                let mut map = setup.map;
                for h in setup.neutral.iter().flat_map(|z| &z.hexes) {
                    map.close(*h);
                }
                map
            },
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
            objectives: setup.objectives,
            objective_holders,
            key_hexes: setup.key_hexes,
            last_turn: setup.last_turn,
            victory: setup.victory,
            outcome: None,
            events: setup.events,
            fired_events: BTreeSet::new(),
            tech_boosts: setup.tech_boosts,
            start_tech,
            bought_boosts: BTreeSet::new(),
            pending_boosts: Vec::new(),
            pending_bridges: Vec::new(),
            pending_combats: Vec::new(),
            next_combat_id: 1,
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
                u.readiness_basis = ReadinessBasis {
                    supply: u.supply,
                    morale_pct: u.morale_pct,
                    exp_stars: u.exp_stars(),
                    hq_pct: 0,
                };
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
        if self.outcome.is_some() {
            return Err(RuleError::GameOver);
        }
        // Tactical battles: units in a combat waiting for its result are locked,
        // and the phase cannot end until every such combat is resolved.
        if let Some(u) = Self::command_units(&command)
            .into_iter()
            .find(|&u| self.in_combat(u))
        {
            return Err(RuleError::UnitInCombat(u.0));
        }
        if matches!(command, Command::EndPhase) && !self.pending_combats.is_empty() {
            return Err(RuleError::CombatsPending);
        }
        let keeps_pending = matches!(
            command,
            Command::Attack { .. } | Command::Advance { .. } | Command::ResolveCombat { .. }
        );
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
            Command::BuildBridge { unit, toward } => self.build_bridge(unit, toward),
            Command::BombardBridge { unit, a, b } => self.bombard_bridge(unit, a, b),
            Command::AirStrike { unit, target } => self.air_strike(unit, target),
            Command::AirStrikeAirfield { unit, hex } => self.airfield_strike(unit, hex),
            Command::AirStrikeBridge { unit, a, b } => self.air_bridge_strike(unit, a, b),
            Command::Rebase { unit, base } => self.rebase(unit, base),
            Command::BuyTechBoost { id } => self.buy_tech_boost(&id),
            Command::DeclareAttack { unit, target } => self.declare_attack(unit, target),
            Command::ResolveCombat { combat, outcome } => self.resolve_combat(combat, outcome),
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

    /// Balance experiment only (`sim --variance`, П10): how losses are drawn.
    /// The game, the client and saved games never call it.
    pub fn set_variance(&mut self, variance: crate::combat::Variance) {
        self.variance = variance;
    }

    /// Hits of one fire by the current variance; `cap`: the target's strength.
    pub(super) fn hits(&mut self, params: &crate::combat::FireParams, cap: Option<u8>) -> u32 {
        crate::combat::draw_hits(&mut self.rng, params, cap, self.variance)
    }

    /// Rule variants in force (SC alignment, `rules.variants` + scenario).
    pub fn variants(&self) -> Variants {
        self.variants
    }

    /// R-COMBAT-02 variant `attack_strips_entrenchment`: levels a ground
    /// attack or an air strike strips from its target (0 when off).
    pub fn attack_entrench_loss(&self) -> u8 {
        if self.variants.attack_strips_entrenchment {
            self.data.rules.combat.attack_entrench_loss
        } else {
            0
        }
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

    /// Sides in phase order (R-TURN-01).
    pub fn sides(&self) -> &[SideState] {
        &self.sides
    }

    pub fn side(&self, side: SideId) -> Option<&SideState> {
        self.sides.iter().find(|s| s.id == side)
    }

    /// R-NM-02: set once the game has ended.
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    /// R-NM-02 / R-EVT-01: current ceasefire turn.
    pub fn last_turn(&self) -> Option<u32> {
        self.last_turn
    }

    /// R-TECH-02: boosts offered by the scenario.
    pub fn tech_boosts(&self) -> &[TechBoost] {
        &self.tech_boosts
    }

    pub fn pending_boosts(&self) -> &[PendingBoost] {
        &self.pending_boosts
    }

    pub fn pending_bridges(&self) -> &[PendingBridge] {
        &self.pending_bridges
    }

    pub fn arrivals(&self) -> &[Arrival] {
        &self.arrivals
    }

    pub fn pending_advance(&self) -> Option<PendingAdvance> {
        self.pending_advance
    }

    /// R-SUP-02 / R-SUP-04: current hex supply map of `side`.
    pub fn hex_supply(&self, side: SideId) -> Result<SupplyMap, RuleError> {
        self.hex_supply_if(side, &[])
    }

    /// R-AI-01 «поиск котлов»: the supply map of `side` if each hex in
    /// `held` were controlled by and occupied with a unit (exerting ZOC) of
    /// the given side — a trial recount, the state does not change.
    pub fn hex_supply_if(
        &self,
        side: SideId,
        held: &[(Hex, SideId)],
    ) -> Result<SupplyMap, RuleError> {
        let (control, occupancy) = self.with_held(held);
        let sources = self.supply_sources(side, &control, &occupancy)?;
        let hqs: Vec<HqPosition> = self
            .units
            .values()
            .filter(|u| u.side == side)
            .filter_map(|u| {
                u.hq_rating.map(|rating| HqPosition {
                    hex: u.hex,
                    rating,
                    strength: u.strength,
                })
            })
            .collect();
        compute_supply(&SupplyInput {
            map: &self.map,
            terrain: &self.data.terrain,
            params: &self.supply_params,
            side,
            sources: &sources,
            occupancy: &occupancy,
            control: &control,
            hqs: &hqs,
        })
    }

    /// R-SUP-02 best supply paths of `side` (cost and predecessor per hex),
    /// from the same sources as [`Self::hex_supply`], without HQ effects.
    pub fn supply_tree(&self, side: SideId) -> Result<SupplyTree, RuleError> {
        self.supply_tree_if(side, &[])
    }

    /// [`Self::supply_tree`] with hypothetical held hexes, as in [`Self::hex_supply_if`].
    pub fn supply_tree_if(
        &self,
        side: SideId,
        held: &[(Hex, SideId)],
    ) -> Result<SupplyTree, RuleError> {
        let (control, occupancy) = self.with_held(held);
        let sources = self.supply_sources(side, &control, &occupancy)?;
        supply_tree(
            &SupplyInput {
                map: &self.map,
                terrain: &self.data.terrain,
                params: &self.supply_params,
                side,
                sources: &sources,
                occupancy: &occupancy,
                control: &control,
                hqs: &[],
            },
            &sources,
            None,
        )
    }

    /// Control and occupancy with hypothetical held hexes.
    fn with_held(
        &self,
        held: &[(Hex, SideId)],
    ) -> (std::borrow::Cow<'_, BTreeMap<Hex, SideId>>, Occupancy) {
        let mut occupancy = self.occupancy();
        if held.is_empty() {
            return (std::borrow::Cow::Borrowed(&self.control), occupancy);
        }
        let mut control = self.control.clone();
        for &(h, side) in held {
            control.insert(h, side);
            occupancy.insert(
                h,
                Occupant {
                    side,
                    exerts_zoc: true,
                },
            );
        }
        (std::borrow::Cow::Owned(control), occupancy)
    }

    /// R-SUP-01 sources of `side` with their values now (isolation variant applied).
    fn supply_sources(
        &self,
        side: SideId,
        control: &BTreeMap<Hex, SideId>,
        occupancy: &Occupancy,
    ) -> Result<Vec<SupplySource>, RuleError> {
        let max = self.supply_params.max;
        // R-SUP-01 variant `source_isolation`: hexes a supply path (R-SUP-02,
        // any cost) reaches from the side's sources of max.
        let linked = if self.variants.source_isolation {
            let main: Vec<SupplySource> = self
                .sources
                .iter()
                .filter(|(h, s)| control.get(h) == Some(&side) && s.base >= max)
                .map(|(&hex, _)| SupplySource { hex, value: max })
                .collect();
            let input = SupplyInput {
                map: &self.map,
                terrain: &self.data.terrain,
                params: &self.supply_params,
                side,
                sources: &main,
                occupancy,
                control,
                hqs: &[],
            };
            Some(
                supply_tree(&input, &main, None)?
                    .cost
                    .into_keys()
                    .collect::<BTreeSet<Hex>>(),
            )
        } else {
            None
        };
        let cap = self.data.rules.supply.isolated_source_cap;
        Ok(self
            .sources
            .iter()
            .filter(|(h, _)| control.get(h) == Some(&side))
            .map(|(&hex, s)| SupplySource {
                hex,
                value: match &linked {
                    Some(l) if s.base < max && !l.contains(&hex) => s.current.min(cap),
                    _ => s.current,
                },
            })
            .collect())
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

    /// R-MOVE-03: hexes in the ZOC of `side`'s enemies (for the map layer).
    pub fn zoc_against(&self, side: SideId) -> BTreeSet<Hex> {
        crate::movement::enemy_zoc(&self.map, &self.occupancy(), side)
    }

    /// R-NM-01: objective hexes of the scenario.
    pub fn objectives(&self) -> &[Objective] {
        &self.objectives
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

    /// R-NM-01: NM a side loses for a destroyed unit: −2 / −3 encircled, or
    /// with the `nm_by_cost` variant −ceil(price / 40), encircled × 1.5
    /// rounded up (infantry 70 → 2, tanks 105 → 3, encircled tanks → 5).
    pub fn unit_nm_loss(&self, class: &str, encircled: bool) -> u32 {
        let nm = &self.data.rules.national_morale;
        if !self.variants.nm_by_cost {
            return u32::from(if encircled {
                nm.unit_destroyed_encircled
            } else {
                nm.unit_destroyed
            });
        }
        let cost = self.data.units.get(class).map_or(0, |c| c.cost_mpp);
        let base = cost.div_ceil(nm.unit_cost_divisor);
        if encircled {
            (base * nm.encircled_pct).div_ceil(100)
        } else {
            base
        }
    }

    /// Removes units at strength 0. R-NM-01: −2 NM per unit, −3 if its hex
    /// had no supply path (encircled); R-HQ-02 for HQs.
    fn remove_destroyed(&mut self, events: &mut Vec<Event>) -> Result<(), RuleError> {
        let dead: Vec<Unit> = self
            .units
            .values()
            .filter(|u| u.strength == 0)
            .cloned()
            .collect();
        if dead.is_empty() {
            return Ok(());
        }
        let mut supply = BTreeMap::new();
        for side in dead.iter().map(|u| u.side).collect::<BTreeSet<_>>() {
            supply.insert(side, self.hex_supply(side)?);
        }
        for unit in dead {
            self.units.remove(&unit.id);
            events.push(Event::UnitDestroyed { unit: unit.id });
            let encircled = supply.get(&unit.side).is_none_or(|m| m.get(unit.hex) == 0);
            let loss = self.unit_nm_loss(&unit.class, encircled);
            self.change_nm(
                unit.side,
                -i32::try_from(loss).unwrap_or(i32::MAX),
                NmReason::UnitDestroyed {
                    unit: unit.id,
                    encircled,
                },
                events,
            )?;
            if unit.hq_rating.is_some() {
                self.hq_destroyed(&unit, events)?;
            }
        }
        Ok(())
    }
}
