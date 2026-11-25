//! R-NM-01, R-NM-02, R-TURN-01 step 3: national morale and victory.

mod common;

use ceasefire_core::game::{
    Command, EndReason, Event, GameState, KeyHex, NmReason, Objective, UnitId, VictoryLevel,
    VictoryThreshold,
};
use ceasefire_core::hex::Hex;
use ceasefire_core::supply::SupplySource;
use ceasefire_core::RuleError;

use common::scenario::{board, setup, start, unit, A, B};

fn nm(s: &GameState, side: ceasefire_core::side::SideId) -> u8 {
    s.side(side).unwrap().national_morale_pct
}

fn objective(q: i32, r: i32, nm_value: u8, vp: i32) -> Objective {
    Objective {
        hex: Hex::new(q, r),
        nm_value,
        vp,
    }
}

#[test]
fn r_nm_01_objective_counts_at_end_of_phase() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("mech", A, Hex::new(2, 0))]);
    sc.sides[0].national_morale_pct = 80;
    sc.objectives = vec![objective(3, 0, 10, 0)];
    let mut s = start(sc);
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(3, 0)],
    })
    .unwrap();
    assert_eq!(nm(&s, B), 100, "not until the end of the phase");
    let events = s.apply(Command::EndPhase).unwrap();
    assert!(events.contains(&Event::ObjectiveChanged {
        hex: Hex::new(3, 0),
        from: Some(B),
        to: Some(A),
    }));
    assert_eq!(
        (nm(&s, A), nm(&s, B)),
        (85, 90),
        "taker +half, loser −value"
    );
}

#[test]
fn r_nm_01_unit_morale_capped_by_nm() {
    let (map, control) = board();
    let mut sc = setup(
        map,
        control,
        vec![
            unit("mech", A, Hex::new(2, 0)),
            unit("infantry", B, Hex::new(5, 2)),
        ],
    );
    sc.objectives = vec![objective(3, 0, 15, 0)];
    let mut s = start(sc);
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(3, 0)],
    })
    .unwrap();
    s.apply(Command::EndPhase).unwrap();
    assert_eq!(nm(&s, B), 85);
    assert!(s.unit(UnitId(2)).unwrap().morale_pct <= 85);
}

#[test]
fn r_nm_01_destroyed_in_encirclement_costs_three() {
    let (map, control) = board();
    // B unit of strength 1 with no supply: attrition kills it on its second phase at 0.
    let mut u = unit("infantry", B, Hex::new(5, 0));
    u.strength = 1;
    u.supply = Some(0);
    let mut s = start(setup(map, control, vec![u]));
    s.apply(Command::EndPhase).unwrap(); // B phase 1: supply 0 (first)
    s.apply(Command::EndPhase).unwrap();
    let events = s.apply(Command::EndPhase).unwrap(); // B phase 2: attrition
    assert!(events.contains(&Event::NationalMorale {
        side: B,
        change: -3,
        value: 97,
        reason: NmReason::UnitDestroyed {
            unit: UnitId(1),
            encircled: true,
        },
    }));
}

#[test]
fn r_nm_01_destroyed_in_supply_costs_two() {
    let (map, control) = board();
    // B defender of strength 1 in the corner (6,-3), its only retreat hex taken.
    let mut d = unit("infantry", B, Hex::new(6, -3));
    d.strength = 1;
    let units = vec![
        d,
        unit("infantry", B, Hex::new(6, -2)),
        unit("tank", A, Hex::new(5, -3)),
    ];
    let mut sc = setup(map, control, units);
    sc.sources.push(SupplySource {
        hex: Hex::new(6, 0),
        value: 10,
    });
    let mut s = start(sc);
    let events = s
        .apply(Command::Attack {
            unit: UnitId(3),
            target: UnitId(1),
        })
        .unwrap();
    assert!(
        events.contains(&Event::NationalMorale {
            side: B,
            change: -2,
            value: 98,
            reason: NmReason::UnitDestroyed {
                unit: UnitId(1),
                encircled: false,
            },
        }),
        "{events:?}"
    );
}

#[test]
fn r_nm_01_key_hex_gives_one_per_own_phase() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![]);
    sc.sides[0].national_morale_pct = 70;
    sc.key_hexes = vec![KeyHex {
        hex: Hex::new(1, 0),
        side: A,
        nm_per_turn: 1,
    }];
    let mut s = start(sc);
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap();
    assert_eq!(nm(&s, A), 72);
}

#[test]
fn r_nm_02_sudden_death() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("mech", A, Hex::new(2, 0))]);
    sc.sides[1].national_morale_pct = 28;
    sc.objectives = vec![objective(3, 0, 10, 0)];
    let mut s = start(sc);
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(3, 0)],
    })
    .unwrap();
    let events = s.apply(Command::EndPhase).unwrap();
    let outcome = s.outcome().expect("game over").clone();
    assert_eq!(
        (outcome.winner, outcome.level, outcome.reason),
        (Some(A), VictoryLevel::Decisive, EndReason::SuddenDeath)
    );
    assert!(events.contains(&Event::GameOver { outcome }));
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::PhaseStarted { .. })));
    assert_eq!(s.apply(Command::EndPhase), Err(RuleError::GameOver));
}

fn by_date(objective_vp: i32) -> ceasefire_core::game::Outcome {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![]);
    sc.last_turn = Some(1);
    sc.objectives = vec![objective(1, 0, 10, objective_vp)];
    sc.victory = vec![
        VictoryThreshold {
            side: A,
            decisive: 30,
            victory: 10,
        },
        VictoryThreshold {
            side: B,
            decisive: 30,
            victory: 10,
        },
    ];
    let mut s = start(sc);
    s.apply(Command::EndPhase).unwrap();
    assert!(s.outcome().is_none(), "B still plays turn 1");
    s.apply(Command::EndPhase).unwrap();
    s.outcome().expect("ends after the last phase").clone()
}

#[test]
fn r_nm_02_victory_by_date() {
    let o = by_date(20);
    assert_eq!(o.vp, vec![(A, 120), (B, 100)], "objective + NM");
    assert_eq!(
        (o.winner, o.level, o.reason),
        (Some(A), VictoryLevel::Victory, EndReason::Date)
    );
    assert_eq!(by_date(35).level, VictoryLevel::Decisive);
    let draw = by_date(5);
    assert_eq!((draw.winner, draw.level), (None, VictoryLevel::Draw));
}
