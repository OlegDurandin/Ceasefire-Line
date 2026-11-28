//! R-AI-01 «Отход» (designer decision 2026-11-27): with open exits, a unit
//! leaves when its steps are worth more through the exit than holding.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::{AiHints, AiParams, AiV1};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{
    Command, ExitZone, GameState, Objective, ScenarioSetup, SideState, UnitId, UnitSetup,
};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
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

fn unit(class: &str, side: SideId, hex: Hex) -> UnitSetup {
    UnitSetup {
        class: class.into(),
        side,
        hex,
        strength: 10,
        morale_pct: 90,
        exp: 0,
        entrenchment: 0,
        supply: Some(10),
        hq_rating: None,
        formation: None,
        nation: None,
        name: None,
        offmap_base: None,
    }
}

/// Plain q 0..=8, r −3..=3; A holds q ≤ 3 (infantry at (3,0)), B holds q ≥ 4
/// (infantry at (5,0), source (8,0)) with an open exit at (7,0), 2 VP a step.
/// `objective`: (5,0) is B's objective worth 60 VP.
fn board(objective: bool) -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=8 {
        for r in -3..=3 {
            map.insert(Hex::new(q, r), HexCell::new("plain"));
            control.insert(Hex::new(q, r), if q <= 3 { A } else { B });
        }
    }
    let src = |q, r| SupplySource {
        hex: Hex::new(q, r),
        value: 10,
    };
    let mut sc = ScenarioSetup {
        hex_km: None,
        supply_decay_ap: None,
        seed: 1,
        start_turn: 1,
        sides: vec![side(A), side(B)],
        map,
        control,
        sources: vec![src(0, 0), src(8, 0)],
        units: vec![
            unit("infantry", A, Hex::new(3, 0)),
            unit("infantry", B, Hex::new(5, 0)),
        ],
        cities: vec![],
        scheduled_income: vec![],
        objectives: if objective {
            vec![Objective {
                hex: Hex::new(5, 0),
                nm_value: 15,
                vp: 60,
                income: 0,
            }]
        } else {
            vec![]
        },
        key_hexes: vec![],
        last_turn: Some(8),
        victory: vec![],
        events: vec![],
        tech_boosts: vec![],
        neutral: vec![],
        offmap_bases: vec![],
        variants: BTreeMap::new(),
        conditions: Default::default(),
    };
    sc.conditions.exits = vec![ExitZone {
        side: B,
        hexes: vec![Hex::new(7, 0)],
        vp_per_step: 2,
        open: true,
    }];
    let mut s = GameState::new(data(), sc).unwrap().0;
    s.apply(Command::EndPhase).unwrap(); // B to move
    s
}

fn play_b(s: &mut GameState) -> AiV1 {
    let mut ai = AiV1::new(AiParams::from_json(AI).unwrap(), AiHints::default(), 1);
    for _ in 0..100 {
        let c = ai.next(s);
        if c == Command::EndPhase {
            break;
        }
        s.apply(c).expect("legal");
    }
    ai
}

/// Leaving is worth more (20 VP through the exit, the line holds little):
/// B takes the infantry out.
#[test]
fn r_ai_01_retreat_takes_the_steps_out() {
    let mut s = board(false);
    let ai = play_b(&mut s);
    assert!(
        s.unit(UnitId(2)).is_none(),
        "{:?}",
        ai.last_plan().map(|p| &p.exit)
    );
    assert_eq!(s.exited_vp(B), 20);
}

/// Holding is worth more (its own objective, 60 VP): B stays.
#[test]
fn r_ai_01_retreat_holds_a_valuable_hex() {
    let mut s = board(true);
    let ai = play_b(&mut s);
    assert!(ai.last_plan().is_some_and(|p| p.exit.is_empty()));
    assert_eq!(s.unit(UnitId(2)).map(|u| u.hex), Some(Hex::new(5, 0)));
    assert_eq!(s.exited_vp(B), 0);
}
