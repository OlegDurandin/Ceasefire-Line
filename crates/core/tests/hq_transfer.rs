//! R-HQ-01, R-HQ-02 (attachment and effects), R-MOVE-05 (strategic transfer).

mod common;

use std::collections::BTreeMap;

use ceasefire_core::game::{Command, Event, GameState, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
use ceasefire_core::side::SideId;
use ceasefire_core::{RuleError, TransferError};

use common::scenario::{board, hq, setup, start, unit, A, B};

fn attached(s: &GameState, id: u32) -> Option<UnitId> {
    s.unit(UnitId(id)).unwrap().attached_hq
}

fn end_turn(s: &mut GameState) {
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap();
}

#[test]
fn r_hq_02_attached_bonus_and_unattached_penalty() {
    let (map, control) = board();
    let mut near = unit("infantry", A, Hex::new(1, 1));
    near.morale_pct = 60;
    let mut foreign = unit("infantry", A, Hex::new(1, -1));
    foreign.morale_pct = 60;
    foreign.nation = Some("other".into());
    let s = start(setup(
        map,
        control,
        vec![near, foreign, hq(A, Hex::new(1, 0), 7)],
    ));
    // Morale 60 + 10 (rest) = 70, supply 10.
    assert_eq!(attached(&s, 1), Some(UnitId(3)));
    assert_eq!(
        s.unit(UnitId(1)).unwrap().morale_pct,
        70,
        "HQ bonus is not stored in morale"
    );
    assert_eq!(
        s.unit(UnitId(1)).unwrap().base_readiness_pct,
        82,
        "(70 + 6)% morale → 76%, + 6 = 82%"
    );
    assert_eq!(attached(&s, 2), None, "other nation");
    assert_eq!(
        s.unit(UnitId(2)).unwrap().base_readiness_pct,
        60,
        "70% − 10"
    );
    assert_eq!(
        s.unit(UnitId(3)).unwrap().base_readiness_pct,
        100,
        "HQ itself (morale 90 + 10): no HQ bonus or penalty"
    );
}

#[test]
fn r_hq_01_formation_first_then_nearest_within_capacity() {
    let (map, control) = board();
    let mut units: Vec<_> = [(0, 0), (2, 0), (1, -1), (1, 1), (0, 1)]
        .iter()
        .map(|&(q, r)| unit("infantry", A, Hex::new(q, r)))
        .collect();
    let mut own = unit("infantry", A, Hex::new(0, 2));
    own.formation = Some("d1".into());
    units.push(own);
    let mut h = hq(A, Hex::new(1, 0), 5);
    h.formation = Some("d1".into());
    units.push(h);
    let s = start(setup(map, control, units));
    let hq_id = Some(UnitId(7));
    assert_eq!(
        attached(&s, 6),
        hq_id,
        "own formation first, though farther"
    );
    for id in 1..=4 {
        assert_eq!(attached(&s, id), hq_id);
    }
    assert_eq!(attached(&s, 5), None, "capacity = rating 5");
}

#[test]
fn r_hq_01_tie_goes_to_higher_rating_and_pins_override() {
    let (map, control) = board();
    let units = vec![
        unit("infantry", A, Hex::new(1, 0)),
        hq(A, Hex::new(0, 0), 6),
        hq(A, Hex::new(2, 0), 8),
    ];
    let mut s = start(setup(map, control, units));
    assert_eq!(
        attached(&s, 1),
        Some(UnitId(3)),
        "equal distance: rating 8 wins"
    );

    s.apply(Command::PinToHq {
        unit: UnitId(1),
        hq: Some(UnitId(2)),
    })
    .unwrap();
    assert_eq!(attached(&s, 1), Some(UnitId(3)), "pin applies next phase");
    end_turn(&mut s);
    assert_eq!(attached(&s, 1), Some(UnitId(2)));
    assert!(matches!(
        s.apply(Command::PinToHq {
            unit: UnitId(1),
            hq: Some(UnitId(1)),
        }),
        Err(RuleError::IllegalPin { .. })
    ));
}

#[test]
fn r_hq_02_destroyed_hq_costs_nm_and_one_phase_unattached() {
    let (map, control) = board();
    // HQ in the corner (0,-3): its only free neighbour is taken, so it cannot retreat.
    let mut weak = hq(A, Hex::new(0, -3), 6);
    weak.strength = 1;
    let units = vec![
        unit("infantry", A, Hex::new(0, -2)),
        weak,
        hq(A, Hex::new(2, -1), 5),
        unit("tank", B, Hex::new(1, -3)),
    ];
    let mut s = start(setup(map, control, units));
    assert_eq!(attached(&s, 1), Some(UnitId(2)), "nearest HQ");
    s.apply(Command::EndPhase).unwrap();
    let events = s
        .apply(Command::Attack {
            unit: UnitId(4),
            target: UnitId(2),
        })
        .unwrap();
    assert!(
        events.contains(&Event::UnitDestroyed { unit: UnitId(2) }),
        "{events:?}"
    );
    assert!(events.contains(&Event::HqDestroyed {
        hq: UnitId(2),
        nm_loss: 3,
        national_morale_pct: 97,
    }));
    s.apply(Command::EndPhase).unwrap();
    assert_eq!(attached(&s, 1), None, "one phase without an HQ");
    end_turn(&mut s);
    assert_eq!(
        attached(&s, 1),
        Some(UnitId(3)),
        "then the next HQ in range"
    );
}

#[test]
fn r_hq_02_experience_grows_faster_under_rating_8() {
    let gain = |rating: u8| {
        let (map, control) = board();
        let mut enemy = unit("infantry", B, Hex::new(3, 0));
        enemy.entrenchment = 2;
        let units = vec![
            unit("infantry", A, Hex::new(2, 0)),
            hq(A, Hex::new(1, 0), rating),
            enemy,
        ];
        let mut s = start(setup(map, control, units));
        let events = s
            .apply(Command::Attack {
                unit: UnitId(1),
                target: UnitId(3),
            })
            .unwrap();
        let vacated = events.iter().any(|e| {
            matches!(
                e,
                Event::Retreated { .. } | Event::UnitDestroyed { unit: UnitId(3) }
            )
        });
        (s.unit(UnitId(1)).map_or(0, |u| u.exp), vacated)
    };
    let (slow, vacated_slow) = gain(7);
    let (fast, vacated_fast) = gain(8);
    assert_eq!(vacated_slow, vacated_fast, "same seed, same combat");
    let base = if vacated_slow { 50 } else { 25 };
    assert_eq!(slow, base);
    assert_eq!(fast, base * 125 / 100);
}

/// Road (0,0)..(10,0), all side A; an off-road hex at (5,1).
fn road_setup(units: Vec<ceasefire_core::game::UnitSetup>, mpp: u32) -> GameState {
    let mut map = HexMap::default();
    let mut control: BTreeMap<Hex, SideId> = BTreeMap::new();
    for q in 0..=10 {
        for r in 0..=1 {
            map.insert(Hex::new(q, r), HexCell::new("plain"));
            control.insert(Hex::new(q, r), A);
        }
    }
    for q in 0..10 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    let mut sc = setup(map, control, units);
    sc.sides[0].mpp = mpp;
    start(sc)
}

fn transfer(s: &mut GameState, id: u32, q: i32, r: i32) -> Result<Vec<Event>, RuleError> {
    s.apply(Command::Transfer {
        unit: UnitId(id),
        to: Hex::new(q, r),
    })
}

fn transfer_err(r: Result<Vec<Event>, RuleError>) -> TransferError {
    match r {
        Err(RuleError::IllegalTransfer { reason, .. }) => reason,
        other => panic!("expected a transfer refusal, got {other:?}"),
    }
}

#[test]
fn r_move_05_transfer_by_road_any_distance() {
    let units = vec![
        unit("tank", A, Hex::new(0, 0)),
        unit("infantry", A, Hex::new(3, 0)),
    ];
    let mut s = road_setup(units, 50);
    assert_eq!(s.transfer_cost(UnitId(1)), Ok(10), "10% of 100");
    let events = transfer(&mut s, 1, 10, 0).unwrap();
    let Event::Transferred { path, cost, .. } = &events[0] else {
        panic!("expected a transfer");
    };
    assert_eq!(
        (path.len(), *cost),
        (10, 10),
        "passes through the friendly unit at (3,0)"
    );
    let tank = s.unit(UnitId(1)).unwrap();
    assert_eq!(
        (tank.hex, tank.ap_halves, tank.strikes_left),
        (Hex::new(10, 0), 0, 0)
    );
    assert_eq!(s.side(A).unwrap().mpp, 40);
}

#[test]
fn r_move_05_transfer_restrictions() {
    let units = vec![
        unit("tank", A, Hex::new(0, 0)),
        unit("infantry", B, Hex::new(6, 1)),
        unit("mech", A, Hex::new(7, 1)),
    ];
    let mut s = road_setup(units, 5);
    assert_eq!(
        transfer_err(transfer(&mut s, 1, 10, 0)),
        TransferError::NoRoute,
        "enemy ZOC on the road"
    );
    assert_eq!(
        transfer_err(transfer(&mut s, 1, 5, 1)),
        TransferError::NoRoute,
        "off the road"
    );
    assert_eq!(
        transfer_err(transfer(&mut s, 3, 10, 0)),
        TransferError::StartsInZoc
    );
    assert_eq!(
        transfer_err(transfer(&mut s, 1, 5, 0)),
        TransferError::NotEnoughMpp
    );

    let mut s = road_setup(vec![unit("tank", A, Hex::new(0, 0))], 50);
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(1, 0)],
    })
    .unwrap();
    assert_eq!(
        transfer_err(transfer(&mut s, 1, 9, 0)),
        TransferError::Acted
    );
}

#[test]
fn r_move_05_only_through_own_hexes() {
    let mut s = road_setup(vec![unit("tank", A, Hex::new(0, 0))], 50);
    // Take (8,0) away from A by putting B there via a fresh setup.
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=10 {
        map.insert(Hex::new(q, 0), HexCell::new("plain"));
        control.insert(Hex::new(q, 0), if q == 8 { B } else { A });
    }
    for q in 0..10 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    let mut sc = setup(map, control, vec![unit("tank", A, Hex::new(0, 0))]);
    sc.sides[0].mpp = 50;
    let mut cut = start(sc);
    assert_eq!(
        transfer_err(transfer(&mut cut, 1, 10, 0)),
        TransferError::NoRoute
    );
    transfer(&mut cut, 1, 7, 0).unwrap();
    transfer(&mut s, 1, 10, 0).unwrap();
}
