//! R-COMBAT-02, R-COMBAT-03: ground combat. Forecast and resolution share
//! [`ground_exchange`], so the player, the AI and the dice see the same numbers.

use serde::{Deserialize, Serialize};

use crate::data::{CombatRules, UnitClass, UnitTag};
use crate::error::RuleError;
use crate::math::{clamp, round_half_up};
use crate::prob::{hits_distribution, LossForecast};
use crate::rng::Pcg32;
use crate::tech::UnitStats;

/// Shot parameters of one side against the other.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FireParams {
    /// Number of shots.
    pub n: u32,
    /// Effective attack.
    pub a: f64,
    /// Effective defense.
    pub d: f64,
    /// Hit probability per shot.
    pub p: f64,
}

/// Everything [`fire_params`] needs; callers fill in context multipliers.
/// Serializable so the UI can show the formula with the very inputs the
/// resolution uses (R-UI-04).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FireInput {
    pub strength: u8,
    pub readiness_pct: u8,
    /// 1.0 normally, 0.5 for SAM fire (R-AIR-03) and bombardment (R-COMBAT-04).
    pub shot_fraction: f64,
    pub attack: f64,
    pub shooter_exp: u8,
    /// Night (R-TURN-02) and similar attack multipliers.
    pub attack_mult: f64,
    pub defense: f64,
    pub target_exp: u8,
    pub terrain_mult: f64,
    pub entrenchment: u8,
    /// Canal crossing (R-MOVE-02) and similar defense multipliers.
    pub defense_mult: f64,
    /// Subtracted from p before clamping (EW, R-TECH-01).
    pub p_shift: f64,
}

/// R-COMBAT-02: `n = round_half_up(Strength × Readiness × fraction)`.
pub fn effective_shots(strength: u8, readiness_pct: u8, fraction: f64) -> u32 {
    let raw = f64::from(strength) * f64::from(readiness_pct) * fraction / 100.0;
    round_half_up(raw).max(0) as u32
}

/// R-COMBAT-02: n, A, D, p. An attack value of 0 means "does not fire" (p = 0).
pub fn fire_params(rules: &CombatRules, input: &FireInput) -> FireParams {
    let n = effective_shots(input.strength, input.readiness_pct, input.shot_fraction);
    let a = input.attack
        * (1.0 + rules.exp_attack_bonus * f64::from(input.shooter_exp))
        * input.attack_mult;
    let d = input.defense
        * input.terrain_mult
        * (1.0 + rules.entrench_defense_bonus * f64::from(input.entrenchment))
        * (1.0 + rules.exp_defense_bonus * f64::from(input.target_exp))
        * input.defense_mult;
    let p = if input.attack <= 0.0 {
        0.0
    } else {
        clamp(
            rules.p_base + rules.p_slope * (a - d) - input.p_shift,
            rules.p_min,
            rules.p_max,
        )
    };
    FireParams { n, a, d, p }
}

/// A unit as it enters combat.
#[derive(Debug, Clone)]
pub struct Combatant<'a> {
    pub class: &'a UnitClass,
    /// Values after technology (R-TECH-01).
    pub stats: UnitStats,
    pub strength: u8,
    pub readiness_pct: u8,
    /// Whole stars.
    pub exp_stars: u8,
    pub entrenchment: u8,
}

/// Circumstances of a ground attack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundAttack {
    /// Defense multiplier of the defender's hex.
    pub terrain_mult: f64,
    pub night: bool,
    pub across_canal: bool,
}

/// Fire of both sides in one ground combat.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    pub vs_defender: FireParams,
    pub vs_attacker: FireParams,
}

fn ground_defense(unit: &Combatant<'_>) -> Result<f64, RuleError> {
    unit.stats
        .defense
        .ground
        .ok_or_else(|| RuleError::NoGroundDefense(unit.class.id.clone()))
}

/// R-COMBAT-02: both sides fire simultaneously with strength at combat start.
/// Return fire ignores the attacker's terrain and entrenchment. Night
/// (R-TURN-02) applies to the attacker's fire only; canal (R-MOVE-02) to the
/// defender's defense.
pub fn ground_exchange(
    rules: &CombatRules,
    attacker: &Combatant<'_>,
    defender: &Combatant<'_>,
    ctx: &GroundAttack,
) -> Result<Exchange, RuleError> {
    let [to_defender, to_attacker] = ground_inputs(rules, attacker, defender, ctx)?;
    Ok(Exchange {
        vs_defender: fire_params(rules, &to_defender),
        vs_attacker: fire_params(rules, &to_attacker),
    })
}

/// Inputs of both fires in a ground combat: `[attacker → defender, defender →
/// attacker]`. [`ground_exchange`] and the UI explanation use this one function.
pub fn ground_inputs(
    rules: &CombatRules,
    attacker: &Combatant<'_>,
    defender: &Combatant<'_>,
    ctx: &GroundAttack,
) -> Result<[FireInput; 2], RuleError> {
    let night_mult = if ctx.night && !attacker.class.has_tag(UnitTag::NightCombat) {
        rules.night_attack_mult
    } else {
        1.0
    };
    let canal_mult = if ctx.across_canal {
        rules.canal_defense_mult
    } else {
        1.0
    };
    let vs_defender = FireInput {
        strength: attacker.strength,
        readiness_pct: attacker.readiness_pct,
        shot_fraction: 1.0,
        attack: attacker.stats.attack.against(defender.class.target_class),
        shooter_exp: attacker.exp_stars,
        attack_mult: night_mult,
        defense: ground_defense(defender)?,
        target_exp: defender.exp_stars,
        terrain_mult: ctx.terrain_mult,
        entrenchment: defender.entrenchment,
        defense_mult: canal_mult,
        p_shift: 0.0,
    };
    let vs_attacker = FireInput {
        strength: defender.strength,
        readiness_pct: defender.readiness_pct,
        shot_fraction: 1.0,
        attack: defender.stats.attack.against(attacker.class.target_class),
        shooter_exp: defender.exp_stars,
        attack_mult: 1.0,
        defense: ground_defense(attacker)?,
        target_exp: attacker.exp_stars,
        terrain_mult: 1.0,
        entrenchment: 0,
        defense_mult: 1.0,
        p_shift: 0.0,
    };
    Ok([vs_defender, vs_attacker])
}

/// R-COMBAT-03: forecast of a ground combat.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExchangeForecast {
    pub exchange: Exchange,
    pub defender_losses: LossForecast,
    pub attacker_losses: LossForecast,
}

/// Forecast of hits from one group of shots against a target of `cap` steps.
pub fn forecast_fire(rules: &CombatRules, params: &FireParams, cap: u8) -> LossForecast {
    let dist = hits_distribution(&[(params.n, params.p)], u32::from(cap));
    LossForecast::from_distribution(
        &dist,
        rules.forecast_low_percentile,
        rules.forecast_high_percentile,
    )
}

/// R-COMBAT-03: expected losses and percentile band for both sides.
pub fn forecast_ground(
    rules: &CombatRules,
    attacker: &Combatant<'_>,
    defender: &Combatant<'_>,
    ctx: &GroundAttack,
) -> Result<ExchangeForecast, RuleError> {
    let exchange = ground_exchange(rules, attacker, defender, ctx)?;
    Ok(ExchangeForecast {
        exchange,
        defender_losses: forecast_fire(rules, &exchange.vs_defender, defender.strength),
        attacker_losses: forecast_fire(rules, &exchange.vs_attacker, attacker.strength),
    })
}

/// Samples hits: `n` Bernoulli trials with probability `p`.
pub fn sample_hits(rng: &mut Pcg32, params: &FireParams) -> u32 {
    (0..params.n).filter(|_| rng.chance(params.p)).count() as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExchangeOutcome {
    pub exchange: Exchange,
    pub defender_losses: u8,
    pub attacker_losses: u8,
}

/// R-COMBAT-02: resolves a ground combat. Defender losses are drawn first.
pub fn resolve_ground(
    rules: &CombatRules,
    attacker: &Combatant<'_>,
    defender: &Combatant<'_>,
    ctx: &GroundAttack,
    rng: &mut Pcg32,
) -> Result<ExchangeOutcome, RuleError> {
    let exchange = ground_exchange(rules, attacker, defender, ctx)?;
    let defender_losses = capped(sample_hits(rng, &exchange.vs_defender), defender.strength);
    let attacker_losses = capped(sample_hits(rng, &exchange.vs_attacker), attacker.strength);
    Ok(ExchangeOutcome {
        exchange,
        defender_losses,
        attacker_losses,
    })
}

fn capped(hits: u32, strength: u8) -> u8 {
    u8::try_from(hits).unwrap_or(u8::MAX).min(strength)
}
