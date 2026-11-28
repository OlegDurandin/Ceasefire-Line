//! R-AI-01 role «Рывок» (rebalancing 2026-11-28): a tank heading for a far
//! objective goes round enemy ZOC instead of fighting a neighbour that does
//! not bar its way.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::params::SideHints;
use ceasefire_ai::{AiHints, AiParams, AiV1};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{Command, GameState, Objective, ScenarioSetup, SideState, UnitSetup};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
use ceasefire_core::side::SideId;
use ceasefire_core::supply::SupplySource;
use ceasefire_core::tech::TechLevels;

const A: SideId = SideId(0);
const B: SideId = SideId(1);
const AI: &str = include_str!("../../../data/ai.json");
const GOAL: Hex = Hex::new(9, -2);

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

/// A unit from JSON, so optional setup fields keep their defaults.
fn unit(class: &str, side: SideId, hex: Hex, strength: u8) -> UnitSetup {
    serde_json::from_value(serde_json::json!({
        "class": class,
        "side": side,
        "hex": hex,
        "strength": strength,
        "morale_pct": 90,
        "exp": 0,
        "entrenchment": 0,
        "supply": 8,
        "hq_rating": null,
        "formation": null,
        "nation": null,
        "name": null,
        "offmap_base": null
    }))
    .unwrap()
}

/// Plain q 0..=10, r −3..=3; A holds q ≤ 2. A's tank at (2,0); B's weak
/// infantry at (3,0) next to it (the tank starts in its ZOC); B's objective
/// GOAL (9,−2), 50 VP. Round the north — (2,−1), (3,−2), (4,−2), (5,−2) — the
/// tank gets past the infantry (closer to GOAL than it is) without entering ZOC.
fn board() -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=10 {
        for r in -3..=3 {
            map.insert(Hex::new(q, r), HexCell::new("plain"));
            control.insert(Hex::new(q, r), if q <= 2 { A } else { B });
        }
    }
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
        units: vec![
            unit("tank", A, Hex::new(2, 0), 10),
            unit("infantry", B, Hex::new(3, 0), 5),
            // Rear guard of A's source, so the tank is free for the attack.
            unit("infantry", A, Hex::new(0, 1), 10),
        ],
        cities: vec![],
        scheduled_income: vec![],
        objectives: vec![Objective {
            hex: GOAL,
            nm_value: 5,
            vp: 50,
            income: 0,
        }],
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

fn first_command(dash_min_distance: u32) -> Command {
    let s = board();
    let mut hints = AiHints::default();
    hints.sides.insert(
        A,
        SideHints {
            aggressiveness: Some(1.0),
            priorities: BTreeMap::new(),
            sequence: vec![GOAL],
            idle_to_objectives: false,
        },
    );
    let mut params = AiParams::from_json(AI).unwrap();
    params.moves.dash_min_distance = dash_min_distance;
    let mut ai = AiV1::new(params, hints, 1);
    ai.next(&s)
}

#[test]
fn r_ai_01_dash_goes_round_instead_of_attacking() {
    match first_command(4) {
        Command::Move { path, .. } => {
            let end = *path.last().unwrap();
            assert!(
                end.distance(GOAL) < Hex::new(2, 0).distance(GOAL),
                "closer to the goal: {path:?}"
            );
            assert!(
                end.distance(Hex::new(3, 0)) > 1,
                "out of the enemy's ZOC: {path:?}"
            );
        }
        other => panic!("expected a move round the enemy, got {other:?}"),
    }
}

#[test]
fn r_ai_01_without_dash_the_tank_attacks_its_neighbour() {
    // Control: the same position with the role off — the weak neighbour is attacked.
    assert!(matches!(first_command(99), Command::Attack { .. }));
}
