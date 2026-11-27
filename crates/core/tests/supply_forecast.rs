//! R-SUP-04: the supply a side would have in a hex after entering it — the
//! trial recount with the hex own and occupied (R-MOVE-06).

mod common;

use ceasefire_core::hex::Hex;
use ceasefire_core::supply::SupplySource;

use common::scenario::{board, setup, start, unit, A, B};

/// Entering the hex next to the own territory: supply comes with the hex;
/// a hex deep in enemy land, cut from own hexes, gets none.
#[test]
fn r_sup_04_supply_after_entry() {
    let (map, control) = board(); // A holds q ≤ 2, source (0,0) value 10
    let s = start(setup(
        map,
        control,
        vec![unit("infantry", A, Hex::new(2, 0))],
    ));
    let now = s.hex_supply(A).unwrap();
    assert_eq!(now.get(Hex::new(3, 0)), 0, "enemy hex: no supply now");
    let next = s.hex_supply_if(A, &[(Hex::new(3, 0), A)]).unwrap();
    assert!(
        next.get(Hex::new(3, 0)) > 0,
        "after entry it is own and supplied"
    );
    let deep = s.hex_supply_if(A, &[(Hex::new(5, 0), A)]).unwrap();
    assert_eq!(deep.get(Hex::new(5, 0)), 0, "cut from own hexes");
}

/// Taking an enemy source in a trial recount counts it as captured: value /
/// `captured_source_divisor` (R-SUP-01), not the enemy's full value.
#[test]
fn r_sup_04_trial_capture_of_a_source_halves_it() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("infantry", B, Hex::new(6, 3))]);
    sc.sources[0].value = 2; // A's own source reaches (3, 0) with less than the captured one
    sc.sources.push(SupplySource {
        hex: Hex::new(3, 0),
        value: 10,
    });
    let s = start(sc);
    let div = s.data().rules.supply.captured_source_divisor;
    let m = s.hex_supply_if(A, &[(Hex::new(3, 0), A)]).unwrap();
    assert_eq!(m.get(Hex::new(3, 0)), 10 / div);
}
