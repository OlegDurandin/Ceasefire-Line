//! GameState: phases, movement and ZOC, attacks, retreat, determinism.

mod common;

use std::collections::BTreeMap;

use ceasefire_core::game::{Command, Event, GameState, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::map::{HexCell, HexMap};
use ceasefire_core::{AttackError, MoveError, RuleError};

use common::scenario::{board, hq, setup, start, unit, A, B};

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

/// Tank (1) at (0,0) with supply 10; enemy infantry (2) dug in at (3,0) on a
/// fortified hex so it holds; friendly infantry (3) next to it at (2,1).
fn r_move_04_setup() -> GameState {
    let (mut map, control) = board();
    map.insert(Hex::new(0, 0), HexCell::new("city"));
    map.insert(Hex::new(3, 0), HexCell::new("fortified"));
    let mut tank = unit("tank", A, Hex::new(0, 0));
    tank.supply = None;
    let mut enemy = unit("infantry", B, Hex::new(3, 0));
    enemy.entrenchment = 5;
    start(setup(
        map,
        control,
        vec![tank, enemy, unit("infantry", A, Hex::new(2, 1))],
    ))
}

#[test]
fn r_move_04_tank_attack_and_exploitation() {
    let mut s = r_move_04_setup();
    assert_eq!(ap(&s, 1), 10, "tank 5 AP");

    mv(&mut s, 1, &[(1, 0), (2, 0)]).unwrap();
    assert_eq!(ap(&s, 1), 6, "3 AP after 2 plain hexes");
    assert!(
        !s.unit(UnitId(1)).unwrap().can_move,
        "entering ZOC ends movement"
    );

    attack(&mut s, 1, 2).unwrap();
    assert_eq!(
        s.unit(UnitId(2)).unwrap().hex,
        Hex::new(3, 0),
        "defender holds"
    );
    assert_eq!(ap(&s, 1), 4, "2 AP after the attack");
    assert!(
        s.unit(UnitId(1)).unwrap().can_move,
        "exploitation with >= 2 AP"
    );

    // Option 1: break out with the remaining 2 AP.
    let mut breakout = s.clone();
    assert_eq!(
        breakout.reachable(UnitId(1)).unwrap().get(&Hex::new(0, 0)),
        Some(&4)
    );
    mv(&mut breakout, 1, &[(1, 0), (0, 0)]).unwrap();
    assert_eq!(ap(&breakout, 1), 0);

    // Option 2: second strike leaves 1 AP and no movement.
    attack(&mut s, 1, 2).unwrap();
    assert_eq!(ap(&s, 1), 2, "1 AP after the second attack");
    assert_eq!(move_err(mv(&mut s, 1, &[(1, 0)])), MoveError::CannotMove);
    assert!(matches!(
        attack(&mut s, 1, 2),
        Err(RuleError::IllegalAttack {
            reason: AttackError::NoStrikes,
            ..
        })
    ));
}

#[test]
fn r_move_04_infantry_loses_ap_after_attack() {
    let mut s = r_move_04_setup();
    attack(&mut s, 3, 2).unwrap();
    assert_eq!(ap(&s, 3), 0);
    assert_eq!(move_err(mv(&mut s, 3, &[(1, 1)])), MoveError::CannotMove);
}

#[test]
fn r_move_06_control_changes_on_entry_and_stays() {
    let (map, control) = board();
    let mut s = start(setup(map, control, vec![unit("mech", A, Hex::new(2, 0))]));
    assert_eq!(s.controller(Hex::new(3, 0)), Some(B));
    let events = mv(&mut s, 1, &[(3, 0)]).unwrap();
    assert!(events.contains(&Event::HexCaptured {
        hex: Hex::new(3, 0),
        side: A,
        from: Some(B),
    }));
    assert_eq!(s.controller(Hex::new(3, 0)), Some(A));
    mv(&mut s, 1, &[(4, 0)]).unwrap();
    assert_eq!(s.controller(Hex::new(3, 0)), Some(A), "vacated hex stays");
    assert_eq!(s.controller(Hex::new(4, 0)), Some(A));
}

#[test]
fn r_move_03_zoc_rules() {
    let (map, control) = board();
    // Enemy at (4,0): its ZOC covers (3,0), (3,1), (4,-1), (5,-1), (5,0), (4,1).
    let units = vec![
        unit("mech", A, Hex::new(2, 0)),
        unit("infantry", B, Hex::new(4, 0)),
        unit("infantry", A, Hex::new(3, 0)),
    ];
    let mut s = start(setup(map, control, units));

    // Entering ZOC ends the move.
    let mut t = s.clone();
    assert_eq!(
        move_err(mv(&mut t, 1, &[(3, -1), (4, -1), (4, -2)])),
        MoveError::ZocStop
    );
    // Enemy hex is blocked.
    assert_eq!(move_err(mv(&mut t, 3, &[(4, 0)])), MoveError::Occupied);
    // Friendly units can be passed through but not stopped on.
    assert_eq!(move_err(mv(&mut t, 1, &[(3, 0)])), MoveError::Occupied);

    // Infantry 3 started the phase in ZOC: leaving costs +1 AP.
    assert!(s.unit(UnitId(3)).unwrap().started_in_zoc);
    mv(&mut s, 3, &[(2, 1)]).unwrap();
    assert_eq!(ap(&s, 3), 2, "3 AP − (1 plain + 1 exit)");

    // ZOC to ZOC is forbidden.
    let (map, control) = board();
    let units = vec![
        unit("infantry", B, Hex::new(4, 0)),
        unit("tank", A, Hex::new(3, 0)),
    ];
    let mut s = start(setup(map, control, units));
    assert_eq!(move_err(mv(&mut s, 2, &[(3, 1)])), MoveError::ZocToZoc);
    // A detour out of ZOC and back in is legal: 1 + 1 (exit) + 1 AP.
    assert_eq!(
        s.reachable(UnitId(2)).unwrap().get(&Hex::new(3, 1)),
        Some(&6)
    );
}

#[test]
fn r_move_03_weak_units_have_no_zoc() {
    let (map, control) = board();
    let mut weak = unit("infantry", B, Hex::new(4, 0));
    weak.strength = 2;
    let mut s = start(setup(
        map,
        control,
        vec![unit("mech", A, Hex::new(2, 0)), weak],
    ));
    mv(&mut s, 1, &[(3, 0), (3, 1), (4, 1)]).unwrap();
}

#[test]
fn r_turn_02_night_first_hex_costs_ap() {
    let (map, control) = board();
    let units = vec![
        unit("tank", A, Hex::new(2, 0)),
        unit("commando", A, Hex::new(2, 1)),
    ];
    let mut s = start(setup(map, control, units));
    assert_eq!(ap(&s, 1), 10, "day");
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap();
    assert!(s.is_night());
    // R-TURN-02 (designer decision 2026-11-27): full AP, the first hex off
    // a road or into enemy ZOC costs +1 AP — on a plain 1 + 1.
    assert_eq!(ap(&s, 1), 10);
    assert_eq!(
        s.reachable(UnitId(1)).unwrap().get(&Hex::new(1, 0)),
        Some(&4)
    );
    assert_eq!(ap(&s, 2), 6, "commando: night combat, no penalty");
    assert_eq!(
        s.reachable(UnitId(2)).unwrap().get(&Hex::new(1, 1)),
        Some(&2)
    );
}

#[test]
fn r_unit_02_low_supply_costs_ap() {
    let (map, mut control) = board();
    // Island of A-controlled hex inside B territory: no supply path.
    control.insert(Hex::new(5, 0), A);
    let mut cut_off = unit("tank", A, Hex::new(5, 0));
    cut_off.supply = Some(5);
    let s = start(setup(map, control, vec![cut_off]));
    // Inertia: 5 − 3 = 2 ≤ 3 → −1 AP.
    assert_eq!(s.unit(UnitId(1)).unwrap().supply, 2);
    assert_eq!(ap(&s, 1), 8);
}

/// R-SUP-05: an encircled unit (supply 8, 1 star, no combat), first three own phases.
fn encircled(nm: u8, morale: u8) -> Vec<(u8, u8, u8, u8)> {
    let (map, mut control) = board();
    control.insert(Hex::new(5, 0), A);
    let mut u = unit("infantry", A, Hex::new(5, 0));
    u.morale_pct = morale;
    u.exp = 100;
    u.supply = Some(8);
    // Attached to a rating-5 HQ: no HQ bonus, no unattached penalty.
    let mut setup = setup(map, control, vec![u, hq(A, Hex::new(1, 0), 5)]);
    setup.sides[0].national_morale_pct = nm;
    let mut s = start(setup);
    let mut seen = Vec::new();
    for phase in 1..=4 {
        if phase > 1 {
            s.apply(Command::EndPhase).unwrap();
            s.apply(Command::EndPhase).unwrap();
        }
        let x = s.unit(UnitId(1)).unwrap();
        seen.push((x.supply, x.morale_pct, x.base_readiness_pct, x.strength));
    }
    seen
}

#[test]
fn r_sup_05_supply_inertia_nm_80() {
    let seen = encircled(80, 80);
    assert_eq!(seen[0], (5, 80, 63, 10));
    assert_eq!(seen[1], (2, 80, 50, 10));
    assert_eq!(seen[2], (0, 65, 34, 10));
    assert_eq!(seen[3].3, 9, "attrition from the second phase at supply 0");
}

#[test]
fn r_sup_05_supply_inertia_nm_100() {
    let seen = encircled(100, 80);
    assert_eq!(seen[0], (5, 90, 71, 10), "supply 5 ≥ 4, no combat: +10");
    assert_eq!(seen[1], (2, 90, 57, 10), "supply 1–3: morale unchanged");
    assert_eq!(seen[2], (0, 75, 39, 10), "supply 0: −15");
}

#[test]
fn morale_does_not_grow_after_combat() {
    let mut s = r_move_04_setup();
    let before = s.unit(UnitId(2)).unwrap().morale_pct;
    mv(&mut s, 1, &[(1, 0), (2, 0)]).unwrap();
    attack(&mut s, 1, 2).unwrap();
    let after_combat = s.unit(UnitId(2)).unwrap().morale_pct;
    s.apply(Command::EndPhase).unwrap();
    // Defender (supply 8 ≥ 4) was attacked since its last phase: no +10.
    assert_eq!(
        s.unit(UnitId(2)).unwrap().morale_pct,
        after_combat.min(before)
    );
}

/// Attacker (1) at (0,0), defender (2) with strength 3 at (1,0).
fn retreat_setup(attacker: &str, full_map: bool) -> GameState {
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    let hexes: Vec<Hex> = if full_map {
        (-2..=3)
            .flat_map(|q| (-2..=2).map(move |r| Hex::new(q, r)))
            .collect()
    } else {
        vec![Hex::new(0, 0), Hex::new(1, 0)]
    };
    for h in hexes {
        map.insert(h, HexCell::new("plain"));
        control.insert(h, if h.q <= 0 { A } else { B });
    }
    let mut a = unit(attacker, A, Hex::new(0, 0));
    a.strength = 2;
    let mut d = unit("infantry", B, Hex::new(1, 0));
    d.strength = 3;
    d.entrenchment = 2;
    start(setup(map, control, vec![a, d]))
}

#[test]
fn r_combat_06_weak_defender_retreats() {
    let mut s = retreat_setup("infantry", true);
    let events = attack(&mut s, 1, 2).unwrap();
    assert!(events.contains(&Event::Retreated {
        unit: UnitId(2),
        from: Hex::new(1, 0),
        to: Hex::new(1, 1),
    }));
    assert_eq!(s.unit_at(Hex::new(1, 0)), None);
    assert_eq!(s.pending_advance(), None, "infantry cannot occupy");
}

#[test]
fn r_combat_06_no_retreat_costs_two_steps() {
    let mut s = retreat_setup("infantry", false);
    let events = attack(&mut s, 1, 2).unwrap();
    let left = s.unit(UnitId(2)).map_or(0, |u| u.strength);
    let extra = events.iter().find_map(|e| match e {
        Event::RetreatBlocked {
            unit: UnitId(2),
            extra_losses,
        } => Some(*extra_losses),
        _ => None,
    });
    // min(2, strength left after the exchange)
    assert!(matches!(extra, Some(1 | 2)), "{events:?}");
    assert_eq!(
        s.unit(UnitId(2)).map(|u| u.hex),
        (left > 0).then_some(Hex::new(1, 0))
    );
}

#[test]
fn r_combat_06_tank_occupies_vacated_hex() {
    let mut s = retreat_setup("tank", true);
    attack(&mut s, 1, 2).unwrap();
    assert_eq!(s.pending_advance().map(|p| p.hex), Some(Hex::new(1, 0)));
    let ap_before = ap(&s, 1);
    s.apply(Command::Advance { unit: UnitId(1) }).unwrap();
    assert_eq!(s.unit(UnitId(1)).unwrap().hex, Hex::new(1, 0));
    assert_eq!(ap(&s, 1), ap_before, "advance costs 0 AP");
    assert_eq!(s.controller(Hex::new(1, 0)), Some(A));
}

#[test]
fn r_combat_04_bombardment() {
    let (map, control) = board();
    let mut target = unit("infantry", B, Hex::new(4, 0));
    target.entrenchment = 2;
    let mut s = start(setup(
        map,
        control,
        vec![unit("artillery", A, Hex::new(2, 0)), target],
    ));
    let f = s.forecast_bombard(UnitId(1), UnitId(2)).unwrap();
    let readiness = u32::from(s.unit(UnitId(1)).unwrap().base_readiness_pct);
    let events = s
        .apply(Command::Bombard {
            unit: UnitId(1),
            target: UnitId(2),
        })
        .unwrap();
    let Event::Bombarded { params, .. } = events[0] else {
        panic!("expected bombardment");
    };
    assert_eq!(
        params.n,
        (10 * readiness + 100) / 200,
        "half shots, rounded half up"
    );
    assert!(f.expected > 0.0);
    assert_eq!(s.unit(UnitId(1)).unwrap().strength, 10, "no return fire");
    assert_eq!(s.unit(UnitId(2)).unwrap().entrenchment, 1);
    assert_eq!(ap(&s, 1), 0, "artillery does not move after firing");
    assert!(matches!(
        attack(&mut s, 1, 2),
        Err(RuleError::IllegalAttack { .. })
    ));
}

#[test]
fn r_combat_03_forecast_matches_resolution_params() {
    let mut s = r_move_04_setup();
    mv(&mut s, 1, &[(1, 0), (2, 0)]).unwrap();
    let f = s.forecast_attack(UnitId(1), UnitId(2)).unwrap();
    let events = attack(&mut s, 1, 2).unwrap();
    let Event::CombatResolved { exchange, .. } = events[0] else {
        panic!("expected combat");
    };
    assert_eq!(exchange, f.exchange);
}

#[test]
fn failed_command_leaves_state_unchanged() {
    let mut s = r_move_04_setup();
    let before = s.clone();
    assert!(mv(&mut s, 1, &[(1, 0), (2, 0), (3, 0)]).is_err());
    assert!(attack(&mut s, 2, 1).is_err(), "not the active side");
    assert_eq!(s, before);
}

#[test]
fn r_ai_02_replay_is_deterministic() {
    let commands = vec![
        Command::Move {
            unit: UnitId(1),
            path: vec![Hex::new(1, 0), Hex::new(2, 0)],
        },
        Command::Attack {
            unit: UnitId(1),
            target: UnitId(2),
        },
        Command::Attack {
            unit: UnitId(3),
            target: UnitId(2),
        },
        Command::EndPhase,
        Command::EndPhase,
    ];
    let run = |cmds: &[Command]| {
        let mut s = r_move_04_setup();
        let events: Vec<Event> = cmds
            .iter()
            .flat_map(|c| s.apply(c.clone()).unwrap())
            .collect();
        (s, events)
    };
    let json = serde_json::to_string(&commands).unwrap();
    let replayed: Vec<Command> = serde_json::from_str(&json).unwrap();
    let (s1, e1) = run(&commands);
    let (s2, e2) = run(&replayed);
    assert_eq!(e1, e2);
    assert_eq!(s1, s2);
    let events_json = serde_json::to_string(&e1).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<Event>>(&events_json).unwrap(),
        e1
    );
}

/// Attacker (1) at (0,0) across a canal edge from a strength-3 defender (2) at (1,0).
fn canal_setup(attacker: &str) -> GameState {
    let (mut map, control) = board();
    map.add_canal(Hex::new(0, 0), Hex::new(1, 0)).unwrap();
    let mut a = unit(attacker, A, Hex::new(0, 0));
    a.strength = 2;
    let mut d = unit("infantry", B, Hex::new(1, 0));
    d.strength = 3;
    let mut sc = setup(map, control, vec![a, d]);
    sc.control.insert(Hex::new(1, 0), B);
    for h in [Hex::new(2, 0), Hex::new(2, -1), Hex::new(1, 1)] {
        sc.control.insert(h, B);
    }
    start(sc)
}

#[test]
fn r_move_02_zoc_does_not_cross_canal_without_bridge() {
    let (mut map, control) = board();
    map.add_canal(Hex::new(2, 0), Hex::new(3, 0)).unwrap();
    let units = vec![
        unit("mech", A, Hex::new(1, 0)),
        unit("infantry", B, Hex::new(3, 0)),
    ];
    let mut s = start(setup(map, control, units));
    // (2,0) is next to the enemy, but across the canal: no ZOC, movement goes on.
    mv(&mut s, 1, &[(2, 0), (2, -1)]).unwrap();
}

#[test]
fn r_combat_06_no_tank_advance_across_canal_without_bridge() {
    let mut s = canal_setup("tank");
    attack(&mut s, 1, 2).unwrap();
    assert_eq!(s.unit_at(Hex::new(1, 0)), None, "defender left");
    assert_eq!(s.pending_advance(), None);
}

#[test]
fn r_combat_06_infantry_advances_across_canal_by_boat() {
    let mut s = canal_setup("infantry");
    attack(&mut s, 1, 2).unwrap();
    let pending = s.pending_advance().expect("infantry may occupy by boat");
    assert!(pending.by_boat);
    let before = s
        .unit(UnitId(1))
        .unwrap()
        .readiness_pct(&s.data().rules.readiness);
    s.apply(Command::Advance { unit: UnitId(1) }).unwrap();
    let u = s.unit(UnitId(1)).unwrap();
    assert_eq!((u.hex, u.ap_halves), (Hex::new(1, 0), 0), "all AP");
    assert_eq!(
        u.readiness_pct(&s.data().rules.readiness),
        before.saturating_sub(30)
    );
}

/// Defender (2) of strength 3 at (1,0), attacker (1) at (0,0); retreat
/// candidates (2,0), (2,-1), (1,1).
fn retreat_choice(tweak: impl FnOnce(&mut ceasefire_core::game::ScenarioSetup)) -> Option<Hex> {
    let (map, control) = board();
    let mut a = unit("infantry", A, Hex::new(0, 0));
    a.strength = 2;
    let mut d = unit("infantry", B, Hex::new(1, 0));
    d.strength = 3;
    let mut sc = setup(map, control, vec![a, d]);
    for h in [
        Hex::new(1, 0),
        Hex::new(2, 0),
        Hex::new(2, -1),
        Hex::new(1, 1),
    ] {
        sc.control.insert(h, B);
    }
    tweak(&mut sc);
    let mut s = start(sc);
    let events = attack(&mut s, 1, 2).unwrap();
    events.iter().find_map(|e| match e {
        Event::Retreated { to, .. } => Some(*to),
        _ => None,
    })
}

#[test]
fn r_combat_06_retreat_never_into_enemy_hex_and_prefers_own() {
    assert_eq!(
        retreat_choice(|_| {}),
        Some(Hex::new(1, 1)),
        "all equal: lowest (q, r)"
    );
    let to = retreat_choice(|sc| {
        sc.control.insert(Hex::new(1, 1), A);
        sc.control.remove(&Hex::new(2, -1));
    });
    assert_eq!(
        to,
        Some(Hex::new(2, 0)),
        "enemy hex excluded, own before neutral"
    );
}

#[test]
fn r_combat_06_retreat_prefers_supply_then_terrain() {
    let to = retreat_choice(|sc| {
        sc.map.insert(Hex::new(2, -1), HexCell::new("hills"));
    });
    assert_eq!(to, Some(Hex::new(2, -1)), "higher terrain defense");
    let to = retreat_choice(|sc| {
        sc.map.insert(Hex::new(2, -1), HexCell::new("hills"));
        // B source at (3,0): (2,0) gets 6, (1,1) and hills (2,-1) get 5.
        sc.sources.push(ceasefire_core::supply::SupplySource {
            hex: Hex::new(3, 0),
            value: 6,
        });
    });
    assert_eq!(to, Some(Hex::new(2, 0)), "supply beats terrain");
}

#[test]
fn r_combat_06_retreat_keeps_away_from_all_attackers_this_phase() {
    let to = retreat_choice(|sc| {
        sc.map.insert(Hex::new(2, 0), HexCell::new("hills"));
        // Artillery at (3,-1) bombards first: (2,0) and (2,-1) are next to it.
        sc.units.push(unit("artillery", A, Hex::new(3, -1)));
        sc.control.insert(Hex::new(3, -1), A);
    });
    assert_eq!(
        to,
        Some(Hex::new(2, 0)),
        "without the bombardment hills win"
    );

    let (map, control) = board();
    let mut a = unit("infantry", A, Hex::new(0, 0));
    a.strength = 2;
    let mut d = unit("infantry", B, Hex::new(1, 0));
    d.strength = 3;
    let mut sc = setup(
        map,
        control,
        vec![a, d, unit("artillery", A, Hex::new(3, -1))],
    );
    for h in [
        Hex::new(1, 0),
        Hex::new(2, 0),
        Hex::new(2, -1),
        Hex::new(1, 1),
    ] {
        sc.control.insert(h, B);
    }
    sc.map.insert(Hex::new(2, 0), HexCell::new("hills"));
    let mut s = start(sc);
    s.apply(Command::Bombard {
        unit: UnitId(3),
        target: UnitId(2),
    })
    .unwrap();
    assert!(
        s.unit(UnitId(2)).is_some(),
        "defender survives the bombardment (seeded)"
    );
    let events = attack(&mut s, 1, 2).unwrap();
    assert!(
        events.contains(&Event::Retreated {
            unit: UnitId(2),
            from: Hex::new(1, 0),
            to: Hex::new(1, 1),
        }),
        "{events:?}"
    );
}

#[test]
fn r_move_01_water_blocks_movement_and_supply() {
    let (mut map, mut control) = board();
    // A lake across row 0 between q = 1 and q = 3.
    for h in [
        Hex::new(2, -1),
        Hex::new(2, 0),
        Hex::new(1, 0),
        Hex::new(2, 1),
        Hex::new(1, 1),
        Hex::new(1, -1),
    ] {
        map.insert(h, HexCell::new("water"));
        control.remove(&h);
    }
    let mut s = start(setup(map, control, vec![unit("mech", A, Hex::new(0, 0))]));
    assert_eq!(move_err(mv(&mut s, 1, &[(1, 0)])), MoveError::Impassable);
    assert!(!s
        .reachable(UnitId(1))
        .unwrap()
        .contains_key(&Hex::new(2, 0)));
    let supply = s.hex_supply(A).unwrap();
    assert_eq!(supply.get(Hex::new(1, 0)), 0, "no supply on water");
}
