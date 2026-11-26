//! Weights of the AI (`data/ai.json`) and the scenario's hints
//! (`scenario.json` → `ai`). Both are data, so tuning needs no rebuild.

use std::collections::BTreeMap;

use ceasefire_core::hex::Hex;
use ceasefire_core::scenario::offset_to_axial;
use ceasefire_core::side::SideId;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AttackWeights {
    /// Minimum attack score (in the attacker's MPP per step) at aggressiveness 0 and 1.
    pub min_score_defensive: f64,
    pub min_score_aggressive: f64,
    /// Bonus (in the defender's MPP per step) when the defender is likely to retreat.
    pub retreat_bonus: f64,
    /// Bonus (in the attacker's MPP per step) for hitting a unit on an objective.
    pub objective_bonus: f64,
    /// Bonus (in the attacker's MPP per step) for a defender already attacked this phase.
    pub focus_bonus: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdvanceWeights {
    pub max_adjacent_enemies: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoveWeights {
    pub goal_weight: f64,
    pub terrain_weight: f64,
    pub entrench_weight: f64,
    pub threat_weight: f64,
    pub support_weight: f64,
    pub objective_bonus: f64,
    pub contact_bonus: f64,
    pub enemy_hex_bonus: f64,
    pub min_gain: f64,
    pub influence_radius: u32,
    pub defend_radius: u32,
    /// Aggressiveness from which a side attacks objectives instead of holding.
    pub attack_from_aggressiveness: f64,
}

/// R-AI-01 «поиск котлов».
#[derive(Debug, Clone, Deserialize)]
pub struct PocketWeights {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    /// Pocket operations on (stage 4 step 7, off until tuned: the AI keeps
    /// its step 6 behaviour; the dependency map still guides tanks).
    pub enabled: bool,
    /// K: chokepoint candidates per phase.
    pub candidates: usize,
    /// At most this many chokepoints in one pocket (pincers).
    pub max_pincers: usize,
    /// Phases of supply inertia (R-SUP-05) the trial recount looks ahead.
    pub lookahead_phases: u8,
    /// A unit counts as cut off at this unit supply or less.
    pub cut_supply_max: u8,
    /// × for units whose supply drops to 0.
    pub zero_supply_mult: f64,
    /// At most this many pocket operations per phase.
    pub max_operations: usize,
    /// A pocket worth less than this V is not an operation.
    pub min_value: f64,
    /// Reachability of a chokepoint: this phase / within two phases.
    pub reach_now: f64,
    pub reach_two_phases: f64,
    /// VP per unit of enemy threat at a chokepoint (expected own losses), subtracted.
    pub loss_weight: f64,
    /// «Сковывание»: infantry and artillery within this many hexes of a cut unit keep contact.
    pub pin_radius: u32,
    /// Tanks and mechs: hex score + this × D(h) / max D.
    pub dependency_weight: f64,
    /// «Добивание»: attack bonus (attacker's MPP per step) on targets at supply 0.
    pub finish_bonus: f64,
    /// No move may end where own supply in a phase drops below this, except on a chokepoint.
    pub min_own_supply: u8,
    /// R-AI-01 common currency: an operation is taken at this expected VP gain or more.
    pub min_vp: f64,
    /// «Добивание» (with R-COMBAT-05 on): steps our attacks take per phase from a unit at supply 0.
    pub finish_steps_per_phase: f64,
    /// Enemy phases counted when the scenario has no last turn.
    pub horizon_phases: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EconomyWeights {
    pub reinforce_at_or_below: u8,
    pub boost_mpp_reserve: u32,
}

/// `data/ai.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct AiParams {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    pub default_aggressiveness: f64,
    /// ± seeded spread of each side's aggressiveness per game: the AI does not
    /// play the same plan every game (designer decision 2026-11-26).
    pub aggressiveness_jitter: f64,
    pub max_commands_per_phase: u32,
    pub attack: AttackWeights,
    pub advance: AdvanceWeights,
    #[serde(rename = "move")]
    pub moves: MoveWeights,
    pub economy: EconomyWeights,
    pub pockets: PocketWeights,
}

impl AiParams {
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("ai.json: {e}"))
    }
}

#[derive(Debug, Deserialize)]
struct PriorityFile {
    hex: [i32; 2],
    weight: f64,
}

#[derive(Debug, Deserialize)]
struct SideHintFile {
    side: u8,
    #[serde(default)]
    aggressiveness: Option<f64>,
    #[serde(default)]
    priorities: Vec<PriorityFile>,
    /// Objectives in the order the side takes them (operation «следующая цель»).
    #[serde(default)]
    sequence: Vec<[i32; 2]>,
}

#[derive(Debug, Deserialize)]
struct HintsFile {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    #[serde(default)]
    sides: Vec<SideHintFile>,
}

/// One side's hints: how readily it attacks and which hexes matter most.
#[derive(Debug, Clone, Default)]
pub struct SideHints {
    pub aggressiveness: Option<f64>,
    /// Extra weight of a hex as a goal (objectives default to their VP).
    pub priorities: BTreeMap<Hex, f64>,
    /// R-AI-01 operation «следующая цель»: objectives in order; the attack
    /// goes to the first one the side does not hold, then on to the next.
    pub sequence: Vec<Hex>,
}

/// `scenario.json` → `ai` (R-AI-01 «подсказки сценария»).
#[derive(Debug, Clone, Default)]
pub struct AiHints {
    pub sides: BTreeMap<SideId, SideHints>,
}

impl AiHints {
    /// Hints from the scenario text; a scenario without `ai` has none.
    pub fn from_scenario_json(json: &str) -> Result<Self, String> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("scenario.json: {e}"))?;
        let Some(ai) = v.get("ai") else {
            return Ok(Self::default());
        };
        let file: HintsFile =
            serde_json::from_value(ai.clone()).map_err(|e| format!("scenario.json ai: {e}"))?;
        let sides = file
            .sides
            .into_iter()
            .map(|s| {
                let hints = SideHints {
                    aggressiveness: s.aggressiveness,
                    priorities: s
                        .priorities
                        .into_iter()
                        .map(|p| (offset_to_axial(p.hex), p.weight))
                        .collect(),
                    sequence: s.sequence.into_iter().map(offset_to_axial).collect(),
                };
                (SideId(s.side), hints)
            })
            .collect();
        Ok(Self { sides })
    }

    pub fn aggressiveness(&self, side: SideId, default: f64) -> f64 {
        self.sides
            .get(&side)
            .and_then(|s| s.aggressiveness)
            .unwrap_or(default)
            .clamp(0.0, 1.0)
    }
}
