//! R-UI-04: the explanations shown in the UI come from the same code as the
//! rules — the formula inputs reproduce the forecast, the readiness inputs
//! reproduce the readiness.

mod common;

use ceasefire_core::combat::fire_params;
use ceasefire_core::game::UnitId;
use ceasefire_core::hex::Hex;
use ceasefire_core::unit::readiness_pct;

use common::scenario::{board, hq, setup, start, unit, A, B};

#[test]
fn r_ui_04_attack_explanation_reproduces_the_forecast() {
    let (map, control) = board();
    let mut def = unit("infantry", B, Hex::new(3, 0));
    def.entrenchment = 2;
    def.exp = 100;
    let g = start(setup(
        map,
        control,
        vec![
            unit("tank", A, Hex::new(2, 0)),
            def,
            hq(A, Hex::new(0, 0), 7),
        ],
    ));
    let rules = &g.data().rules.combat;
    let [to_def, to_att] = g.explain_attack(UnitId(1), UnitId(2)).unwrap();
    let f = g.forecast_attack(UnitId(1), UnitId(2)).unwrap();
    assert_eq!(fire_params(rules, &to_def), f.exchange.vs_defender);
    assert_eq!(fire_params(rules, &to_att), f.exchange.vs_attacker);
    assert_eq!(
        to_def.entrenchment, 2,
        "the defender's trenches are in the input"
    );
    assert_eq!(to_def.target_exp, 1);
}

#[test]
fn r_ui_04_readiness_basis_reproduces_base_readiness() {
    let (map, control) = board();
    let mut low = unit("infantry", A, Hex::new(1, 1));
    low.morale_pct = 60;
    let g = start(setup(
        map,
        control,
        vec![
            low,
            unit("tank", A, Hex::new(2, 0)),
            hq(A, Hex::new(1, 0), 8),
        ],
    ));
    let rules = &g.data().rules.readiness;
    for u in g.units() {
        let b = u.readiness_basis;
        assert_eq!(
            readiness_pct(rules, b.supply, b.morale_pct, b.exp_stars, b.hq_pct),
            u.base_readiness_pct,
            "unit {:?}",
            u.id
        );
    }
    assert_eq!(
        g.unit(UnitId(1)).unwrap().readiness_basis.hq_pct,
        9,
        "attached to a rating-8 HQ: +(8 − 5) × 3"
    );
}
