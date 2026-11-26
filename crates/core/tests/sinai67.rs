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
    // 1967 (designer decision 2026-11-26): ferries at Ismailia and Suez
    // (supply only) and the El Ferdan railway bridge, built on 5 June.
    use ceasefire_core::map::BridgeState;
    let states: Vec<BridgeState> = setup.map.bridges().map(|(_, s)| s).collect();
    assert_eq!(
        states.iter().filter(|s| **s == BridgeState::Ferry).count(),
        2
    );
    assert_eq!(
        states.iter().filter(|s| **s == BridgeState::Built).count(),
        1
    );
    assert!(setup.map.airfields().count() >= 9);
}

/// Stage 4, step 6: the scenario starts in both `first_strike` variants;
/// every unit says where it comes from; every name has a Russian string.
#[test]
fn sinai67_scenario_starts_in_both_first_strike_variants() {
    use std::sync::Arc;

    use ceasefire_core::game::GameState;
    let events = include_str!("../../../data/scenarios/sinai67/events.json");
    let ru: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("../../../data/locale/ru.json")).unwrap();
    let raw: serde_json::Value = serde_json::from_str(SCENARIO).unwrap();
    for u in raw["units"].as_array().unwrap() {
        assert!(u["source"].is_string(), "unit without a source: {u}");
        assert!(u["confirmed"].is_boolean());
        if let Some(name) = u["name"].as_str() {
            assert!(ru.contains_key(name), "no Russian name for {name}");
        }
    }
    let egyptian_aircraft = |scenario: &str| {
        let setup = load_scenario(scenario, MAP, Some(events)).expect("scenario parses");
        assert_eq!(validate(&common::data(), &setup), vec![]);
        let (s, _) = GameState::new(Arc::new(common::data()), setup).expect("the game starts");
        s.air_units().filter(|a| a.side.0 == 1).count()
    };
    // Historical: about 70% of the Egyptian air force lost on 5 June — 3 of 10 units left.
    assert_eq!(egyptian_aircraft(SCENARIO), 3);
    let mut free = raw.clone();
    free["options"]["first_strike"] = "free".into();
    assert_eq!(egyptian_aircraft(&free.to_string()), 10);
}
