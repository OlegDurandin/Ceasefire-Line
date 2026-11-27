//! `sim ai-vs-ai --dump-battles DIR`: every ground attack of the games as a
//! tactical battle input — the core's R-TAC-03 snapshot, which carries the
//! support of both sides (T-SUP-01/02) — for `sim tactical`.

use std::fs;
use std::path::Path;

use ceasefire_core::game::{Command, Event, GameState};
use ceasefire_tactical::BattleInput;

/// If `cmd` is a ground attack, the battle input it would give (declared on
/// a copy of the state; the game itself is not touched).
pub fn battle_of(s: &GameState, cmd: &Command) -> Option<BattleInput> {
    let Command::Attack { unit, target } = cmd else {
        return None;
    };
    let mut trial = s.clone();
    let events = trial
        .apply(Command::DeclareAttack {
            unit: *unit,
            target: *target,
        })
        .ok()?;
    events.into_iter().find_map(|e| match e {
        Event::CombatDeclared { snapshot, .. } => Some(BattleInput {
            scenario: *snapshot,
        }),
        _ => None,
    })
}

pub fn write(dir: &Path, name: &str, input: &BattleInput) -> Result<(), String> {
    let path = dir.join(format!("{name}.json"));
    let text = serde_json::to_string(input).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}
