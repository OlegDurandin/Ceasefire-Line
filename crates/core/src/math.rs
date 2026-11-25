//! Deterministic numeric helpers. Only basic IEEE-754 operations are used.

/// Rounds half up (towards +inf at exactly .5).
pub fn round_half_up(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

/// Clamps without panicking on inverted bounds (unlike `f64::clamp`).
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    x.max(lo).min(hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_half_up_rounds_halves_up() {
        assert_eq!(round_half_up(6.4), 6);
        assert_eq!(round_half_up(3.5), 4);
        assert_eq!(round_half_up(4.5), 5);
        assert_eq!(round_half_up(0.0), 0);
    }

    #[test]
    fn clamp_bounds() {
        assert_eq!(clamp(1.2, 0.05, 0.9), 0.9);
        assert_eq!(clamp(-1.0, 0.05, 0.9), 0.05);
        assert_eq!(clamp(0.3, 0.05, 0.9), 0.3);
    }
}
