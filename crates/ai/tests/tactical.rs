//! Tactical AI (T-TECH-01): plays a real dumped battle to the end, every
//! command legal, deterministic by seed.

use std::sync::Arc;

use ceasefire_ai::tactical::TacticalAi;
use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_tactical::battle::TurnMode;
use ceasefire_tactical::{build_battle, BattleInput, TacResult, TacRules};

fn play(seed: u64, mode: TurnMode) -> (TacResult, usize) {
    let rules = Arc::new(TacRules::from_json(include_str!("../../../data/tactical.json")).unwrap());
    let data = GameData::from_json(GameDataJson {
        rules: include_str!("../../../data/rules.json"),
        terrain: include_str!("../../../data/terrain.json"),
        units: include_str!("../../../data/units.json"),
        tech: include_str!("../../../data/tech.json"),
    })
    .unwrap();
    let input: BattleInput =
        serde_json::from_str(include_str!("../../tactical/tests/fixtures/battle.json")).unwrap();
    let mut b = build_battle(rules, &data, &input, seed, mode).unwrap();
    let ai = TacticalAi::new(seed);
    let mut n = 0;
    while b.result.is_none() {
        b.apply(ai.next(&b)).expect("the AI's command is legal");
        n += 1;
        assert!(n < 3000, "the battle ends");
    }
    (b.result.unwrap(), n)
}

#[test]
fn t_tech_01_ai_plays_a_dumped_battle_deterministically() {
    for mode in [TurnMode::Alternating, TurnMode::IgougoOverwatch] {
        let (a, n) = play(3, mode);
        let (b, m) = play(3, mode);
        assert_eq!((a.clone(), n), (b, m), "same seed, same battle");
        assert!(a.turns >= 1 && n > 0);
    }
}
