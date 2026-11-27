//! R-AI-01 п. 3 «экономика» (stage 4 step 7): reinforcements, purchases,
//! boosts in expected VP per MPP, made after the plan and before actions.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::{AiHints, AiParams, AiV1};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{
    Command, GameState, Objective, ScenarioSetup, SideState, UnitId, UnitSetup,
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

fn side(id: SideId, mpp: u32) -> SideState {
    SideState {
        id,
        tech: TechLevels::default(),
        national_morale_pct: 100,
        mpp,
        base_income: 0,
        purchase_limits: BTreeMap::new(),
        purchased: BTreeMap::new(),
        loss_sensitivity: 1,
        steps_lost: 0,
    }
}

fn unit(class: &str, side: SideId, hex: Hex, strength: u8) -> UnitSetup {
    UnitSetup {
        class: class.into(),
        side,
        hex,
        strength,
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

/// Plain q 0..=8, r −3..=3; A holds q ≤ 3 (sources (0,−3) 10 and (3,3) 8),
/// B holds q ≥ 4 (source (8,0)). A's objective at (3,2).
fn setup(units: Vec<UnitSetup>, last_turn: u32, mpp: u32) -> ScenarioSetup {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=8 {
        for r in -3..=3 {
            map.insert(Hex::new(q, r), HexCell::new("plain"));
            control.insert(Hex::new(q, r), if q <= 3 { A } else { B });
        }
    }
    let src = |q, r, value| SupplySource {
        hex: Hex::new(q, r),
        value,
    };
    let mut a = side(A, mpp);
    a.purchase_limits.insert("infantry".into(), 1);
    ScenarioSetup {
        hex_km: None,
        supply_decay_ap: None,
        seed: 1,
        start_turn: 1,
        sides: vec![a, side(B, 0)],
        map,
        control,
        sources: vec![src(0, -3, 10), src(3, 3, 8), src(8, 0, 10)],
        units,
        cities: vec![],
        scheduled_income: vec![],
        objectives: vec![Objective {
            hex: Hex::new(3, 2),
            nm_value: 10,
            vp: 10,
        }],
        key_hexes: vec![],
        last_turn: Some(last_turn),
        victory: vec![],
        events: vec![],
        tech_boosts: vec![],
        neutral: vec![],
        offmap_bases: vec![],
        variants: BTreeMap::new(),
    }
}

/// A's phase played by AI v1: the commands it gave.
fn phase(sc: ScenarioSetup, hints: AiHints) -> (GameState, Vec<Command>) {
    let mut s = GameState::new(data(), sc).unwrap().0;
    let mut ai = AiV1::new(AiParams::from_json(AI).unwrap(), hints, 1);
    let mut cmds = Vec::new();
    for _ in 0..200 {
        let c = ai.next(&s);
        if c == Command::EndPhase {
            break;
        }
        s.apply(c.clone()).unwrap();
        cmds.push(c);
    }
    (s, cmds)
}

fn reinforced(cmds: &[Command], id: UnitId) -> bool {
    cmds.iter()
        .any(|c| matches!(c, Command::Reinforce { unit, .. } if *unit == id))
}

fn purchases(cmds: &[Command]) -> Vec<Option<Hex>> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::Purchase { at, .. } => Some(*at),
            _ => None,
        })
        .collect()
}

/// One turn before the end: a purchase would arrive after it — none; the worn
/// defender of the objective is reinforced.
#[test]
fn r_ai_01_economy_no_purchase_before_the_end() {
    let units = vec![
        unit("infantry", A, Hex::new(3, 2), 5),
        unit("infantry", B, Hex::new(5, 2), 10),
    ];
    let (_, cmds) = phase(setup(units, 2, 300), AiHints::default());
    assert!(purchases(&cmds).is_empty(), "{cmds:?}");
    assert!(reinforced(&cmds, UnitId(1)), "{cmds:?}");
}

/// Plenty of MPP and nobody worn: a purchase within the force limit, at the
/// qualifying source nearest the threatened objective (`arrival_choice`).
#[test]
fn r_ai_01_economy_buys_and_picks_the_arrival_source() {
    let units = vec![
        unit("infantry", A, Hex::new(3, 2), 10),
        unit("infantry", B, Hex::new(5, 2), 10),
    ];
    let mut sc = setup(units, 10, 300);
    sc.variants.insert("arrival_choice".into(), true);
    let (s, cmds) = phase(sc, AiHints::default());
    assert_eq!(purchases(&cmds), vec![Some(Hex::new(3, 3))], "{cmds:?}");
    assert_eq!(s.side(A).unwrap().purchased.get("infantry"), Some(&1));
}

/// With `reinforce_locks`: the worn unit that attacks this phase by the plan
/// is not reinforced; the worn one out of action is.
#[test]
fn r_ai_01_economy_does_not_reinforce_an_attacker() {
    let units = vec![
        unit("infantry", A, Hex::new(3, 1), 6), // next to the weak enemy: attacks
        unit("infantry", A, Hex::new(2, 2), 6), // near the objective, out of contact
        unit("infantry", B, Hex::new(4, 1), 2),
    ];
    let mut sc = setup(units, 10, 300);
    sc.variants.insert("reinforce_locks".into(), true); // a reinforced unit would not act
    let (_, cmds) = phase(sc, AiHints::default());
    assert!(
        cmds.iter()
            .any(|c| matches!(c, Command::Attack { unit, .. } if *unit == UnitId(1))),
        "the first one attacks: {cmds:?}"
    );
    assert!(!reinforced(&cmds, UnitId(1)), "{cmds:?}");
    assert!(reinforced(&cmds, UnitId(2)), "{cmds:?}");
}

/// Two sectors: the price of one transfer of the dearest own unit stays.
#[test]
fn r_ai_01_economy_keeps_a_transfer_reserve() {
    let units = || {
        vec![
            unit("tank", A, Hex::new(3, 2), 3),
            unit("infantry", A, Hex::new(2, 2), 3),
            unit("infantry", B, Hex::new(5, 2), 10),
        ]
    };
    let hints = AiHints {
        sectors: vec![
            ("north".into(), vec![Hex::new(0, -3)]),
            ("south".into(), vec![Hex::new(3, 3)]),
        ],
        ..AiHints::default()
    };
    let (s, _) = phase(setup(units(), 10, 40), hints);
    let reserve = s.transfer_cost(UnitId(1)).unwrap();
    assert!(reserve > 0);
    assert!(s.side(A).unwrap().mpp >= reserve, "kept {reserve}");
    // Without sectors the same MPP goes further.
    let (one, _) = phase(setup(units(), 10, 40), AiHints::default());
    assert!(one.side(A).unwrap().mpp < s.side(A).unwrap().mpp);
}
