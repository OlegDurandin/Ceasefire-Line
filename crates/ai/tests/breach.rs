//! R-AI-01 stage 4 step 7: pocket operations persist; a breakthrough sector
//! is chosen by P(breach), and the AI does not burn tanks head-on.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::{play_phase, AiHints, AiParams, AiV1};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{
    Command, Event, GameState, ScenarioSetup, SideState, UnitId, UnitSetup,
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

fn unit(class: &str, side: SideId, hex: Hex, strength: u8, entrenchment: u8) -> UnitSetup {
    UnitSetup {
        class: class.into(),
        side,
        hex,
        strength,
        morale_pct: 90,
        exp: 0,
        entrenchment,
        supply: Some(8),
        hq_rating: None,
        formation: None,
        nation: None,
        name: None,
        offmap_base: None,
    }
}

/// q 0..=8, r −3..=3. A holds q ≤ 2: two tanks, infantry and artillery.
/// B holds q ≥ 3: a line of 5 infantry brigades at q = 3, r −2..=2 on hills,
/// strength 10, entrenchment 3; the line's flanks (3,±3) are mountains;
/// `weak`: the flank brigade at (3,2) stands on plain at strength 6, no
/// entrenchment. Mountains on q = 5 except the pass (5,0); B's source at (8,0).
fn line(weak: bool, seed: u64) -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=8 {
        for r in -3..=3 {
            let t = if (q == 5 && r != 0) || (q == 3 && (r == 3 || r == -3)) {
                "mountains"
            } else if q == 3 && !(weak && r == 2) {
                "hills"
            } else {
                "plain"
            };
            map.insert(Hex::new(q, r), HexCell::new(t));
            control.insert(Hex::new(q, r), if q <= 2 { A } else { B });
        }
    }
    let mut units = vec![
        unit("tank", A, Hex::new(1, 1), 10, 0),
        unit("tank", A, Hex::new(1, 2), 10, 0),
        unit("infantry", A, Hex::new(2, 1), 10, 0),
        unit("infantry", A, Hex::new(2, 2), 10, 0),
        unit("artillery", A, Hex::new(1, 3), 10, 0),
    ];
    for r in -2..=2 {
        let (st, en) = if weak && r == 2 { (6, 0) } else { (10, 3) };
        units.push(unit("infantry", B, Hex::new(3, r), st, en));
    }
    let setup = ScenarioSetup {
        hex_km: None,
        supply_decay_ap: None,
        seed,
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
        variants: BTreeMap::new(),
        conditions: Default::default(),
    };
    GameState::new(data(), setup).unwrap().0
}

fn ai(seed: u64) -> AiV1 {
    AiV1::new(AiParams::from_json(AI).unwrap(), AiHints::default(), seed)
}

/// Inertia: three own phases in a row with nothing changing — the same
/// operation and the same tanks.
#[test]
fn r_ai_01_operations_persist_between_phases() {
    let mut s = line(true, 1);
    let mut ai = ai(1);
    let mut seen = Vec::new();
    for _ in 0..3 {
        let p = ai.plan_for(&s).clone();
        seen.push((
            p.pockets
                .iter()
                .map(|q| q.hexes.clone())
                .collect::<Vec<_>>(),
            p.envelop.clone(),
        ));
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
    }
    assert!(!seen[0].0.is_empty(), "an operation: {seen:?}");
    assert!(seen.windows(2).all(|w| w[0] == w[1]), "{seen:?}");
}

/// A weak flank brigade: the AI breaks through there and has a tank on a
/// chokepoint of its operation within two own phases in ≥ 70% of seeds.
#[test]
fn r_ai_01_breaks_through_the_weak_spot() {
    let seeds = 20;
    let mut ok = 0;
    for seed in 1..=seeds {
        let mut s = line(true, seed);
        let mut ai = ai(seed);
        let plan = ai.plan_for(&s).clone();
        let chokes = plan.chokes.clone();
        if seed == 1 {
            assert!(
                plan.breaches.iter().any(|b| b.hex == Hex::new(3, 2)),
                "the weak flank is the sector: {plan:?}"
            );
        }
        for _ in 0..2 {
            let max = ai.max_commands();
            play_phase(&mut s, |st| ai.next(st), max).unwrap(); // A
            if s.units().any(|u| u.side == A && chokes.contains(&u.hex)) {
                break;
            }
            s.apply(Command::EndPhase).unwrap(); // B passes
        }
        if s.units().any(|u| u.side == A && chokes.contains(&u.hex)) {
            ok += 1;
        }
    }
    assert!(ok * 10 >= seeds * 7, "{ok} of {seeds} seeds");
}

/// No weak spot: P(breach) is below the bar — no breakthrough, and the tanks
/// do not attack the entrenched line head-on.
#[test]
fn r_ai_01_no_head_on_without_a_breach() {
    let mut s = line(false, 1);
    let mut ai = ai(1);
    let plan = ai.plan_for(&s).clone();
    assert!(plan.breaches.is_empty(), "{plan:?}");
    let max = ai.max_commands();
    let r = play_phase(&mut s, |st| ai.next(st), max).unwrap();
    let tanks = [UnitId(1), UnitId(2)];
    let tank_attacks = r
        .events
        .iter()
        .filter(|e| matches!(e, Event::CombatResolved { attacker, .. } if tanks.contains(attacker)))
        .count();
    assert_eq!(tank_attacks, 0, "{:?}", r.commands);
}
