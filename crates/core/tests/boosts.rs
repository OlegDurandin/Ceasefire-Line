//! R-TECH-02: tech boosts for MPP.

mod common;

use ceasefire_core::game::{Command, Event, GameState, TechBoost};
use ceasefire_core::hex::Hex;
use ceasefire_core::tech::TechBranch;
use ceasefire_core::{EconomyError, RuleError};

use common::scenario::{board, setup, start, unit, A, B};

/// GDD R-TECH-02 table: «Экстренная поставка ПТУР», Israel, antitank, 60 MPP, 6 turns.
fn atgm(id: &str) -> TechBoost {
    TechBoost {
        id: id.into(),
        side: A,
        branch: TechBranch::Antitank,
        cost: 60,
        turns: 6,
    }
}

fn game(boosts: Vec<TechBoost>, mpp: u32, start_level: u8) -> GameState {
    let (map, control) = board();
    let mut sc = setup(map, control, vec![unit("infantry", A, Hex::new(1, 0))]);
    sc.sides[0].mpp = mpp;
    sc.sides[0].tech = sc.sides[0]
        .tech
        .clone()
        .with(TechBranch::Antitank, start_level);
    sc.tech_boosts = boosts;
    start(sc)
}

fn buy(s: &mut GameState, id: &str) -> Result<Vec<Event>, RuleError> {
    s.apply(Command::BuyTechBoost { id: id.into() })
}

fn refused(r: Result<Vec<Event>, RuleError>) -> EconomyError {
    match r {
        Err(RuleError::IllegalEconomy(e)) => e,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn level(s: &GameState) -> u8 {
    s.side(A).unwrap().tech.level(TechBranch::Antitank).unwrap()
}

#[test]
fn r_tech_02_emergency_atgm_example() {
    let mut s = game(vec![atgm("isr_atgm")], 100, 1);
    let events = buy(&mut s, "isr_atgm").unwrap();
    assert_eq!(
        events,
        vec![Event::TechBoostBought {
            id: "isr_atgm".into(),
            side: A,
            cost: 60,
            ready_turn: 7,
        }]
    );
    assert_eq!(s.side(A).unwrap().mpp, 40);
    for _ in 0..5 {
        s.apply(Command::EndPhase).unwrap();
        s.apply(Command::EndPhase).unwrap();
        assert_eq!(level(&s), 1, "turn {}: not yet", s.turn());
    }
    s.apply(Command::EndPhase).unwrap();
    let events = s.apply(Command::EndPhase).unwrap();
    assert_eq!(s.turn(), 7);
    assert!(events.contains(&Event::TechAdvanced {
        side: A,
        branch: TechBranch::Antitank,
        level: 2,
        boost: "isr_atgm".into(),
    }));
    // Combat values follow the side's level at once: hard attack 2 + 4.
    let d = s.data();
    let class = d.units.get("infantry").unwrap();
    let stats = d.tech.unit_stats(class, &s.side(A).unwrap().tech).unwrap();
    assert_eq!(stats.attack.hard, 6.0);
}

#[test]
fn r_tech_02_once_own_side_and_mpp() {
    let mut s = game(vec![atgm("isr_atgm")], 100, 1);
    assert_eq!(refused(buy(&mut s, "nope")), EconomyError::UnknownBoost);
    buy(&mut s, "isr_atgm").unwrap();
    assert_eq!(
        refused(buy(&mut s, "isr_atgm")),
        EconomyError::BoostAlreadyBought
    );

    let mut s = game(vec![atgm("isr_atgm")], 59, 1);
    assert_eq!(refused(buy(&mut s, "isr_atgm")), EconomyError::NotEnoughMpp);

    let mut theirs = atgm("syr_atgm");
    theirs.side = B;
    let mut s = game(vec![theirs], 100, 1);
    assert_eq!(refused(buy(&mut s, "syr_atgm")), EconomyError::NotYourBoost);
}

#[test]
fn r_tech_02_at_most_one_level_above_start() {
    // Two boosts on the same branch: the second would make +2.
    let mut s = game(vec![atgm("a"), atgm("b")], 200, 1);
    buy(&mut s, "a").unwrap();
    assert_eq!(
        refused(buy(&mut s, "b")),
        EconomyError::BoostCap,
        "pending boost counts"
    );
    // Already at the top level.
    let mut s = game(vec![atgm("a")], 200, 3);
    assert_eq!(refused(buy(&mut s, "a")), EconomyError::BoostCap);
}
