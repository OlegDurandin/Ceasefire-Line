//! `data/tactical.json`: every number of the tactical rules (T-*).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::TacError;

#[derive(Debug, Clone, Deserialize)]
pub struct MapRules {
    pub cols: i32,
    pub rows: i32,
    pub key_points_min: usize,
    pub key_points_max: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TurnRules {
    pub campaign: u32,
    pub historical: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LosRules {
    pub day: u32,
    pub night: u32,
    pub recon_bonus: u32,
    pub hill_bonus: u32,
    pub concealed_day: u32,
    pub concealed_recon: u32,
    pub illumination_radius: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CombatRules {
    pub p_base: f64,
    pub p_slope: f64,
    pub p_min: f64,
    pub p_max: f64,
    pub shots_per_step: f64,
    pub damage_per_step: u32,
    pub exp_bonus: f64,
    pub entrench_bonus: f64,
    /// By distance: index 0 unused, 1 adjacent, 2, 3.
    pub range_mult: Vec<f64>,
    pub at_range_mult: f64,
    pub move_fire_mult: f64,
    pub ambush_mult: f64,
    pub night_mult: f64,
    pub flank_mult: f64,
    pub suppressed_fire_mult: f64,
    pub tank_assault_cover_mult: f64,
    pub combined_arms_mult: f64,
    pub forecast_low: f64,
    pub forecast_high: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntrenchRules {
    pub per_turn_still: u8,
    pub dig: u8,
    pub dig_infantry: u8,
    pub max: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TerrainDef {
    pub id: String,
    pub move_cost: u32,
    pub defense: f64,
    pub level: u8,
    pub blocks: bool,
    pub cover: bool,
    pub hidden_but_adjacent: bool,
    pub start_entrench: u8,
}

#[derive(Debug, Clone, Deserialize)]
struct TerrainFile {
    id: String,
    #[serde(rename = "move")]
    move_cost: u32,
    defense: f64,
    level: u8,
    blocks: bool,
    cover: bool,
    hidden_but_adjacent: bool,
    start_entrench: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MineRules {
    pub loss_chance: f64,
    pub loss_steps: u8,
    pub fields_min: usize,
    pub fields_max: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitKind {
    pub kind: String,
    /// The strategic class its attack and defense come from (with technology).
    pub class: String,
    #[serde(rename = "move")]
    pub move_points: u32,
    pub range: u32,
    pub tags: Vec<String>,
}

impl UnitKind {
    pub fn has(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtraIf {
    pub level: u8,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Breakdown {
    pub class: String,
    pub battalions: Vec<String>,
    pub extra: Vec<String>,
    #[serde(default)]
    pub extra_if_antitank: Option<ExtraIf>,
    pub support: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReserveRules {
    pub arrival_turn: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SupportBand {
    pub min_supply: u8,
    pub artillery: u8,
    pub air: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Salvo {
    pub shots_center: u32,
    pub shots_neighbour: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtilleryRules {
    pub shots_center: u32,
    pub shots_neighbour: u32,
    pub unseen_mult: f64,
    pub entrench_loss: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AirRules {
    pub shots: u32,
    pub sam_abort_chance: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SupportRules {
    pub bands: Vec<SupportBand>,
    pub hq_bonus_rating: u8,
    pub air_min_readiness_pct: u8,
    pub artillery: ArtilleryRules,
    pub mortar: Salvo,
    pub air: AirRules,
    pub smoke_turns: u32,
    pub smoke_radius: u32,
}

impl SupportRules {
    /// T-SUP-02: calls by the source's supply.
    pub fn band(&self, supply: u8) -> (u8, u8) {
        self.bands
            .iter()
            .find(|b| supply >= b.min_supply)
            .map_or((0, 0), |b| (b.artillery, b.air))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Thresholds {
    pub decisive: i32,
    pub victory: i32,
    pub defeat: i32,
    pub rout: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultRules {
    pub key_point: i32,
    pub step: i32,
    pub thresholds: Thresholds,
    pub percentiles: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiRules {
    pub jitter: f64,
    pub fire_weight: f64,
    pub key_point_weight: f64,
    pub cover_weight: f64,
    pub exposure_weight: f64,
    pub smoke_min_threat: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct RulesFile {
    map: MapRules,
    turns: TurnRules,
    los: LosRules,
    combat: CombatRules,
    entrench: EntrenchRules,
    terrain: Vec<TerrainFile>,
    mines: MineRules,
    units: Vec<UnitKind>,
    breakdown: Vec<Breakdown>,
    extra_steps: u8,
    reserve: ReserveRules,
    support: SupportRules,
    result: ResultRules,
    ai: AiRules,
    terrain_mix: serde_json::Value,
}

/// The tactical rules, parsed.
#[derive(Debug, Clone)]
pub struct TacRules {
    pub map: MapRules,
    pub turns: TurnRules,
    pub los: LosRules,
    pub combat: CombatRules,
    pub entrench: EntrenchRules,
    pub terrain: BTreeMap<String, TerrainDef>,
    pub mines: MineRules,
    pub units: BTreeMap<String, UnitKind>,
    pub breakdown: BTreeMap<String, Breakdown>,
    pub extra_steps: u8,
    pub reserve: ReserveRules,
    pub support: SupportRules,
    pub result: ResultRules,
    pub ai: AiRules,
    /// Strategic terrain → (tactical terrain, weight).
    pub terrain_mix: BTreeMap<String, Vec<(String, u32)>>,
    /// Strategic terrain → hill levels (min, max).
    pub hill_levels: BTreeMap<String, (u8, u8)>,
}

impl TacRules {
    pub fn from_json(json: &str) -> Result<Self, TacError> {
        let f: RulesFile = serde_json::from_str(json).map_err(|e| TacError::Data(e.to_string()))?;
        let mut terrain_mix = BTreeMap::new();
        let mut hill_levels = BTreeMap::new();
        if let Some(obj) = f.terrain_mix.as_object() {
            for (k, v) in obj {
                if k == "hill_levels" {
                    for (t, lv) in v.as_object().into_iter().flatten() {
                        let a = lv[0].as_u64().unwrap_or(1) as u8;
                        let b = lv[1].as_u64().unwrap_or(1) as u8;
                        hill_levels.insert(t.clone(), (a, b));
                    }
                    continue;
                }
                let Some(list) = v.as_array() else { continue };
                let mix: Vec<(String, u32)> = list
                    .iter()
                    .filter_map(|x| Some((x[0].as_str()?.to_owned(), x[1].as_u64()? as u32)))
                    .collect();
                terrain_mix.insert(k.clone(), mix);
            }
        }
        let rules = Self {
            map: f.map,
            turns: f.turns,
            los: f.los,
            combat: f.combat,
            entrench: f.entrench,
            terrain: f
                .terrain
                .into_iter()
                .map(|t| {
                    (
                        t.id.clone(),
                        TerrainDef {
                            id: t.id,
                            move_cost: t.move_cost,
                            defense: t.defense,
                            level: t.level,
                            blocks: t.blocks,
                            cover: t.cover,
                            hidden_but_adjacent: t.hidden_but_adjacent,
                            start_entrench: t.start_entrench,
                        },
                    )
                })
                .collect(),
            mines: f.mines,
            units: f.units.into_iter().map(|u| (u.kind.clone(), u)).collect(),
            breakdown: f
                .breakdown
                .into_iter()
                .map(|b| (b.class.clone(), b))
                .collect(),
            extra_steps: f.extra_steps,
            reserve: f.reserve,
            support: f.support,
            result: f.result,
            ai: f.ai,
            terrain_mix,
            hill_levels,
        };
        if rules.combat.range_mult.len() < 4 {
            return Err(TacError::Data("combat.range_mult needs 4 entries".into()));
        }
        Ok(rules)
    }

    pub fn kind(&self, kind: &str) -> Result<&UnitKind, TacError> {
        self.units
            .get(kind)
            .ok_or_else(|| TacError::Data(format!("unknown unit kind `{kind}`")))
    }

    pub fn terrain(&self, id: &str) -> Result<&TerrainDef, TacError> {
        self.terrain
            .get(id)
            .ok_or_else(|| TacError::Data(format!("unknown terrain `{id}`")))
    }
}
