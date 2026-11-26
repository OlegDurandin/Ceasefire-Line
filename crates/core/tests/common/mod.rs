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

/// Small scenarios for GameState tests.
pub mod scenario {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use ceasefire_core::game::{GameState, ScenarioSetup, SideState, UnitSetup};
    use ceasefire_core::hex::Hex;
    use ceasefire_core::map::{HexCell, HexMap};
    use ceasefire_core::side::SideId;
    use ceasefire_core::supply::SupplySource;
    use ceasefire_core::tech::TechLevels;

    pub const A: SideId = SideId(0);
    pub const B: SideId = SideId(1);

    /// Plain hexes q in 0..=6, r in -3..=3; side A controls q <= 2, side B the rest.
    pub fn board() -> (HexMap, BTreeMap<Hex, SideId>) {
        let mut map = HexMap::default();
        let mut control = BTreeMap::new();
        for q in 0..=6 {
            for r in -3..=3 {
                map.insert(Hex::new(q, r), HexCell::new("plain"));
                control.insert(Hex::new(q, r), if q <= 2 { A } else { B });
            }
        }
        (map, control)
    }

    pub fn unit(class: &str, side: SideId, hex: Hex) -> UnitSetup {
        UnitSetup {
            class: class.into(),
            side,
            hex,
            strength: 10,
            morale_pct: 90,
            exp: 0,
            entrenchment: 0,
            supply: Some(8),
            hq_rating: None,
            formation: None,
            nation: None,
            name: None,
            offmap_base: None,
        }
    }

    pub fn setup(
        map: HexMap,
        control: BTreeMap<Hex, SideId>,
        units: Vec<UnitSetup>,
    ) -> ScenarioSetup {
        ScenarioSetup {
            hex_km: None,
            supply_decay_ap: None,
            seed: 42,
            start_turn: 1,
            sides: vec![side(A), side(B)],
            map,
            control,
            sources: vec![SupplySource {
                hex: Hex::new(0, 0),
                value: 10,
            }],
            units,
            cities: vec![],
            scheduled_income: vec![],
            objectives: vec![],
            key_hexes: vec![],
            last_turn: None,
            victory: vec![],
            events: vec![],
            tech_boosts: vec![],
            neutral: vec![],
            offmap_bases: vec![],
        }
    }

    pub fn side(id: SideId) -> SideState {
        SideState {
            id,
            tech: TechLevels::default(),
            national_morale_pct: 100,
            mpp: 0,
            base_income: 0,
            purchase_limits: BTreeMap::new(),
            purchased: BTreeMap::new(),
            loss_sensitivity: 1,
            steps_lost: 0,
        }
    }

    /// HQ of rating `rating`: neutral effect at 5 (R-HQ-02).
    pub fn hq(side: SideId, hex: Hex, rating: u8) -> UnitSetup {
        UnitSetup {
            hq_rating: Some(rating),
            ..unit("hq", side, hex)
        }
    }

    pub fn start(setup: ScenarioSetup) -> GameState {
        GameState::new(Arc::new(super::data()), setup)
            .expect("valid setup")
            .0
    }
}
