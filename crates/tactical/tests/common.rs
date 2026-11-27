#![allow(dead_code)]
//! Helpers: rules, game data, a hand-made open battle, a synthetic snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ceasefire_core::combat::{Exchange, ExchangeForecast, FireParams};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::hex::Hex;
use ceasefire_core::prob::LossForecast;
use ceasefire_core::rng::Pcg32;
use ceasefire_core::tactical::{
    AreaHex, Origin, Role, TacticalScenario, TacticalSide, TacticalSupport, TacticalUnit,
};
use ceasefire_core::tech::TechLevels;
use ceasefire_tactical::battle::{Battle, Support, TacUnit, TurnMode};
use ceasefire_tactical::map::{Cell, TacMap};
use ceasefire_tactical::{BattleInput, TacRules};

pub fn rules() -> Arc<TacRules> {
    Arc::new(TacRules::from_json(include_str!("../../../data/tactical.json")).unwrap())
}

pub fn data() -> GameData {
    GameData::from_json(GameDataJson {
        rules: include_str!("../../../data/rules.json"),
        terrain: include_str!("../../../data/terrain.json"),
        units: include_str!("../../../data/units.json"),
        tech: include_str!("../../../data/tech.json"),
    })
    .unwrap()
}

/// An all-open 10 × 14 map.
pub fn open_map(r: &TacRules) -> TacMap {
    let mut cells = BTreeMap::new();
    for row in 0..r.map.rows {
        for col in 0..r.map.cols {
            cells.insert(
                TacMap::at(col, row),
                Cell {
                    terrain: "open".into(),
                    level: 0,
                    mine: false,
                    mine_known: false,
                },
            );
        }
    }
    TacMap {
        cols: r.map.cols,
        rows: r.map.rows,
        cells,
        key_points: vec![TacMap::at(5, 3)],
        canal: BTreeSet::new(),
        bridges: BTreeSet::new(),
    }
}

pub fn unit(id: u32, role: Role, kind: &str, steps: u8, col: i32, row: i32) -> TacUnit {
    let (soft, hard, def, armor) = match kind {
        "tank_bn" => (5.0, 5.0, 5.0, true),
        "mech_bn" | "recon_co" => (4.0, 3.0, 4.0, true),
        "at_co" => (0.0, 6.0, 4.0, false),
        _ => (4.0, 2.0, 4.0, false),
    };
    TacUnit {
        id,
        role,
        kind: kind.into(),
        attack_soft: soft,
        attack_hard: hard,
        defense: def,
        air_defense: 3.0,
        armor,
        steps,
        damage: 0,
        readiness_pct: 100,
        exp_stars: 0,
        entrench: 0,
        hex: TacMap::at(col, row),
        arrives: None,
        revealed_until: 0,
        suppressed: false,
        hit_this_turn: false,
        moved_this_turn: false,
        overwatch: false,
        activated: false,
    }
}

/// A hand-made battle on an open map, day, 6 turns.
pub fn battle(units: Vec<TacUnit>, mode: TurnMode) -> Battle {
    let r = rules();
    let map = open_map(&r);
    let control = map
        .key_points
        .iter()
        .map(|h| (*h, Role::Defender))
        .collect();
    let mut b = Battle {
        rules: Arc::clone(&r),
        map,
        units: units.into_iter().map(|u| (u.id, u)).collect(),
        turn: 1,
        max_turns: 6,
        night: false,
        mode,
        active: Role::Attacker,
        passed: [false; 2],
        activations: 0,
        smoke: BTreeMap::new(),
        lit: BTreeMap::new(),
        fired_from: BTreeMap::new(),
        control,
        steps_lost: [0, 0],
        support: [Support::default(), Support::default()],
        pending_bridges: Vec::new(),
        rng: Pcg32::new(7, 1),
        result: None,
        log: Vec::new(),
        reference: None,
    };
    let mut ev = Vec::new();
    b.start_turn(&mut ev);
    b
}

fn lf() -> LossForecast {
    LossForecast {
        expected: 0.0,
        low: 0,
        high: 0,
    }
}

/// A snapshot: `att_class` attacks `def_class`, both strength 10.
pub fn snapshot(
    att_class: &str,
    def_class: &str,
    night: bool,
    origin: Origin,
    def_tech: TechLevels,
) -> BattleInput {
    let fp = FireParams {
        n: 0,
        a: 0.0,
        d: 0.0,
        p: 0.0,
    };
    let area = std::iter::once((0, 0))
        .chain([(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)])
        .map(|(dq, dr)| AreaHex {
            dq,
            dr,
            terrain: "plain".into(),
            pass: false,
            owner: Some(if (dq, dr) == (0, 1) { 0 } else { 1 }),
        })
        .collect();
    let unit = |side: u8, role: Role, class: &str, dq: i32, dr: i32| TacticalUnit {
        side,
        role,
        class: class.into(),
        nation: None,
        strength: 10,
        readiness_pct: 90,
        morale_pct: 90,
        exp: 0,
        entrenchment: 2,
        dq,
        dr,
    };
    BattleInput {
        scenario: TacticalScenario {
            format: 1,
            origin,
            night,
            hex_km: 10.0,
            across_canal: false,
            area,
            edges: vec![],
            sides: vec![
                TacticalSide {
                    side: 0,
                    role: Role::Attacker,
                    tech: TechLevels::default(),
                    support: TacticalSupport::default(),
                },
                TacticalSide {
                    side: 1,
                    role: Role::Defender,
                    tech: def_tech,
                    support: TacticalSupport::default(),
                },
            ],
            units: vec![
                unit(0, Role::Attacker, att_class, 0, 1),
                unit(1, Role::Defender, def_class, 0, 0),
            ],
            forecast: ExchangeForecast {
                exchange: Exchange {
                    vs_defender: fp,
                    vs_attacker: fp,
                },
                defender_losses: lf(),
                attacker_losses: lf(),
            },
            outcome_percentiles: BTreeMap::new(),
        },
    }
}

pub fn hex(col: i32, row: i32) -> Hex {
    TacMap::at(col, row)
}
