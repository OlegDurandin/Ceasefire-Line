//! R-AIR-01, R-AIR-02 in the game (stage 4, no SAM umbrellas): sorties,
//! interception and escort, strikes on units and airfields, rest, rebasing,
//! off-map bases.

mod common;

use ceasefire_core::error::RuleError;
use ceasefire_core::game::{
    AirBase, AirError, CityValue, Command, Event, GameState, MapEdge, OffMapBase, ScenarioSetup,
    UnitId, UnitSetup,
};
use ceasefire_core::hex::Hex;
use ceasefire_core::tech::{TechBranch, TechLevels};

use common::scenario::{board, setup, start, unit, A, B};

const A_CITY: Hex = Hex { q: 0, r: 0 };
const B_CITY: Hex = Hex { q: 6, r: 0 };

/// A strike group (and more aircraft) of side A against B's dug-in infantry at (4, 0).
fn air_setup(extra: Vec<UnitSetup>) -> ScenarioSetup {
    let (map, control) = board();
    let mut target = unit("infantry", B, Hex::new(4, 0));
    target.entrenchment = 2;
    let mut units = vec![
        unit("strike", A, A_CITY),
        target,
        unit("infantry", A, Hex::new(1, 0)),
    ];
    units.extend(extra);
    let mut sc = setup(map, control, units);
    sc.cities = vec![
        CityValue {
            hex: A_CITY,
            value: 1,
        },
        CityValue {
            hex: Hex::new(2, 0),
            value: 1,
        },
        CityValue {
            hex: B_CITY,
            value: 1,
        },
    ];
    sc.sides[0].tech = TechLevels::default().with(TechBranch::Aviation, 2);
    sc
}

const STRIKE: UnitId = UnitId(1);
const TARGET: UnitId = UnitId(2);

fn refused(r: Result<Vec<Event>, RuleError>) -> AirError {
    match r {
        Err(RuleError::IllegalAir { reason, .. }) => reason,
        other => panic!("expected an air refusal, got {other:?}"),
    }
}

#[test]
fn r_air_02_strike_uses_the_forecast_and_tires_both_sides() {
    let mut s = start(air_setup(vec![]));
    let f = s.forecast_air_strike(STRIKE, TARGET).unwrap();
    assert!(f.interceptors.is_empty() && f.escorts.is_empty());
    let events = s
        .apply(Command::AirStrike {
            unit: STRIKE,
            target: TARGET,
        })
        .unwrap();
    let hit = events
        .iter()
        .find_map(|e| match e {
            Event::AirStrike {
                params,
                return_params,
                ..
            } => Some((*params, *return_params)),
            _ => None,
        })
        .expect("strike event");
    assert_eq!(hit.0, f.strike, "prognosis = resolution");
    assert_eq!(Some(hit.1), f.return_fire);
    let air = &s.data().rules.air;
    assert_eq!(
        s.air_unit(STRIKE).unwrap().fatigue_pct,
        air.sortie_readiness_loss_pct
    );
    if let Some(t) = s.unit(TARGET) {
        assert_eq!(t.event_penalty_pct, air.ground_strike_readiness_loss_pct);
    }
    // One sortie per phase (strikes: 1).
    let again = s.apply(Command::AirStrike {
        unit: STRIKE,
        target: TARGET,
    });
    if s.unit(TARGET).is_some() {
        assert_eq!(refused(again), AirError::AlreadyFlew);
    }
}

#[test]
fn r_air_01_no_sorties_at_night_and_rest_after_a_quiet_phase() {
    let mut s = start(air_setup(vec![]));
    s.apply(Command::AirStrike {
        unit: STRIKE,
        target: TARGET,
    })
    .unwrap();
    s.apply(Command::EndPhase).unwrap(); // B, turn 1
    s.apply(Command::EndPhase).unwrap(); // A, turn 2
    assert!(s.is_night());
    if s.unit(TARGET).is_some() {
        assert_eq!(
            refused(s.apply(Command::AirStrike {
                unit: STRIKE,
                target: TARGET
            })),
            AirError::Night
        );
    }
    let fatigue = s.air_unit(STRIKE).map(|a| a.fatigue_pct);
    assert_eq!(fatigue, Some(10), "flew in its last phase: no rest yet");
    s.apply(Command::EndPhase).unwrap();
    s.apply(Command::EndPhase).unwrap(); // A, turn 3: a phase without sorties behind
    assert_eq!(s.air_unit(STRIKE).map(|a| a.fatigue_pct), Some(0));
}

#[test]
fn r_air_02_escort_takes_one_interceptor_the_second_hits_the_group() {
    let escort = unit("fighter", A, A_CITY);
    let mig1 = unit("fighter", B, B_CITY);
    let mig2 = unit("fighter", B, B_CITY);
    let mut s = start(air_setup(vec![escort, mig1, mig2]));
    let f = s.forecast_air_strike(STRIKE, TARGET).unwrap();
    assert_eq!(f.escorts, vec![UnitId(4)]);
    assert_eq!(f.interceptors.len(), 2);
    assert_eq!(f.escort_duels.len(), 1);
    assert_eq!(f.escort_duels[0].a, UnitId(4));
    assert_eq!(f.group_duels.len(), 1);
    assert_eq!(
        f.group_duels[0].b, STRIKE,
        "the second interceptor fights the strike group"
    );
    let events = s
        .apply(Command::AirStrike {
            unit: STRIKE,
            target: TARGET,
        })
        .unwrap();
    let battles = events
        .iter()
        .filter(|e| matches!(e, Event::AirBattle { .. }))
        .count();
    assert_eq!(battles, 2);
    for id in &f.interceptors {
        if let Some(m) = s.air_unit(*id) {
            assert!(m.intercepted);
        }
    }
}

#[test]
fn r_air_02_strike_on_an_airfield_hits_parked_aircraft() {
    let parked = unit("fighter", B, B_CITY);
    let mut s = start(air_setup(vec![parked]));
    let f = s.forecast_airfield_strike(STRIKE, B_CITY).unwrap();
    assert_eq!(f.strike.d, s.data().rules.air.parked_air_defense);
    // The parked fighter also intercepts (its base is within range of the target).
    let events = s
        .apply(Command::AirStrikeAirfield {
            unit: STRIKE,
            hex: B_CITY,
        })
        .unwrap();
    if s.air_unit(STRIKE).is_some() {
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::AirfieldStrike { .. })));
        if let Some(m) = s.air_unit(UnitId(4)) {
            assert!(m.fatigue_pct >= s.data().rules.air.airfield_strike_readiness_loss_pct);
        }
    }
    // Fighters do not strike airfields.
    let mut s2 = start(air_setup(vec![
        unit("fighter", A, A_CITY),
        unit("fighter", B, B_CITY),
    ]));
    assert_eq!(
        refused(s2.apply(Command::AirStrikeAirfield {
            unit: UnitId(4),
            hex: B_CITY
        })),
        AirError::NotInfrastructureStriker
    );
}

#[test]
fn r_air_01_rebase_takes_the_phase_and_needs_an_own_base() {
    let mut s = start(air_setup(vec![]));
    assert_eq!(
        refused(s.apply(Command::Rebase {
            unit: STRIKE,
            base: AirBase::Hex { hex: B_CITY }
        })),
        AirError::NotABase
    );
    s.apply(Command::Rebase {
        unit: STRIKE,
        base: AirBase::Hex {
            hex: Hex::new(2, 0),
        },
    })
    .unwrap();
    assert_eq!(
        refused(s.apply(Command::AirStrike {
            unit: STRIKE,
            target: TARGET
        })),
        AirError::Rebased
    );
}

#[test]
fn r_air_01_offmap_base_range_counts_from_the_edge() {
    let mut near = air_setup(vec![]);
    near.units[0].offmap_base = Some("rear".into());
    near.offmap_bases = vec![OffMapBase {
        id: "rear".into(),
        side: A,
        edge: MapEdge::West,
        distance_km: 50.0,
    }];
    let s = start(near.clone());
    assert_eq!(
        s.air_unit(STRIKE).unwrap().base,
        AirBase::OffMap { id: "rear".into() }
    );
    let d = s
        .base_distance(&AirBase::OffMap { id: "rear".into() }, Hex::new(4, 0))
        .unwrap();
    assert!(d >= 4 + 5, "edge distance plus 50 km ({d})");
    assert!(s.forecast_air_strike(STRIKE, TARGET).is_ok());
    let mut far = near;
    far.offmap_bases[0].distance_km = 5000.0;
    let s = start(far);
    assert!(matches!(
        s.forecast_air_strike(STRIKE, TARGET),
        Err(RuleError::IllegalAir {
            reason: AirError::OutOfRange,
            ..
        })
    ));
}

#[test]
fn r_air_01_aircraft_need_an_own_base() {
    let mut sc = air_setup(vec![]);
    sc.units[0].hex = Hex::new(1, 1); // own hex, but no city or airfield
    assert!(GameState::new(std::sync::Arc::new(common::data()), sc).is_err());
}
