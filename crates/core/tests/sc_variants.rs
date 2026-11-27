//! SC alignment of the core loop (designer task 2026-11-26): rule variants
//! from `rules.variants`, each off by default, switched on per scenario.

mod common;

use std::collections::BTreeMap;

use ceasefire_core::game::{Command, GameState, ScenarioSetup, UnitId, UnitSetup};
use ceasefire_core::hex::Hex;

use common::scenario::{board, start, unit, A, B};

/// Every boolean variant, so each test states its own rules.
const VARIANTS: [&str; 19] = [
    "attack_strips_entrenchment",
    "prepared_attack",
    "unsupplied_target",
    "hq_supply_node",
    "transfer_morale",
    "zero_supply_ap",
    "reinforce_locks",
    "nm_by_cost",
    "hq_strength_scaling",
    "fortified_hold",
    "pocket_desertion",
    "source_isolation",
    "air_strength_threshold",
    "transfer_supply_min",
    "hq_contact_priority",
    "armor_city_penalty",
    "arrival_choice",
    "nm_morale_multiplier",
    "breakthrough",
];

/// The common test scenario with every variant switched off.
fn setup(
    map: ceasefire_core::map::HexMap,
    control: BTreeMap<Hex, ceasefire_core::side::SideId>,
    units: Vec<UnitSetup>,
) -> ScenarioSetup {
    let mut sc = common::scenario::setup(map, control, units);
    sc.variants = VARIANTS.iter().map(|v| ((*v).to_owned(), false)).collect();
    sc
}

/// One variant on over the all-off base.
fn with(variant: &str, mut sc: ScenarioSetup) -> ScenarioSetup {
    sc.variants.insert(variant.to_owned(), true);
    sc
}

/// Designer decisions: 2026-11-26 — П1, П2, П3, П7, П9 on, ZOC from
/// strength 5 (П6); 2026-11-27 (rebalancing A/B) — `reinforce_locks` (П8),
/// `source_isolation`, `armor_city_penalty` and `breakthrough` (R-MOVE-07)
/// on; П4, П5, `fortified_hold`, `nm_morale_multiplier` and the flags without
/// effect off until «Голаны-73».
#[test]
fn sc_alignment_defaults() {
    let (map, control) = board();
    let s = start(common::scenario::setup(map, control, vec![]));
    let v = s.variants();
    assert!(v.attack_strips_entrenchment && v.prepared_attack && v.unsupplied_target);
    assert!(v.zero_supply_ap && v.nm_by_cost);
    assert!(v.reinforce_locks && v.source_isolation && v.armor_city_penalty && v.breakthrough);
    assert!(!v.hq_supply_node && !v.transfer_morale && !v.fortified_hold);
    assert!(!v.nm_morale_multiplier && !v.pocket_desertion && !v.hq_strength_scaling);
    assert!(!v.air_strength_threshold && !v.transfer_supply_min && !v.hq_contact_priority);
    assert!(!v.arrival_choice);
    assert_eq!(s.data().rules.movement.zoc_min_strength, 5);
}

/// B's dug-in infantry at (3, 0) and three A infantry brigades next to it.
fn three_on_one() -> ScenarioSetup {
    let (map, mut control) = board();
    control.insert(Hex::new(3, -1), A); // all three attackers in supply
    let mut target = unit("infantry", B, Hex::new(3, 0));
    target.entrenchment = 2;
    let units: Vec<UnitSetup> = vec![
        target,
        unit("infantry", A, Hex::new(2, 0)),
        unit("infantry", A, Hex::new(2, 1)),
        unit("infantry", A, Hex::new(3, -1)),
    ];
    setup(map, control, units)
}

const TARGET: UnitId = UnitId(1);

/// Expected defender losses of each of the three attacks, in order.
fn three_attacks(mut s: GameState) -> (Vec<f64>, Vec<u8>) {
    let mut expected = Vec::new();
    let mut entrenchment = Vec::new();
    for attacker in [UnitId(2), UnitId(3), UnitId(4)] {
        let Some(t) = s.unit(TARGET) else { break };
        entrenchment.push(t.entrenchment);
        let f = s.forecast_attack(attacker, TARGET).unwrap();
        expected.push(f.defender_losses.expected);
        s.apply(Command::Attack {
            unit: attacker,
            target: TARGET,
        })
        .unwrap();
    }
    (expected, entrenchment)
}

#[test]
fn r_combat_02_without_the_variant_only_bombardment_strips() {
    let s = start(three_on_one());
    assert!(!s.variants().attack_strips_entrenchment);
    let (_, entr) = three_attacks(s);
    assert_eq!(
        entr,
        vec![2, 2, 2],
        "without the variant only bombardment strips"
    );
}

/// П1: the first attack strips the entrenchment, the next ones finish the
/// unit off — the expected losses grow from attack to attack.
#[test]
fn r_combat_02_attacks_strip_entrenchment_three_attacks_in_a_row() {
    let s = start(with("attack_strips_entrenchment", three_on_one()));
    let (e, entr) = three_attacks(s);
    assert_eq!(entr, vec![2, 1, 0]);
    assert_eq!(e.len(), 3);
    assert!(e[0] < e[1] && e[1] < e[2], "expected losses grow: {e:?}");
}

#[test]
fn unknown_variant_is_an_error() {
    let sc = with("no_such_rule", three_on_one());
    assert!(GameState::new(std::sync::Arc::new(common::data()), sc).is_err());
}

/// A's infantry at (2, 0) next to B's infantry at (3, 0); `target_supply` is
/// the defender's unit supply.
fn duel(target_supply: u8) -> ScenarioSetup {
    let (map, control) = board();
    let mut target = unit("infantry", B, Hex::new(3, 0));
    target.supply = Some(target_supply);
    setup(
        map,
        control,
        vec![target, unit("infantry", A, Hex::new(1, 0))],
    )
}

const ATTACKER: UnitId = UnitId(2);

/// П2: an attacker that has not moved this phase gets +10 readiness points
/// for this attack only; after a move — none.
#[test]
fn r_combat_07_prepared_attack() {
    // The attacker standing next to the target from the start.
    let still = |variant: bool| {
        let mut sc = duel(8);
        sc.units[1].hex = Hex::new(2, 0);
        let s = start(if variant {
            with("prepared_attack", sc)
        } else {
            sc
        });
        let m = s.attack_modifiers(ATTACKER, TARGET).unwrap();
        (
            s.explain_attack(ATTACKER, TARGET).unwrap()[0].readiness_pct,
            m.prepared_pct,
        )
    };
    let (off, none) = still(false);
    let (on, bonus) = still(true);
    assert_eq!((none, bonus), (0, 10));
    assert_eq!(on, off + 10, "+10 points for this attack");
    // After a move: no bonus, even with the variant.
    let moved = |variant: bool| {
        let s = duel(8);
        let mut s = start(if variant {
            with("prepared_attack", s)
        } else {
            s
        });
        s.apply(Command::Move {
            unit: ATTACKER,
            path: vec![Hex::new(2, 0)],
        })
        .unwrap();
        (
            s.explain_attack(ATTACKER, TARGET).unwrap()[0].readiness_pct,
            s.attack_modifiers(ATTACKER, TARGET).unwrap().prepared_pct,
        )
    };
    assert_eq!(moved(true), moved(false));
    assert_eq!(moved(true).1, 0);
}

/// П3: against a unit with supply 0 the attacker's A is ×1.5; the target's
/// return fire does not change.
#[test]
fn r_combat_05_attack_on_a_unit_without_supply() {
    let fire = |variant: bool, supply: u8| {
        let mut sc = duel(supply);
        sc.units[1].hex = Hex::new(2, 0);
        let s = start(if variant {
            with("unsupplied_target", sc)
        } else {
            sc
        });
        let m = s.attack_modifiers(ATTACKER, TARGET).unwrap();
        let f = s.forecast_attack(ATTACKER, TARGET).unwrap();
        (f.exchange, m.unsupplied_target_mult)
    };
    let (base, m0) = fire(true, 3);
    let (hit, m1) = fire(true, 0);
    let (off, _) = fire(false, 0);
    assert_eq!((m0, m1), (1.0, 1.5));
    common::assert_close(hit.vs_defender.a, base.vs_defender.a * 1.5, 1e-9, "A × 1.5");
    assert!(hit.vs_defender.p > base.vs_defender.p);
    assert_eq!(off.vs_defender.a, base.vs_defender.a, "off: no change");
    assert_eq!(
        hit.vs_attacker.a, off.vs_attacker.a,
        "return fire unchanged"
    );
    assert_eq!(hit.vs_attacker.p, off.vs_attacker.p);
}

/// Road (0,0)..(6,0) on the board, all side A's.
fn road_board(units: Vec<UnitSetup>, variant: Option<&str>) -> GameState {
    let (mut map, mut control) = board();
    for q in 0..6 {
        map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
    }
    for q in 0..=6 {
        control.insert(Hex::new(q, 0), A);
    }
    let mut sc = setup(map, control, units);
    sc.sides[0].mpp = 100;
    start(match variant {
        Some(v) => with(v, sc),
        None => sc,
    })
}

/// П5: after a strategic transfer morale −15 (not below 0); it recovers by
/// R-SUP-05 (+10 in the next own phase with supply ≥ 4 and no combat).
#[test]
fn r_move_05_transfer_costs_morale() {
    let after = |variant: Option<&str>| {
        let mut s = road_board(vec![unit("infantry", A, Hex::new(0, 0))], variant);
        let before = s.unit(UnitId(1)).unwrap().morale_pct;
        s.apply(Command::Transfer {
            unit: UnitId(1),
            to: Hex::new(6, 0),
        })
        .unwrap();
        let moved = s.unit(UnitId(1)).unwrap().morale_pct;
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
        (before, moved, s.unit(UnitId(1)).unwrap().morale_pct)
    };
    let (b, m, next) = after(Some("transfer_morale"));
    assert_eq!(m, b - 15);
    assert_eq!(next, m + 10, "recovers by R-SUP-05");
    let (b, m, _) = after(None);
    assert_eq!(m, b, "off: no change");
    // Not below 0.
    let mut low = unit("infantry", A, Hex::new(0, 0));
    low.morale_pct = 4; // 14 after the phase-start rest
    let mut s = road_board(vec![low], Some("transfer_morale"));
    s.apply(Command::Transfer {
        unit: UnitId(1),
        to: Hex::new(6, 0),
    })
    .unwrap();
    assert_eq!(s.unit(UnitId(1)).unwrap().morale_pct, 0);
}

/// П7: supply 1–3 −1 AP, supply 0 −2 AP; at least 1 AP stays.
#[test]
fn r_unit_02_zero_supply_costs_two_ap() {
    let ap = |variant: bool| {
        let (map, control) = board();
        let mut units = vec![
            unit("tank", A, Hex::new(1, 0)),     // supply 3 (source 3)
            unit("tank", A, Hex::new(5, 0)),     // B's hex: supply 0
            unit("infantry", A, Hex::new(5, 2)), // supply 0, 3 AP
        ];
        for u in &mut units {
            u.supply = Some(0);
        }
        let mut sc = setup(map, control, units);
        sc.sources[0].value = 3;
        let s = start(if variant {
            with("zero_supply_ap", sc)
        } else {
            sc
        });
        [1, 2, 3].map(|i| {
            let u = s.unit(UnitId(i)).unwrap();
            (u.supply.min(4), u.ap_halves)
        })
    };
    // Halves of AP: tank 5 AP, infantry 3 AP.
    assert_eq!(ap(false), [(3, 8), (0, 8), (0, 4)]);
    assert_eq!(ap(true), [(3, 8), (0, 6), (0, 2)]);
}

/// П8: a unit that moved or attacked is not reinforced (before), a reinforced
/// one neither moves nor attacks (after); no elite steps next to the enemy.
#[test]
fn r_eco_02_reinforcement_locks_the_unit() {
    use ceasefire_core::{EconomyError, RuleError};
    let game = |variant: bool| {
        let (map, control) = board();
        let mut weak = unit("infantry", A, Hex::new(2, 0));
        weak.strength = 6;
        let mut far = unit("infantry", A, Hex::new(0, 2));
        far.strength = 6;
        let units = vec![unit("infantry", B, Hex::new(3, 0)), weak, far];
        let mut sc = setup(map, control, units);
        sc.sides[0].mpp = 200;
        start(if variant {
            with("reinforce_locks", sc)
        } else {
            sc
        })
    };
    let reinforce = |s: &mut GameState, id: u32, elite: bool| {
        s.apply(Command::Reinforce {
            unit: UnitId(id),
            steps: 1,
            elite,
        })
    };
    let attack = |s: &mut GameState| {
        s.apply(Command::Attack {
            unit: UnitId(2),
            target: UnitId(1),
        })
    };
    // Off: elite next to the enemy, then the attack.
    let mut s = game(false);
    reinforce(&mut s, 2, true).unwrap();
    assert!(attack(&mut s).is_ok());
    // On: no elite next to the enemy, a green step locks the unit.
    let mut s = game(true);
    assert_eq!(
        reinforce(&mut s, 2, true),
        Err(RuleError::IllegalEconomy(EconomyError::EliteNearEnemy))
    );
    reinforce(&mut s, 2, false).unwrap();
    assert!(attack(&mut s).is_err(), "reinforced: no attack");
    reinforce(&mut s, 3, true).unwrap(); // elite away from the enemy
    assert!(s
        .apply(Command::Move {
            unit: UnitId(3),
            path: vec![Hex::new(1, 2)],
        })
        .is_err());
    // Before: after a move no reinforcement (already R-ECO-02).
    let mut s = game(true);
    s.apply(Command::Move {
        unit: UnitId(3),
        path: vec![Hex::new(1, 2)],
    })
    .unwrap();
    assert_eq!(
        reinforce(&mut s, 3, false),
        Err(RuleError::IllegalEconomy(EconomyError::UnitActed))
    );
}

/// П9: −ceil(price / 40) NM for a destroyed unit, encircled × 1.5 rounded up.
#[test]
fn r_nm_01_unit_loss_by_price() {
    let (map, control) = board();
    let sc = setup(map, control, vec![unit("infantry", A, Hex::new(0, 0))]);
    let on = start(with("nm_by_cost", sc.clone()));
    let off = start(sc);
    assert_eq!(on.unit_nm_loss("infantry", false), 2, "70 → 2");
    assert_eq!(on.unit_nm_loss("tank", false), 3, "105 → 3");
    assert_eq!(on.unit_nm_loss("tank", true), 5, "3 × 1.5 = 4.5 → 5");
    assert_eq!(on.unit_nm_loss("infantry", true), 3, "2 × 1.5 = 3");
    assert_eq!(on.unit_nm_loss("artillery", false), 2, "50 → 2");
    // HQ 120 → 3 (and −3 more by R-HQ-02 on top, unchanged).
    assert_eq!(on.unit_nm_loss("hq", false), 3);
    assert_eq!(
        (
            off.unit_nm_loss("tank", false),
            off.unit_nm_loss("tank", true)
        ),
        (2, 3)
    );
}

/// R-NM-01 variant `nm_morale_multiplier` (SC 7.29.4): NM bands
/// below 95 → ×0.95, below 75 → ×0.90, below 40 → ×0.85.
#[test]
fn r_nm_01_morale_bands() {
    let (map, control) = board();
    let s = start(setup(map, control, vec![]));
    let nm = &s.data().rules.national_morale;
    let f: Vec<u8> = [100, 95, 94, 75, 74, 40, 39, 0]
        .iter()
        .map(|&x| nm.morale_factor_pct(x))
        .collect();
    assert_eq!(f, vec![100, 100, 95, 95, 90, 90, 85, 85]);
}

/// R-NM-01: without the variant NM caps unit morale; with it the morale
/// stays and enters readiness scaled by the NM band (NM 70 → ×0.90).
#[test]
fn r_nm_01_nm_scales_morale_instead_of_capping() {
    let run = |on: bool| {
        let (map, control) = board();
        let mut u = unit("infantry", A, Hex::new(0, 0));
        u.morale_pct = 90;
        let mut sc = setup(map, control, vec![u]);
        sc.sides[0].national_morale_pct = 70;
        if on {
            sc = with("nm_morale_multiplier", sc);
        }
        let mut s = start(sc);
        // A full turn: A's next phase recomputes morale and readiness.
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
        let u = s.unit(UnitId(1)).unwrap();
        (
            u.morale_pct,
            u.readiness_basis.morale_pct,
            s.readiness_morale_pct(A, 90),
        )
    };
    assert_eq!(run(false), (70, 70, 90), "capped at NM");
    // Rest +10 lifts morale to 100 above NM; readiness takes 100 × 0.90.
    assert_eq!(run(true), (100, 90, 81), "no cap, scaled in readiness");
}

/// П10 (sim only): `sc` variance draws round(E) + {−1, 0, +1} within [0, cap].
#[test]
fn sim_variance_sc_stays_next_to_the_expectation() {
    use ceasefire_core::combat::{draw_hits, FireParams, Variance};
    use ceasefire_core::rng::Pcg32;
    let mut rng = Pcg32::new(7, 0);
    let p = FireParams {
        n: 9,
        a: 5.5,
        d: 5.2,
        p: 0.32,
    };
    // E = 2.88 → 3; draws are 2, 3 or 4.
    let draws: Vec<u32> = (0..60)
        .map(|_| draw_hits(&mut rng, &p, Some(10), Variance::Sc))
        .collect();
    assert!(draws.iter().all(|d| (2..=4).contains(d)), "{draws:?}");
    assert!([2, 3, 4].iter().all(|k| draws.contains(k)));
    // Capped at the target's strength.
    assert!((0..30).all(|_| draw_hits(&mut rng, &p, Some(2), Variance::Sc) <= 2));
    assert_eq!(
        Variance::default(),
        Variance::Binomial,
        "the game draws binomially"
    );
}

/// R-ECO-02 / R-MOVE-05: with `reinforce_locks` a reinforced unit is not
/// transferred this phase; without the flag it is.
#[test]
fn r_eco_02_reinforced_unit_is_not_transferred() {
    use ceasefire_core::{RuleError, TransferError};
    let transfer_after_reinforce = |variant: Option<&str>| {
        let mut worn = unit("infantry", A, Hex::new(0, 0));
        worn.strength = 6;
        let mut s = road_board(vec![worn], variant);
        s.apply(Command::Reinforce {
            unit: UnitId(1),
            steps: 1,
            elite: false,
        })
        .unwrap();
        s.apply(Command::Transfer {
            unit: UnitId(1),
            to: Hex::new(6, 0),
        })
    };
    assert_eq!(
        transfer_after_reinforce(Some("reinforce_locks")),
        Err(RuleError::IllegalTransfer {
            unit: 1,
            reason: TransferError::Reinforced
        })
    );
    assert!(transfer_after_reinforce(None).is_ok());
}

/// R-COMBAT-07: every strike of the phase is prepared while the unit has not
/// moved (the tanks' second strike too); an advance after combat ends it.
#[test]
fn r_combat_07_bonus_per_strike_until_the_unit_moves() {
    use ceasefire_core::game::Event;
    // Tanks at (2,0) against two weak B units at (3,0) and (3,-1).
    let game = || {
        let (map, mut control) = board();
        control.insert(Hex::new(3, -1), A);
        let mut first = unit("infantry", B, Hex::new(3, 0));
        first.strength = 2;
        let second = unit("infantry", B, Hex::new(2, 1));
        let units = vec![first, second, unit("tank", A, Hex::new(2, 0))];
        start(with("prepared_attack", setup(map, control, units)))
    };
    const TANKS: UnitId = UnitId(3);
    let attack = |s: &mut GameState, target: u32| {
        let bonus = s
            .attack_modifiers(TANKS, UnitId(target))
            .unwrap()
            .prepared_pct;
        let events = s
            .apply(Command::Attack {
                unit: TANKS,
                target: UnitId(target),
            })
            .unwrap();
        (bonus, events)
    };
    // Two strikes without moving: the bonus on both.
    let mut s = game();
    let (b1, _) = attack(&mut s, 2);
    let (b2, _) = attack(&mut s, 2);
    assert_eq!((b1, b2), (10, 10));
    // Strike → advance into the vacated hex → strike: the bonus on the first only.
    let mut s = game();
    let (b1, events) = attack(&mut s, 1);
    assert_eq!(b1, 10);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Retreated { .. } | Event::UnitDestroyed { .. })),
        "strength 2 retreats or dies: {events:?}"
    );
    s.apply(Command::Advance { unit: TANKS }).unwrap();
    assert!(s
        .unit(UnitId(2))
        .unwrap()
        .hex
        .is_adjacent(s.unit(TANKS).unwrap().hex));
    let (b2, _) = attack(&mut s, 2);
    assert_eq!(b2, 0, "advance counts as movement");
}

/// R-HQ-02 `hq_strength_scaling`: readiness bonus (rating − 5) × 3 × strength
/// / 10, rounded half up; the supply bonus only at HQ strength ≥ 5.
#[test]
fn r_hq_02_hq_bonus_scales_with_strength() {
    let game = |hq_strength: u8, variant: bool| {
        let (map, control) = board();
        let mut hq = common::scenario::hq(A, Hex::new(1, 0), 8);
        hq.strength = hq_strength;
        let units = vec![unit("infantry", A, Hex::new(1, 1)), hq];
        let sc = setup(map, control, units);
        start(if variant {
            with("hq_strength_scaling", sc)
        } else {
            sc
        })
    };
    let bonus = |s: &GameState| s.unit(UnitId(1)).unwrap().readiness_basis.hq_pct;
    // Rating 8: full bonus 9 points.
    assert_eq!(bonus(&game(10, true)), 9);
    assert_eq!(bonus(&game(5, true)), 5, "9 × 5 / 10 = 4.5 → 5");
    assert_eq!(bonus(&game(3, true)), 3, "2.7 → 3");
    assert_eq!(bonus(&game(3, false)), 9, "off: full bonus");
    // Supply bonus: +2 (rating ≥ 8) at strength 5, none at 4 with the variant.
    let supply = |s: &GameState| s.hex_supply(A).unwrap().get(Hex::new(2, 0));
    let plain = {
        let (map, control) = board();
        start(setup(
            map,
            control,
            vec![unit("infantry", A, Hex::new(1, 1))],
        ))
    };
    let base = supply(&plain);
    assert_eq!(supply(&game(5, true)), (base + 2).min(10));
    assert_eq!(supply(&game(4, true)), base, "weak HQ: no supply bonus");
    assert_eq!(supply(&game(4, false)), (base + 2).min(10));
}

/// R-COMBAT-06 / R-MOVE-01 `fortified_hold`: a defender in a city does not
/// retreat and takes no «nowhere to go» losses; a unit that entered an own
/// city gets floor(max / 2) entrenchment at the start of its next phase.
#[test]
fn r_combat_06_strongholds_hold() {
    use ceasefire_core::game::Event;
    use ceasefire_core::map::HexCell;
    let fight = |variant: bool| {
        let (mut map, control) = board();
        map.insert(Hex::new(3, 0), HexCell::new("city"));
        let mut weak = unit("infantry", B, Hex::new(3, 0));
        weak.strength = 3;
        weak.entrenchment = 2; // a weak attack: the defender survives at strength ≤ 3
        let sc = setup(
            map,
            control,
            vec![weak, unit("infantry", A, Hex::new(2, 0))],
        );
        let mut s = start(if variant {
            with("fortified_hold", sc)
        } else {
            sc
        });
        s.apply(Command::Attack {
            unit: UnitId(2),
            target: UnitId(1),
        })
        .unwrap()
    };
    let moved_or_hit = |ev: &[Event]| {
        ev.iter()
            .any(|e| matches!(e, Event::Retreated { .. } | Event::RetreatBlocked { .. }))
    };
    assert!(
        moved_or_hit(&fight(false)),
        "strength ≤ 3: retreat or extra losses"
    );
    let held = fight(true);
    assert!(!moved_or_hit(&held), "a city holds: {held:?}");
    assert!(
        !held
            .iter()
            .any(|e| matches!(e, Event::UnitDestroyed { .. })),
        "survives this combat (seed 42)"
    );

    let entered = |variant: bool| {
        let (mut map, control) = board();
        map.insert(Hex::new(2, 0), HexCell::new("city")); // max entrenchment 4
        let sc = setup(map, control, vec![unit("tank", A, Hex::new(1, 0))]);
        let mut s = start(if variant {
            with("fortified_hold", sc)
        } else {
            sc
        });
        s.apply(Command::Move {
            unit: UnitId(1),
            path: vec![Hex::new(2, 0)],
        })
        .unwrap();
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
        s.unit(UnitId(1)).unwrap().entrenchment
    };
    assert_eq!(entered(true), 2, "floor(4 / 2)");
    assert_eq!(entered(false), 0, "off: moved, no growth");
}

/// R-SUP-03 `pocket_desertion`: at the start of its phase a unit at supply 0
/// with ≥ 2 adjacent enemies (strength ≥ 5, supply ≥ 4) loses 1 step with
/// 50% (seeded); with one enemy, or without the variant, never.
#[test]
fn r_sup_03_desertion_in_a_pocket() {
    // A's infantry cut off at (5, 0) in B's territory; B's units around it.
    let game = |seed: u64, enemies: usize, variant: bool| {
        let (map, control) = board();
        let mut cut = unit("infantry", A, Hex::new(5, 0));
        cut.supply = Some(0);
        let mut units = vec![cut];
        for h in [Hex::new(6, 0), Hex::new(4, 0)].into_iter().take(enemies) {
            units.push(unit("infantry", B, h));
        }
        let mut sc = setup(map, control, units);
        sc.seed = seed;
        start(if variant {
            with("pocket_desertion", sc)
        } else {
            sc
        })
    };
    let lost = |s: &GameState| 10 - s.unit(UnitId(1)).unwrap().strength;
    let hits: Vec<u8> = (1..=20).map(|seed| lost(&game(seed, 2, true))).collect();
    assert!(hits.iter().all(|&l| l <= 1), "{hits:?}");
    let n = hits.iter().filter(|&&l| l == 1).count();
    assert!((4..=16).contains(&n), "about half of 20 seeds: {n}");
    assert!(
        (1..=20).all(|seed| lost(&game(seed, 1, true)) == 0),
        "one enemy: no pressure"
    );
    assert!((1..=20).all(|seed| lost(&game(seed, 2, false)) == 0), "off");
    assert!(
        game(1, 2, true).desertion_risk(UnitId(1)),
        "«под угрозой распада»"
    );
    assert!(!game(1, 1, true).desertion_risk(UnitId(1)));
}

/// R-SUP-01 `source_isolation`: a source below 10 with no path through own
/// hexes to a source of 10 gives at most 3; a besieged source (≥ 2 adjacent
/// enemy ground units of strength ≥ 5) loses 1 per owner's phase.
#[test]
fn r_sup_01_isolated_and_besieged_sources() {
    use ceasefire_core::supply::SupplySource;
    let isolated = |variant: bool| {
        let (map, mut control) = board();
        control.insert(Hex::new(5, 2), A); // an own island in B's territory
        let mut sc = setup(map, control, vec![]);
        sc.sources.push(SupplySource {
            hex: Hex::new(5, 2),
            value: 8,
        });
        sc.sources.push(SupplySource {
            hex: Hex::new(2, 2),
            value: 8,
        });
        let s = start(if variant {
            with("source_isolation", sc)
        } else {
            sc
        });
        let m = s.hex_supply(A).unwrap();
        (m.get(Hex::new(5, 2)), m.get(Hex::new(2, 2)))
    };
    assert_eq!(isolated(true).0, 3, "cut off from the source of 10");
    assert!(isolated(true).1 >= 8, "linked through own hexes");
    assert_eq!(isolated(false).0, 8, "off: full value");

    let besieged = |variant: bool| {
        let (map, control) = board();
        let units = vec![
            unit("infantry", B, Hex::new(3, 2)),
            unit("infantry", B, Hex::new(3, 1)),
        ];
        let mut sc = setup(map, control, units);
        sc.sources.push(SupplySource {
            hex: Hex::new(2, 2),
            value: 8,
        });
        let mut s = start(if variant {
            with("source_isolation", sc)
        } else {
            sc
        });
        let first = s.source(Hex::new(2, 2)).map(|v| v.current);
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
        let second = s.source(Hex::new(2, 2)).map(|v| v.current);
        (first, second)
    };
    assert_eq!(
        besieged(true),
        (Some(7), Some(6)),
        "−1 at each owner's phase"
    );
    assert_eq!(besieged(false), (Some(8), Some(8)));
}

/// R-SUP-01 `source_isolation` (designer amendment): the link to a source of
/// 10 follows the R-SUP-02 path rules — a canal only by a bridge.
#[test]
fn r_sup_01_isolation_follows_supply_paths_across_a_canal() {
    use ceasefire_core::supply::SupplySource;
    let east_bank = |bridge: bool| {
        let (mut map, mut control) = board();
        // Own hexes (3,0), (4,0) behind a canal on the sides (2,0)|(3,0) and (2,1)|(3,0).
        control.insert(Hex::new(3, 0), A);
        control.insert(Hex::new(4, 0), A);
        map.add_canal(Hex::new(2, 0), Hex::new(3, 0)).unwrap();
        map.add_canal(Hex::new(2, 1), Hex::new(3, 0)).unwrap();
        if bridge {
            map.add_bridge(Hex::new(2, 0), Hex::new(3, 0)).unwrap();
        }
        let mut sc = setup(map, control, vec![]);
        sc.sources.push(SupplySource {
            hex: Hex::new(4, 0),
            value: 8,
        });
        start(with("source_isolation", sc))
            .hex_supply(A)
            .unwrap()
            .get(Hex::new(4, 0))
    };
    assert_eq!(east_bank(false), 3, "no bridge: cut off");
    assert_eq!(east_bank(true), 8, "a bridge links it");
}

/// SC п. 11: how many levels П1 strips is the attacker class's
/// `strips_entrenchment` (1 by default, 0 for HQs).
#[test]
fn r_combat_02_strips_entrenchment_by_attacker_class() {
    let d = common::data();
    assert_eq!(d.units.get("hq").unwrap().strips_entrenchment, 0);
    assert_eq!(d.units.get("infantry").unwrap().strips_entrenchment, 1);
    // Infantry that strips nothing: the П1 attacks leave the entrenchment.
    let mut units: serde_json::Value =
        serde_json::from_str(include_str!("../../../data/units.json")).unwrap();
    for u in units["units"].as_array_mut().unwrap() {
        if u["id"] == "infantry" {
            u["strips_entrenchment"] = 0.into();
        }
    }
    let units = units.to_string();
    let data = ceasefire_core::data::GameData::from_json(ceasefire_core::data::GameDataJson {
        rules: include_str!("../../../data/rules.json"),
        terrain: include_str!("../../../data/terrain.json"),
        units: &units,
        tech: include_str!("../../../data/tech.json"),
    })
    .unwrap();
    let s = GameState::new(
        std::sync::Arc::new(data),
        with("attack_strips_entrenchment", three_on_one()),
    )
    .unwrap()
    .0;
    let (_, entr) = three_attacks(s);
    assert_eq!(entr, vec![2, 2, 2]);
}

/// SC п. 30 `air_strength_threshold`: fighters below strength 5 neither
/// intercept nor escort; without the variant they do.
#[test]
fn r_air_02_worn_fighters_do_not_intercept_or_escort() {
    use ceasefire_core::tech::{TechBranch, TechLevels};
    let sc = |on: bool| {
        let (map, control) = board();
        let mut escort = unit("fighter", A, Hex::new(0, 0));
        escort.strength = 4;
        let mut mig = unit("fighter", B, Hex::new(6, 0));
        mig.strength = 4;
        let units = vec![
            unit("strike", A, Hex::new(0, 0)),
            unit("infantry", B, Hex::new(4, 0)),
            unit("infantry", A, Hex::new(1, 0)),
            escort,
            mig,
        ];
        let mut sc = setup(map, control, units);
        sc.cities = [Hex::new(0, 0), Hex::new(6, 0)]
            .into_iter()
            .map(|hex| ceasefire_core::game::CityValue { hex, value: 1 })
            .collect();
        sc.sides[0].tech = TechLevels::default().with(TechBranch::Aviation, 2);
        if on {
            with("air_strength_threshold", sc)
        } else {
            sc
        }
    };
    let off = start(sc(false))
        .forecast_air_strike(UnitId(1), UnitId(2))
        .unwrap();
    assert_eq!((off.escorts.len(), off.interceptors.len()), (1, 1));
    let on = start(sc(true))
        .forecast_air_strike(UnitId(1), UnitId(2))
        .unwrap();
    assert!(
        on.escorts.is_empty() && on.interceptors.is_empty(),
        "{on:?}"
    );
}

/// SC п. 34 `transfer_supply_min`: a strategic transfer needs unit supply
/// ≥ 4. The road board with the source at `source` (its value sets the supply).
#[test]
fn r_move_05_transfer_needs_supply_4() {
    use ceasefire_core::error::{RuleError, TransferError};
    let try_at = |source: u8, variant: Option<&str>| {
        let (mut map, mut control) = board();
        for q in 0..6 {
            map.add_road(Hex::new(q, 0), Hex::new(q + 1, 0)).unwrap();
        }
        for q in 0..=6 {
            control.insert(Hex::new(q, 0), A);
        }
        let mut u = unit("infantry", A, Hex::new(0, 0));
        u.supply = Some(source); // no inertia from a higher start
        let mut sc = setup(map, control, vec![u]);
        sc.sides[0].mpp = 100;
        sc.sources[0].value = source;
        let mut s = start(match variant {
            Some(v) => with(v, sc),
            None => sc,
        });
        let have = s.unit(UnitId(1)).unwrap().supply;
        let r = s.apply(Command::Transfer {
            unit: UnitId(1),
            to: Hex::new(6, 0),
        });
        (have, r)
    };
    let (have, r) = try_at(3, Some("transfer_supply_min"));
    assert_eq!(have, 3);
    assert!(matches!(
        r,
        Err(RuleError::IllegalTransfer {
            reason: TransferError::LowSupply,
            ..
        })
    ));
    assert!(try_at(3, None).1.is_ok(), "off: allowed");
    let (have, r) = try_at(4, Some("transfer_supply_min"));
    assert!(have == 4 && r.is_ok(), "supply {have}: allowed");
}

/// SC п. 17 `hq_contact_priority`: an HQ with one free slot takes its unit
/// in contact (farther) over the nearer one in the rear; and of the other
/// units the one in contact goes first. Without the variant — the nearest.
#[test]
fn r_hq_01_units_in_contact_attach_first() {
    let sc = |on: bool| {
        let (map, control) = board();
        let mut hq = unit("hq", A, Hex::new(0, 0));
        hq.hq_rating = Some(1);
        hq.formation = Some("d1".into());
        let mut rear = unit("infantry", A, Hex::new(0, 1));
        rear.formation = Some("d1".into());
        let mut front = unit("infantry", A, Hex::new(2, 0));
        front.formation = Some("d1".into());
        let units = vec![hq, rear, front, unit("infantry", B, Hex::new(3, 0))];
        let sc = setup(map, control, units);
        if on {
            with("hq_contact_priority", sc)
        } else {
            sc
        }
    };
    let attached =
        |s: &GameState| [UnitId(2), UnitId(3)].map(|id| s.unit(id).unwrap().attached_hq.is_some());
    assert_eq!(attached(&start(sc(false))), [true, false], "nearest first");
    assert_eq!(attached(&start(sc(true))), [false, true], "contact first");

    // Other units (no formation): the one in contact wins the slot.
    let sc = |on: bool| {
        let (map, control) = board();
        let mut hq = unit("hq", A, Hex::new(0, 0));
        hq.hq_rating = Some(1);
        let units = vec![
            hq,
            unit("infantry", A, Hex::new(0, 1)),
            unit("infantry", A, Hex::new(2, 0)),
            unit("infantry", B, Hex::new(3, 0)),
        ];
        let sc = setup(map, control, units);
        if on {
            with("hq_contact_priority", sc)
        } else {
            sc
        }
    };
    assert_eq!(attached(&start(sc(false))), [true, false]);
    assert_eq!(attached(&start(sc(true))), [false, true]);
}

/// SC п. 8 `armor_city_penalty`: a tank defending a city takes the terrain
/// ×1,0 instead of ×1,5; infantry in the city keeps ×1,5; on plain nothing changes.
#[test]
fn r_combat_02_armor_in_a_city_loses_the_city_bonus() {
    let d_of = |defender: &str, terrain: &str, on: bool| {
        let (mut map, control) = board();
        map.insert(Hex::new(3, 0), ceasefire_core::map::HexCell::new(terrain));
        let units = vec![
            unit(defender, B, Hex::new(3, 0)),
            unit("infantry", A, Hex::new(2, 0)),
        ];
        let sc = setup(map, control, units);
        let s = start(if on {
            with("armor_city_penalty", sc)
        } else {
            sc
        });
        s.forecast_attack(UnitId(2), UnitId(1))
            .unwrap()
            .exchange
            .vs_defender
            .d
    };
    let (off, on) = (d_of("tank", "city", false), d_of("tank", "city", true));
    assert!(
        (off / on - 1.5).abs() < 1e-9,
        "tank in a city: {off} → {on}"
    );
    assert_eq!(
        d_of("infantry", "city", false),
        d_of("infantry", "city", true)
    );
    assert_eq!(d_of("tank", "plain", false), d_of("tank", "plain", true));
}

/// SC п. 26 `arrival_choice`: a purchase may name its arrival source — own,
/// current value ≥ 8, no enemy ground unit next to it; the unit appears there.
/// Without a choice — the old rule; without the variant a choice is refused.
#[test]
fn r_eco_02_arrival_at_the_chosen_source() {
    use ceasefire_core::error::{EconomyError, RuleError};
    use ceasefire_core::supply::SupplySource;
    let sc = |on: bool| {
        let (map, control) = board();
        let units = vec![unit("infantry", B, Hex::new(3, 2))];
        let mut sc = setup(map, control, units);
        sc.sources.extend([
            SupplySource {
                hex: Hex::new(2, -2),
                value: 8,
            },
            SupplySource {
                hex: Hex::new(2, 2), // next to B's infantry
                value: 8,
            },
            SupplySource {
                hex: Hex::new(1, -3),
                value: 7,
            },
        ]);
        sc.sides[0].mpp = 300;
        sc.sides[0].purchase_limits.insert("mech".into(), 3);
        start(if on { with("arrival_choice", sc) } else { sc })
    };
    let bad = |r: Result<Vec<ceasefire_core::game::Event>, RuleError>| {
        matches!(r, Err(RuleError::IllegalEconomy(EconomyError::BadArrival)))
    };
    let mut s = sc(true);
    assert_eq!(s.arrival_choices(A), vec![Hex::new(0, 0), Hex::new(2, -2)]);
    for h in [Hex::new(2, 2), Hex::new(1, -3), Hex::new(4, 0)] {
        let r = s.apply(Command::Purchase {
            class: "mech".into(),
            at: Some(h),
        });
        assert!(bad(r), "{h:?} refused");
    }
    s.apply(Command::Purchase {
        class: "mech".into(),
        at: Some(Hex::new(2, -2)),
    })
    .unwrap();
    for _ in 0..4 {
        s.apply(Command::EndPhase).unwrap();
    }
    let hexes: Vec<Hex> = s.units().filter(|u| u.side == A).map(|u| u.hex).collect();
    assert_eq!(hexes, vec![Hex::new(2, -2)], "arrived at the chosen source");

    let mut s = sc(false);
    assert!(s.arrival_choices(A).is_empty());
    let r = s.apply(Command::Purchase {
        class: "mech".into(),
        at: Some(Hex::new(2, -2)),
    });
    assert!(bad(r), "off: a choice is refused");
}
