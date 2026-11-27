//! R-MOVE-07 breakthrough after a won combat; R-TURN-02 night combat as a
//! side property of the scenario.

mod common;

use std::collections::BTreeMap;

use ceasefire_core::game::{Command, Event, GameState, UnitId, UnitSetup};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::HexMap;
use ceasefire_core::{MoveError, RuleError};

use common::scenario::{board, setup, start, unit, A, B};

fn mv(state: &mut GameState, id: u32, path: &[(i32, i32)]) -> Result<Vec<Event>, RuleError> {
    state.apply(Command::Move {
        unit: UnitId(id),
        path: path.iter().map(|&(q, r)| Hex::new(q, r)).collect(),
    })
}

fn attack(state: &mut GameState, id: u32, target: u32) -> Result<Vec<Event>, RuleError> {
    state.apply(Command::Attack {
        unit: UnitId(id),
        target: UnitId(target),
    })
}

fn ap(state: &GameState, id: u32) -> u32 {
    state.unit(UnitId(id)).unwrap().ap_halves
}

fn move_err(r: Result<Vec<Event>, RuleError>) -> MoveError {
    match r {
        Err(RuleError::IllegalMove { reason, .. }) => reason,
        other => panic!("expected illegal move, got {other:?}"),
    }
}

/// A solid enemy line along q = 3: full infantry at (3,-2), (3,-1), (3,1),
/// (3,2) and a broken one (strength 2) at (3,0) that cannot retreat ((4,-1)
/// and (4,0) are held by A), so a combat on it is always won. Unit 1 —
/// `attacker` at (2,0) facing the gap.
fn line(attacker: &str, variants: &[(&str, bool)]) -> GameState {
    let (map, mut control) = board();
    control.insert(Hex::new(4, -1), A);
    control.insert(Hex::new(4, 0), A);
    let weak = UnitSetup {
        strength: 2,
        ..unit("infantry", B, Hex::new(3, 0))
    };
    let mut units = vec![unit(attacker, A, Hex::new(2, 0)), weak];
    for r in [-2, -1, 1, 2] {
        units.push(unit("infantry", B, Hex::new(3, r)));
    }
    let mut s = setup(map, control, units);
    s.variants = variants
        .iter()
        .map(|&(k, on)| (k.to_string(), on))
        .collect::<BTreeMap<_, _>>();
    start(s)
}

/// Attack the gap (unit 2) and occupy it.
fn break_in(s: &mut GameState) {
    let events = attack(s, 1, 2).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::UnitDestroyed { .. })),
        "the broken defender is destroyed: {events:?}"
    );
    s.apply(Command::Advance { unit: UnitId(1) }).unwrap();
    assert_eq!(s.unit(UnitId(1)).unwrap().hex, Hex::new(3, 0));
}

#[test]
fn r_move_07_breakthrough_passes_the_line() {
    let mut s = line("tank", &[]);
    assert_eq!(ap(&s, 1), 10, "tank 5 AP");
    break_in(&mut s);
    assert_eq!(ap(&s, 1), 8, "4 AP after the attack; the advance is free");
    let u = s.unit(UnitId(1)).unwrap();
    assert!(u.breakthrough && u.can_move, "in ZOC, but breaking through");

    // (4,0): ZOC of one unit (3,1) — 1 + 1 AP, does not stop; (5,0) is
    // clear — 1 AP. Two hexes past the line for 3 of the 4 AP.
    let reach = s.reachable(UnitId(1)).unwrap();
    assert_eq!(reach.get(&Hex::new(4, 0)), Some(&4));
    assert_eq!(reach.get(&Hex::new(5, 0)), Some(&6));
    // (4,1) is in the ZOC of (3,1) and (3,2): the breakthrough ends there.
    assert_eq!(reach.get(&Hex::new(4, 1)), Some(&8));
    let mut t = s.clone();
    assert_eq!(
        move_err(mv(&mut t, 1, &[(4, 0), (4, 1), (5, 1)])),
        MoveError::ZocStop
    );

    mv(&mut s, 1, &[(4, 0), (5, 0)]).unwrap();
    assert_eq!(ap(&s, 1), 2, "1 AP left");
    assert_eq!(s.unit(UnitId(1)).unwrap().hex, Hex::new(5, 0));
}

#[test]
fn r_move_07_variant_off_stops_in_zoc() {
    let mut s = line("tank", &[("breakthrough", false)]);
    break_in(&mut s);
    let u = s.unit(UnitId(1)).unwrap();
    assert!(!u.breakthrough);
    assert!(!u.can_move, "entering ZOC by the advance ends movement");
}

#[test]
fn r_move_07_without_a_won_combat_zoc_stops() {
    // The full line, no gap: the tank attacks (3,-1) and it holds.
    let (map, control) = board();
    let mut units = vec![unit("tank", A, Hex::new(2, 0))];
    for r in [-1, 0, 1] {
        let mut u = unit("infantry", B, Hex::new(3, r));
        u.entrenchment = 2;
        units.push(u);
    }
    let mut s = start(setup(map, control, units));
    attack(&mut s, 1, 3).unwrap();
    assert_eq!(s.unit(UnitId(3)).unwrap().hex, Hex::new(3, 0), "holds");
    assert!(s.pending_advance().is_none());
    let u = s.unit(UnitId(1)).unwrap();
    assert!(!u.breakthrough && u.can_move);
    // (2,0) → (2,1): from ZOC into ZOC — forbidden as usual.
    assert_eq!(move_err(mv(&mut s, 1, &[(2, 1)])), MoveError::ZocToZoc);
}

#[test]
fn r_move_07_infantry_does_not_break_through() {
    // Infantry wins the same combat; it does not occupy (no `occupy_hex`,
    // R-COMBAT-06) and keeps no AP (R-MOVE-04), so no breakthrough.
    let mut s = line("infantry", &[]);
    attack(&mut s, 1, 2).unwrap();
    assert!(s.unit(UnitId(2)).is_none(), "defender destroyed");
    let u = s.unit(UnitId(1)).unwrap();
    assert!(!u.breakthrough);
    assert_eq!(ap(&s, 1), 0);
    assert_eq!(move_err(mv(&mut s, 1, &[(3, 0)])), MoveError::CannotMove);

    // Mech does break through.
    let mut m = line("mech", &[]);
    break_in(&mut m);
    assert!(m.unit(UnitId(1)).unwrap().breakthrough);
}

#[test]
fn r_move_07_a_new_attack_ends_the_breakthrough() {
    let mut s = line("tank", &[]);
    break_in(&mut s);
    // Second strike from the gap on (3,1); it holds, so no new breakthrough.
    attack(&mut s, 1, 5).unwrap();
    let u = s.unit(UnitId(1)).unwrap();
    assert!(!u.breakthrough);
}

/// Infantry of side A at (2,0), enemy infantry at (3,0); turn 2 is night.
fn night(classes: &[&str]) -> GameState {
    let (map, control) = board();
    let mut s = setup(
        map,
        control,
        vec![
            unit("infantry", A, Hex::new(2, 0)),
            unit("infantry", B, Hex::new(3, 0)),
        ],
    );
    s.start_turn = 2;
    s.sides[0].night_combat = classes.iter().map(|c| c.to_string()).collect();
    start(s)
}

#[test]
fn r_turn_02_side_night_combat() {
    let plain = night(&[]);
    assert!(plain.is_night());
    assert_eq!(
        ap(&plain, 1),
        6,
        "full AP at night; the first hex costs more"
    );
    assert!(plain.unit(UnitId(1)).unwrap().night_surcharge_due);
    let fire = plain.explain_attack(UnitId(1), UnitId(2)).unwrap();
    assert!((fire[0].attack_mult - 0.8).abs() < 1e-9);

    let trained = night(&["infantry"]);
    assert_eq!(ap(&trained, 1), 6);
    assert!(!trained.unit(UnitId(1)).unwrap().night_surcharge_due);
    let fire = trained.explain_attack(UnitId(1), UnitId(2)).unwrap();
    assert!((fire[0].attack_mult - 1.0).abs() < 1e-9);
}

#[test]
fn r_turn_02_night_combat_class_must_exist() {
    let (map, control) = board();
    let mut s = setup(map, control, vec![]);
    s.sides[0].night_combat = vec!["paratroops".into()];
    assert!(GameState::new(std::sync::Arc::new(common::data()), s).is_err());
}

/// Night (turn 2): `class` of A at (0,0), a road along r = 0 from q = 0 to
/// 6; enemy infantry at (3,-2), its ZOC covers (2,-1) next to the road.
fn night_road(class: &str, day: bool) -> GameState {
    let (mut map, control): (HexMap, _) = board();
    for q in 0..6 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    let mut s = setup(
        map,
        control,
        vec![
            unit(class, A, Hex::new(0, 0)),
            unit("infantry", B, Hex::new(3, -2)),
        ],
    );
    s.start_turn = if day { 1 } else { 2 };
    start(s)
}

#[test]
fn r_turn_02_night_column_on_road_keeps_full_ap() {
    let mut s = night_road("mech", false);
    assert_eq!(ap(&s, 1), 8, "no AP taken at the phase start");
    mv(&mut s, 1, &[(1, 0), (2, 0), (3, 0)]).unwrap();
    assert_eq!(ap(&s, 1), 5, "3 road hexes × 0,5 AP, no surcharge");
    assert!(s.unit(UnitId(1)).unwrap().night_surcharge_due);
}

#[test]
fn r_turn_02_night_leaving_the_road_pays_once() {
    let mut s = night_road("mech", false);
    assert_eq!(
        s.reachable(UnitId(1)).unwrap().get(&Hex::new(0, 1)),
        Some(&4)
    );
    mv(&mut s, 1, &[(0, 1)]).unwrap();
    assert_eq!(ap(&s, 1), 4, "plain 1 AP + night 1 AP");
    assert!(!s.unit(UnitId(1)).unwrap().night_surcharge_due);
    mv(&mut s, 1, &[(0, 2)]).unwrap();
    assert_eq!(ap(&s, 1), 2, "once per phase");

    // By day — no surcharge.
    let mut d = night_road("mech", true);
    mv(&mut d, 1, &[(0, 1)]).unwrap();
    assert_eq!(ap(&d, 1), 6);
}

#[test]
fn r_turn_02_night_entering_zoc_pays_if_not_yet_paid() {
    // Tank, 5 AP = 10 half-points.
    let mut s = night_road("tank", false);
    mv(&mut s, 1, &[(1, 0), (2, 0), (2, -1)]).unwrap();
    assert_eq!(ap(&s, 1), 4, "2 road hexes + ZOC hex 1 AP + night 1 AP");

    // Paid off the road first: the ZOC hex costs no more.
    let mut t = night_road("tank", false);
    mv(&mut t, 1, &[(0, 1), (1, 0), (2, 0), (2, -1)]).unwrap();
    assert_eq!(
        ap(&t, 1),
        1,
        "(0,1) 1 + night 1, (1,0) 1, road 0,5, ZOC hex 1"
    );
}

#[test]
fn r_move_06_breakthrough_keeps_control_of_zoc_hexes() {
    // The line of `line()`, but the gap's retreat is blocked by two weak A
    // units at (4,-1) and (4,0) (no ZOC below strength 5); B has a source at (6,1).
    let (map, control) = board();
    let weak = UnitSetup {
        strength: 2,
        ..unit("infantry", B, Hex::new(3, 0))
    };
    let mut units = vec![unit("tank", A, Hex::new(2, 0)), weak];
    for r in [-2, -1, 1, 2] {
        units.push(unit("infantry", B, Hex::new(3, r)));
    }
    for h in [Hex::new(4, -1), Hex::new(4, 0)] {
        units.push(UnitSetup {
            strength: 3,
            ..unit("infantry", A, h)
        });
    }
    let mut sc = setup(map, control, units);
    sc.sources.push(ceasefire_core::supply::SupplySource {
        hex: Hex::new(6, 1),
        value: 10,
    });
    let mut s = start(sc);
    break_in(&mut s);
    assert_eq!(s.controller(Hex::new(4, 0)), Some(B));
    let before = s.hex_supply(B).unwrap();

    // (4,0) is in the ZOC of (3,1): passed through, it stays B's; the
    // final hex (5,-1) changes hands.
    mv(&mut s, 1, &[(4, 0), (5, -1)]).unwrap();
    assert_eq!(s.controller(Hex::new(4, 0)), Some(B));
    assert_eq!(s.controller(Hex::new(5, -1)), Some(A));
    // The southern half of the line keeps its path from (6,1) by (5,1),
    // (4,1); the northern half is cut by the tank's own ZOC, not by control.
    let after = s.hex_supply(B).unwrap();
    for r in [1, 2] {
        let h = Hex::new(3, r);
        assert_eq!(after.get(h), before.get(h), "B supply at {h:?}");
    }
}
