//! Game data: rules, terrain and unit classes, parsed from JSON strings.
//! The core never reads files; callers pass the JSON text.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::RuleError;
use crate::tech::TechCatalog;

fn parse<T: DeserializeOwned>(json: &str) -> Result<T, RuleError> {
    serde_json::from_str(json).map_err(|e| RuleError::Parse(e.to_string()))
}

/// Converts AP to integer half-points so path costs stay exact.
pub fn ap_to_halves(ap: f64) -> Result<u32, RuleError> {
    let halves = ap * 2.0;
    if !(0.0..=f64::from(u32::MAX)).contains(&halves) || halves.fract() != 0.0 {
        return Err(RuleError::InvalidAp(ap));
    }
    Ok(halves as u32)
}

// ---------------------------------------------------------------- rules

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    pub p_base: f64,
    pub p_slope: f64,
    pub p_min: f64,
    pub p_max: f64,
    pub exp_attack_bonus: f64,
    pub exp_defense_bonus: f64,
    pub entrench_defense_bonus: f64,
    pub night_attack_mult: f64,
    pub canal_defense_mult: f64,
    pub forecast_low_percentile: f64,
    pub forecast_high_percentile: f64,
    /// R-COMBAT-07 (variant `prepared_attack`): readiness points of an
    /// attacker that has not moved this phase, for this attack only.
    pub prepared_attack_readiness_pct: u8,
    /// R-COMBAT-05 (variant `unsupplied_target`): × attacker's A when the
    /// target's unit supply is 0.
    pub unsupplied_target_attack_mult: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessRules {
    pub base: f64,
    pub per_supply: f64,
    pub exp_bonus: f64,
    pub max: f64,
    pub fatigue_per_attack_pct: u8,
    pub fatigue_per_attacked_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamRules {
    pub shot_fraction: f64,
    pub next_sam_mult: f64,
}

/// R-AIR-01, R-AIR-02: sorties, interception, strikes on airfields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirRules {
    /// Readiness lost per sortie (strike, escort, interception).
    pub sortie_readiness_loss_pct: u8,
    /// Readiness regained after an own phase without sorties.
    pub rest_readiness_gain_pct: u8,
    /// Readiness the target of a ground strike loses.
    pub ground_strike_readiness_loss_pct: u8,
    /// Air defense of aircraft parked at an airfield.
    pub parked_air_defense: f64,
    /// Readiness every aircraft in a struck airfield's hex loses.
    pub airfield_strike_readiness_loss_pct: u8,
    /// Fighters intercept or escort only at or above this readiness.
    pub intercept_min_readiness_pct: u8,
    /// R-AIR-02 variant `air_strength_threshold`: and only at or above this strength.
    pub intercept_min_strength: u8,
    /// A fighter intercepts sorties whose target is within this range of its base.
    pub intercept_radius_km: f64,
    /// At most this many interceptors meet one strike (the best ones).
    pub max_intercepts_per_strike: usize,
    /// Readiness lost by an aircraft forced to another base when its own fell (R-TURN-03).
    pub base_lost_readiness_loss_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyRules {
    pub max: u8,
    pub decay_ap: f64,
    pub bridge_penalty: u8,
    /// R-MOVE-02 / R-SUP-02: supply across a canal by ferry (no movement).
    pub ferry_penalty: u8,
    pub hq_bonus_radius_km: f64,
    pub hq_min_supply: u8,
    pub hq_bonus: u8,
    pub hq_bonus_high: u8,
    pub hq_high_rating: u8,
    /// R-SUP-06 (variant `hq_supply_node`): an HQ with its own supply S at
    /// least this works as a source of `min(hq_node_max, S + hq_node_bonus)`
    /// within `hq_bonus_radius_km`, × strength / `hq_node_strength_divisor`
    /// (floor) at any strength.
    pub hq_node_min_supply: u8,
    pub hq_node_bonus: u8,
    pub hq_node_max: u8,
    pub hq_node_strength_divisor: u8,
    /// R-SUP-03: supply at or below this gives −AP and no entrenchment growth.
    pub low_supply_max: u8,
    pub entrench_min_supply: u8,
    /// Supply inertia: a unit's supply drops at most this much per own phase.
    pub max_drop_per_phase: u8,
    /// R-SUP-03: attrition starts at this many consecutive phases at supply 0.
    pub attrition_after_phases: u8,
    pub attrition_steps: u8,
    /// R-SUP-03 variant `pocket_desertion`: a unit at supply 0 with at least
    /// `desertion_min_enemies` adjacent enemy ground units of strength and
    /// supply at least these loses `desertion_steps` with `desertion_chance`.
    pub desertion_min_enemies: u8,
    pub desertion_enemy_min_strength: u8,
    pub desertion_enemy_min_supply: u8,
    pub desertion_chance: f64,
    pub desertion_steps: u8,
    /// R-SUP-01: captured source works at value / divisor (floor).
    pub captured_source_divisor: u8,
    pub captured_recovery_per_turn: u8,
    /// R-SUP-01 variant `source_isolation`: a source below `max` with no path
    /// through own hexes to an own source of `max` gives at most this.
    pub isolated_source_cap: u8,
    /// Siege: a source below `max` loses `siege_loss_per_phase` at the start of
    /// its owner's phase with at least `siege_min_enemies` adjacent enemy
    /// ground units of strength ≥ `siege_enemy_min_strength`.
    pub siege_min_enemies: u8,
    pub siege_enemy_min_strength: u8,
    pub siege_loss_per_phase: u8,
}

/// R-MOVE-03, R-MOVE-04, R-UNIT-02. AP values may be multiples of 0.5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementRules {
    pub zoc_min_strength: u8,
    pub zoc_exit_ap: f64,
    pub attack_ap: f64,
    pub exploit_min_ap: f64,
    pub night_ap_penalty: f64,
    pub low_supply_ap_penalty: f64,
    /// R-UNIT-02 (variant `zero_supply_ap`): AP penalty at supply 0, instead
    /// of `low_supply_ap_penalty`.
    pub zero_supply_ap_penalty: f64,
    pub min_ap: f64,
    /// R-MOVE-02: readiness penalty for a boat crossing, until the next own phase.
    pub boat_crossing_readiness_pct: u8,
    /// R-MOVE-05 (variant `transfer_morale`): morale lost by a strategic transfer.
    pub transfer_morale_loss: u8,
    /// R-MOVE-05 (variant `transfer_supply_min`): the unit's supply must be at least this.
    pub transfer_min_supply: u8,
}

/// Unit morale changes (GDD section 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoraleRules {
    pub loss_per_step: u8,
    pub no_supply_loss: u8,
    pub rest_gain: u8,
    /// Unit supply needed for `rest_gain` (no combat since the last own phase).
    pub rest_min_supply: u8,
}

/// R-COMBAT-06.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetreatRules {
    pub max_strength: u8,
    pub heavy_loss_steps: u8,
    pub low_readiness_pct: u8,
    pub blocked_extra_losses: u8,
}

/// Experience in hundredths of a star (GDD section 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperienceRules {
    pub per_combat: u16,
    pub win: u16,
    pub max: u16,
}

/// R-MOVE-02: chance that a strike destroys a bridge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeRules {
    pub bombard_destroy_chance: f64,
    /// For air strikes (R-AIR-02), used once aviation is in.
    pub air_strike_destroy_chance: f64,
}

/// Tactical battles (GDD «Тактические бои»): result → percentile of the
/// forecast distribution, and the snapshot format version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalRules {
    pub percentiles: BTreeMap<crate::tactical::TacticalOutcome, f64>,
    pub snapshot_format: u32,
}

/// R-NM-01, R-NM-02.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NationalMoraleRules {
    pub unit_destroyed: u8,
    pub unit_destroyed_encircled: u8,
    /// R-NM-01 variant `nm_by_cost`: a destroyed unit costs
    /// ceil(price / `unit_cost_divisor`) NM, encircled × `encircled_pct`%
    /// (rounded up) instead of the fixed values above.
    pub unit_cost_divisor: u32,
    pub encircled_pct: u32,
    /// NM drops by the side's sensitivity for every this many steps lost.
    pub steps_per_loss: u32,
    /// Sudden death at NM at or below this.
    pub sudden_death_pct: u8,
    /// New owner of an objective gains this share of its NM value (floor).
    pub objective_taken_share_pct: u8,
}

/// R-HQ-01, R-HQ-02.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HqRules {
    pub attach_radius_km: f64,
    pub rating_base: u8,
    pub bonus_per_rating_pct: u8,
    pub fast_exp_rating: u8,
    pub fast_exp_pct: u16,
    pub unattached_readiness_pct: u8,
    pub destroyed_nm_loss: u8,
    /// R-HQ-02 variant `hq_strength_scaling`: the readiness bonus is
    /// × HQ strength / `strength_divisor` (rounded half up), and the R-SUP-02
    /// supply bonus needs HQ strength ≥ `supply_bonus_min_strength`.
    pub strength_divisor: u8,
    pub supply_bonus_min_strength: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRules {
    pub max_strength: u8,
}

/// R-SUP-03: reinforcement band, chosen by the highest `min_supply` reached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReinforceBand {
    pub min_supply: u8,
    pub max_steps: u8,
    pub cost_pct: u32,
}

/// R-ECO-01..03. Costs are integer percent to keep MPP exact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EconomyRules {
    /// Price of one step as percent of the unit price.
    pub reinforce_cost_pct: u32,
    pub elite_cost_pct: u32,
    /// Sorted by `min_supply` descending (checked on load).
    pub reinforce_bands: Vec<ReinforceBand>,
    pub purchase_delay_turns: u32,
    /// R-MOVE-05: strategic transfer price as percent of the unit price.
    pub transfer_cost_pct: u32,
    /// R-TECH-02: boosts raise a branch at most this far above its start.
    pub boost_max_levels_above_start: u8,
    /// Bought units arrive in an own source with at least this value.
    pub arrival_source_value: u8,
}

impl EconomyRules {
    /// R-SUP-03: band for a unit's supply; `None` at supply below every band.
    pub fn band(&self, supply: u8) -> Option<&ReinforceBand> {
        self.reinforce_bands.iter().find(|b| supply >= b.min_supply)
    }
}

/// R-COMBAT-04.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BombardRules {
    pub shot_fraction: f64,
    pub entrench_loss: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaleRules {
    pub default_hex_km: f64,
}

/// SC alignment of the core loop (designer task 2026-11-26): rule variants,
/// each off until its A/B run decides. The numbers live in their sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Variants {
    /// R-COMBAT-02: every ground attack and air strike strips entrenchment.
    pub attack_strips_entrenchment: bool,
    /// R-COMBAT-07: prepared attack.
    pub prepared_attack: bool,
    /// R-COMBAT-05: attacks on a unit without supply hit harder.
    pub unsupplied_target: bool,
    /// R-SUP-06: HQ as a supply node (replaces the R-SUP-02 HQ bonus).
    pub hq_supply_node: bool,
    /// R-MOVE-05: strategic transfer costs morale.
    pub transfer_morale: bool,
    /// R-UNIT-02: supply 0 costs more AP than supply 1–3.
    pub zero_supply_ap: bool,
    /// R-ECO-02: a reinforced unit does not act; no elite steps next to the enemy.
    pub reinforce_locks: bool,
    /// R-NM-01: NM for a destroyed unit by its price.
    pub nm_by_cost: bool,
    /// R-HQ-02: HQ bonuses scale with the HQ's strength.
    pub hq_strength_scaling: bool,
    /// R-COMBAT-06, R-MOVE-01: cities and fortified areas hold.
    pub fortified_hold: bool,
    /// R-SUP-03: units in a pocket under pressure desert.
    pub pocket_desertion: bool,
    /// R-SUP-01: cut-off sources give little, besieged ones dwindle.
    pub source_isolation: bool,
    /// R-AIR-02: fighters intercept and escort only at strength ≥ `air.intercept_min_strength` (SC п. 30).
    pub air_strength_threshold: bool,
    /// R-MOVE-05: strategic transfer only at unit supply ≥ `movement.transfer_min_supply` (SC п. 34).
    pub transfer_supply_min: bool,
}

impl Variants {
    /// Applies `name → on` overrides (scenario `variants`, sim `--variant`).
    pub fn with_overrides(self, overrides: &BTreeMap<String, bool>) -> Result<Self, RuleError> {
        if overrides.is_empty() {
            return Ok(self);
        }
        let mut v = serde_json::to_value(self).map_err(|e| RuleError::Parse(e.to_string()))?;
        for (k, on) in overrides {
            match v.get_mut(k) {
                Some(slot) => *slot = serde_json::Value::Bool(*on),
                None => {
                    return Err(RuleError::InvalidData(format!(
                        "unknown rule variant `{k}`"
                    )))
                }
            }
        }
        serde_json::from_value(v).map_err(|e| RuleError::Parse(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub combat: CombatRules,
    pub readiness: ReadinessRules,
    /// R-AIR-03: SAM fire.
    pub sam: SamRules,
    pub air: AirRules,
    pub supply: SupplyRules,
    pub scale: ScaleRules,
    pub movement: MovementRules,
    pub morale: MoraleRules,
    pub retreat: RetreatRules,
    pub experience: ExperienceRules,
    pub bombard: BombardRules,
    pub unit: UnitRules,
    pub economy: EconomyRules,
    pub hq: HqRules,
    pub national_morale: NationalMoraleRules,
    pub bridge: BridgeRules,
    pub tactical: TacticalRules,
    pub variants: Variants,
}

impl Rules {
    pub fn from_json(json: &str) -> Result<Self, RuleError> {
        let rules: Rules = parse(json)?;
        rules.validate()?;
        Ok(rules)
    }

    fn validate(&self) -> Result<(), RuleError> {
        let c = &self.combat;
        if !(0.0 <= c.p_min && c.p_min <= c.p_max && c.p_max <= 1.0) {
            return Err(RuleError::InvalidData(
                "combat: need 0 <= p_min <= p_max <= 1".into(),
            ));
        }
        if !(0.0 < c.forecast_low_percentile
            && c.forecast_low_percentile < c.forecast_high_percentile
            && c.forecast_high_percentile < 1.0)
        {
            return Err(RuleError::InvalidData(
                "combat: forecast percentiles must satisfy 0 < low < high < 1".into(),
            ));
        }
        if ap_to_halves(self.supply.decay_ap)? == 0 {
            return Err(RuleError::InvalidData(
                "supply: decay_ap must be > 0".into(),
            ));
        }
        let m = &self.movement;
        for ap in [
            m.zoc_exit_ap,
            m.attack_ap,
            m.exploit_min_ap,
            m.night_ap_penalty,
            m.low_supply_ap_penalty,
            m.zero_supply_ap_penalty,
            m.min_ap,
        ] {
            ap_to_halves(ap)?;
        }
        let bands = &self.economy.reinforce_bands;
        if bands.windows(2).any(|w| w[0].min_supply <= w[1].min_supply) {
            return Err(RuleError::InvalidData(
                "economy: reinforce_bands must be sorted by min_supply, descending".into(),
            ));
        }
        let outcomes = crate::tactical::TacticalOutcome::ALL;
        if outcomes.iter().any(|o| {
            !self
                .tactical
                .percentiles
                .get(o)
                .is_some_and(|q| (0.0..=1.0).contains(q))
        }) {
            return Err(RuleError::InvalidData(
                "tactical: every outcome needs a quantile in [0, 1]".into(),
            ));
        }
        if self.national_morale.unit_cost_divisor == 0 {
            return Err(RuleError::InvalidData(
                "national_morale: unit_cost_divisor must be > 0".into(),
            ));
        }
        if self.national_morale.steps_per_loss == 0 {
            return Err(RuleError::InvalidData(
                "national_morale: steps_per_loss must be > 0".into(),
            ));
        }
        if self.hq.strength_divisor == 0 {
            return Err(RuleError::InvalidData(
                "hq: strength_divisor must be > 0".into(),
            ));
        }
        if self.supply.hq_node_strength_divisor == 0 {
            return Err(RuleError::InvalidData(
                "supply: hq_node_strength_divisor must be > 0".into(),
            ));
        }
        if self.supply.captured_source_divisor == 0 {
            return Err(RuleError::InvalidData(
                "supply: captured_source_divisor must be > 0".into(),
            ));
        }
        if self.scale.default_hex_km <= 0.0 {
            return Err(RuleError::InvalidData(
                "scale: default_hex_km must be > 0".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- terrain

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainDef {
    pub id: String,
    pub ap: f64,
    #[serde(default)]
    pub wheeled_ap: Option<f64>,
    pub defense_mult: f64,
    pub max_entrench: u8,
    /// R-MOVE-01: mountains are entered only through hexes marked as passes.
    #[serde(default)]
    pub requires_pass: bool,
    /// R-MOVE-01: lakes — no ground movement or supply, nobody controls them.
    #[serde(default)]
    pub impassable: bool,
    /// City or fortified area: R-COMBAT-06 / R-MOVE-01 variant `fortified_hold`.
    #[serde(default)]
    pub stronghold: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerrainFile {
    road_ap: f64,
    /// R-MOVE-01: dirt road (track) — entering costs this whatever the terrain.
    track_ap: f64,
    terrain: Vec<TerrainDef>,
}

/// Terrain type with AP costs pre-converted to half-points.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    pub def: TerrainDef,
    pub ap_halves: u32,
    pub wheeled_ap_halves: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainCatalog {
    pub road_ap_halves: u32,
    pub track_ap_halves: u32,
    terrain: BTreeMap<String, Terrain>,
}

impl TerrainCatalog {
    pub fn from_json(json: &str) -> Result<Self, RuleError> {
        let file: TerrainFile = parse(json)?;
        let mut terrain = BTreeMap::new();
        for def in file.terrain {
            let ap_halves = ap_to_halves(def.ap)?;
            let wheeled_ap_halves = match def.wheeled_ap {
                Some(ap) => ap_to_halves(ap)?,
                None => ap_halves,
            };
            let id = def.id.clone();
            let entry = Terrain {
                def,
                ap_halves,
                wheeled_ap_halves,
            };
            if terrain.insert(id.clone(), entry).is_some() {
                return Err(RuleError::InvalidData(format!("duplicate terrain `{id}`")));
            }
        }
        Ok(Self {
            road_ap_halves: ap_to_halves(file.road_ap)?,
            track_ap_halves: ap_to_halves(file.track_ap)?,
            terrain,
        })
    }

    pub fn get(&self, id: &str) -> Result<&Terrain, RuleError> {
        self.terrain
            .get(id)
            .ok_or_else(|| RuleError::UnknownTerrain(id.to_owned()))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Terrain> {
        self.terrain.values()
    }
}

// ---------------------------------------------------------------- units

/// R-COMBAT-01: target classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetClass {
    Soft,
    Hard,
    Air,
    Naval,
}

impl TargetClass {
    pub fn is_ground(self) -> bool {
        matches!(self, TargetClass::Soft | TargetClass::Hard)
    }
}

/// R-COMBAT-01: one attack value per target class.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackValues {
    pub soft: f64,
    pub hard: f64,
    pub air: f64,
    pub naval: f64,
}

impl AttackValues {
    pub fn against(&self, class: TargetClass) -> f64 {
        match class {
            TargetClass::Soft => self.soft,
            TargetClass::Hard => self.hard,
            TargetClass::Air => self.air,
            TargetClass::Naval => self.naval,
        }
    }
}

/// R-COMBAT-01: ground defense (vs ground and naval fire) and air defense.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseValues {
    /// `None` for aircraft: they cannot be attacked by ground units.
    pub ground: Option<f64>,
    pub air: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitTag {
    BoatCrossing,
    Exploitation,
    OccupyHex,
    HeliTransfer,
    NightCombat,
    Bombard,
    BuildBridge,
    Sam,
    NoFireAfterMove,
    Intercept,
    Escort,
    StrikeInfrastructure,
    Sead,
    Blockade,
    ShoreBombard,
    Hq,
}

fn default_strips_entrenchment() -> u8 {
    1
}

fn default_entrench_per_turn() -> u32 {
    1
}

/// Unit class card (GDD section 13), level-1 technology values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitClass {
    pub id: String,
    pub target_class: TargetClass,
    pub cost_mpp: u32,
    /// Ground and naval units.
    #[serde(default)]
    pub ap: Option<f64>,
    /// Aircraft operating radius.
    #[serde(default)]
    pub radius_km: Option<f64>,
    pub strikes: u32,
    pub attack: AttackValues,
    pub defense: DefenseValues,
    #[serde(default)]
    pub range_km: Option<f64>,
    #[serde(default)]
    pub sam_radius_km: Option<f64>,
    #[serde(default)]
    pub heli_range_km: Option<f64>,
    pub exerts_zoc: bool,
    #[serde(default = "default_entrench_per_turn")]
    pub entrench_per_turn: u32,
    /// R-COMBAT-02 (П1, `attack_strips_entrenchment`): entrenchment levels
    /// this class's attack or air strike strips from its target (SC п. 11).
    #[serde(default = "default_strips_entrenchment")]
    pub strips_entrenchment: u8,
    #[serde(default)]
    pub tags: Vec<UnitTag>,
}

impl UnitClass {
    pub fn has_tag(&self, tag: UnitTag) -> bool {
        self.tags.contains(&tag)
    }

    fn validate(&self) -> Result<(), RuleError> {
        let is_air = self.target_class == TargetClass::Air;
        if is_air && self.radius_km.is_none() {
            return Err(RuleError::InvalidData(format!(
                "`{}`: aircraft need radius_km",
                self.id
            )));
        }
        if !is_air {
            let ap = self.ap.ok_or_else(|| {
                RuleError::InvalidData(format!("`{}`: non-air units need ap", self.id))
            })?;
            ap_to_halves(ap)?;
            if self.defense.ground.is_none() {
                return Err(RuleError::InvalidData(format!(
                    "`{}`: non-air units need ground defense",
                    self.id
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitFile {
    units: Vec<UnitClass>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitCatalog {
    units: Vec<UnitClass>,
}

impl UnitCatalog {
    pub fn from_json(json: &str) -> Result<Self, RuleError> {
        let file: UnitFile = parse(json)?;
        let mut seen = std::collections::BTreeSet::new();
        for unit in &file.units {
            unit.validate()?;
            if !seen.insert(unit.id.as_str()) {
                return Err(RuleError::InvalidData(format!(
                    "duplicate unit `{}`",
                    unit.id
                )));
            }
        }
        Ok(Self { units: file.units })
    }

    pub fn get(&self, id: &str) -> Result<&UnitClass, RuleError> {
        self.units
            .iter()
            .find(|u| u.id == id)
            .ok_or_else(|| RuleError::UnknownUnitClass(id.to_owned()))
    }

    pub fn iter(&self) -> impl Iterator<Item = &UnitClass> {
        self.units.iter()
    }
}

// ---------------------------------------------------------------- bundle

/// All static game data, validated together.
#[derive(Debug, Clone, PartialEq)]
pub struct GameData {
    pub rules: Rules,
    pub terrain: TerrainCatalog,
    pub units: UnitCatalog,
    pub tech: TechCatalog,
}

/// Raw JSON texts of the `data/*.json` files.
#[derive(Debug, Clone, Copy)]
pub struct GameDataJson<'a> {
    pub rules: &'a str,
    pub terrain: &'a str,
    pub units: &'a str,
    pub tech: &'a str,
}

impl GameData {
    pub fn from_json(json: GameDataJson<'_>) -> Result<Self, RuleError> {
        let units = UnitCatalog::from_json(json.units)?;
        let tech = TechCatalog::from_json(json.tech)?;
        tech.validate(&units)?;
        Ok(Self {
            rules: Rules::from_json(json.rules)?,
            terrain: TerrainCatalog::from_json(json.terrain)?,
            units,
            tech,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ap_halves() {
        assert_eq!(ap_to_halves(0.5), Ok(1));
        assert_eq!(ap_to_halves(3.0), Ok(6));
        assert_eq!(ap_to_halves(0.3), Err(RuleError::InvalidAp(0.3)));
        assert_eq!(ap_to_halves(-1.0), Err(RuleError::InvalidAp(-1.0)));
    }
}
