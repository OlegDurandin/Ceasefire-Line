//! R-MOVE-01: a dirt road (track) — entering costs 1 AP whatever the terrain;
//! supply follows it at the same cost; it does not work into enemy ZOC and
//! carries no strategic transfer (R-MOVE-05).

mod common;

use ceasefire_core::game::{Command, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::HexCell;

use common::scenario::{board, setup, start, unit, A, B};

/// Hills along r = 0 from q = 1 to 5 (2 AP each), with a track through them.
fn hilly_track(track: bool) -> ceasefire_core::game::ScenarioSetup {
    let (mut map, mut control) = board();
    for q in 1..=5 {
        map.insert(Hex::new(q, 0), HexCell::new("hills"));
        control.insert(Hex::new(q, 0), A);
        if track {
            map.add_track(Hex::new(q - 1, 0), Hex::new(q, 0)).unwrap();
        }
    }
    setup(map, control, vec![unit("infantry", A, Hex::new(0, 0))])
}

#[test]
fn r_move_01_track_costs_one_ap_whatever_the_terrain() {
    let plain = start(hilly_track(false));
    let track = start(hilly_track(true));
    let reach = |g: &ceasefire_core::game::GameState| g.reachable(UnitId(1)).unwrap();
    // Infantry 3 AP: on hills 2 AP a hex → one hex; on the track 1 AP → three.
    assert_eq!(reach(&plain).get(&Hex::new(1, 0)), Some(&4), "hills: 2 AP");
    assert!(!reach(&plain).contains_key(&Hex::new(2, 0)));
    assert_eq!(
        reach(&track).get(&Hex::new(3, 0)),
        Some(&6),
        "track: 1 AP a hex"
    );
}

#[test]
fn r_move_01_supply_runs_along_a_track_at_its_cost() {
    let plain = start(hilly_track(false));
    let track = start(hilly_track(true));
    let far = Hex::new(5, 0);
    assert!(
        track.hex_supply(A).unwrap().get(far) > plain.hex_supply(A).unwrap().get(far),
        "cheaper path along the track"
    );
}

#[test]
fn r_move_01_track_does_not_work_into_enemy_zoc() {
    let mut sc = hilly_track(true);
    sc.units.push(unit("infantry", B, Hex::new(3, -1)));
    let g = start(sc);
    // (2,0) is next to the enemy at (3,-1): entering it costs the hills. 2 AP.
    assert_eq!(
        g.reachable(UnitId(1)).unwrap().get(&Hex::new(2, 0)),
        Some(&6)
    );
}

#[test]
fn r_move_05_no_strategic_transfer_by_track() {
    let mut sc = hilly_track(true);
    sc.sides[0].mpp = 100;
    let mut g = start(sc);
    assert!(g.transfer_route(UnitId(1), Hex::new(5, 0)).is_err());
    assert!(g
        .apply(Command::Transfer {
            unit: UnitId(1),
            to: Hex::new(5, 0)
        })
        .is_err());
}
