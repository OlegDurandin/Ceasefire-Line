//! R-AI-01 «поиск котлов» (stage 4 step 7): the enemy's supply dependency
//! and its chokepoints.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::pockets::{best_pocket, chokepoints};
use ceasefire_ai::AiParams;
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{Command, GameState, ScenarioSetup, SideState, UnitId, UnitSetup};
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

/// B's salient: column q = 3 (plain, B's units at r −1..1), fed from two
/// sources by two corridors far apart: north (4,−3)–(6,−3), south (4,3)–(6,3).
/// The rest of q ≥ 4 is mountains.
fn two_corridors() -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=6 {
        for r in -3..=3 {
            let open = q <= 3 || ((r == -3 || r == 3) && q >= 4);
            map.insert(
                Hex::new(q, r),
                HexCell::new(if open { "plain" } else { "mountains" }),
            );
            control.insert(Hex::new(q, r), if q <= 2 { A } else { B });
        }
    }
    let src = |q, r| SupplySource {
        hex: Hex::new(q, r),
        value: 10,
    };
    let setup = ScenarioSetup {
        hex_km: None,
        supply_decay_ap: None,
        seed: 1,
        start_turn: 1,
        sides: vec![side(A), side(B)],
        map,
        control,
        sources: vec![src(0, 0), src(6, -3), src(6, 3)],
        units: vec![
            unit("tank", A, Hex::new(2, 0)),
            unit("tank", A, Hex::new(1, 3)), // no ZOC on B's hexes
            unit("infantry", B, Hex::new(3, -1)),
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

/// Item 2: two parallel supply paths — the AI picks a pair of chokepoints
/// (pincers), not one.
#[test]
fn r_ai_01_two_paths_need_pincers() {
    let s = two_corridors();
    let w = AiParams::from_json(AI).unwrap().pockets;
    let c = chokepoints(&s, B, w.candidates).unwrap();
    let best = c
        .iter()
        .map(|(h, _)| best_pocket(&s, A, B, *h, &w).unwrap())
        .max_by(|a, b| a.value.total_cmp(&b.value))
        .unwrap();
    assert_eq!(best.hexes.len(), 2, "{best:?}");
    assert_eq!(best.cut.len(), 3, "the whole salient is cut: {best:?}");
    let _ = best_pocket;
    // One chokepoint alone cuts nothing.
    let single = ceasefire_ai::pockets::pocket_value(&s, A, B, &best.hexes[..1], &w).unwrap();
    assert_eq!(single.0, 0.0, "{single:?}");
}

/// Test 1a: a salient fed by one road — the road hex is in the top three.
#[test]
fn r_ai_01_salient_fed_by_one_road() {
    let s = salient();
    let c = chokepoints(&s, B, k()).unwrap();
    let top3: Vec<Hex> = c.iter().take(3).map(|x| x.0).collect();
    // One trunk gives one candidate: one of the road hexes (the narrowest).
    assert!(
        top3.contains(&Hex::new(4, 0)) || top3.contains(&Hex::new(5, 0)),
        "{c:?}"
    );
}

/// Test 1b: «Синай-67» at the start, Israel's view — the Mitla and/or Giddi
/// pass is among the K candidates (supply from Suez comes by the ferry).
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
    let passes = [offset_to_axial([8, 17]), offset_to_axial([8, 15])];
    let found: Vec<[i32; 2]> = c.iter().map(|x| axial_to_offset(x.0)).collect();
    assert!(c.iter().any(|(h, _)| passes.contains(h)), "{found:?}");
}

/// Item 3: «Охват» — with pocket operations on, the pincers become an
/// operation and each chokepoint gets one of A's two tanks.
#[test]
fn r_ai_01_envelopment_assigns_tanks_to_both_chokepoints() {
    let s = two_corridors();
    let mut params = AiParams::from_json(AI).unwrap();
    params.pockets.enabled = true;
    let mut ai = ceasefire_ai::AiV1::new(params, ceasefire_ai::AiHints::default(), 1);
    let plan = ai.plan_for(&s).clone();
    let op = plan.pockets.first().expect("an operation");
    assert_eq!(op.hexes.len(), 2, "{plan:?}");
    let mut targets: Vec<Hex> = plan.envelop.values().copied().collect();
    targets.sort();
    let mut hexes = op.hexes.clone();
    hexes.sort();
    assert_eq!(targets, hexes, "one tank per chokepoint");
}

/// Item 5: q 0..=8, r −3..=3. A holds q ≤ 2 (source (0,0)), infantry at
/// (2,0) and (2,2). B holds q ≥ 3: mountains except the road (4,0)–(8,0) to
/// its source (8,0) and the salient (3,0) (`salient` terrain) and (3,1), each
/// with infantry in contact. B's reserve infantry at (7,0) if `reserve`.
/// B to move.
fn neck(reserve: bool, salient: &str, variants: &[&str]) -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=8 {
        for r in -3..=3 {
            let h = Hex::new(q, r);
            let t = if q <= 2 || (r == 0 && q >= 4) || h == Hex::new(3, 1) {
                "plain"
            } else if h == Hex::new(3, 0) {
                salient
            } else {
                "mountains"
            };
            map.insert(h, HexCell::new(t));
            control.insert(h, if q <= 2 { A } else { B });
        }
    }
    for q in 4..8 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    let mut units = vec![
        unit("infantry", A, Hex::new(2, 0)),
        unit("infantry", A, Hex::new(2, 2)),
        unit("infantry", B, Hex::new(3, 0)),
        unit("infantry", B, Hex::new(3, 1)),
    ];
    if reserve {
        units.push(unit("infantry", B, Hex::new(7, 0)));
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
                hex: Hex::new(8, 0),
                value: 10,
            },
        ],
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
        variants: variants.iter().map(|v| (v.to_string(), true)).collect(),
    };
    let mut s = GameState::new(data(), setup).unwrap().0;
    while s.active_side() != B {
        s.apply(Command::EndPhase).unwrap();
    }
    s
}

fn defend_plan(s: &GameState) -> ceasefire_ai::v1::Plan {
    let params = AiParams::from_json(AI).unwrap();
    assert!(params.defend.enabled);
    let mut ai = ceasefire_ai::AiV1::new(params, ceasefire_ai::AiHints::default(), 1);
    ai.plan_for(s).clone()
}

fn id_at(s: &GameState, h: Hex) -> UnitId {
    s.unit_at(h).unwrap().id
}

/// Item 5a: the own chokepoint is the trunk hex nearest to the enemy, and
/// the free reserve goes to it.
#[test]
fn r_ai_01_reserve_goes_to_the_own_chokepoint() {
    let s = neck(true, "plain", &[]);
    let own = ceasefire_ai::pockets::own_chokepoints(&s, B, 6).unwrap();
    assert_eq!(own.first().map(|c| c.0), Some(Hex::new(4, 0)), "{own:?}");
    let plan = defend_plan(&s);
    assert_eq!(
        plan.guard.get(&id_at(&s, Hex::new(7, 0))),
        Some(&Hex::new(4, 0)),
        "{plan:?}"
    );
    assert!(plan.withdraw.is_empty(), "{plan:?}");
}

/// Item 5b: no reserve — the units the pocket would cut withdraw past the
/// neck; with `fortified_hold`, the defender of a city stays.
#[test]
fn r_ai_01_cut_units_withdraw_unless_they_hold_a_city() {
    let s = neck(false, "plain", &[]);
    let plan = defend_plan(&s);
    let (inf, other) = (id_at(&s, Hex::new(3, 0)), id_at(&s, Hex::new(3, 1)));
    assert_eq!(plan.withdraw.get(&inf), Some(&Hex::new(5, 0)), "{plan:?}");
    assert_eq!(plan.withdraw.get(&other), Some(&Hex::new(5, 0)), "{plan:?}");

    let s = neck(false, "city", &["fortified_hold"]);
    let plan = defend_plan(&s);
    let (inf, other) = (id_at(&s, Hex::new(3, 0)), id_at(&s, Hex::new(3, 1)));
    assert!(!plan.withdraw.contains_key(&inf), "{plan:?}");
    assert!(plan.withdraw.contains_key(&other), "{plan:?}");
}

/// Item 5: no enemy near the neck — nothing to defend.
#[test]
fn r_ai_01_no_threat_no_guard() {
    let s = neck(true, "plain", &[]);
    let mut params = AiParams::from_json(AI).unwrap();
    params.defend.threat_radius = 0;
    let mut ai = ceasefire_ai::AiV1::new(params, ceasefire_ai::AiHints::default(), 1);
    let plan = ai.plan_for(&s).clone();
    assert!(
        plan.guard.is_empty() && plan.withdraw.is_empty(),
        "{plan:?}"
    );
}
