//! R-MOVE-02, R-SUP-02 (designer decision 2026-11-26): ferries carry supply
//! across the canal only; bridges blown by a scenario event (R-EVT-01).

mod common;

use ceasefire_core::game::{Command, Effect, Event, GameState, Predicate, ScenarioEvent, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::BridgeState;

use common::scenario::{board, setup, start, unit, A, B};

/// A canal between q = 1 and q = 2 on every row; A holds q ≤ 3.
fn canal_board(crossing: Option<&str>) -> GameState {
    let (mut map, mut control) = board();
    for r in -3..=3 {
        map.add_canal(Hex::new(1, r), Hex::new(2, r)).unwrap();
        if r > -3 {
            map.add_canal(Hex::new(1, r), Hex::new(2, r - 1)).unwrap();
        }
    }
    match crossing {
        Some("ferry") => map.add_ferry(Hex::new(1, 0), Hex::new(2, 0)).unwrap(),
        Some("bridge") => map.add_bridge(Hex::new(1, 0), Hex::new(2, 0)).unwrap(),
        _ => {}
    }
    for q in 0..=3 {
        for r in -3..=3 {
            control.insert(Hex::new(q, r), A);
        }
    }
    // A tank: infantry may cross by boat anyway (R-MOVE-02).
    let units = vec![
        unit("tank", A, Hex::new(1, 0)),
        unit("artillery", A, Hex::new(0, 1)),
    ];
    start(setup(map, control, units))
}

#[test]
fn r_sup_02_ferry_carries_supply_at_minus_three() {
    let supply = |c| canal_board(c).hex_supply(A).unwrap().get(Hex::new(2, 0));
    assert_eq!(supply(None), 0, "no crossing: cut");
    let bridge = supply(Some("bridge"));
    let ferry = supply(Some("ferry"));
    assert!(bridge > 0 && ferry > 0);
    assert_eq!(bridge - ferry, 1, "bridge −2, ferry −3");
}

#[test]
fn r_move_02_no_movement_by_ferry_and_strikes_destroy_it() {
    let mut s = canal_board(Some("ferry"));
    assert!(s
        .apply(Command::Move {
            unit: UnitId(1),
            path: vec![Hex::new(2, 0)],
        })
        .is_err());
    // Artillery fires at the ferry until it is destroyed (30% a shot).
    let mut destroyed = false;
    for _ in 0..20 {
        if s.map().bridge_state(Hex::new(1, 0), Hex::new(2, 0)) == Some(BridgeState::FerryDestroyed)
        {
            destroyed = true;
            break;
        }
        let _ = s.apply(Command::BombardBridge {
            unit: UnitId(2),
            a: Hex::new(1, 0),
            b: Hex::new(2, 0),
        });
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
    }
    assert!(destroyed, "a bombarded ferry goes down");
    assert_eq!(s.hex_supply(A).unwrap().get(Hex::new(2, 0)), 0);
}

/// R-EVT-01: `unit_near` + `destroy_bridge` (El Ferdan, «Синай-67»).
#[test]
fn r_evt_01_bridge_blown_when_the_enemy_comes_near() {
    let (mut map, control) = board();
    map.add_canal(Hex::new(5, 0), Hex::new(6, 0)).unwrap();
    map.add_bridge(Hex::new(5, 0), Hex::new(6, 0)).unwrap();
    let mut sc = setup(
        map,
        control,
        vec![
            unit("tank", A, Hex::new(1, 0)),
            unit("infantry", B, Hex::new(6, 2)),
        ],
    );
    sc.events = vec![ScenarioEvent {
        id: "blow".into(),
        phase_of: None,
        trigger: Predicate::UnitNear {
            side: A,
            hex: Hex::new(5, 0),
            km: 20,
        },
        conditions: vec![],
        effects: vec![Effect::DestroyBridge {
            a: Hex::new(5, 0),
            b: Hex::new(6, 0),
        }],
    }];
    let mut s = start(sc);
    assert!(
        s.map().has_bridge(Hex::new(5, 0), Hex::new(6, 0)),
        "far away: stands"
    );
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(2, 0), Hex::new(3, 0)],
    })
    .unwrap();
    let events = s.apply(Command::EndPhase).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::BridgeBlown { .. })),
        "{events:?}"
    );
    assert_eq!(
        s.map().bridge_state(Hex::new(5, 0), Hex::new(6, 0)),
        Some(BridgeState::Destroyed)
    );
}
