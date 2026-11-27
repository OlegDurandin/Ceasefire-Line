//! T-COMBAT-01/02/03, T-LOS-03: fire, damage, suppression, assault, ambush.

mod common;

use ceasefire_core::tactical::Role;
use ceasefire_tactical::battle::TurnMode;
use ceasefire_tactical::{Action, TacCommand, TacEvent};
use common::*;

/// Section 6 example: a tank battalion (3 steps, readiness 90%, soft attack
/// 5) fires from 2 hexes at infantry in the open (defense 4, entrenchment 1):
/// n = 5, A = 4, D = 4.6, p = 0.264, expected 1.32 damage, P(suppression) ≈ 0.78.
#[test]
fn t_combat_01_example_of_section_6() {
    let mut tank = unit(1, Role::Attacker, "tank_bn", 3, 4, 10);
    tank.readiness_pct = 90;
    let mut inf = unit(2, Role::Defender, "inf_bn", 3, 4, 8);
    inf.entrench = 1;
    let b = battle(vec![tank, inf], TurnMode::Alternating);
    // The defender sees the tank: no ambush.
    let mut b = b;
    b.units.get_mut(&1).unwrap().revealed_until = 99;
    let f = b.forecast(1, 2, false, false).unwrap();
    assert_eq!(f.params.n, 5);
    assert!((f.params.a - 4.0).abs() < 1e-9);
    assert!((f.params.d - 4.6).abs() < 1e-9);
    assert!((f.params.p - 0.264).abs() < 1e-9);
    assert!((f.expected - 1.32).abs() < 0.01);
    assert!((f.p_suppress - 0.78).abs() < 0.01);
    assert!(!f.ambush);
}

/// Hits are damage; 2 damage is a step; the rest carries over.
#[test]
fn t_combat_01_damage_carries_over() {
    let shooter = unit(1, Role::Attacker, "tank_bn", 4, 4, 9);
    let target = unit(2, Role::Defender, "inf_bn", 3, 4, 8);
    let mut b = battle(vec![shooter, target], TurnMode::Alternating);
    let ev = b
        .apply(TacCommand::Act {
            unit: 1,
            support: None,
            action: Action::Fire { target: 2 },
        })
        .unwrap();
    let hits: u32 = ev
        .iter()
        .filter_map(|e| match e {
            TacEvent::Fired {
                target: 2, hits, ..
            } => Some(*hits),
            _ => None,
        })
        .sum();
    let t = b.unit(2).unwrap();
    assert_eq!(
        u32::from(3 - t.steps) * 2 + u32::from(t.damage),
        hits.min(6)
    );
    if hits > 0 {
        assert!(t.suppressed, "any hit suppresses");
    }
}

/// T-COMBAT-02: a suppressed unit cannot assault or overwatch; its fire is ×0.5.
#[test]
fn t_combat_02_suppression() {
    let mut a = unit(1, Role::Attacker, "inf_bn", 3, 4, 9);
    a.suppressed = true;
    a.revealed_until = 99;
    let d = unit(2, Role::Defender, "inf_bn", 3, 4, 8);
    let mut b = battle(vec![a, d], TurnMode::Alternating);
    b.units.get_mut(&1).unwrap().suppressed = true;
    assert!(b
        .apply(TacCommand::Act {
            unit: 1,
            support: None,
            action: Action::Assault { target: 2 }
        })
        .is_err());
    assert!(b
        .apply(TacCommand::Act {
            unit: 1,
            support: None,
            action: Action::Overwatch
        })
        .is_err());
    let s = b.forecast(1, 2, false, false).unwrap();
    b.units.get_mut(&1).unwrap().suppressed = false;
    let n = b.forecast(1, 2, false, false).unwrap();
    assert!((s.params.a - n.params.a * 0.5).abs() < 1e-9);
}

/// T-COMBAT-03: a strong assault on a 1-step defender — it withdraws (or is
/// destroyed) and the attacker takes the hex.
#[test]
fn t_combat_03_assault_takes_the_hex() {
    let a = unit(1, Role::Attacker, "tank_bn", 4, 4, 9);
    let d = unit(2, Role::Defender, "inf_bn", 1, 4, 8);
    let mut b = battle(vec![a, d], TurnMode::Alternating);
    let target_hex = hex(4, 8);
    b.apply(TacCommand::Act {
        unit: 1,
        support: None,
        action: Action::Assault { target: 2 },
    })
    .unwrap();
    let a = b.unit(1).unwrap();
    if a.steps > 0 {
        assert_eq!(a.hex, target_hex, "the attacker occupies");
        assert!(b
            .unit(2)
            .is_none_or(|d| d.hex != target_hex || d.steps == 0));
    }
}

/// T-COMBAT-03: tanks without infantry assault cover at ×0.7; with own
/// infantry next to the target at ×1.2.
#[test]
fn t_combat_03_combined_arms() {
    let tank = unit(1, Role::Attacker, "tank_bn", 4, 4, 9);
    let d = unit(2, Role::Defender, "inf_bn", 3, 4, 8);
    let mut b = battle(vec![tank.clone(), d.clone()], TurnMode::Alternating);
    b.map.cells.get_mut(&hex(4, 8)).unwrap().terrain = "village".into();
    let alone = b.forecast(1, 2, false, true).unwrap().params.a;
    let inf = unit(3, Role::Attacker, "inf_bn", 3, 3, 8);
    let mut b2 = battle(vec![tank, d, inf], TurnMode::Alternating);
    b2.map.cells.get_mut(&hex(4, 8)).unwrap().terrain = "village".into();
    let together = b2.forecast(1, 2, false, true).unwrap().params.a;
    assert!((alone - 5.0 * 0.7).abs() < 1e-9, "{alone}");
    assert!((together - 5.0 * 1.2).abs() < 1e-9, "{together}");
}

/// T-LOS-03: the first shot of an unseen unit is ×1.3 and draws no reply.
/// An AT company in a village 3 hexes off (concealed: seen within 2 only).
#[test]
fn t_los_03_ambush() {
    let at = unit(1, Role::Attacker, "at_co", 1, 4, 11);
    let tank = unit(2, Role::Defender, "tank_bn", 3, 4, 8);
    let mut b = battle(vec![at, tank], TurnMode::Alternating);
    b.map.cells.get_mut(&hex(4, 11)).unwrap().terrain = "village".into();
    b.units.get_mut(&2).unwrap().revealed_until = 99; // the tank is seen
    let f = b.forecast(1, 2, false, false).unwrap();
    assert!(f.ambush, "concealed at 3 hexes: an ambush");
    assert!((f.params.a - 6.0 * 1.3).abs() < 1e-9, "{}", f.params.a);
    let ev = b
        .apply(TacCommand::Act {
            unit: 1,
            support: None,
            action: Action::Fire { target: 2 },
        })
        .unwrap();
    let replies = ev
        .iter()
        .filter(|e| matches!(e, TacEvent::Fired { shooter: 2, .. }))
        .count();
    assert_eq!(replies, 0, "no return fire");
    // Firing reveals it.
    assert!(b.visible_to(Role::Defender, b.unit(1).unwrap()));
}
