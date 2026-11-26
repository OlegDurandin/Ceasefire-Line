//! Stage 4, step 1: the baseline player plays «Голаны-67» for both sides to
//! the end, every command accepted by the core, the same game for the same seed.

use std::sync::Arc;

use ceasefire_ai::{greedy_next, play_phase};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{Command, GameState};
use ceasefire_core::scenario::load_scenario;

fn data() -> GameData {
    GameData::from_json(GameDataJson {
        rules: include_str!("../../../data/rules.json"),
        terrain: include_str!("../../../data/terrain.json"),
        units: include_str!("../../../data/units.json"),
        tech: include_str!("../../../data/tech.json"),
    })
    .expect("data/*.json must load")
}

fn play(seed: u64) -> (GameState, Vec<Command>, u32) {
    let mut setup = load_scenario(
        include_str!("../../../data/scenarios/golan67/scenario.json"),
        include_str!("../../../data/scenarios/golan67/map.json"),
        Some(include_str!("../../../data/scenarios/golan67/events.json")),
    )
    .expect("scenario parses");
    setup.seed = seed;
    let (mut s, _) = GameState::new(Arc::new(data()), setup).expect("game starts");
    let (mut log, mut refused) = (Vec::new(), 0);
    for _ in 0..40 {
        if s.outcome().is_some() {
            break;
        }
        let r = play_phase(&mut s, greedy_next, 200).expect("phase plays");
        refused += r.refused;
        log.extend(r.commands);
    }
    (s, log, refused)
}

#[test]
fn greedy_plays_golan67_to_the_end() {
    let (s, log, refused) = play(1967);
    assert!(s.outcome().is_some(), "the date ends the game");
    assert_eq!(refused, 0, "every command the AI gives is legal");
    assert!(log.iter().any(|c| matches!(c, Command::Attack { .. })));
    assert!(log.iter().any(|c| matches!(c, Command::Move { .. })));
}

#[test]
fn greedy_is_deterministic() {
    let (a, log_a, _) = play(7);
    let (b, log_b, _) = play(7);
    assert_eq!(log_a, log_b);
    assert_eq!(a.outcome(), b.outcome());
}
