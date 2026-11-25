//! Loss distributions for forecasts (R-COMBAT-03). Exact, no sampling.

use serde::{Deserialize, Serialize};

/// Tolerance for cumulative sums when looking up percentiles.
const CDF_EPSILON: f64 = 1e-12;

/// Distribution of total hits from groups of independent shots `(n, p)`,
/// with losses capped at `cap` (the target's strength).
pub fn hits_distribution(shots: &[(u32, f64)], cap: u32) -> Vec<f64> {
    let mut dist = vec![1.0];
    for &(n, p) in shots {
        for _ in 0..n {
            let mut next = vec![0.0; dist.len() + 1];
            for (k, &w) in dist.iter().enumerate() {
                next[k] += w * (1.0 - p);
                next[k + 1] += w * p;
            }
            dist = next;
        }
    }
    let cap = cap as usize;
    if dist.len() > cap + 1 {
        let tail: f64 = dist[cap + 1..].iter().sum();
        dist.truncate(cap + 1);
        dist[cap] += tail;
    }
    dist
}

/// R-COMBAT-03: expected losses and the low/high percentile band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LossForecast {
    pub expected: f64,
    pub low: u32,
    pub high: u32,
}

impl LossForecast {
    pub fn from_distribution(dist: &[f64], low_q: f64, high_q: f64) -> Self {
        let expected = dist.iter().enumerate().map(|(k, &w)| k as f64 * w).sum();
        Self {
            expected,
            low: percentile(dist, low_q),
            high: percentile(dist, high_q),
        }
    }
}

/// Point of a distribution for a quantile `q`: `q ≤ 0` is the smallest and
/// `q ≥ 1` the largest outcome with non-zero probability; otherwise the
/// percentile (smallest k with P(X ≤ k) ≥ q).
pub fn quantile(dist: &[f64], q: f64) -> u32 {
    let support = || {
        dist.iter()
            .enumerate()
            .filter(|(_, &w)| w > 0.0)
            .map(|(k, _)| k as u32)
    };
    if q <= 0.0 {
        support().next().unwrap_or(0)
    } else if q >= 1.0 {
        support().next_back().unwrap_or(0)
    } else {
        percentile(dist, q)
    }
}

/// Smallest k with P(X <= k) >= q.
pub fn percentile(dist: &[f64], q: f64) -> u32 {
    let mut cumulative = 0.0;
    for (k, &w) in dist.iter().enumerate() {
        cumulative += w;
        if cumulative + CDF_EPSILON >= q {
            return k as u32;
        }
    }
    dist.len().saturating_sub(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binomial_mean_is_np() {
        let dist = hits_distribution(&[(9, 0.3)], 10);
        let f = LossForecast::from_distribution(&dist, 0.1, 0.9);
        assert!((f.expected - 9.0 * 0.3).abs() < 1e-9);
        assert!((dist.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn cap_folds_tail() {
        let dist = hits_distribution(&[(5, 0.9)], 2);
        assert_eq!(dist.len(), 3);
        assert!((dist.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn quantile_extremes_are_the_support_bounds() {
        let dist = hits_distribution(&[(9, 0.05)], 10);
        assert_eq!(quantile(&dist, 0.0), 0);
        assert_eq!(quantile(&dist, 1.0), 9, "every shot hits, however unlikely");
        assert_eq!(quantile(&dist, 0.5), percentile(&dist, 0.5));
        let capped = hits_distribution(&[(9, 0.5)], 4);
        assert_eq!(quantile(&capped, 1.0), 4, "capped at the target's strength");
    }

    #[test]
    fn percentiles_of_certain_outcome() {
        let dist = hits_distribution(&[(3, 1.0)], 10);
        let f = LossForecast::from_distribution(&dist, 0.1, 0.9);
        assert_eq!((f.low, f.high), (3, 3));
    }
}
