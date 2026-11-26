//! R-AI-02: same state and seed → same result; sampling matches the forecast.

mod common;

use ceasefire_core::combat::{forecast_ground, resolve_ground, GroundAttack};
use ceasefire_core::rng::Pcg32;
use ceasefire_core::tech::TechLevels;

use common::{assert_close, data, unit};

#[test]
fn same_seed_same_outcomes() {
    let d = data();
    let tech = TechLevels::default();
    let a = unit(&d, "tank", &tech, 10, 90, 1, 0);
    let b = unit(&d, "infantry", &tech, 8, 80, 0, 2);
    let ctx = GroundAttack {
        terrain_mult: 1.0,
        night: false,
        across_canal: false,
        attack_mult: 1.0,
    };
    let run = |seed| {
        let mut rng = Pcg32::new(seed, 0);
        (0..100)
            .map(|_| {
                let o = resolve_ground(&d.rules.combat, &a, &b, &ctx, &mut rng).unwrap();
                (o.defender_losses, o.attacker_losses)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(7), run(7));
    assert_ne!(run(7), run(8));
}

#[test]
fn r_combat_03_monte_carlo_matches_forecast() {
    let d = data();
    let tech = TechLevels::default();
    let a = unit(&d, "tank", &tech, 10, 90, 1, 0);
    let b = unit(&d, "infantry", &tech, 8, 80, 0, 2);
    let ctx = GroundAttack {
        terrain_mult: 1.0,
        night: false,
        across_canal: false,
        attack_mult: 1.0,
    };
    let f = forecast_ground(&d.rules.combat, &a, &b, &ctx).unwrap();
    let mut rng = Pcg32::new(2026, 1);
    let runs = 20_000;
    let (mut def, mut att) = (0u64, 0u64);
    for _ in 0..runs {
        let o = resolve_ground(&d.rules.combat, &a, &b, &ctx, &mut rng).unwrap();
        def += u64::from(o.defender_losses);
        att += u64::from(o.attacker_losses);
    }
    assert_close(
        def as f64 / runs as f64,
        f.defender_losses.expected,
        0.05,
        "MC defender",
    );
    assert_close(
        att as f64 / runs as f64,
        f.attacker_losses.expected,
        0.05,
        "MC attacker",
    );
}
