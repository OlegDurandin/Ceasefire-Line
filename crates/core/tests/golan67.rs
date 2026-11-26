//! Scenario loading + 4 turns of random legal commands on «Голаны-67»
//! (data/scenarios/golan67): no rule errors, deterministic by seed.

mod common;

use std::sync::Arc;

use ceasefire_core::game::{Command, EndReason, Event, GameState, UnitId};
use ceasefire_core::rng::Pcg32;
use ceasefire_core::scenario::{load_scenario, validate, ScenarioIssue};

const SCENARIO: &str = include_str!("../../../data/scenarios/golan67/scenario.json");
const MAP: &str = include_str!("../../../data/scenarios/golan67/map.json");
const EVENTS: &str = include_str!("../../../data/scenarios/golan67/events.json");
const LOCALE_RU: &str = include_str!("../../../data/locale/ru.json");

fn load() -> (
    Arc<ceasefire_core::data::GameData>,
    ceasefire_core::game::ScenarioSetup,
) {
    let data = Arc::new(common::data());
    (
        data,
        load_scenario(SCENARIO, MAP, Some(EVENTS)).expect("scenario parses"),
    )
}

#[test]
fn golan67_loads_and_validates() {
    let (data, setup) = load();
    assert_eq!(setup.hex_km, Some(5.0));
    assert_eq!(setup.map.hexes().count(), 22 * 18);
    assert_eq!(validate(&data, &setup), vec![]);
    assert_eq!(setup.events.len(), 3);
    assert_eq!(setup.tech_boosts.len(), 2);
    // Every message key used by the scenario has a Russian string.
    let ru: std::collections::BTreeMap<String, String> = serde_json::from_str(LOCALE_RU).unwrap();
    for e in &setup.events {
        for effect in &e.effects {
            if let ceasefire_core::game::Effect::Message { key } = effect {
                assert!(ru.contains_key(key), "missing ru string for {key}");
            }
        }
    }
    let (_, events) = GameState::new(data, setup).unwrap();
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::SetupWarning { .. })));
}

#[test]
fn validation_reports_scenario_problems() {
    let (data, mut setup) = load();
    // Syrian HQ removed, a unit stacked on another, one on Hermon without a pass.
    setup.units.retain(|u| !(u.class == "hq" && u.side.0 == 1));
    let first = setup.units[0].hex;
    setup.units[1].hex = first;
    setup.units[2].hex = ceasefire_core::scenario::offset_to_axial([7, 0]);
    let issues = validate(&data, &setup);
    assert!(
        issues.contains(&ScenarioIssue::NoHq { side: 1 }),
        "{issues:?}"
    );
    assert!(issues.contains(&ScenarioIssue::LayerClash {
        hex: ceasefire_core::scenario::axial_to_offset(first),
        layer: "ground",
    }));
    assert!(issues.contains(&ScenarioIssue::Impassable {
        unit: 2,
        hex: [7, 0]
    }));
}

#[test]
fn validation_reports_event_boost_and_water_problems() {
    let (data, mut setup) = load();
    // R-EVT-01: duplicate id, unknown class, unknown side, off-map hex.
    setup.events[0].id = setup.events[1].id.clone();
    if let ceasefire_core::game::Effect::Reinforcements { units } = &mut setup.events[0].effects[0]
    {
        units[0].class = "battleship".into();
        units[1].hex = ceasefire_core::scenario::offset_to_axial([40, 40]);
    }
    setup.events[1].phase_of = Some(ceasefire_core::side::SideId(7));
    // R-TECH-02: duplicate boost id.
    let dup = setup.tech_boosts[0].clone();
    setup.tech_boosts.push(dup);
    // R-MOVE-01: a controlled water hex.
    let lake = ceasefire_core::scenario::offset_to_axial([10, 10]);
    setup
        .map
        .insert(lake, ceasefire_core::map::HexCell::new("water"));

    let issues = validate(&data, &setup);
    let ev = "kuneitra_falls".to_owned();
    for expected in [
        ScenarioIssue::DuplicateEvent { id: ev.clone() },
        ScenarioIssue::EventUnknownClass {
            event: ev.clone(),
            class: "battleship".into(),
        },
        ScenarioIssue::EventHexOffMap {
            event: ev.clone(),
            hex: [40, 40],
        },
        ScenarioIssue::EventUnknownSide { event: ev, side: 7 },
        ScenarioIssue::DuplicateBoost {
            id: "isr_emergency_atgm".into(),
        },
        ScenarioIssue::ControlledWater { hex: [10, 10] },
    ] {
        assert!(
            issues.contains(&expected),
            "missing {expected:?} in {issues:?}"
        );
    }
}

/// Plays until the game ends, choosing only commands the core's own queries
/// call legal; any rule error is a bug.
fn play_logged(seed: u64) -> (GameState, Vec<Event>, Vec<Command>) {
    let (data, setup) = load();
    let (mut s, mut events) = GameState::new(data, setup).unwrap();
    let mut rng = Pcg32::new(seed, 99);
    let log = std::cell::RefCell::new(Vec::new());
    let apply = |s: &mut GameState, c: Command, events: &mut Vec<Event>| {
        let out = s
            .apply(c.clone())
            .unwrap_or_else(|e| panic!("{c:?} rejected: {e}"));
        log.borrow_mut().push(c);
        events.extend(out);
    };
    while s.outcome().is_none() {
        let side = s.active_side();
        let ids: Vec<UnitId> = s.units().filter(|u| u.side == side).map(|u| u.id).collect();
        for id in ids {
            let Some(unit) = s.unit(id).cloned() else {
                continue;
            };
            match rng.next_u32() % 4 {
                0 => {
                    let targets: Vec<UnitId> =
                        s.units().filter(|t| t.side != side).map(|t| t.id).collect();
                    if let Some(&t) = targets.iter().find(|&&t| s.forecast_attack(id, t).is_ok()) {
                        apply(
                            &mut s,
                            Command::Attack {
                                unit: id,
                                target: t,
                            },
                            &mut events,
                        );
                        if s.pending_advance().is_some_and(|p| p.unit == id) && rng.chance(0.5) {
                            apply(&mut s, Command::Advance { unit: id }, &mut events);
                        }
                    } else if let Some(&t) =
                        targets.iter().find(|&&t| s.forecast_bombard(id, t).is_ok())
                    {
                        apply(
                            &mut s,
                            Command::Bombard {
                                unit: id,
                                target: t,
                            },
                            &mut events,
                        );
                    }
                }
                1 | 2 => {
                    let reach: Vec<_> = s.reachable(id).unwrap().into_keys().collect();
                    if !reach.is_empty() {
                        let to = reach[rng.next_u32() as usize % reach.len()];
                        let path = s
                            .path_to(id, to)
                            .unwrap()
                            .expect("reachable hex has a path");
                        apply(&mut s, Command::Move { unit: id, path }, &mut events);
                    }
                }
                _ => {
                    let mpp = s.side(side).unwrap().mpp;
                    if unit.strength < 10 && s.reinforce_cost(id, 1, false).is_ok_and(|c| c <= mpp)
                    {
                        apply(
                            &mut s,
                            Command::Reinforce {
                                unit: id,
                                steps: 1,
                                elite: false,
                            },
                            &mut events,
                        );
                    }
                }
            }
        }
        let boosts: Vec<String> = s
            .tech_boosts()
            .iter()
            .filter(|b| b.side == side)
            .map(|b| b.id.clone())
            .collect();
        for id in boosts {
            if rng.chance(0.3) {
                // Buying may be refused (MPP, already bought); that must be atomic.
                let before = s.clone();
                let c = Command::BuyTechBoost { id };
                match s.apply(c.clone()) {
                    Ok(_) => log.borrow_mut().push(c),
                    Err(_) => assert_eq!(s, before, "refused command changed the state"),
                }
            }
        }
        apply(&mut s, Command::EndPhase, &mut events);
    }
    (s, events, log.into_inner())
}

fn play(seed: u64) -> (GameState, Vec<Event>) {
    let (s, events, _) = play_logged(seed);
    (s, events)
}

/// Saves (stage 3): the command log of a game replays it exactly.
#[test]
fn golan67_command_log_replays_the_game() {
    let (end, _, log) = play_logged(7);
    assert!(!log.is_empty());
    let (data, setup) = load();
    let (mut s, _) = GameState::new(data, setup).unwrap();
    for c in log {
        s.apply(c).unwrap();
    }
    assert_eq!(s, end, "same scenario, seed and commands → same game");
}

#[test]
fn golan67_four_turns_of_random_legal_play() {
    let mut fights_total = 0;
    for seed in 0..20 {
        let (s, events) = play(seed);
        let outcome = s.outcome().unwrap();
        if outcome.reason == EndReason::Date {
            assert_eq!(s.turn(), 4, "seed {seed}");
        }
        let fights = events
            .iter()
            .filter(|e| matches!(e, Event::CombatResolved { .. }))
            .count();
        let moves = events
            .iter()
            .filter(|e| matches!(e, Event::UnitMoved { .. }))
            .count();
        assert!(moves > 0, "seed {seed}: nothing moved");
        fights_total += fights;
    }
    assert!(fights_total > 0, "random play never reached a fight");
    eprintln!("combats over 20 games: {fights_total}");
}

#[test]
fn golan67_replay_is_deterministic() {
    let (s1, e1) = play(7);
    let (s2, e2) = play(7);
    assert_eq!(e1, e2);
    assert_eq!(s1, s2);
    let (_, e3) = play(8);
    assert_ne!(e1, e3, "another seed plays differently");
}
