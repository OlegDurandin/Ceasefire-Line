//! R-ECO-01..03, R-SUP-03: income, reinforcement, purchases.

mod common;

use ceasefire_core::game::{CityValue, Command, Event, GameState, ScheduledIncome, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::{EconomyError, RuleError};

use common::scenario::{board, setup, start, unit, A};

fn mpp(s: &GameState) -> u32 {
    s.side(A).unwrap().mpp
}

fn end_turn(s: &mut GameState) {
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap();
}

fn reinforce(s: &mut GameState, id: u32, steps: u8, elite: bool) -> Result<Vec<Event>, RuleError> {
    s.apply(Command::Reinforce {
        unit: UnitId(id),
        steps,
        elite,
    })
}

fn refused(r: Result<Vec<Event>, RuleError>) -> EconomyError {
    match r {
        Err(RuleError::IllegalEconomy(e)) => e,
        other => panic!("expected an economy refusal, got {other:?}"),
    }
}

#[test]
fn r_eco_01_income_base_cities_and_airlift() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![]);
    sc.sides[0].base_income = 35;
    // (2,0): 2 AP from the source → supply 9.
    sc.cities = vec![
        CityValue {
            hex: Hex::new(2, 0),
            value: 10,
        },
        CityValue {
            hex: Hex::new(2, 1),
            value: 5,
        },
        // Enemy-held city brings nothing.
        CityValue {
            hex: Hex::new(4, 0),
            value: 10,
        },
    ];
    sc.scheduled_income = vec![ScheduledIncome {
        side: A,
        from_turn: 2,
        until_turn: None,
        amount: 15,
    }];
    let mut s = start(sc);
    // 35 + round_half_up((10·9 + 5·9) / 10) = 35 + round(13.5) = 49
    assert_eq!(mpp(&s), 49);
    end_turn(&mut s);
    assert_eq!(mpp(&s), 49 + 49 + 15, "airlift from turn 2");
}

#[test]
fn r_sup_03_reinforcement_full_supply() {
    let (map, control) = board();
    let mut u = unit("infantry", A, Hex::new(1, 0));
    u.strength = 5;
    u.exp = 100;
    let mut sc = setup(map, control, vec![u]);
    sc.sides[0].mpp = 100;
    let mut s = start(sc);
    assert_eq!(s.unit(UnitId(1)).unwrap().supply, 10);

    assert_eq!(
        s.reinforce_cost(UnitId(1), 3, false),
        Ok(21),
        "3 × 10% of 70"
    );
    reinforce(&mut s, 1, 3, false).unwrap();
    let x = s.unit(UnitId(1)).unwrap();
    assert_eq!((x.strength, x.exp), (8, 62), "1 star × 5/8 → 0.62 star");
    assert_eq!(mpp(&s), 79);
    assert_eq!(
        refused(reinforce(&mut s, 1, 1, false)),
        EconomyError::StepLimit
    );
}

#[test]
fn r_sup_03_reinforcement_bands_and_elite() {
    let (map, mut control) = board();
    // A pocket at (5,0) with no supply path: unit supply 8 → 5 → 2 → 0.
    control.insert(Hex::new(5, 0), A);
    let mut u = unit("infantry", A, Hex::new(5, 0));
    u.strength = 4;
    u.exp = 200;
    let mut sc = setup(map, control, vec![u]);
    sc.sides[0].mpp = 200;
    let mut s = start(sc);
    assert_eq!(s.unit(UnitId(1)).unwrap().supply, 5);
    assert_eq!(s.reinforce_cost(UnitId(1), 2, false), Ok(21), "×1.5");
    assert_eq!(
        s.reinforce_cost(UnitId(1), 2, true),
        Ok(25),
        "elite: 25.2 → 25"
    );
    assert_eq!(
        refused(reinforce(&mut s, 1, 3, false)),
        EconomyError::StepLimit
    );
    reinforce(&mut s, 1, 2, true).unwrap();
    assert_eq!(
        s.unit(UnitId(1)).unwrap().exp,
        200,
        "elite keeps experience"
    );

    end_turn(&mut s);
    assert_eq!(s.unit(UnitId(1)).unwrap().supply, 2);
    assert_eq!(s.reinforce_cost(UnitId(1), 1, false), Ok(14), "×2");
    assert_eq!(
        refused(reinforce(&mut s, 1, 2, false)),
        EconomyError::StepLimit
    );

    end_turn(&mut s);
    assert_eq!(s.unit(UnitId(1)).unwrap().supply, 0);
    assert_eq!(
        refused(reinforce(&mut s, 1, 1, false)),
        EconomyError::NoSupply
    );
}

#[test]
fn r_eco_02_reinforcement_limits() {
    let (map, control) = board();
    let mut weak = unit("infantry", A, Hex::new(1, 0));
    weak.strength = 5;
    let units = vec![weak, unit("tank", A, Hex::new(1, 1))];
    let mut sc = setup(map, control, units);
    sc.sides[0].mpp = 10;
    let mut s = start(sc);
    assert_eq!(
        refused(reinforce(&mut s, 2, 1, false)),
        EconomyError::FullStrength
    );
    assert_eq!(
        refused(reinforce(&mut s, 1, 2, false)),
        EconomyError::NotEnoughMpp
    );
    assert_eq!(
        refused(reinforce(&mut s, 1, 0, false)),
        EconomyError::ZeroSteps
    );
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(1, -1)],
    })
    .unwrap();
    assert_eq!(
        refused(reinforce(&mut s, 1, 1, false)),
        EconomyError::UnitActed
    );
}

#[test]
fn r_eco_03_purchase_limit_and_arrival() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![]);
    sc.sides[0].mpp = 250;
    sc.sides[0].purchase_limits.insert("tank".into(), 1);
    let mut s = start(sc);

    let events = s
        .apply(Command::Purchase {
            class: "tank".into(),
        })
        .unwrap();
    assert_eq!(
        events,
        vec![Event::Purchased {
            side: A,
            class: "tank".into(),
            cost: 105,
            arrives_turn: 3,
        }]
    );
    assert_eq!(mpp(&s), 145);
    let limit = s.apply(Command::Purchase {
        class: "tank".into(),
    });
    assert_eq!(refused(limit), EconomyError::PurchaseLimit);
    let no_limit = s.apply(Command::Purchase {
        class: "mech".into(),
    });
    assert_eq!(
        refused(no_limit),
        EconomyError::PurchaseLimit,
        "not in the scenario limits"
    );

    end_turn(&mut s);
    assert!(s.units().next().is_none(), "turn 2: still on the way");
    end_turn(&mut s);
    let tank = s.units().next().expect("arrived on turn 3");
    assert_eq!(
        (tank.class.as_str(), tank.hex, tank.strength),
        ("tank", Hex::new(0, 0), 10)
    );
    assert_eq!(tank.ap_halves, 10, "refreshed in the arrival phase");
    assert_eq!(s.side(A).unwrap().purchased.get("tank"), Some(&1));
}

#[test]
fn r_eco_02_arrival_waits_for_a_free_source() {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("infantry", A, Hex::new(0, 0))]);
    sc.sides[0].mpp = 100;
    sc.sides[0].purchase_limits.insert("mech".into(), 1);
    let mut s = start(sc);
    s.apply(Command::Purchase {
        class: "mech".into(),
    })
    .unwrap();
    end_turn(&mut s);
    s.apply(Command::EndPhase).unwrap();
    let events = s.apply(Command::EndPhase).unwrap();
    assert!(events.contains(&Event::ArrivalDelayed {
        side: A,
        class: "mech".into(),
    }));
    assert_eq!(s.arrivals().len(), 1);
    s.apply(Command::Move {
        unit: UnitId(1),
        path: vec![Hex::new(1, 0)],
    })
    .unwrap();
    end_turn(&mut s);
    assert!(s
        .units()
        .any(|u| u.class == "mech" && u.hex == Hex::new(0, 0)));
}
