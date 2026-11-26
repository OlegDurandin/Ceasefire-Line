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

/// A's infantry at (2, 0) next to B's infantry at (3, 0); `target_supply` is
/// the defender's unit supply.
fn duel(target_supply: u8) -> ScenarioSetup {
    let (map, control) = board();
    let mut target = unit("infantry", B, Hex::new(3, 0));
    target.supply = Some(target_supply);
    setup(
        map,
        control,
        vec![target, unit("infantry", A, Hex::new(1, 0))],
    )
}

const ATTACKER: UnitId = UnitId(2);

/// П2: an attacker that has not moved this phase gets +10 readiness points
/// for this attack only; after a move — none.
#[test]
fn r_combat_07_prepared_attack() {
    // The attacker standing next to the target from the start.
    let still = |variant: bool| {
        let mut sc = duel(8);
        sc.units[1].hex = Hex::new(2, 0);
        let s = start(if variant {
            with("prepared_attack", sc)
        } else {
            sc
        });
        let m = s.attack_modifiers(ATTACKER, TARGET).unwrap();
        (
            s.explain_attack(ATTACKER, TARGET).unwrap()[0].readiness_pct,
            m.prepared_pct,
        )
    };
    let (off, none) = still(false);
    let (on, bonus) = still(true);
    assert_eq!((none, bonus), (0, 10));
    assert_eq!(on, off + 10, "+10 points for this attack");
    // After a move: no bonus, even with the variant.
    let moved = |variant: bool| {
        let s = duel(8);
        let mut s = start(if variant {
            with("prepared_attack", s)
        } else {
            s
        });
        s.apply(Command::Move {
            unit: ATTACKER,
            path: vec![Hex::new(2, 0)],
        })
        .unwrap();
        (
            s.explain_attack(ATTACKER, TARGET).unwrap()[0].readiness_pct,
            s.attack_modifiers(ATTACKER, TARGET).unwrap().prepared_pct,
        )
    };
    assert_eq!(moved(true), moved(false));
    assert_eq!(moved(true).1, 0);
}

/// П3: against a unit with supply 0 the attacker's A is ×1.5; the target's
/// return fire does not change.
#[test]
fn r_combat_05_attack_on_a_unit_without_supply() {
    let fire = |variant: bool, supply: u8| {
        let mut sc = duel(supply);
        sc.units[1].hex = Hex::new(2, 0);
        let s = start(if variant {
            with("unsupplied_target", sc)
        } else {
            sc
        });
        let m = s.attack_modifiers(ATTACKER, TARGET).unwrap();
        let f = s.forecast_attack(ATTACKER, TARGET).unwrap();
        (f.exchange, m.unsupplied_target_mult)
    };
    let (base, m0) = fire(true, 3);
    let (hit, m1) = fire(true, 0);
    let (off, _) = fire(false, 0);
    assert_eq!((m0, m1), (1.0, 1.5));
    common::assert_close(hit.vs_defender.a, base.vs_defender.a * 1.5, 1e-9, "A × 1.5");
    assert!(hit.vs_defender.p > base.vs_defender.p);
    assert_eq!(off.vs_defender.a, base.vs_defender.a, "off: no change");
    assert_eq!(
        hit.vs_attacker.a, off.vs_attacker.a,
        "return fire unchanged"
    );
    assert_eq!(hit.vs_attacker.p, off.vs_attacker.p);
}

/// Road (0,0)..(6,0) on the board, all side A's.
fn road_board(units: Vec<UnitSetup>, variant: Option<&str>) -> GameState {
    let (mut map, mut control) = board();
    for q in 0..6 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    for q in 0..=6 {
        control.insert(Hex::new(q, 0), A);
    }
    let mut sc = setup(map, control, units);
    sc.sides[0].mpp = 100;
    start(match variant {
        Some(v) => with(v, sc),
        None => sc,
    })
}

/// П5: after a strategic transfer morale −15 (not below 0); it recovers by
/// R-SUP-05 (+10 in the next own phase with supply ≥ 4 and no combat).
#[test]
fn r_move_05_transfer_costs_morale() {
    let after = |variant: Option<&str>| {
        let mut s = road_board(vec![unit("infantry", A, Hex::new(0, 0))], variant);
        let before = s.unit(UnitId(1)).unwrap().morale_pct;
        s.apply(Command::Transfer {
            unit: UnitId(1),
            to: Hex::new(6, 0),
        })
        .unwrap();
        let moved = s.unit(UnitId(1)).unwrap().morale_pct;
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
        (before, moved, s.unit(UnitId(1)).unwrap().morale_pct)
    };
    let (b, m, next) = after(Some("transfer_morale"));
    assert_eq!(m, b - 15);
    assert_eq!(next, m + 10, "recovers by R-SUP-05");
    let (b, m, _) = after(None);
    assert_eq!(m, b, "off: no change");
    // Not below 0.
    let mut low = unit("infantry", A, Hex::new(0, 0));
    low.morale_pct = 4; // 14 after the phase-start rest
    let mut s = road_board(vec![low], Some("transfer_morale"));
    s.apply(Command::Transfer {
        unit: UnitId(1),
        to: Hex::new(6, 0),
    })
    .unwrap();
    assert_eq!(s.unit(UnitId(1)).unwrap().morale_pct, 0);
}
