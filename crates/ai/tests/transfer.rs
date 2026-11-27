//! R-AI-01 «сектора», R-MOVE-05 (stage 4 step 7): the AI moves reserves
//! between sectors — acceptance tests A/B/C on a synthetic map with two fronts.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_ai::{AiHints, AiParams, AiV1};
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{Command, GameState, ScenarioSetup, SideState, UnitId, UnitSetup};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
use ceasefire_core::side::SideId;
use ceasefire_core::supply::SupplySource;
use ceasefire_core::tech::TechLevels;

const A: SideId = SideId(0);
const B: SideId = SideId(1);
const AI: &str = include_str!("../../../data/ai.json");
const RESERVE: UnitId = UnitId(3);

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

/// Plain q 0..=9, r −6..=6. A holds q ≤ 3 with a rear road along q = 1
/// (r −5..5) and its source at (0,0); B holds q ≥ 4, source (9,0).
/// Sectors of A: north r ≤ −3, south r ≥ 3. A: weak infantry in the north
/// (3,−4), infantry (3,4) and the reserve tank (1,4) in the south.
/// `north_threat`: B's two tanks at the northern front, else far away.
fn two_fronts(north_threat: bool, mpp: u32) -> (ScenarioSetup, AiHints) {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for q in 0..=9 {
        for r in -6..=6 {
            map.insert(Hex::new(q, r), HexCell::new("plain"));
            control.insert(Hex::new(q, r), if q <= 3 { A } else { B });
        }
    }
    for r in -5..5 {
        map.add_road(Hex::new(1, r), Hex::new(1, r + 1)).unwrap();
    }
    let b_units = if north_threat {
        vec![
            unit("tank", B, Hex::new(4, -4), 10),
            unit("tank", B, Hex::new(5, -5), 10),
        ]
    } else {
        vec![
            unit("tank", B, Hex::new(9, -1), 10),
            unit("tank", B, Hex::new(9, 1), 10),
        ]
    };
    let mut units = vec![
        unit("infantry", A, Hex::new(3, -4), 4),
        unit("infantry", A, Hex::new(3, 4), 10),
        unit("tank", A, Hex::new(1, 4), 10),
    ];
    units.extend(b_units);
    let src = |q, r| SupplySource {
        hex: Hex::new(q, r),
        value: 10,
    };
    let sc = ScenarioSetup {
        hex_km: None,
        supply_decay_ap: None,
        seed: 1,
        start_turn: 1,
        sides: vec![side(A, mpp), side(B, 0)],
        map,
        control,
        sources: vec![src(0, 0), src(9, 0)],
        units,
        cities: vec![],
        scheduled_income: vec![],
        objectives: vec![],
        key_hexes: vec![],
        last_turn: Some(10),
        victory: vec![],
        events: vec![],
        tech_boosts: vec![],
        neutral: vec![],
        offmap_bases: vec![],
        variants: BTreeMap::new(),
    };
    let sector = |rows: std::ops::RangeInclusive<i32>| -> Vec<Hex> {
        (0..=3)
            .flat_map(|q| rows.clone().map(move |r| Hex::new(q, r)))
            .collect()
    };
    let hints = AiHints {
        sectors: vec![
            ("north".into(), sector(-6..=-3)),
            ("south".into(), sector(3..=6)),
        ],
        ..AiHints::default()
    };
    (sc, hints)
}

/// A plays `phases` of its phases with AI v1 (B only ends its own); every
/// command A's AI gives must be legal.
fn play(sc: ScenarioSetup, hints: AiHints, phases: u32) -> (GameState, Vec<Command>) {
    let mut s = GameState::new(data(), sc).unwrap().0;
    let mut ai = AiV1::new(AiParams::from_json(AI).unwrap(), hints, 1);
    let mut cmds = Vec::new();
    for _ in 0..phases {
        for _ in 0..200 {
            let c = ai.next(&s);
            if c == Command::EndPhase {
                break;
            }
            s.apply(c.clone()).expect("the AI gives legal commands");
            cmds.push(c);
        }
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap(); // B's phase
    }
    (s, cmds)
}

fn transfers(cmds: &[Command]) -> Vec<(UnitId, Hex)> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::Transfer { unit, to } => Some((*unit, *to)),
            _ => None,
        })
        .collect()
}

/// A: the northern threat exceeds the forces there, the southern reserve is
/// free and the MPP cover a transfer — the reserve goes north within 2 phases.
#[test]
fn r_ai_01_transfer_a_reserve_to_the_threatened_sector() {
    let (sc, hints) = two_fronts(true, 100);
    let (_, cmds) = play(sc, hints, 2);
    let t = transfers(&cmds);
    assert_eq!(t.len(), 1, "{cmds:?}");
    assert_eq!(t[0].0, RESERVE);
    assert!(t[0].1.r <= -3, "to the northern sector: {:?}", t[0].1);
}

/// B: no threat anywhere — nothing is transferred.
#[test]
fn r_ai_01_transfer_b_nothing_without_a_threat() {
    let (sc, hints) = two_fronts(false, 100);
    let (_, cmds) = play(sc, hints, 2);
    assert!(transfers(&cmds).is_empty(), "{cmds:?}");
}

/// C: the MPP do not cover the transfer — none is made, and the rest of the
/// phase's economy goes on: the worn infantry in the north is reinforced.
#[test]
fn r_ai_01_transfer_c_not_without_the_mpp() {
    let (sc, hints) = two_fronts(true, 9);
    let s0 = GameState::new(data(), sc.clone()).unwrap().0;
    assert!(s0.transfer_cost(RESERVE).unwrap() > 9);
    let (_, cmds) = play(sc, hints, 1);
    assert!(transfers(&cmds).is_empty(), "{cmds:?}");
    assert!(
        cmds.iter()
            .any(|c| matches!(c, Command::Reinforce { unit, .. } if *unit == UnitId(1))),
        "{cmds:?}"
    );
}
