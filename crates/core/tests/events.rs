//! R-EVT-01: scenario events and the ceasefire date.

mod common;

use ceasefire_core::game::{
    Command, Effect, Event, GameState, NmReason, Predicate, ScenarioEvent, ScenarioSetup, UnitId,
};
use ceasefire_core::hex::Hex;

use common::scenario::{board, setup, start, unit, A, B};

fn event(
    id: &str,
    phase_of: Option<ceasefire_core::side::SideId>,
    trigger: Predicate,
    effects: Vec<Effect>,
) -> ScenarioEvent {
    ScenarioEvent {
        id: id.into(),
        phase_of,
        trigger,
        conditions: vec![],
        effects,
    }
}

fn with_events(events: Vec<ScenarioEvent>) -> ScenarioSetup {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("infantry", A, Hex::new(0, 0))]);
    sc.events = events;
    sc
}

fn end_phase(s: &mut GameState) -> Vec<Event> {
    s.apply(Command::EndPhase).unwrap()
}

fn fired(events: &[Event], id: &str) -> bool {
    events.contains(&Event::ScenarioEventFired { id: id.into() })
}

#[test]
fn r_evt_01_turn_trigger_reinforcements_in_own_phase() {
    let reserves = event(
        "reserves",
        Some(A),
        Predicate::Turn { turn: 2 },
        vec![Effect::Reinforcements {
            units: vec![
                unit("tank", A, Hex::new(0, 0)),
                unit("mech", A, Hex::new(2, 3)),
            ],
        }],
    );
    let mut s = start(with_events(vec![reserves]));
    let b_phase = end_phase(&mut s);
    assert!(!fired(&b_phase, "reserves"), "B's phase is skipped");
    let a_phase = end_phase(&mut s);
    assert!(fired(&a_phase, "reserves"));
    // (0,0) is taken by the infantry: nearest free own hex, lowest (q, r) on ties.
    let tank = s.units().find(|u| u.class == "tank").unwrap();
    assert_eq!(tank.hex.distance(Hex::new(0, 0)), 1);
    assert_eq!(tank.hex, Hex::new(0, -1));
    assert!(tank.ap_halves > 0, "refreshed in the arrival phase");
    assert!(s.unit_at(Hex::new(2, 3)).is_some_and(|u| u.class == "mech"));
    let later = end_phase(&mut s);
    assert!(
        !later
            .iter()
            .any(|e| matches!(e, Event::ScenarioEventFired { .. })),
        "fires once"
    );
}

#[test]
fn r_evt_01_mpp_nm_and_message() {
    let ev = event(
        "airlift",
        None,
        Predicate::Turn { turn: 1 },
        vec![
            Effect::Mpp {
                side: A,
                amount: 15,
            },
            Effect::Nm {
                side: B,
                change: -7,
            },
            Effect::Message {
                key: "event.test".into(),
            },
        ],
    );
    let (s, events) =
        GameState::new(std::sync::Arc::new(common::data()), with_events(vec![ev])).unwrap();
    assert!(events.contains(&Event::MppChanged {
        side: A,
        amount: 15,
        total: 15
    }));
    assert!(events.contains(&Event::NationalMorale {
        side: B,
        change: -7,
        value: 93,
        reason: NmReason::ScenarioEvent {
            id: "airlift".into()
        },
    }));
    assert!(events.contains(&Event::Message {
        key: "event.test".into()
    }));
    assert_eq!(s.side(A).unwrap().mpp, 15);
}

#[test]
fn r_evt_01_ceasefire_moves_earlier() {
    let ev = event(
        "pressure",
        None,
        Predicate::Turn { turn: 2 },
        vec![Effect::ShiftLastTurn { turns: -1 }],
    );
    let mut sc = with_events(vec![ev]);
    sc.last_turn = Some(4);
    let mut s = start(sc);
    end_phase(&mut s);
    let events = end_phase(&mut s);
    assert!(events.contains(&Event::CeasefireMoved { last_turn: 3 }));
    while s.outcome().is_none() {
        end_phase(&mut s);
    }
    assert_eq!(s.turn(), 3, "the game ends after turn 3");

    // Never earlier than the current turn.
    let ev = event(
        "panic",
        None,
        Predicate::Turn { turn: 2 },
        vec![Effect::ShiftLastTurn { turns: -5 }],
    );
    let mut sc = with_events(vec![ev]);
    sc.last_turn = Some(4);
    let mut s = start(sc);
    end_phase(&mut s);
    end_phase(&mut s);
    assert_eq!(s.last_turn(), Some(2));
}

#[test]
fn r_evt_01_hex_control_trigger_and_conditions() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("mech", A, Hex::new(2, 0))]);
    let mut ev = event(
        "breakthrough",
        None,
        Predicate::HexControl {
            hex: Hex::new(4, 0),
            side: A,
        },
        vec![Effect::Message { key: "x".into() }],
    );
    ev.conditions = vec![Predicate::Turn { turn: 2 }];
    sc.events = vec![ev];
    let mut s = start(sc);
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(3, 0), Hex::new(4, 0)],
    })
    .unwrap();
    let b_phase = end_phase(&mut s);
    assert!(
        !fired(&b_phase, "breakthrough"),
        "condition: not before turn 2"
    );
    let a_phase = end_phase(&mut s);
    assert!(fired(&a_phase, "breakthrough"));
}

#[test]
fn r_evt_01_losses_trigger() {
    let (map, control) = board();
    let mut u = unit("infantry", B, Hex::new(5, 0));
    u.supply = Some(0);
    let mut sc = setup(map, control, vec![u]);
    sc.events = vec![event(
        "bleeding",
        None,
        Predicate::LossesAtLeast { side: B, steps: 1 },
        vec![Effect::Message { key: "x".into() }],
    )];
    let mut s = start(sc);
    let mut all = Vec::new();
    for _ in 0..6 {
        all.extend(end_phase(&mut s));
    }
    assert!(all.iter().any(|e| matches!(e, Event::Attrition { .. })));
    assert!(fired(&all, "bleeding"));
}

/// R-EVT-01: composite triggers and effects on a side's units («приказ об
/// отходе» in «Голанах-67»): morale −20 (not below 0) and entrenchment −2
/// (not below 0) for every ground unit of the side.
#[test]
fn r_evt_01_any_of_trigger_morale_and_entrenchment_effects() {
    let order = ScenarioEvent {
        id: "withdrawal".into(),
        phase_of: Some(A),
        trigger: Predicate::AnyOf {
            of: vec![
                Predicate::NmAtMost { side: B, pct: 50 },
                Predicate::Turn { turn: 2 },
            ],
        },
        conditions: vec![],
        effects: vec![
            Effect::UnitsMorale {
                side: B,
                change: -20,
            },
            Effect::UnitsEntrenchment {
                side: B,
                change: -2,
            },
            Effect::UnitsReadinessPenalty { side: B, pct: 15 },
        ],
    };
    let never = ScenarioEvent {
        id: "never".into(),
        phase_of: None,
        trigger: Predicate::AllOf {
            of: vec![
                Predicate::Turn { turn: 2 },
                Predicate::NmAtMost { side: B, pct: 50 },
            ],
        },
        conditions: vec![],
        effects: vec![Effect::Message { key: "x".into() }],
    };
    let (map, control) = board();
    let mut dug_in = unit("infantry", B, Hex::new(4, 0));
    dug_in.entrenchment = 2;
    let mut shaken = unit("infantry", B, Hex::new(5, 0));
    shaken.morale_pct = 5; // +10 rest in B's phase, then −20: clamped at 0
    let mut sc = setup(
        map,
        control,
        vec![unit("infantry", A, Hex::new(0, 0)), dug_in, shaken],
    );
    sc.events = vec![order, never];
    let mut s = start(sc);
    end_phase(&mut s); // B's phase of turn 1
    let before = s.unit_at(Hex::new(4, 0)).unwrap().clone();
    let own_before = s.unit_at(Hex::new(0, 0)).unwrap().morale_pct;
    let events = end_phase(&mut s); // A's phase of turn 2: the order fires
    assert!(fired(&events, "withdrawal"));
    assert!(!fired(&events, "never"), "all_of needs every predicate");
    assert!(events.contains(&Event::UnitsMoraleChanged {
        side: B,
        change: -20
    }));
    assert!(events.contains(&Event::UnitsEntrenchmentChanged {
        side: B,
        change: -2
    }));
    let after = s.unit_at(Hex::new(4, 0)).unwrap();
    assert_eq!(
        i32::from(after.morale_pct),
        i32::from(before.morale_pct) - 20
    );
    assert_eq!(
        i32::from(after.entrenchment),
        (i32::from(before.entrenchment) - 2).max(0)
    );
    assert_eq!(
        s.unit_at(Hex::new(5, 0)).unwrap().morale_pct,
        0,
        "not below 0"
    );
    let own = s.unit_at(Hex::new(0, 0)).unwrap();
    assert!(own.morale_pct >= own_before, "the other side is untouched");
    // Readiness −15 points at once (like fatigue), gone at the start of B's next phase.
    let rules = common::data().rules.readiness.clone();
    let dug_in_now = s.unit_at(Hex::new(4, 0)).unwrap();
    assert_eq!(
        i32::from(dug_in_now.readiness_pct(&rules)),
        i32::from(before.readiness_pct(&rules)) - 15
    );
    assert!(events.contains(&Event::UnitsReadinessPenalty { side: B, pct: 15 }));
    end_phase(&mut s); // B's phase of turn 2
    assert_eq!(s.unit_at(Hex::new(4, 0)).unwrap().event_penalty_pct, 0);
}
