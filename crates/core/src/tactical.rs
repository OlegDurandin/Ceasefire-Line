//! Tactical battles (GDD «Тактические бои»): the strategic core declares a
//! combat, exports it as a snapshot (the same format as a standalone mobile
//! scenario) and takes the result back as a point in the forecast
//! distribution. The tactical rules themselves are stage 7.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::combat::ExchangeForecast;
use crate::map::BridgeState;
use crate::tech::TechLevels;

/// Result of a tactical battle, from the attacker's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalOutcome {
    AttackerDecisive,
    AttackerVictory,
    Draw,
    AttackerDefeat,
    AttackerRout,
}

impl TacticalOutcome {
    pub const ALL: [TacticalOutcome; 5] = [
        TacticalOutcome::AttackerDecisive,
        TacticalOutcome::AttackerVictory,
        TacticalOutcome::Draw,
        TacticalOutcome::AttackerDefeat,
        TacticalOutcome::AttackerRout,
    ];
}

/// Where a combat result came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatSource {
    /// Dice on the strategic map (R-COMBAT-02).
    Auto,
    /// Played on the tactical level; losses at the outcome's percentile.
    Tactical { outcome: TacticalOutcome },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Attacker,
    Defender,
}

/// Where a tactical scenario comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// Exported from a strategic game; the result goes back to this combat.
    Campaign { combat: u32, turn: u32 },
    /// A standalone mobile scenario.
    Standalone { id: String },
}

/// One hex of the battle area, relative axial coordinates (defender = 0, 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaHex {
    pub dq: i32,
    pub dr: i32,
    pub terrain: String,
    pub pass: bool,
    pub owner: Option<u8>,
}

/// A hex side inside the area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaEdge {
    pub a: [i32; 2],
    pub b: [i32; 2],
    pub road: bool,
    pub canal: bool,
    pub bridge: Option<BridgeState>,
}

/// What a side brings to the battle beyond its brigade (T-SUP-01/02).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TacticalSupport {
    /// Unit supply of the best own artillery whose range reaches the battle hex.
    #[serde(default)]
    pub artillery_supply: Option<u8>,
    /// Own strike aircraft able to reach the hex now (not flown, not rebased).
    #[serde(default)]
    pub air_sorties: u8,
    /// Supply of the best such aircraft's base, and its readiness.
    #[serde(default)]
    pub air_supply: Option<u8>,
    #[serde(default)]
    pub air_readiness_pct: u8,
    /// An enemy SAM umbrella covers the battle hex (R-AIR-03).
    #[serde(default)]
    pub enemy_sam: bool,
    /// Rating of the brigade's HQ (R-HQ-01).
    #[serde(default)]
    pub hq_rating: Option<u8>,
    /// Classes of own combat brigades in hexes adjacent to the battle hex (reserves).
    #[serde(default)]
    pub reserves: Vec<String>,
    /// Own engineers within 2 hexes.
    #[serde(default)]
    pub engineers: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacticalSide {
    pub side: u8,
    pub role: Role,
    pub tech: TechLevels,
    /// T-SUP-01/02 (added 2026-11-27; absent in older snapshots).
    #[serde(default)]
    pub support: TacticalSupport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacticalUnit {
    pub side: u8,
    pub role: Role,
    pub class: String,
    pub nation: Option<String>,
    pub strength: u8,
    pub readiness_pct: u8,
    pub morale_pct: u8,
    /// Hundredths of a star.
    pub exp: u16,
    pub entrenchment: u8,
    pub dq: i32,
    pub dr: i32,
}

/// The battle snapshot: a self-contained mini-scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacticalScenario {
    pub format: u32,
    pub origin: Origin,
    pub night: bool,
    pub hex_km: f64,
    pub across_canal: bool,
    pub area: Vec<AreaHex>,
    pub edges: Vec<AreaEdge>,
    pub sides: Vec<TacticalSide>,
    pub units: Vec<TacticalUnit>,
    /// The stakes: the strategic forecast this battle moves within.
    pub forecast: ExchangeForecast,
    pub outcome_percentiles: BTreeMap<TacticalOutcome, f64>,
}

impl TacticalScenario {
    pub fn to_json(&self) -> Result<String, crate::RuleError> {
        serde_json::to_string(self).map_err(|e| crate::RuleError::Parse(e.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Self, crate::RuleError> {
        serde_json::from_str(json).map_err(|e| crate::RuleError::Parse(e.to_string()))
    }
}
