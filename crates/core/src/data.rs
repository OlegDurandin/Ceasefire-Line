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
pub struct AirDefenseRules {
    pub shot_fraction: f64,
    pub next_sam_mult: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyRules {
    pub max: u8,
    pub decay_ap: f64,
    pub bridge_penalty: u8,
    pub hq_bonus_radius_km: f64,
    pub hq_min_supply: u8,
    pub hq_bonus: u8,
    pub hq_bonus_high: u8,
    pub hq_high_rating: u8,
    /// R-SUP-03: supply at or below this gives −AP and no entrenchment growth.
    pub low_supply_max: u8,
    pub entrench_min_supply: u8,
    /// Supply inertia: a unit's supply drops at most this much per own phase.
    pub max_drop_per_phase: u8,
    /// R-SUP-03: attrition starts at this many consecutive phases at supply 0.
    pub attrition_after_phases: u8,
    pub attrition_steps: u8,
    /// R-SUP-01: captured source works at value / divisor (floor).
    pub captured_source_divisor: u8,
    pub captured_recovery_per_turn: u8,
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
    pub min_ap: f64,
    /// R-MOVE-02: readiness penalty for a boat crossing, until the next own phase.
    pub boat_crossing_readiness_pct: u8,
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

/// R-NM-01, R-NM-02.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NationalMoraleRules {
    pub unit_destroyed: u8,
    pub unit_destroyed_encircled: u8,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub combat: CombatRules,
    pub readiness: ReadinessRules,
    pub air_defense: AirDefenseRules,
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
        if self.national_morale.steps_per_loss == 0 {
            return Err(RuleError::InvalidData(
                "national_morale: steps_per_loss must be > 0".into(),
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerrainFile {
    road_ap: f64,
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
