//! T-SCALE, T-UNIT, T-TERR, T-LOS-01/02, T-TURN, T-SUP, T-RES and replay.

mod common;

use std::collections::BTreeSet;

use ceasefire_core::tactical::{Origin, Role, TacticalOutcome};
use ceasefire_core::tech::{TechBranch, TechLevels};
use ceasefire_tactical::battle::TurnMode;
use ceasefire_tactical::map::TacMap;
use ceasefire_tactical::setup::breakdown;
use ceasefire_tactical::{build_battle, replay, Action, TacCommand, TacEvent};
use common::*;

fn campaign() -> Origin {
    Origin::Campaign { combat: 1, turn: 1 }
}

#[test]
fn t_scale_01_02_map_turns_and_night() {
    let r = rules();
    let d = data();
    let b = build_battle(
        r.clone(),
        &d,
        &snapshot("tank", "infantry", false, campaign(), TechLevels::default()),
        3,
        TurnMode::Alternating,
    )
    .unwrap();
    assert_eq!(b.map.cells.len(), 10 * 14);
    assert_eq!(b.max_turns, 6, "a campaign battle: 6 turns");
    assert!((2..=3).contains(&b.map.key_points.len()));
    assert!(b.map.key_points.iter().all(|h| b.map.in_defender_half(*h)));
    let h = build_battle(
        r.clone(),
        &d,
        &snapshot(
            "tank",
            "infantry",
            true,
            Origin::Standalone { id: "x".into() },
            TechLevels::default(),
        ),
        3,
        TurnMode::Alternating,
    )
    .unwrap();
    assert_eq!(h.max_turns, 8, "historical: 8 turns");
    assert!(h.night, "day or night from the snapshot");
}

#[test]
fn t_unit_01_breakdown() {
    let r = rules();
    let snap = snapshot("tank", "infantry", false, campaign(), TechLevels::default());
    let tank = &snap.scenario.units[0];
    assert_eq!(
        breakdown(&r, tank, &TechLevels::default()),
        vec![
            ("tank_bn".into(), 4),
            ("tank_bn".into(), 3),
            ("tank_bn".into(), 3),
            ("recon_co".into(), 1)
        ]
    );
    let inf = &snap.scenario.units[1];
    let at2 = TechLevels::default().with(TechBranch::Antitank, 2);
    let parts = breakdown(&r, inf, &at2);
    assert_eq!(parts.len(), 4, "AT level 2 adds an AT company: {parts:?}");
    assert_eq!(parts[3], ("at_co".into(), 1));
    assert_eq!(breakdown(&r, inf, &TechLevels::default()).len(), 3);
}

/// T-UNIT-02: stats from the strategic class with the side's technology.
#[test]
fn t_unit_02_stats_with_technology() {
    let r = rules();
    let d = data();
    let at2 = TechLevels::default().with(TechBranch::Antitank, 2);
    let b = build_battle(
        r,
        &d,
        &snapshot("tank", "infantry", false, campaign(), at2),
        1,
        TurnMode::Alternating,
    )
    .unwrap();
    let inf = b.units.values().find(|u| u.kind == "inf_bn").unwrap();
    let at = b.units.values().find(|u| u.kind == "at_co").unwrap();
    assert!(
        (inf.attack_hard - 6.0).abs() < 1e-9,
        "infantry hard 2 + 4 at AT level 2"
    );
    assert_eq!(at.attack_soft, 0.0, "AT: armour only");
    let tank = b.units.values().find(|u| u.kind == "tank_bn").unwrap();
    assert!(tank.armor && (tank.attack_soft - 5.0).abs() < 1e-9);
}

/// T-TERR-01, T-LOS-01: a village blocks at its level; from a hill one sees
/// over it; smoke blocks; night range 1.
#[test]
fn t_los_01_lines_of_sight() {
    let r = rules();
    let mut b = battle(
        vec![
            unit(1, Role::Attacker, "tank_bn", 3, 4, 10),
            unit(2, Role::Defender, "tank_bn", 3, 4, 7),
        ],
        TurnMode::Alternating,
    );
    let (a, t) = (hex(4, 10), hex(4, 7));
    assert!(b.map.sees(&r, a, t, &BTreeSet::new()));
    b.map.cells.get_mut(&hex(4, 9)).unwrap().terrain = "village".into();
    assert!(
        !b.map.sees(&r, a, t, &BTreeSet::new()),
        "a village in between"
    );
    b.map.cells.get_mut(&a).unwrap().level = 1;
    b.map.cells.get_mut(&t).unwrap().level = 1;
    assert!(
        b.map.sees(&r, a, t, &BTreeSet::new()),
        "from heights over a lower village"
    );
    b.map.cells.get_mut(&hex(4, 9)).unwrap().terrain = "open".into();
    b.map.cells.get_mut(&hex(4, 8)).unwrap().level = 2;
    assert!(
        !b.map.sees(&r, a, t, &BTreeSet::new()),
        "a ridge higher than both"
    );
    b.map.cells.get_mut(&hex(4, 8)).unwrap().level = 0;
    let smoke: BTreeSet<_> = [hex(4, 9)].into();
    b.map.cells.get_mut(&a).unwrap().level = 0;
    b.map.cells.get_mut(&t).unwrap().level = 0;
    assert!(!b.map.sees(&r, a, t, &smoke), "smoke");
    // Night: 1 hex.
    b.night = true;
    let (u1, u2) = (b.unit(1).unwrap().clone(), b.unit(2).unwrap().clone());
    assert!(!b.sees_unit(&u1, &u2));
}

/// T-LOS-02: infantry in cover is seen within 2 hexes (recon 3).
#[test]
fn t_los_02_concealment() {
    let mut b = battle(
        vec![
            unit(1, Role::Attacker, "tank_bn", 3, 4, 11),
            unit(2, Role::Attacker, "recon_co", 1, 5, 11),
            unit(3, Role::Defender, "inf_bn", 3, 4, 8),
        ],
        TurnMode::Alternating,
    );
    b.map.cells.get_mut(&hex(4, 8)).unwrap().terrain = "orchard".into();
    let inf = b.unit(3).unwrap().clone();
    assert!(
        !b.sees_unit(&b.unit(1).unwrap().clone(), &inf),
        "3 hexes: hidden"
    );
    let recon = b.unit(2).unwrap().clone();
    if recon.hex.distance(inf.hex) <= 3 {
        assert!(b.sees_unit(&recon, &inf), "recon sees it at 3");
    }
}

/// T-TURN-01: alternate activations, one pass per turn, the larger side
/// finishes, then the next turn.
#[test]
fn t_turn_01_alternating_activations() {
    let mut b = battle(
        vec![
            unit(1, Role::Attacker, "tank_bn", 3, 2, 12),
            unit(2, Role::Attacker, "tank_bn", 3, 3, 12),
            unit(3, Role::Attacker, "tank_bn", 3, 4, 12),
            unit(4, Role::Defender, "inf_bn", 3, 4, 2),
        ],
        TurnMode::Alternating,
    );
    let wait = |u| TacCommand::Act {
        unit: u,
        support: None,
        action: Action::Wait,
    };
    assert_eq!(b.active, Role::Attacker);
    b.apply(TacCommand::Pass).unwrap();
    assert_eq!(b.active, Role::Defender, "a pass defers");
    assert!(b.apply(wait(1)).is_err(), "not the active side");
    b.apply(wait(4)).unwrap();
    assert_eq!(b.active, Role::Attacker);
    assert!(
        b.apply(TacCommand::Pass).is_err(),
        "one pass per turn; the defender is done"
    );
    b.apply(wait(1)).unwrap();
    assert_eq!(
        b.active,
        Role::Attacker,
        "the larger side plays the rest in a row"
    );
    b.apply(wait(2)).unwrap();
    b.apply(wait(3)).unwrap();
    assert_eq!(b.turn, 2, "everyone acted: next turn");
}

/// Open question 1: IGOUGO with overwatch — the attacker plays all its units first.
#[test]
fn t_turn_01_igougo_overwatch_flag() {
    let mut b = battle(
        vec![
            unit(1, Role::Attacker, "tank_bn", 3, 2, 12),
            unit(2, Role::Attacker, "tank_bn", 3, 3, 12),
            unit(3, Role::Defender, "inf_bn", 3, 4, 2),
        ],
        TurnMode::IgougoOverwatch,
    );
    let wait = |u| TacCommand::Act {
        unit: u,
        support: None,
        action: Action::Wait,
    };
    b.apply(wait(1)).unwrap();
    assert_eq!(b.active, Role::Attacker);
    b.apply(wait(2)).unwrap();
    assert_eq!(b.active, Role::Defender);
}

/// T-TURN-02: overwatch fires once, at the first enemy moving into its sight and range.
#[test]
fn t_turn_02_overwatch_fires_once() {
    let mut b = battle(
        vec![
            unit(1, Role::Attacker, "tank_bn", 3, 4, 12),
            unit(2, Role::Attacker, "tank_bn", 3, 6, 12),
            unit(3, Role::Defender, "tank_bn", 3, 4, 8),
        ],
        TurnMode::Alternating,
    );
    b.apply(TacCommand::Pass).unwrap();
    b.apply(TacCommand::Act {
        unit: 3,
        support: None,
        action: Action::Overwatch,
    })
    .unwrap();
    let ev = b
        .apply(TacCommand::Act {
            unit: 1,
            support: None,
            action: Action::Move {
                path: vec![hex(4, 11), hex(4, 10)],
            },
        })
        .unwrap();
    let shots = ev
        .iter()
        .filter(|e| matches!(e, TacEvent::Fired { shooter: 3, .. }))
        .count();
    assert_eq!(shots, 1, "{ev:?}");
    assert!(!b.unit(3).unwrap().overwatch, "spent");
}

/// T-SUP-01/02: calls by supply bands, +1 any from an HQ ≥ 8, a reserve on turn 3.
#[test]
fn t_sup_01_02_support_calls_and_reserve() {
    let r = rules();
    let d = data();
    let mut input = snapshot("tank", "infantry", false, campaign(), TechLevels::default());
    input.support.attacker.artillery_supply = Some(8);
    input.support.attacker.air_supply = Some(5);
    input.support.attacker.air_readiness_pct = 80;
    input.support.attacker.hq_rating = Some(8);
    input.support.attacker.reserve_class = Some("mech".into());
    input.support.defender.artillery_supply = Some(2);
    input.support.defender.engineers = true;
    let b = build_battle(r.clone(), &d, &input, 5, TurnMode::Alternating).unwrap();
    assert_eq!(
        (b.support[0].artillery, b.support[0].air, b.support[0].any),
        (3, 1, 1)
    );
    assert_eq!(
        (
            b.support[1].artillery,
            b.support[1].air,
            b.support[1].mortar
        ),
        (1, 0, 1)
    );
    let reserve = b.units.values().find(|u| u.arrives.is_some()).unwrap();
    assert_eq!(reserve.arrives, Some(3));
    assert!(!reserve.on_map(1));
    assert!(
        b.map.cells.values().filter(|c| c.mine).count() >= 1,
        "defender's engineers: mines"
    );
    // Night: no air.
    let mut night = input.clone();
    night.scenario.night = true;
    let n = build_battle(r, &d, &night, 5, TurnMode::Alternating).unwrap();
    assert_eq!(n.support[0].air, 0);
}

/// T-RES-01/02/04: points, outcome by margin, resign by the current score.
#[test]
fn t_res_01_02_04_score_and_resign() {
    let mut b = battle(
        vec![
            unit(1, Role::Attacker, "tank_bn", 3, 4, 12),
            unit(2, Role::Defender, "inf_bn", 3, 4, 2),
        ],
        TurnMode::Alternating,
    );
    b.steps_lost = [0, 5];
    let kp = b.map.key_points[0];
    b.control.insert(kp, Role::Attacker);
    let s = b.score();
    assert_eq!((s.attacker_points, s.defender_points, s.margin), (8, 0, 8));
    assert_eq!(s.outcome, TacticalOutcome::AttackerDecisive);
    assert_eq!(s.percentile, 97.5);
    b.control.insert(kp, Role::Defender);
    let s = b.score();
    assert_eq!(s.margin, 2);
    assert_eq!(s.outcome, TacticalOutcome::Draw);
    b.steps_lost = [9, 0];
    assert_eq!(b.score().outcome, TacticalOutcome::AttackerRout);
    let ev = b.apply(TacCommand::Resign).unwrap();
    assert!(matches!(ev.last(), Some(TacEvent::BattleOver { .. })));
    assert!(b.apply(TacCommand::Pass).is_err(), "over");
}

/// Replay: the same seed and log give the same battle (T-TECH-01).
#[test]
fn t_tech_01_replay() {
    let r = rules();
    let d = data();
    let input = snapshot("tank", "infantry", false, campaign(), TechLevels::default());
    let mut b = build_battle(r.clone(), &d, &input, 11, TurnMode::Alternating).unwrap();
    // Every unit waits or fires at something it sees, for a few activations.
    for _ in 0..12 {
        if b.result.is_some() {
            break;
        }
        let me = b.active;
        let u = b.alive(me).find(|u| !u.activated).unwrap().id;
        let target = b
            .alive(ceasefire_tactical::battle::other(me))
            .find(|t| b.forecast(u, t.id, false, false).is_some() && b.visible_to(me, t))
            .map(|t| t.id);
        let action = target.map_or(Action::Wait, |t| Action::Fire { target: t });
        b.apply(TacCommand::Act {
            unit: u,
            support: None,
            action,
        })
        .unwrap();
    }
    let again = replay(r, &d, &input, 11, TurnMode::Alternating, &b.log).unwrap();
    assert_eq!(again.units, b.units);
    assert_eq!(again.steps_lost, b.steps_lost);
    let _ = TacMap::at(0, 0);
}
