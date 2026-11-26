//! R-COMBAT-02 context modifiers and R-TECH-01 levels.

mod common;

use ceasefire_core::combat::{ground_exchange, GroundAttack};
use ceasefire_core::tech::{TechBranch, TechLevels};
use ceasefire_core::RuleError;

use common::{assert_close, data, unit};

const EPS: f64 = 1e-9;

fn ctx(night: bool, across_canal: bool) -> GroundAttack {
    GroundAttack {
        terrain_mult: 1.0,
        night,
        across_canal,
        attack_mult: 1.0,
    }
}

#[test]
fn r_turn_02_night_reduces_attacker_fire_only() {
    let d = data();
    let t = TechLevels::default();
    let tanks = unit(&d, "tank", &t, 10, 90, 1, 0);
    let inf = unit(&d, "infantry", &t, 8, 80, 0, 2);
    let day = ground_exchange(&d.rules.combat, &tanks, &inf, &ctx(false, false)).unwrap();
    let night = ground_exchange(&d.rules.combat, &tanks, &inf, &ctx(true, false)).unwrap();
    assert_close(night.vs_defender.a, day.vs_defender.a * 0.8, EPS, "night A");
    assert_eq!(night.vs_attacker, day.vs_attacker);
}

#[test]
fn r_turn_02_night_combat_units_ignore_night() {
    let d = data();
    let t = TechLevels::default();
    let commando = unit(&d, "commando", &t, 8, 80, 0, 0);
    let inf = unit(&d, "infantry", &t, 8, 80, 0, 0);
    let day = ground_exchange(&d.rules.combat, &commando, &inf, &ctx(false, false)).unwrap();
    let night = ground_exchange(&d.rules.combat, &commando, &inf, &ctx(true, false)).unwrap();
    assert_eq!(day, night);
}

#[test]
fn r_move_02_canal_multiplies_defense() {
    let d = data();
    let t = TechLevels::default();
    let tanks = unit(&d, "tank", &t, 10, 90, 0, 0);
    let inf = unit(&d, "infantry", &t, 8, 80, 0, 0);
    let x = ground_exchange(&d.rules.combat, &tanks, &inf, &ctx(false, true)).unwrap();
    assert_close(x.vs_defender.d, 4.0 * 1.3, EPS, "D across canal");
}

#[test]
fn r_combat_02_zero_attack_does_not_fire() {
    let d = data();
    let t = TechLevels::default();
    let tanks = unit(&d, "tank", &t, 10, 90, 0, 0);
    let eng = unit(&d, "engineer", &t, 6, 80, 0, 0);
    let x = ground_exchange(&d.rules.combat, &tanks, &eng, &ctx(false, false)).unwrap();
    assert_eq!(x.vs_attacker.p, 0.0);
}

#[test]
fn r_combat_02_p_is_clamped() {
    let d = data();
    let t = TechLevels::default();
    let hq = unit(&d, "hq", &t, 5, 80, 0, 0);
    let tanks = unit(&d, "tank", &t, 10, 90, 3, 0);
    let x = ground_exchange(&d.rules.combat, &hq, &tanks, &ctx(false, false)).unwrap();
    assert_close(x.vs_defender.p, 0.05, EPS, "p floor");
}

#[test]
fn r_combat_02_aircraft_cannot_be_ground_targets() {
    let d = data();
    let t = TechLevels::default();
    let tanks = unit(&d, "tank", &t, 10, 90, 0, 0);
    let jet = unit(&d, "fighter", &t, 10, 90, 0, 0);
    assert_eq!(
        ground_exchange(&d.rules.combat, &tanks, &jet, &ctx(false, false)),
        Err(RuleError::NoGroundDefense("fighter".into()))
    );
}

#[test]
fn r_tech_01_armor_level_2() {
    let d = data();
    let t = TechLevels::default().with(TechBranch::Armor, 2);
    let tanks = unit(&d, "tank", &t, 10, 90, 0, 0);
    assert_close(tanks.stats.attack.hard, 6.0, EPS, "hard attack");
    assert_close(
        tanks.stats.defense.ground.unwrap(),
        6.0,
        EPS,
        "ground defense",
    );
}

#[test]
fn r_tech_01_invalid_level_is_error() {
    let d = data();
    let class = d.units.get("tank").unwrap();
    let t = TechLevels::default().with(TechBranch::Armor, 4);
    assert!(matches!(
        d.tech.unit_stats(class, &t),
        Err(RuleError::InvalidTechLevel { .. })
    ));
}
