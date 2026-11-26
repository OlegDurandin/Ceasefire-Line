//! R-MOVE-06: neutral closed territory of a scenario — ground units do not
//! enter, supply does not pass.

mod common;

use std::sync::Arc;

use ceasefire_core::game::{Command, GameState, NeutralZone, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::scenario::{axial_to_offset, validate, ScenarioIssue};

use common::scenario::{board, setup, start, unit, A};

/// The column q = 1 is a neutral country.
fn wall() -> NeutralZone {
    NeutralZone {
        id: "lebanon".into(),
        hexes: (-3..=3).map(|r| Hex::new(1, r)).collect(),
    }
}

#[test]
fn r_move_06_ground_units_do_not_enter_neutral_territory() {
    let (map, mut control) = board();
    for h in &wall().hexes {
        control.remove(h);
    }
    let mut s = setup(map, control, vec![unit("tank", A, Hex::new(0, 0))]);
    s.neutral = vec![wall()];
    let mut g = start(s);
    let reach = g.reachable(UnitId(1)).unwrap();
    assert!(
        !reach.contains_key(&Hex::new(1, 0)),
        "neutral hex is not reachable"
    );
    assert!(
        !reach.keys().any(|h| h.q >= 1),
        "nothing beyond the neutral column either: {reach:?}"
    );
    let err = g.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(1, 0)],
    });
    assert!(err.is_err(), "moving in is refused");
}

#[test]
fn r_move_06_supply_does_not_pass_neutral_territory() {
    // Even if the data wrongly gave the neutral hexes to side A, supply stops there.
    let open = start(setup(board().0, board().1, vec![]));
    let mut s = setup(board().0, board().1, vec![]);
    s.neutral = vec![wall()];
    let closed = start(s);
    let beyond = Hex::new(2, 0);
    assert!(
        open.hex_supply(A).unwrap().get(beyond) > 0,
        "without the wall supply reaches q = 2"
    );
    assert_eq!(
        closed.hex_supply(A).unwrap().get(beyond),
        0,
        "the neutral column blocks it"
    );
    assert_eq!(closed.hex_supply(A).unwrap().get(Hex::new(1, 0)), 0);
}

#[test]
fn r_move_06_validation_reports_scenario_content_in_neutral_territory() {
    let (map, control) = board();
    let mut s = setup(map, control, vec![unit("infantry", A, Hex::new(1, 0))]);
    s.sources[0].hex = Hex::new(1, 1);
    s.neutral = vec![NeutralZone {
        id: "jordan".into(),
        hexes: vec![Hex::new(1, 0), Hex::new(1, 1), Hex::new(40, 40)],
    }];
    let data = Arc::new(common::data());
    let issues = validate(&data, &s);
    for expected in [
        ScenarioIssue::Neutral {
            what: "unit",
            hex: axial_to_offset(Hex::new(1, 0)),
        },
        ScenarioIssue::Neutral {
            what: "control",
            hex: axial_to_offset(Hex::new(1, 0)),
        },
        ScenarioIssue::Neutral {
            what: "source",
            hex: axial_to_offset(Hex::new(1, 1)),
        },
        ScenarioIssue::MarkerOffMap {
            what: "neutral",
            hex: axial_to_offset(Hex::new(40, 40)),
        },
    ] {
        assert!(
            issues.contains(&expected),
            "missing {expected:?} in {issues:?}"
        );
    }
    // The core still starts such a game; validation is what flags it.
    assert!(GameState::new(data, s).is_ok());
}
