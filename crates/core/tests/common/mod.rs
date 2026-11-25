#![allow(dead_code)]

use ceasefire_core::combat::Combatant;
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::tech::TechLevels;

pub fn data() -> GameData {
    GameData::from_json(GameDataJson {
        rules: include_str!("../../../../data/rules.json"),
        terrain: include_str!("../../../../data/terrain.json"),
        units: include_str!("../../../../data/units.json"),
        tech: include_str!("../../../../data/tech.json"),
    })
    .expect("data/*.json must load")
}

pub fn unit<'a>(
    data: &'a GameData,
    id: &str,
    tech: &TechLevels,
    strength: u8,
    readiness_pct: u8,
    exp_stars: u8,
    entrenchment: u8,
) -> Combatant<'a> {
    let class = data.units.get(id).expect("unit class");
    Combatant {
        class,
        stats: data.tech.unit_stats(class, tech).expect("stats"),
        strength,
        readiness_pct,
        exp_stars,
        entrenchment,
    }
}

pub fn assert_close(actual: f64, expected: f64, tol: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tol,
        "{what}: expected {expected} ± {tol}, got {actual}"
    );
}
