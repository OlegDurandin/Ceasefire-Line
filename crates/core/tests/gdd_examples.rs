//! Mandatory tests: worked examples from docs/gdd.md (tolerance ±0.01).

// p = 0.318 is the GDD value, not an approximation of 1/π.
#![allow(clippy::approx_constant)]

mod common;

use std::collections::BTreeMap;

use ceasefire_core::air::{
    air_fire_input, forecast_barrage, parked_strike_input, strike_input, strike_return_input,
};
use ceasefire_core::combat::{fire_params, forecast_fire};
use ceasefire_core::combat::{forecast_ground, GroundAttack};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
use ceasefire_core::movement::Occupancy;
use ceasefire_core::side::SideId;
use ceasefire_core::supply::{compute_supply, HqPosition, SupplyInput, SupplyParams, SupplySource};
use ceasefire_core::tech::{TechBranch, TechLevels};
use ceasefire_core::unit::readiness_pct;

use common::{assert_close, data, unit};

const EPS: f64 = 1e-9;
const TOL: f64 = 0.01;

#[test]
fn r_unit_01_readiness_example() {
    let d = data();
    // supply 8, morale 85%, 1 star: 90% × 0.85 × 1.05 = 80.3% → 80%
    assert_eq!(readiness_pct(&d.rules.readiness, 8, 85, 1, 0), 80);
}

fn plain(d: &ceasefire_core::data::GameData) -> GroundAttack {
    GroundAttack {
        terrain_mult: d.terrain.get("plain").unwrap().def.defense_mult,
        night: false,
        across_canal: false,
    }
}

#[test]
fn r_combat_02_tank_vs_infantry_1967() {
    let d = data();
    let tech = TechLevels::default();
    let tanks = unit(&d, "tank", &tech, 10, 90, 1, 0);
    let infantry = unit(&d, "infantry", &tech, 8, 80, 0, 2);

    let f = forecast_ground(&d.rules.combat, &tanks, &infantry, &plain(&d)).unwrap();
    let at = f.exchange.vs_defender;
    assert_eq!(at.n, 9);
    assert_close(at.a, 5.5, EPS, "A tanks");
    assert_close(at.d, 5.2, EPS, "D infantry");
    assert_close(at.p, 0.318, EPS, "p tanks");
    assert_close(f.defender_losses.expected, 2.86, TOL, "E[infantry losses]");

    let back = f.exchange.vs_attacker;
    assert_eq!(back.n, 6);
    assert_close(back.a, 2.0, EPS, "A infantry");
    assert_close(back.d, 5.5, EPS, "D tanks");
    assert_close(back.p, 0.09, EPS, "p infantry");
    assert_close(f.attacker_losses.expected, 0.54, TOL, "E[tank losses]");
}

#[test]
fn r_combat_02_tank_vs_infantry_1973() {
    let d = data();
    // 1973 per docs/decisions.md: tanks at armor level 2, infantry at
    // antitank level 2 (hard attack 2 + 4 = 6).
    let tank_tech = TechLevels::default().with(TechBranch::Armor, 2);
    let inf_tech = TechLevels::default().with(TechBranch::Antitank, 2);
    let tanks = unit(&d, "tank", &tank_tech, 10, 90, 1, 0);
    let infantry = unit(&d, "infantry", &inf_tech, 8, 80, 0, 2);
    assert_close(
        infantry.stats.attack.hard,
        6.0,
        EPS,
        "hard attack with ATGM 2",
    );

    let f = forecast_ground(&d.rules.combat, &tanks, &infantry, &plain(&d)).unwrap();
    assert_close(f.exchange.vs_defender.p, 0.318, EPS, "p tanks");
    assert_close(f.defender_losses.expected, 2.86, TOL, "E[infantry losses]");
    let back = f.exchange.vs_attacker;
    assert_eq!(back.n, 6);
    assert_close(back.a, 6.0, EPS, "A infantry");
    assert_close(back.d, 6.6, EPS, "D tanks, armor 2");
    assert_close(back.p, 0.264, EPS, "p infantry");
    assert_close(f.attacker_losses.expected, 1.58, TOL, "E[tank losses]");
}

/// GDD section 17, "Эффект ПТУР": 1967 tanks lose ≤ 1/3 of infantry losses,
/// 1973 ≥ 1/2.
#[test]
fn balance_17_antitank_effect() {
    let d = data();
    for (level, check) in [(1u8, true), (2, false)] {
        let tanks_tech = TechLevels::default().with(TechBranch::Armor, level);
        let inf_tech = TechLevels::default().with(TechBranch::Antitank, level);
        let tanks = unit(&d, "tank", &tanks_tech, 10, 90, 1, 0);
        let infantry = unit(&d, "infantry", &inf_tech, 8, 80, 0, 2);
        let f = forecast_ground(&d.rules.combat, &tanks, &infantry, &plain(&d)).unwrap();
        let ratio = f.attacker_losses.expected / f.defender_losses.expected;
        if check {
            assert!(ratio <= 1.0 / 3.0, "1967 ratio {ratio}");
        } else {
            assert!(ratio >= 0.5, "1973 ratio {ratio}");
        }
    }
}

#[test]
fn r_sup_02_el_arish_example() {
    let d = data();
    // (0,0) El Arish (8) → 6 hexes by road (3 AP) → 2 hexes of hard desert (2 AP).
    let mut map = HexMap::default();
    map.insert(Hex::new(0, 0), HexCell::new("city"));
    for q in 1..=8 {
        map.insert(Hex::new(q, 0), HexCell::new("plain"));
    }
    for q in 0..6 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    let params = SupplyParams::for_scale(&d.rules.supply, 10.0, None).unwrap();
    let sources = [SupplySource {
        hex: Hex::new(0, 0),
        value: 8,
    }];
    let occupancy = Occupancy::new();
    let control: BTreeMap<Hex, SideId> = map.hexes().map(|h| (h, SideId(0))).collect();
    let target = Hex::new(8, 0);

    let supply = |hqs: &[HqPosition]| {
        compute_supply(&SupplyInput {
            map: &map,
            terrain: &d.terrain,
            params: &params,
            side: SideId(0),
            sources: &sources,
            occupancy: &occupancy,
            control: &control,
            hqs,
        })
        .unwrap()
        .get(target)
    };

    assert_eq!(supply(&[]), 6);
    // Division HQ next to the target; its own supply is 6 ≥ 5.
    let hq = HqPosition {
        hex: Hex::new(7, 0),
        rating: 6,
    };
    assert_eq!(supply(&[hq]), 7);
}

#[test]
fn r_air_03_sam_umbrella_example() {
    let d = data();
    let sam_tech = TechLevels::default().with(TechBranch::AirDefense, 2);
    let air_tech = TechLevels::default().with(TechBranch::Aviation, 2);
    let sam = unit(&d, "sam_mobile", &sam_tech, 8, 90, 0, 0);
    let aircraft = unit(&d, "strike", &air_tech, 10, 100, 0, 0);
    assert_close(sam.stats.attack.air, 9.0, EPS, "SAM air attack, level 2");
    assert_close(
        aircraft.stats.defense.air,
        5.0,
        EPS,
        "strike air defense, level 2",
    );

    let one = forecast_barrage(&d.rules, std::slice::from_ref(&sam), &aircraft, 0.0);
    assert_eq!(one.shots.len(), 1);
    assert_eq!(one.shots[0].params.n, 2, "round(8 × 0.9 × 0.3) = 2");
    assert_close(one.shots[0].params.p, 0.54, EPS, "p SAM");
    assert_close(one.losses.expected, 1.08, TOL, "E[losses], one SAM");

    let two = forecast_barrage(&d.rules, &[sam.clone(), sam], &aircraft, 0.0);
    assert_close(two.shots[1].params.p, 0.54 * 0.8, EPS, "p second SAM");
    assert_close(two.losses.expected, 1.94, TOL, "E[losses], two SAMs");
}

/// R-AIR-02, example А: strike level 2 on entrenched infantry; terrain does
/// not count, entrenchment does.
#[test]
fn r_air_02_a_strike_on_entrenched_infantry() {
    let d = data();
    let c = &d.rules.combat;
    let air = TechLevels::default().with(TechBranch::Aviation, 2);
    let strike = unit(&d, "strike", &air, 10, 90, 0, 0);
    let inf = unit(&d, "infantry", &TechLevels::default(), 8, 80, 0, 2);
    assert_close(
        strike.stats.attack.soft,
        6.0,
        EPS,
        "strike soft attack, level 2",
    );
    let hit = fire_params(c, &strike_input(&strike, &inf));
    assert_eq!(hit.n, 9);
    assert_close(hit.d, 3.9, EPS, "D = 3 × 1,3");
    assert_close(hit.p, 0.426, EPS, "p");
    assert_close(
        forecast_fire(c, &hit, inf.strength).expected,
        3.83,
        TOL,
        "E[infantry losses]",
    );
    let back = fire_params(c, &strike_return_input(&inf, &strike));
    assert_eq!(back.n, 6);
    assert_close(back.a, 1.0, EPS, "A = infantry air attack");
    assert_close(back.d, 5.0, EPS, "D = strike air defense, level 2");
    assert_close(back.p, 0.06, EPS, "p");
    assert_close(
        forecast_fire(c, &back, strike.strength).expected,
        0.36,
        TOL,
        "E[strike losses]",
    );
}

/// R-AIR-02, example Б: a MiG-21 (level 1) meets an F-4 escort (level 2, 1★);
/// the escort takes the interceptor on, the strike group loses nothing.
#[test]
fn r_air_02_b_interceptor_against_escort() {
    let d = data();
    let c = &d.rules.combat;
    let mig = unit(
        &d,
        "fighter",
        &TechLevels::default().with(TechBranch::Aviation, 1),
        8,
        80,
        0,
        0,
    );
    let f4 = unit(
        &d,
        "fighter",
        &TechLevels::default().with(TechBranch::Aviation, 2),
        10,
        90,
        1,
        0,
    );
    assert_close(mig.stats.attack.air, 6.0, EPS, "MiG air attack");
    assert_close(f4.stats.defense.air, 6.0, EPS, "F-4 air defense");
    let escort = fire_params(c, &air_fire_input(&f4, &mig));
    assert_close(escort.p, 0.462, EPS, "p escort");
    assert_close(
        forecast_fire(c, &escort, mig.strength).expected,
        4.16,
        TOL,
        "E[MiG losses]",
    );
    let interceptor = fire_params(c, &air_fire_input(&mig, &f4));
    assert_close(interceptor.p, 0.264, EPS, "p MiG");
    assert_close(
        forecast_fire(c, &interceptor, f4.strength).expected,
        1.58,
        TOL,
        "E[escort losses]",
    );
}

/// R-AIR-02, example В: the same MiG against a strike group without escort.
#[test]
fn r_air_02_c_interceptor_against_strike_group() {
    let d = data();
    let c = &d.rules.combat;
    let mig = unit(
        &d,
        "fighter",
        &TechLevels::default().with(TechBranch::Aviation, 1),
        8,
        80,
        0,
        0,
    );
    let strike = unit(
        &d,
        "strike",
        &TechLevels::default().with(TechBranch::Aviation, 2),
        10,
        90,
        0,
        0,
    );
    let at_strike = fire_params(c, &air_fire_input(&mig, &strike));
    assert_close(at_strike.p, 0.36, EPS, "p MiG");
    assert_close(
        forecast_fire(c, &at_strike, strike.strength).expected,
        2.16,
        TOL,
        "E[strike losses]",
    );
    let back = fire_params(c, &air_fire_input(&strike, &mig));
    assert_close(back.p, 0.18, EPS, "p strike group");
    assert_close(
        forecast_fire(c, &back, mig.strength).expected,
        1.62,
        TOL,
        "E[MiG losses]",
    );
}

/// R-AIR-02, example Г: a strike on an airfield with MiGs on the ground.
#[test]
fn r_air_02_d_strike_on_parked_aircraft() {
    let d = data();
    let c = &d.rules.combat;
    let strike = unit(
        &d,
        "strike",
        &TechLevels::default().with(TechBranch::Aviation, 2),
        10,
        90,
        0,
        0,
    );
    let hit = fire_params(
        c,
        &parked_strike_input(&strike, d.rules.air.parked_air_defense),
    );
    assert_close(hit.d, 2.0, EPS, "D = parked_air_defense");
    assert_close(hit.p, 0.54, EPS, "p");
    assert_close(
        forecast_fire(c, &hit, 10).expected,
        4.86,
        TOL,
        "E[losses on the ground]",
    );
    assert_eq!(d.rules.air.airfield_strike_readiness_loss_pct, 30);
}
