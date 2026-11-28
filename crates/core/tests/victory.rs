//! R-NM-02, R-EVT-01 (designer decision 2026-11-27): per-side sudden death,
//! exit hexes, VP for the enemy's lost steps, and the event that opens exits.

mod common;

use ceasefire_core::game::{
    Command, Effect, EndReason, Event, ExitZone, GameState, Predicate, ScenarioEvent, StepVp,
    SupplyVp, UnitId,
};
use ceasefire_core::hex::Hex;
use ceasefire_core::supply::SupplySource;

use common::scenario::{board, setup, start, unit, A, B};

/// B's infantry at (4, 0) next to an exit hex (5, 0) of B worth 2 VP a step.
fn with_exit(open: bool) -> GameState {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("infantry", B, Hex::new(4, 0))]);
    sc.conditions.exits = vec![ExitZone {
        side: B,
        hexes: vec![Hex::new(5, 0)],
        vp_per_step: 2,
        open,
    }];
    let mut s = start(sc);
    s.apply(Command::EndPhase).unwrap(); // B to move
    s
}

fn vp(s: &GameState, side: ceasefire_core::side::SideId) -> i32 {
    s.victory_points()
        .into_iter()
        .find(|x| x.0 == side)
        .unwrap()
        .1
}

/// An open exit: the unit that enters it leaves the map, 2 VP per step.
#[test]
fn r_nm_02_steps_leave_through_an_open_exit_for_vp() {
    let mut s = with_exit(true);
    let before = vp(&s, B);
    let events = s
        .apply(Command::Move {
            unit: UnitId(1),
            path: vec![Hex::new(5, 0)],
        })
        .unwrap();
    assert!(s.unit(UnitId(1)).is_none(), "left the map");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::UnitExited {
            steps: 10,
            vp: 20,
            ..
        }
    )));
    assert_eq!(vp(&s, B), before + 20);
    assert_eq!(s.exited_vp(B), 20);
    assert_eq!(s.side(B).unwrap().steps_lost, 0, "an exit is not a loss");
}

/// A closed exit is an ordinary hex.
#[test]
fn r_nm_02_closed_exit_is_an_ordinary_hex() {
    let mut s = with_exit(false);
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(5, 0)],
    })
    .unwrap();
    assert_eq!(s.unit(UnitId(1)).unwrap().hex, Hex::new(5, 0));
    assert_eq!(s.exited_vp(B), 0);
}

/// R-EVT-01: «приказ об отходе» — morale −10, entrenchment −1, exits open.
#[test]
fn r_evt_01_withdrawal_order_opens_the_exits() {
    let (map, control) = board();
    let mut dug = unit("infantry", B, Hex::new(4, 0));
    dug.entrenchment = 2;
    let mut sc = setup(map, control, vec![dug]);
    sc.conditions.exits = vec![ExitZone {
        side: B,
        hexes: vec![Hex::new(5, 0)],
        vp_per_step: 1,
        open: false,
    }];
    sc.events = vec![ScenarioEvent {
        id: "withdrawal".into(),
        important: false,
        phase_of: Some(B),
        trigger: Predicate::AnyOf {
            of: vec![
                Predicate::Turn { turn: 4 },
                Predicate::HexControl {
                    hex: Hex::new(4, 0),
                    side: A,
                },
            ],
        },
        conditions: vec![],
        effects: vec![
            Effect::UnitsMorale {
                side: B,
                change: -10,
            },
            Effect::UnitsEntrenchment {
                side: B,
                change: -1,
            },
            Effect::OpenExits { side: B },
        ],
    }];
    let mut s = start(sc);
    assert!(!s.conditions().exits[0].open);
    let morale = s.unit(UnitId(1)).unwrap().morale_pct;
    // Turns 1–3: closed; B's phase on turn 4 fires it.
    for _ in 0..7 {
        s.apply(Command::EndPhase).unwrap();
    }
    assert_eq!(s.turn(), 4);
    assert!(s.conditions().exits[0].open);
    let u = s.unit(UnitId(1)).unwrap();
    assert!(
        u.morale_pct < morale,
        "morale fell: {} → {}",
        morale,
        u.morale_pct
    );
    assert!(u.entrenchment <= 1);
}

/// Sudden death per side: a side with `None` never breaks; one without an
/// entry uses the rules' threshold.
#[test]
fn r_nm_02_sudden_death_per_side() {
    let run = |b_threshold: Option<Option<u8>>| {
        let (map, control) = board();
        let mut sc = setup(map, control, vec![]);
        sc.sides[1].national_morale_pct = 15;
        if let Some(t) = b_threshold {
            sc.conditions.sudden_death.insert(B, t);
        }
        let mut s = start(sc);
        // NM is checked at the end of each phase; a finished game refuses more.
        for _ in 0..2 {
            if s.apply(Command::EndPhase).is_err() {
                break;
            }
        }
        s.outcome().cloned()
    };
    let o = run(None).expect("default threshold 20: B broken");
    assert_eq!((o.winner, o.reason), (Some(A), EndReason::SuddenDeath));
    assert!(run(Some(None)).is_none(), "B never breaks");
    assert!(run(Some(Some(10))).is_none(), "B breaks only at 10");
}

/// VP to a side for every step its enemies lost.
#[test]
fn r_nm_02_vp_for_enemy_steps_lost() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![]);
    sc.sides[1].steps_lost = 7;
    sc.conditions.vp_per_enemy_step.insert(
        A,
        StepVp {
            vp: 3,
            per_steps: 1,
        },
    );
    let s = start(sc);
    let base = s.side(A).unwrap().national_morale_pct as i32;
    assert_eq!(vp(&s, A), base + 21);
}

/// R-NM-02: half a point a step — 1 VP per 2 enemy steps, whole groups only.
#[test]
fn r_nm_02_vp_per_enemy_steps_in_groups() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![]);
    sc.sides[1].steps_lost = 7;
    sc.conditions.vp_per_enemy_step.insert(
        A,
        StepVp {
            vp: 1,
            per_steps: 2,
        },
    );
    let s = start(sc);
    let base = s.side(A).unwrap().national_morale_pct as i32;
    assert_eq!(vp(&s, A), base + 3, "7 steps → 3 groups of 2");
}

/// R-NM-02 «коридор снабжения»: A's town at (5, 0) with its own source 6 and
/// 15 VP for A's hex supply ≥ 4 there; `corridor` — A holds (3, 0) and (4, 0)
/// to its main source 10 at (0, 0); `town_side` — who holds the town.
fn pocket(corridor: bool, town_side: ceasefire_core::side::SideId) -> GameState {
    let (map, mut control) = board();
    let town = Hex::new(5, 0);
    if corridor {
        control.insert(Hex::new(3, 0), A);
        control.insert(Hex::new(4, 0), A);
    }
    control.insert(town, town_side);
    let mut sc = setup(map, control, vec![]);
    sc.sources.push(SupplySource {
        hex: town,
        value: 6,
    });
    sc.variants.insert("source_isolation".into(), true);
    sc.conditions.supply_vp = vec![SupplyVp {
        side: A,
        hex: town,
        min_supply: 4,
        vp: 15,
    }];
    start(sc)
}

#[test]
fn r_nm_02_supply_corridor_vp_open_corridor_gives_vp() {
    let s = pocket(true, A);
    assert!(s.hex_supply(A).unwrap().get(Hex::new(5, 0)) >= 4);
    assert_eq!(s.supply_vp_now(), vec![(A, 15)]);
    let base = s.side(A).unwrap().national_morale_pct as i32;
    assert_eq!(vp(&s, A), base + 15);
}

#[test]
fn r_nm_02_supply_corridor_vp_cut_off_gives_none() {
    let s = pocket(false, A);
    // R-SUP-01 `source_isolation`: the town's own source 6 gives at most 3.
    let here = s.hex_supply(A).unwrap().get(Hex::new(5, 0));
    assert!((1..4).contains(&here), "isolated supply {here}");
    assert_eq!(s.supply_vp_now(), vec![(A, 0)]);
    let base = s.side(A).unwrap().national_morale_pct as i32;
    assert_eq!(vp(&s, A), base);
}

#[test]
fn r_nm_02_supply_corridor_vp_town_held_by_enemy_gives_none() {
    let s = pocket(true, B);
    assert_eq!(s.hex_supply(A).unwrap().get(Hex::new(5, 0)), 0);
    assert_eq!(s.supply_vp_now(), vec![(A, 0)]);
    let base = s.side(A).unwrap().national_morale_pct as i32;
    assert_eq!(vp(&s, A), base);
}
