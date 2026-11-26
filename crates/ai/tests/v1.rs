//! Stage 4, step 2: AI v1 on «Голаны-67» — legal, deterministic, and better
//! than the baseline in each seat (mirrored series: v1 scores ≥ 60%, a draw
//! counts half; GDD section 17 counting).

use std::sync::Arc;

use ceasefire_ai::{greedy_next, play_phase, AiHints, AiParams, AiV1};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{Command, GameState};
use ceasefire_core::scenario::load_scenario;
use ceasefire_core::side::SideId;

const SCENARIO: &str = include_str!("../../../data/scenarios/golan67/scenario.json");
const MAP: &str = include_str!("../../../data/scenarios/golan67/map.json");
const EVENTS: &str = include_str!("../../../data/scenarios/golan67/events.json");
const AI: &str = include_str!("../../../data/ai.json");

fn data() -> Arc<GameData> {
    Arc::new(
        GameData::from_json(GameDataJson {
            rules: include_str!("../../../data/rules.json"),
            terrain: include_str!("../../../data/terrain.json"),
            units: include_str!("../../../data/units.json"),
            tech: include_str!("../../../data/tech.json"),
        })
        .expect("data/*.json must load"),
    )
}

/// One game; `v1_side` plays AI v1, the other side the baseline (`None`: v1 both).
fn game(
    data: &Arc<GameData>,
    seed: u64,
    v1_side: Option<SideId>,
) -> (Option<SideId>, Vec<Command>, u32) {
    let mut setup = load_scenario(SCENARIO, MAP, Some(EVENTS)).unwrap();
    setup.seed = seed;
    let (mut s, _) = GameState::new(Arc::clone(data), setup).unwrap();
    let mut ai = AiV1::new(
        AiParams::from_json(AI).unwrap(),
        AiHints::from_scenario_json(SCENARIO).unwrap(),
        seed,
    );
    let (mut log, mut refused) = (Vec::new(), 0);
    for _ in 0..40 {
        if s.outcome().is_some() {
            break;
        }
        let v1 = v1_side.is_none_or(|side| side == s.active_side());
        let r = if v1 {
            let max = ai.max_commands();
            play_phase(&mut s, |st| ai.next(st), max)
        } else {
            play_phase(&mut s, greedy_next, 400)
        }
        .unwrap();
        refused += r.refused;
        log.extend(r.commands);
    }
    (
        s.outcome().expect("the date ends the game").winner,
        log,
        refused,
    )
}

#[test]
fn v1_is_legal_and_deterministic() {
    let d = data();
    let (w1, log1, refused) = game(&d, 11, None);
    let (w2, log2, _) = game(&d, 11, None);
    assert_eq!(refused, 0);
    assert_eq!(log1, log2);
    assert_eq!(w1, w2);
}

#[test]
fn v1_beats_the_baseline_in_a_mirrored_series() {
    let d = data();
    let games = 60u64;
    let mut score = 0.0;
    for seed in 0..games {
        for side in [SideId(0), SideId(1)] {
            let (winner, _, _) = game(&d, 1000 + seed, Some(side));
            score += match winner {
                Some(w) if w == side => 1.0,
                None => 0.5,
                Some(_) => 0.0,
            };
        }
    }
    let share = score / (2 * games) as f64;
    assert!(share >= 0.60, "v1 scored {share:.2} against the baseline");
}

/// Stage 4, step 4d: with aircraft added to «Голаны-67», AI v1 flies strikes
/// (before ground attacks) and every command stays legal.
#[test]
fn v1_flies_air_strikes() {
    use ceasefire_core::game::{MapEdge, OffMapBase, UnitSetup};
    use ceasefire_core::scenario::offset_to_axial;
    let d = data();
    let mut setup = load_scenario(SCENARIO, MAP, Some(EVENTS)).unwrap();
    let base = offset_to_axial([1, 10]);
    setup.map.add_airfield(base);
    setup.offmap_bases = vec![OffMapBase {
        id: "syr".into(),
        side: SideId(1),
        edge: MapEdge::East,
        distance_km: 30.0,
    }];
    let plane = |class: &str, side: u8, offmap: Option<&str>| UnitSetup {
        class: class.into(),
        side: SideId(side),
        hex: base,
        strength: 10,
        morale_pct: 85,
        exp: 0,
        entrenchment: 0,
        supply: None,
        hq_rating: None,
        formation: None,
        nation: None,
        name: None,
        offmap_base: offmap.map(Into::into),
    };
    setup.units.push(plane("strike", 0, None));
    setup.units.push(plane("fighter", 1, Some("syr")));
    let (mut s, _) = GameState::new(Arc::clone(&d), setup).unwrap();
    let mut ai = AiV1::new(
        AiParams::from_json(AI).unwrap(),
        AiHints::from_scenario_json(SCENARIO).unwrap(),
        1,
    );
    let (mut strikes, mut refused) = (0, 0);
    for _ in 0..40 {
        if s.outcome().is_some() {
            break;
        }
        let max = ai.max_commands();
        let r = play_phase(&mut s, |st| ai.next(st), max).unwrap();
        refused += r.refused;
        strikes += r
            .commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    Command::AirStrike { .. } | Command::AirStrikeAirfield { .. }
                )
            })
            .count();
    }
    assert_eq!(refused, 0);
    assert!(strikes > 0, "the AI used its aircraft");
}

/// Stage 4, step 6: AI v1 plays «Синай-67» (48 brigades and HQs, aircraft)
/// for both sides to the end; every command legal.
#[test]
fn v1_plays_sinai67_to_the_end() {
    let scenario = include_str!("../../../data/scenarios/sinai67/scenario.json");
    let setup = load_scenario(
        scenario,
        include_str!("../../../data/scenarios/sinai67/map.json"),
        Some(include_str!("../../../data/scenarios/sinai67/events.json")),
    )
    .unwrap();
    let (mut s, _) = GameState::new(data(), setup).unwrap();
    let mut ai = AiV1::new(
        AiParams::from_json(AI).unwrap(),
        AiHints::from_scenario_json(scenario).unwrap(),
        3,
    );
    let mut refused = 0;
    for _ in 0..40 {
        if s.outcome().is_some() {
            break;
        }
        let max = ai.max_commands();
        refused += play_phase(&mut s, |st| ai.next(st), max).unwrap().refused;
    }
    assert!(s.outcome().is_some());
    assert_eq!(refused, 0);
}
