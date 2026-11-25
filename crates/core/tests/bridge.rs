//! R-MOVE-02: boat crossing, bridge building and bridge strikes.

mod common;

use ceasefire_core::game::{Command, Event, GameState, ScenarioSetup, UnitId, UnitSetup};
use ceasefire_core::hex::Hex;
use ceasefire_core::{BridgeError, MoveError, RuleError};

use common::scenario::{board, setup, start, unit, A, B};

/// The board with a canal between columns q = 2 and q = 3.
fn canal_setup(units: Vec<UnitSetup>) -> ScenarioSetup {
    let (mut map, control) = board();
    for r in -3..=3 {
        map.add_canal(Hex::new(2, r), Hex::new(3, r)).unwrap();
        map.add_canal(Hex::new(2, r), Hex::new(3, r - 1)).ok();
    }
    setup(map, control, units)
}

fn mv(s: &mut GameState, id: u32, path: &[(i32, i32)]) -> Result<Vec<Event>, RuleError> {
    s.apply(Command::Move {
        unit: UnitId(id),
        path: path.iter().map(|&(q, r)| Hex::new(q, r)).collect(),
    })
}

fn move_err(r: Result<Vec<Event>, RuleError>) -> MoveError {
    match r {
        Err(RuleError::IllegalMove { reason, .. }) => reason,
        other => panic!("expected an illegal move, got {other:?}"),
    }
}

fn build(s: &mut GameState, id: u32, q: i32, r: i32) -> Result<Vec<Event>, RuleError> {
    s.apply(Command::BuildBridge {
        unit: UnitId(id),
        toward: Hex::new(q, r),
    })
}

fn bridge_err(r: Result<Vec<Event>, RuleError>) -> BridgeError {
    match r {
        Err(RuleError::IllegalBridge { reason, .. }) => reason,
        other => panic!("expected a bridge refusal, got {other:?}"),
    }
}

#[test]
fn r_move_02_infantry_crosses_by_boat() {
    let mut s = start(canal_setup(vec![
        unit("infantry", A, Hex::new(1, 0)),
        unit("tank", A, Hex::new(2, 1)),
    ]));
    assert_eq!(
        s.reachable(UnitId(1)).unwrap().get(&Hex::new(3, 0)),
        Some(&6),
        "all 3 AP"
    );
    assert_eq!(
        move_err(mv(&mut s, 2, &[(3, 1)])),
        MoveError::Impassable,
        "armour needs a bridge"
    );
    assert_eq!(
        move_err(mv(&mut s, 1, &[(2, 0), (3, 0), (4, 0)])),
        MoveError::BoatCrossingNotLast
    );

    let before = s
        .unit(UnitId(1))
        .unwrap()
        .readiness_pct(&s.data().rules.readiness);
    let events = mv(&mut s, 1, &[(2, 0), (3, 0)]).unwrap();
    assert!(matches!(
        events[0],
        Event::UnitMoved {
            by_boat: true,
            ap_spent_halves: 6,
            ..
        }
    ));
    let u = s.unit(UnitId(1)).unwrap();
    assert_eq!((u.hex, u.ap_halves), (Hex::new(3, 0), 0));
    assert_eq!(u.readiness_pct(&s.data().rules.readiness), before - 30);
    assert_eq!(s.controller(Hex::new(3, 0)), Some(A));
}

#[test]
fn r_move_02_boat_needs_minimum_ap_left() {
    let mut s = start(canal_setup(vec![unit("infantry", A, Hex::new(0, 0))]));
    // 2 AP to reach (2,0), 1 AP left: enough to cross.
    mv(&mut s, 1, &[(1, 0), (2, 0), (3, 0)]).unwrap();
    let mut s = start(canal_setup(vec![unit("infantry", A, Hex::new(0, 1))]));
    // Three plain steps to the bank use all 3 AP: nothing left for the boat.
    assert_eq!(
        move_err(mv(&mut s, 1, &[(1, 1), (2, 1), (2, 0), (3, 0)])),
        MoveError::NotEnoughAp
    );
}

fn bridgehead() -> GameState {
    let mut sc = canal_setup(vec![
        unit("engineer", A, Hex::new(2, 0)),
        unit("infantry", A, Hex::new(3, 0)),
        unit("tank", A, Hex::new(1, 0)),
        unit("artillery", A, Hex::new(1, 1)),
    ]);
    sc.control.insert(Hex::new(3, 0), A);
    start(sc)
}

fn end_turn(s: &mut GameState) {
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap();
}

#[test]
fn r_move_02_engineer_bridges_and_armour_crosses_next_phase() {
    let mut s = bridgehead();
    assert_eq!(
        move_err(mv(&mut s, 3, &[(2, 0), (3, 0), (4, 0)])),
        MoveError::Impassable
    );
    let events = build(&mut s, 1, 3, 0).unwrap();
    assert_eq!(
        events,
        vec![Event::BridgeStarted {
            unit: UnitId(1),
            a: Hex::new(2, 0),
            b: Hex::new(3, 0),
        }]
    );
    assert_eq!(
        s.unit(UnitId(1)).unwrap().ap_halves,
        0,
        "engineer spends the phase"
    );
    assert!(
        !s.map().has_bridge(Hex::new(2, 0), Hex::new(3, 0)),
        "not yet"
    );
    assert_eq!(
        move_err(mv(&mut s, 3, &[(2, 0), (3, 0), (4, 0)])),
        MoveError::Impassable
    );
    assert_eq!(bridge_err(build(&mut s, 1, 3, 0)), BridgeError::Acted);

    s.apply(Command::EndPhase).unwrap();
    let events = s.apply(Command::EndPhase).unwrap();
    assert!(events.contains(&Event::BridgeBuilt {
        unit: UnitId(1),
        a: Hex::new(2, 0),
        b: Hex::new(3, 0),
        completed: true,
    }));
    // Through the engineer, over the bridge, through the infantry.
    mv(&mut s, 3, &[(2, 0), (3, 0), (4, 0)]).unwrap();
}

#[test]
fn r_move_02_bridge_lost_if_engineer_dies() {
    // Engineer of strength 1 on the bank at (2,-3), cornered: its only
    // retreat hex (2,-2) is taken, so the B tank's attack destroys it.
    let mut engineer = unit("engineer", A, Hex::new(2, -3));
    engineer.strength = 1;
    let mut sc = canal_setup(vec![
        engineer,
        unit("infantry", A, Hex::new(3, -3)),
        unit("infantry", A, Hex::new(2, -2)),
        unit("tank", B, Hex::new(1, -3)),
    ]);
    sc.control.insert(Hex::new(3, -3), A);
    let mut s = start(sc);
    build(&mut s, 1, 3, -3).unwrap();
    assert_eq!(s.pending_bridges().len(), 1);
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::Attack {
        unit: UnitId(4),
        target: UnitId(1),
    })
    .unwrap();
    assert!(s.unit(UnitId(1)).is_none(), "engineer destroyed");
    let events = s.apply(Command::EndPhase).unwrap();
    assert!(events.contains(&Event::BridgeBuilt {
        unit: UnitId(1),
        a: Hex::new(2, -3),
        b: Hex::new(3, -3),
        completed: false,
    }));
    assert!(!s.map().has_bridge(Hex::new(2, -3), Hex::new(3, -3)));
    assert!(s.pending_bridges().is_empty());
}

#[test]
fn r_move_02_bridge_preconditions() {
    let mut s = bridgehead();
    assert_eq!(bridge_err(build(&mut s, 2, 2, 0)), BridgeError::NotEngineer);
    assert_eq!(
        bridge_err(build(&mut s, 1, 3, -1)),
        BridgeError::NoFriendlyOnFarBank
    );
    assert_eq!(bridge_err(build(&mut s, 1, 1, 0)), BridgeError::NoCanal);
    build(&mut s, 1, 3, 0).unwrap();

    let mut s = bridgehead();
    mv(&mut s, 1, &[(2, -1)]).unwrap();
    assert_eq!(
        bridge_err(build(&mut s, 1, 3, -1)),
        BridgeError::Acted,
        "moved this phase"
    );
}

#[test]
fn r_move_02_bombarded_bridge_falls_about_30_percent() {
    let runs = 400u64;
    let mut destroyed = 0;
    for seed in 0..runs {
        let mut sc = canal_setup(vec![
            unit("engineer", A, Hex::new(2, 0)),
            unit("infantry", A, Hex::new(3, 0)),
            unit("artillery", A, Hex::new(1, 1)),
        ]);
        sc.control.insert(Hex::new(3, 0), A);
        sc.seed = seed;
        let mut s = start(sc);
        build(&mut s, 1, 3, 0).unwrap();
        end_turn(&mut s);
        let events = s
            .apply(Command::BombardBridge {
                unit: UnitId(3),
                a: Hex::new(2, 0),
                b: Hex::new(3, 0),
            })
            .unwrap();
        let Event::BridgeStrike {
            destroyed: d,
            chance,
            ..
        } = events[0]
        else {
            panic!("expected a bridge strike");
        };
        assert!((chance - 0.3).abs() < 1e-12);
        assert_eq!(d, !s.map().has_bridge(Hex::new(2, 0), Hex::new(3, 0)));
        destroyed += u64::from(d);
    }
    let share = destroyed as f64 / runs as f64;
    assert!((0.24..=0.36).contains(&share), "share {share}");
}

#[test]
fn r_move_02_rebuild_after_destruction() {
    let mut sc = canal_setup(vec![
        unit("engineer", A, Hex::new(2, 0)),
        unit("infantry", A, Hex::new(3, 0)),
        unit("artillery", A, Hex::new(1, 1)),
    ]);
    sc.control.insert(Hex::new(3, 0), A);
    // Find a seed where the strike destroys the bridge.
    let mut s = (0..100)
        .find_map(|seed| {
            let mut sc = sc.clone();
            sc.seed = seed;
            let mut s = start(sc);
            build(&mut s, 1, 3, 0).unwrap();
            end_turn(&mut s);
            s.apply(Command::BombardBridge {
                unit: UnitId(3),
                a: Hex::new(2, 0),
                b: Hex::new(3, 0),
            })
            .unwrap();
            (!s.map().has_bridge(Hex::new(2, 0), Hex::new(3, 0))).then_some(s)
        })
        .expect("some seed destroys it");
    end_turn(&mut s);
    build(&mut s, 1, 3, 0).unwrap();
    assert!(!s.map().has_bridge(Hex::new(2, 0), Hex::new(3, 0)));
    end_turn(&mut s);
    assert!(
        s.map().has_bridge(Hex::new(2, 0), Hex::new(3, 0)),
        "rebuilt by the next own phase"
    );
}
