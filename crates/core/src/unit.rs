//! R-UNIT-01: unit readiness.

use crate::data::ReadinessRules;
use crate::math::round_half_up;

/// R-UNIT-01: readiness in whole percent.
///
/// `min(max, (base + per_supply·Supply) · Morale/100 · (1 + exp_bonus·Exp))`,
/// rounded half up to whole percent, then `modifier_pct` (HQ bonus, fatigue,
/// other temporary penalties, in percentage points) is added and the result
/// clamped to 0..=100.
pub fn readiness_pct(
    rules: &ReadinessRules,
    supply: u8,
    morale_pct: u8,
    exp_stars: u8,
    modifier_pct: i32,
) -> u8 {
    let base = (rules.base + rules.per_supply * f64::from(supply))
        * (f64::from(morale_pct) / 100.0)
        * (1.0 + rules.exp_bonus * f64::from(exp_stars));
    let pct = round_half_up(base.min(rules.max) * 100.0) + i64::from(modifier_pct);
    pct.clamp(0, 100) as u8
}

/// R-UNIT-01: fatigue penalty in percentage points (negative), until end of turn.
pub fn fatigue_pct(rules: &ReadinessRules, attacks_made: u32, attacks_received: u32) -> i32 {
    let made = attacks_made.saturating_mul(u32::from(rules.fatigue_per_attack_pct));
    let received = attacks_received.saturating_mul(u32::from(rules.fatigue_per_attacked_pct));
    -i32::try_from(made.saturating_add(received)).unwrap_or(i32::MAX)
}
