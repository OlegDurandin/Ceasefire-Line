//! SC alignment of the core loop (designer task 2026-11-26): rule variants
//! from `rules.variants`, each off by default, switched on per scenario.

mod common;

use std::collections::BTreeMap;

use ceasefire_core::game::{Command, GameState, ScenarioSetup, UnitId, UnitSetup};
use ceasefire_core::hex::Hex;

use common::scenario::{board, setup, start, unit, A, B};

fn with(variant: &str, mut sc: ScenarioSetup) -> ScenarioSetup {
    sc.variants = BTreeMap::from([(variant.to_owned(), true)]);
    sc
}

/// B's dug-in infantry at (3, 0) and three A infantry brigades next to it.
fn three_on_one() -> ScenarioSetup {
    let (map, mut control) = board();
    control.insert(Hex::new(3, -1), A); // all three attackers in supply
    let mut target = unit("infantry", B, Hex::new(3, 0));
    target.entrenchment = 2;
    let units: Vec<UnitSetup> = vec![
        target,
        unit("infantry", A, Hex::new(2, 0)),
        unit("infantry", A, Hex::new(2, 1)),
        unit("infantry", A, Hex::new(3, -1)),
    ];
    setup(map, control, units)
}

const TARGET: UnitId = UnitId(1);

/// Expected defender losses of each of the three attacks, in order.
fn three_attacks(mut s: GameState) -> (Vec<f64>, Vec<u8>) {
    let mut expected = Vec::new();
    let mut entrenchment = Vec::new();
    for attacker in [UnitId(2), UnitId(3), UnitId(4)] {
        let Some(t) = s.unit(TARGET) else { break };
        entrenchment.push(t.entrenchment);
        let f = s.forecast_attack(attacker, TARGET).unwrap();
        expected.push(f.defender_losses.expected);
        s.apply(Command::Attack {
            unit: attacker,
            target: TARGET,
        })
        .unwrap();
    }
    (expected, entrenchment)
}

#[test]
fn r_combat_02_variant_is_off_by_default() {
    let s = start(three_on_one());
    assert!(!s.variants().attack_strips_entrenchment);
    let (_, entr) = three_attacks(s);
    assert_eq!(
        entr,
        vec![2, 2, 2],
        "without the variant only bombardment strips"
    );
}

/// П1: the first attack strips the entrenchment, the next ones finish the
/// unit off — the expected losses grow from attack to attack.
#[test]
fn r_combat_02_attacks_strip_entrenchment_three_attacks_in_a_row() {
    let s = start(with("attack_strips_entrenchment", three_on_one()));
    let (e, entr) = three_attacks(s);
    assert_eq!(entr, vec![2, 1, 0]);
    assert_eq!(e.len(), 3);
    assert!(e[0] < e[1] && e[1] < e[2], "expected losses grow: {e:?}");
}

#[test]
fn unknown_variant_is_an_error() {
    let sc = with("no_such_rule", three_on_one());
    assert!(GameState::new(std::sync::Arc::new(common::data()), sc).is_err());
}
