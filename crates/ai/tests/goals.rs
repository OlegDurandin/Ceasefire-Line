//! R-AI-01 «следующая цель» (rebalancing 2026-11-28): an objective's value
//! reaches the choice of goals — the dearer a far objective, the more units
//! head for it, even when the hint `sequence` names a nearer one first.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::params::SideHints;
use ceasefire_ai::{AiHints, AiParams, AiV1};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{GameState, Objective, ScenarioSetup, SideState, UnitSetup};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
use ceasefire_core::side::SideId;
use ceasefire_core::supply::SupplySource;
use ceasefire_core::tech::TechLevels;

const A: SideId = SideId(0);
const B: SideId = SideId(1);
const AI: &str = include_str!("../../../data/ai.json");
const NEAR: Hex = Hex::new(4, 0);
const FAR: Hex = Hex::new(12, 0);

fn data() -> Arc<GameData> {
    Arc::new(
        GameData::from_json(GameDataJson {
            rules: include_str!("../../../data/rules.json"),
            terrain: include_str!("../../../data/terrain.json"),
            units: include_str!("../../../data/units.json"),
            tech: include_str!("../../../data/tech.json"),
        })
        .unwrap(),
    )
}

fn side(id: SideId) -> SideState {
    SideState {
        id,
        tech: TechLevels::default(),
        national_morale_pct: 100,
        mpp: 0,
        base_income: 0,
        purchase_limits: BTreeMap::new(),
        purchase_alt: Default::default(),
        purchased: BTreeMap::new(),
        loss_sensitivity: 1,
        steps_lost: 0,
        night_combat: Vec::new(),
    }
}

fn tank(hex: Hex) -> UnitSetup {
    UnitSetup {
        class: "tank".into(),
        side: A,
        hex,
        strength: 10,
        morale_pct: 90,
        exp: 0,
        entrenchment: 0,
        supply: Some(8),
        hq_rating: None,
        formation: None,
        nation: None,
        model: None,
        name: None,
        offmap_base: None,
    }
}

/// Plain q 0..=14, r −3..=3; A holds q ≤ 1 with six tanks at q = 0 and 1.
/// Objectives of B: NEAR (4,0) at 10 VP — first in A's sequence — and FAR
/// (12,0) at `far_vp`.
fn board(far_vp: i32) -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=14 {
        for r in -3..=3 {
            map.insert(Hex::new(q, r), HexCell::new("plain"));
            control.insert(Hex::new(q, r), if q <= 1 { A } else { B });
        }
    }
    let units = [(0, -1), (0, 0), (0, 1), (1, -1), (1, 0), (1, 1)]
        .into_iter()
        .map(|(q, r)| tank(Hex::new(q, r)))
        .collect();
    let setup = ScenarioSetup {
        hex_km: None,
        supply_decay_ap: None,
        seed: 1,
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
        objectives: vec![
            Objective {
                hex: NEAR,
                nm_value: 5,
                vp: 10,
                income: 0,
            },
            Objective {
                hex: FAR,
                nm_value: 5,
                vp: far_vp,
                income: 0,
            },
        ],
        key_hexes: vec![],
        last_turn: None,
        victory: vec![],
        events: vec![],
        tech_boosts: vec![],
        neutral: vec![],
        offmap_bases: vec![],
        variants: BTreeMap::new(),
        conditions: Default::default(),
    };
    GameState::new(data(), setup).unwrap().0
}

/// Units of A whose goal is FAR.
fn to_far(far_vp: i32) -> usize {
    let s = board(far_vp);
    let mut hints = AiHints::default();
    hints.sides.insert(
        A,
        SideHints {
            aggressiveness: Some(1.0),
            priorities: BTreeMap::new(),
            sequence: vec![NEAR, FAR],
            idle_to_objectives: false,
        },
    );
    let mut ai = AiV1::new(AiParams::from_json(AI).unwrap(), hints, 1);
    ai.goal_hexes(&s).values().filter(|h| **h == FAR).count()
}

#[test]
fn r_ai_01_goals_follow_objective_value() {
    let counts: Vec<usize> = [10, 50, 200, 400].into_iter().map(to_far).collect();
    assert_eq!(counts[0], 0, "a cheap far objective waits for the sequence");
    assert!(
        counts.windows(2).all(|w| w[0] <= w[1]),
        "more value, never fewer units: {counts:?}"
    );
    assert!(
        counts[2] > counts[0],
        "a dear far objective draws units: {counts:?}"
    );
}
