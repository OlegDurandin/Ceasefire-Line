//! R-MOVE-01: forest (GDD section 14, «Срединные земли») — 2 AP, defense ×1,3,
//! entrenchment up to 3; hard defenders ×1,1 (with `armor_city_penalty`).

mod common;

use ceasefire_core::game::{Command, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::HexCell;

use common::scenario::{board, setup, start, unit, A, B};

/// Forest everywhere but (0, 0), all of it A's.
fn forest_row(units: Vec<ceasefire_core::game::UnitSetup>) -> ceasefire_core::game::ScenarioSetup {
    let (mut map, mut control) = board();
    let hexes: Vec<Hex> = control.keys().copied().collect();
    for h in hexes {
        if h != Hex::new(0, 0) {
            map.insert(h, HexCell::new("forest"));
        }
        control.insert(h, A);
    }
    setup(map, control, units)
}

#[test]
fn r_move_01_forest_costs_two_ap() {
    let s = start(forest_row(vec![unit("tank", A, Hex::new(0, 0))]));
    let reach = s.reachable(UnitId(1)).unwrap();
    // Tank 5 AP, reach in half-AP: 2 AP a forest hex → two hexes, not three.
    assert_eq!(reach.get(&Hex::new(1, 0)), Some(&4));
    assert_eq!(reach.get(&Hex::new(2, 0)), Some(&8));
    assert!(!reach.contains_key(&Hex::new(3, 0)));
}

/// D of a defender in forest against an attack from the plain.
fn forest_d(defender: &str) -> f64 {
    let (mut map, control) = board();
    map.insert(Hex::new(3, 0), HexCell::new("forest"));
    let s = start(setup(
        map,
        control,
        vec![
            unit(defender, B, Hex::new(3, 0)),
            unit("infantry", A, Hex::new(2, 0)),
        ],
    ));
    s.forecast_attack(UnitId(2), UnitId(1))
        .unwrap()
        .exchange
        .vs_defender
        .d
}

#[test]
fn r_move_01_forest_defense_soft_1_3_hard_1_1() {
    // Infantry def 4 × 1,3; tank def 5 × 1,1 (`armor_city_penalty` is on by default).
    common::assert_close(forest_d("infantry"), 4.0 * 1.3, 1e-9, "infantry D");
    common::assert_close(forest_d("tank"), 5.0 * 1.1, 1e-9, "tank D");
}

#[test]
fn r_move_01_forest_entrenchment_caps_at_three() {
    let mut inf = unit("infantry", A, Hex::new(1, 0));
    inf.entrenchment = 2;
    let mut s = start(forest_row(vec![inf]));
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap();
    // Infantry would dig +2 → 4; forest allows 3.
    assert_eq!(s.unit(UnitId(1)).unwrap().entrenchment, 3);
}
