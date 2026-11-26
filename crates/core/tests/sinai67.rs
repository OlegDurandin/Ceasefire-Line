//! Stage 4, step 5: the «Синай-67» map (data/scenarios/sinai67) — size,
//! the Suez Canal without bridges in 1967, airfields, validation.

mod common;

use ceasefire_core::scenario::{load_scenario, validate};

const SCENARIO: &str = include_str!("../../../data/scenarios/sinai67/scenario.json");
const MAP: &str = include_str!("../../../data/scenarios/sinai67/map.json");

#[test]
fn sinai67_map_loads_and_validates() {
    let data = common::data();
    let setup = load_scenario(SCENARIO, MAP, None).expect("scenario parses");
    assert_eq!(setup.hex_km, Some(10.0));
    assert_eq!(setup.map.hexes().count(), 29 * 19);
    assert_eq!(validate(&data, &setup), vec![]);
    // 1967: ferries, no bridges over the canal.
    assert_eq!(setup.map.bridges().count(), 0);
    assert!(setup.map.airfields().count() >= 9);
}
