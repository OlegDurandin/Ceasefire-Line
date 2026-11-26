//! R-AIR-03: SAM umbrella fire against a sortie, before the strike.

use serde::{Deserialize, Serialize};

use crate::combat::{fire_params, sample_hits, Combatant, FireInput, FireParams};
use crate::data::Rules;
use crate::prob::{hits_distribution, LossForecast};
use crate::rng::Pcg32;

/// One SAM's fire; `sam_index` points into the slice given to [`sam_barrage`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SamShot {
    pub sam_index: usize,
    pub params: FireParams,
}

/// R-AIR-03: every covering SAM fires with half its effective strength.
/// SAMs fire from best to worst hit chance (ties keep input order); the k-th
/// SAM's chance is multiplied by `next_sam_mult^(k-1)`. EW (`ew_penalty`) is
/// subtracted before clamping.
pub fn sam_barrage(
    rules: &Rules,
    sams: &[Combatant<'_>],
    aircraft: &Combatant<'_>,
    ew_penalty: f64,
) -> Vec<SamShot> {
    let mut shots: Vec<SamShot> = sams
        .iter()
        .enumerate()
        .map(|(sam_index, sam)| SamShot {
            sam_index,
            params: fire_params(
                &rules.combat,
                &FireInput {
                    strength: sam.strength,
                    readiness_pct: sam.readiness_pct,
                    shot_fraction: rules.sam.shot_fraction,
                    attack: sam.stats.attack.air,
                    shooter_exp: sam.exp_stars,
                    attack_mult: 1.0,
                    defense: aircraft.stats.defense.air,
                    target_exp: aircraft.exp_stars,
                    terrain_mult: 1.0,
                    entrenchment: 0,
                    defense_mult: 1.0,
                    p_shift: ew_penalty,
                },
            ),
        })
        .collect();
    shots.sort_by(|x, y| y.params.p.total_cmp(&x.params.p));
    let mut mult = 1.0;
    for shot in &mut shots {
        shot.params.p *= mult;
        mult *= rules.sam.next_sam_mult;
    }
    shots
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarrageForecast {
    pub shots: Vec<SamShot>,
    pub losses: LossForecast,
}

/// R-AIR-03 + R-COMBAT-03: expected aircraft losses under the umbrella.
pub fn forecast_barrage(
    rules: &Rules,
    sams: &[Combatant<'_>],
    aircraft: &Combatant<'_>,
    ew_penalty: f64,
) -> BarrageForecast {
    let shots = sam_barrage(rules, sams, aircraft, ew_penalty);
    let groups: Vec<(u32, f64)> = shots.iter().map(|s| (s.params.n, s.params.p)).collect();
    let dist = hits_distribution(&groups, u32::from(aircraft.strength));
    let losses = LossForecast::from_distribution(
        &dist,
        rules.combat.forecast_low_percentile,
        rules.combat.forecast_high_percentile,
    );
    BarrageForecast { shots, losses }
}

/// R-AIR-03: samples aircraft losses (capped at its strength).
pub fn resolve_barrage(
    rules: &Rules,
    sams: &[Combatant<'_>],
    aircraft: &Combatant<'_>,
    ew_penalty: f64,
    rng: &mut Pcg32,
) -> u8 {
    let hits: u32 = sam_barrage(rules, sams, aircraft, ew_penalty)
        .iter()
        .map(|s| sample_hits(rng, &s.params))
        .sum();
    u8::try_from(hits).unwrap_or(u8::MAX).min(aircraft.strength)
}

/// R-AIR-02: fire of an aircraft at a ground unit. Terrain does not count,
/// entrenchment and experience do: D = AirDef × (1 + 0,15·Entr) × (1 + 0,1·Exp).
pub fn strike_input(striker: &Combatant<'_>, target: &Combatant<'_>) -> FireInput {
    FireInput {
        strength: striker.strength,
        readiness_pct: striker.readiness_pct,
        shot_fraction: 1.0,
        attack: striker.stats.attack.against(target.class.target_class),
        shooter_exp: striker.exp_stars,
        attack_mult: 1.0,
        defense: target.stats.defense.air,
        target_exp: target.exp_stars,
        terrain_mult: 1.0,
        entrenchment: target.entrenchment,
        defense_mult: 1.0,
        p_shift: 0.0,
    }
}

/// R-AIR-02: the target answers with its own air attack only.
pub fn strike_return_input(target: &Combatant<'_>, striker: &Combatant<'_>) -> FireInput {
    FireInput {
        strength: target.strength,
        readiness_pct: target.readiness_pct,
        shot_fraction: 1.0,
        attack: target.stats.attack.air,
        shooter_exp: target.exp_stars,
        attack_mult: 1.0,
        defense: striker.stats.defense.air,
        target_exp: striker.exp_stars,
        terrain_mult: 1.0,
        entrenchment: 0,
        defense_mult: 1.0,
        p_shift: 0.0,
    }
}

/// R-AIR-02: one aircraft's fire at another in an air battle (interception,
/// escort); both fire at once, like a ground combat.
pub fn air_fire_input(shooter: &Combatant<'_>, target: &Combatant<'_>) -> FireInput {
    FireInput {
        strength: shooter.strength,
        readiness_pct: shooter.readiness_pct,
        shot_fraction: 1.0,
        attack: shooter.stats.attack.air,
        shooter_exp: shooter.exp_stars,
        attack_mult: 1.0,
        defense: target.stats.defense.air,
        target_exp: target.exp_stars,
        terrain_mult: 1.0,
        entrenchment: 0,
        defense_mult: 1.0,
        p_shift: 0.0,
    }
}

/// R-AIR-02: a strike on aircraft parked at an airfield; they defend with
/// `parked_air_defense` and are hit with the striker's soft attack.
pub fn parked_strike_input(striker: &Combatant<'_>, parked_air_defense: f64) -> FireInput {
    FireInput {
        strength: striker.strength,
        readiness_pct: striker.readiness_pct,
        shot_fraction: 1.0,
        attack: striker.stats.attack.soft,
        shooter_exp: striker.exp_stars,
        attack_mult: 1.0,
        defense: parked_air_defense,
        target_exp: 0,
        terrain_mult: 1.0,
        entrenchment: 0,
        defense_mult: 1.0,
        p_shift: 0.0,
    }
}
