//! Tactical battles: declared combats, results from the tactical level as
//! percentiles of the forecast, the battle snapshot, the phase lock.

mod common;

use ceasefire_core::game::{Command, Event, GameState, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::HexCell;
use ceasefire_core::prob::{hits_distribution, percentile};
use ceasefire_core::tactical::{CombatSource, Origin, Role, TacticalOutcome, TacticalScenario};
use ceasefire_core::RuleError;

use common::scenario::{board, setup, start, unit, A, B};

/// Israeli tank (1) at (2,0) next to an entrenched infantry brigade (2) at (3,0) on hills.
fn duel() -> GameState {
    let (mut map, control) = board();
    map.insert(Hex::new(3, 0), HexCell::new("hills"));
    let mut d = unit("infantry", B, Hex::new(3, 0));
    d.entrenchment = 2;
    start(setup(
        map,
        control,
        vec![unit("tank", A, Hex::new(2, 0)), d],
    ))
}

fn declare(s: &mut GameState) -> (u32, TacticalScenario) {
    let events = s
        .apply(Command::DeclareAttack {
            unit: UnitId(1),
            target: UnitId(2),
        })
        .unwrap();
    match &events[0] {
        Event::CombatDeclared {
            combat, snapshot, ..
        } => (*combat, (**snapshot).clone()),
        other => panic!("expected a declared combat, got {other:?}"),
    }
}

fn resolve(
    s: &mut GameState,
    combat: u32,
    outcome: Option<TacticalOutcome>,
) -> (u8, u8, CombatSource) {
    let events = s.apply(Command::ResolveCombat { combat, outcome }).unwrap();
    match events[0] {
        Event::CombatResolved {
            attacker_losses,
            defender_losses,
            source,
            ..
        } => (attacker_losses, defender_losses, source),
        ref other => panic!("expected a resolved combat, got {other:?}"),
    }
}

#[test]
fn declared_combat_locks_units_and_the_phase() {
    let mut s = duel();
    let (combat, _) = declare(&mut s);
    assert_eq!(s.pending_combats().len(), 1);
    assert_eq!(
        s.unit(UnitId(1)).unwrap().ap_halves,
        8,
        "1 AP charged at declaration"
    );
    assert_eq!(s.apply(Command::EndPhase), Err(RuleError::CombatsPending));
    assert_eq!(
        s.apply(Command::Move {
            unit: UnitId(1),
            path: vec![Hex::new(1, 0)]
        }),
        Err(RuleError::UnitInCombat(1))
    );
    assert_eq!(
        s.apply(Command::ResolveCombat {
            combat: 99,
            outcome: None
        }),
        Err(RuleError::UnknownCombat(99))
    );
    let (_, _, source) = resolve(&mut s, combat, None);
    assert_eq!(source, CombatSource::Auto);
    assert!(s.pending_combats().is_empty());
    s.apply(Command::EndPhase).unwrap();
}

#[test]
fn tactical_outcomes_pick_percentiles_of_the_forecast() {
    let mut results = Vec::new();
    for outcome in TacticalOutcome::ALL {
        let mut s = duel();
        let (combat, snap) = declare(&mut s);
        let (a, d, source) = resolve(&mut s, combat, Some(outcome));
        assert_eq!(source, CombatSource::Tactical { outcome });
        // Same numbers straight from the snapshot's forecast.
        let q = snap.outcome_percentiles[&outcome];
        let x = snap.forecast.exchange;
        let def = percentile(
            &hits_distribution(&[(x.vs_defender.n, x.vs_defender.p)], 10),
            q,
        );
        let att = percentile(
            &hits_distribution(&[(x.vs_attacker.n, x.vs_attacker.p)], 10),
            1.0 - q,
        );
        assert_eq!((u32::from(a), u32::from(d)), (att, def), "{outcome:?}");
        results.push((outcome, a, d));
    }
    // From decisive to rout: defender losses never grow, attacker losses never shrink.
    for w in results.windows(2) {
        assert!(w[0].2 >= w[1].2 && w[0].1 <= w[1].1, "{results:?}");
    }
    let (best, worst) = (results[0], results[4]);
    assert!(
        best.2 > worst.2 || best.1 < worst.1,
        "skill must matter: {results:?}"
    );
}

#[test]
fn draw_is_the_median_of_the_forecast() {
    let mut s = duel();
    let (combat, snap) = declare(&mut s);
    let (a, d, _) = resolve(&mut s, combat, Some(TacticalOutcome::Draw));
    let x = snap.forecast.exchange;
    assert_eq!(
        u32::from(d),
        percentile(
            &hits_distribution(&[(x.vs_defender.n, x.vs_defender.p)], 10),
            0.5
        )
    );
    assert_eq!(
        u32::from(a),
        percentile(
            &hits_distribution(&[(x.vs_attacker.n, x.vs_attacker.p)], 10),
            0.5
        )
    );
}

#[test]
fn snapshot_is_a_self_contained_mini_scenario() {
    let mut s = duel();
    let (combat, snap) = declare(&mut s);
    assert_eq!(snap.origin, Origin::Campaign { combat, turn: 1 });
    assert_eq!(snap.area.len(), 7, "defender's hex and six neighbours");
    let center = snap.area.iter().find(|h| (h.dq, h.dr) == (0, 0)).unwrap();
    assert_eq!(center.terrain, "hills");
    let roles: Vec<_> = snap
        .units
        .iter()
        .map(|u| (u.role, u.class.as_str(), u.dq, u.dr))
        .collect();
    assert_eq!(
        roles,
        vec![
            (Role::Attacker, "tank", -1, 0),
            (Role::Defender, "infantry", 0, 0)
        ]
    );
    assert_eq!(snap.units[1].entrenchment, 2);
    assert!(!snap.night);
    assert_eq!(snap.sides.len(), 2);
    // The same JSON format for campaign battles and standalone scenarios.
    let json = snap.to_json().unwrap();
    assert_eq!(TacticalScenario::from_json(&json).unwrap(), snap);
    let mut standalone = snap.clone();
    standalone.origin = Origin::Standalone {
        id: "golan_tel_fakhr".into(),
    };
    assert!(TacticalScenario::from_json(&standalone.to_json().unwrap()).is_ok());
}

#[test]
fn attack_command_is_declare_plus_auto() {
    // Same seed: Attack and DeclareAttack + ResolveCombat(None) give the same result.
    let mut a = duel();
    let direct = a
        .apply(Command::Attack {
            unit: UnitId(1),
            target: UnitId(2),
        })
        .unwrap();
    let mut b = duel();
    let (combat, _) = declare(&mut b);
    let queued = b
        .apply(Command::ResolveCombat {
            combat,
            outcome: None,
        })
        .unwrap();
    assert_eq!(direct, queued);
    assert_eq!(a.unit(UnitId(1)), b.unit(UnitId(1)));
    assert_eq!(a.unit(UnitId(2)), b.unit(UnitId(2)));
}
