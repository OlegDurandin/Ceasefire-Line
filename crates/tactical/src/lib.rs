//! Ceasefire Line — tactical battles of the mobile version (stage T1:
//! rules without graphics). Specification: `docs/tactical.md` (rules T-*).
//!
//! Built on the strategic core (T-TECH-01): its hexes, PCG32, the hit
//! probability formula, binomial losses and the forecast. The battle is a
//! `Command → Events` state machine, deterministic by seed: the command log
//! replays it. Pure: no I/O, no clock, no global state.

pub mod battle;
pub mod map;
pub mod rules;
pub mod setup;

pub use battle::{Action, Battle, SupportCall, TacCommand, TacEvent, TacResult, TurnMode};
pub use rules::TacRules;
pub use setup::{build_battle, BattleInput, SideSupport, SupportInfo};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TacError {
    #[error("tactical data: {0}")]
    Data(String),
    #[error("illegal tactical command: {0}")]
    Illegal(String),
}

/// Replays a battle from its seed and command log (T-RES-04, T-TECH-01).
pub fn replay(
    rules: std::sync::Arc<TacRules>,
    data: &ceasefire_core::data::GameData,
    input: &BattleInput,
    seed: u64,
    mode: TurnMode,
    log: &[TacCommand],
) -> Result<Battle, TacError> {
    let mut b = build_battle(rules, data, input, seed, mode)?;
    for c in log {
        b.apply(c.clone())?;
    }
    Ok(b)
}

/// WASM bundle probe for the T1 size check (T-TECH-01): parses the embedded
/// tactical rules so the rules code stays in the binary.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn tactical_size_probe() -> i32 {
    match TacRules::from_json(include_str!("../../../data/tactical.json")) {
        Ok(r) => i32::try_from(r.units.len()).unwrap_or(0),
        Err(_) => -1,
    }
}
