//! R-AI-01 «поиск котлов» (stage 4 step 7): the enemy's supply dependency
//! and its chokepoints.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::pockets::chokepoints;
use ceasefire_ai::AiParams;
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{GameState, ScenarioSetup, SideState, UnitSetup};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
use ceasefire_core::scenario::{axial_to_offset, load_scenario, offset_to_axial};
use ceasefire_core::side::SideId;
use ceasefire_core::supply::SupplySource;
use ceasefire_core::tech::TechLevels;

const A: SideId = SideId(0);
const B: SideId = SideId(1);
const AI: &str = include_str!("../../../data/ai.json");

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

fn k() -> usize {
    AiParams::from_json(AI).unwrap().pockets.candidates
}

fn side(id: SideId) -> SideState {
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

fn unit(class: &str, side: SideId, hex: Hex) -> UnitSetup {
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

/// q 0..=6, r −3..=3. A holds q ≤ 2 (source at (0,0)). B holds q ≥ 3: all
/// mountains (impassable) except a road strip (6,0)–(5,0)–(4,0) to a salient
/// of two plain hexes (3,0), (3,1) held by B's units; B's source at (6,0).
fn salient() -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    let open = [(6, 0), (5, 0), (4, 0), (3, 0), (3, 1)];
    for q in 0..=6 {
        for r in -3..=3 {
            let plain = q <= 2 || open.contains(&(q, r));
            map.insert(
                Hex::new(q, r),
                HexCell::new(if plain { "plain" } else { "mountains" }),
            );
            control.insert(Hex::new(q, r), if q <= 2 { A } else { B });
        }
    }
    for q in 4..6 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    let setup = ScenarioSetup {
        hex_km: None,
        supply_decay_ap: None,
        seed: 1,
        start_turn: 1,
        sides: vec![side(A), side(B)],
        map,
        control,
        sources: vec![
            SupplySource {
                hex: Hex::new(0, 0),
                value: 10,
            },
            SupplySource {
                hex: Hex::new(6, 0),
                value: 10,
            },
        ],
        units: vec![
            unit("infantry", A, Hex::new(2, 0)),
            unit("infantry", B, Hex::new(3, 0)),
            unit("tank", B, Hex::new(3, 1)),
        ],
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
        variants: BTreeMap::new(),
    };
    GameState::new(data(), setup).unwrap().0
}

/// Test 1a: a salient fed by one road — the road hex is in the top three.
#[test]
fn r_ai_01_salient_fed_by_one_road() {
    let s = salient();
    let c = chokepoints(&s, B, k()).unwrap();
    let top3: Vec<Hex> = c.iter().take(3).map(|x| x.0).collect();
    assert!(top3.contains(&Hex::new(4, 0)), "{c:?}");
}

/// Test 1b: «Синай-67» at the start, Israel's view — the Mitla or Giddi
/// pass, or Bir Gifgafa (its hex or a neighbour: the hex itself holds an
/// Egyptian HQ), is among the K candidates. With no canal crossings in the
/// 1967 data all Egyptian supply on the Sinai comes from Kantara (1,7), so
/// the passes carry none (docs/decisions.md).
#[test]
fn r_ai_01_sinai_passes_are_chokepoints() {
    let setup = load_scenario(
        include_str!("../../../data/scenarios/sinai67/scenario.json"),
        include_str!("../../../data/scenarios/sinai67/map.json"),
        Some(include_str!("../../../data/scenarios/sinai67/events.json")),
    )
    .unwrap();
    let (s, _) = GameState::new(data(), setup).unwrap();
    let c = chokepoints(&s, B, k()).unwrap();
    let gifgafa = offset_to_axial([10, 13]);
    let passes = [offset_to_axial([8, 17]), offset_to_axial([8, 15])];
    let hit = c
        .iter()
        .any(|(h, _)| passes.contains(h) || h.distance(gifgafa) <= 1);
    let found: Vec<[i32; 2]> = c.iter().map(|x| axial_to_offset(x.0)).collect();
    assert!(hit, "{found:?}");
}
