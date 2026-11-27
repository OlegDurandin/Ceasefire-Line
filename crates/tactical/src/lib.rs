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

/// WASM bundle probe for the T1 size check (T-TECH-01): builds a battle
/// from an embedded snapshot with the game data and plays it to the end with
/// waits and passes, so the rules, setup and battle code stay in the binary.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn tactical_size_probe(seed: u64) -> i32 {
    use ceasefire_core::data::{GameData, GameDataJson};
    let Ok(rules) = TacRules::from_json(include_str!("../../../data/tactical.json")) else {
        return -1;
    };
    let Ok(data) = GameData::from_json(GameDataJson {
        rules: include_str!("../../../data/rules.json"),
        terrain: include_str!("../../../data/terrain.json"),
        units: include_str!("../../../data/units.json"),
        tech: include_str!("../../../data/tech.json"),
    }) else {
        return -2;
    };
    let Ok(input) =
        serde_json::from_str::<BattleInput>(include_str!("../tests/fixtures/battle.json"))
    else {
        return -3;
    };
    let Ok(mut b) = build_battle(
        std::sync::Arc::new(rules),
        &data,
        &input,
        seed,
        TurnMode::Alternating,
    ) else {
        return -4;
    };
    for _ in 0..500 {
        if b.result.is_some() {
            break;
        }
        let unit = b.alive(b.active).find(|u| !u.activated).map(|u| u.id);
        let cmd = match unit {
            Some(unit) => TacCommand::Act {
                unit,
                support: None,
                action: Action::Wait,
            },
            None => TacCommand::Pass,
        };
        if b.apply(cmd).is_err() {
            return -5;
        }
    }
    b.result.map_or(-6, |r| r.margin)
}
