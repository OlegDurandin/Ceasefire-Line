//! Deterministic 2D gradient noise for `mapgen procgen` (R-UI-01): a hash of
//! the lattice point picks one of eight gradients, quintic fade, fBm and
//! ridged fBm. Only basic `f64` operations, so the same seed gives the same
//! numbers everywhere.

use serde::Deserialize;

/// One noise channel: feature size and octaves.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoiseConfig {
    /// Size of the largest features, km.
    pub scale_km: f64,
    pub octaves: u32,
    /// Frequency multiplier per octave.
    pub lacunarity: f64,
    /// Amplitude multiplier per octave.
    pub gain: f64,
}

/// SplitMix64 finaliser.
pub fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A seed for channel `k` of a map seed.
pub fn channel(seed: u64, k: u64) -> u64 {
    mix(seed ^ mix(k.wrapping_mul(0x632b_e59b_d9b4_e019)))
}

const D: f64 = std::f64::consts::FRAC_1_SQRT_2;
const GRADIENTS: [(f64, f64); 8] = [
    (1.0, 0.0),
    (D, D),
    (0.0, 1.0),
    (-D, D),
    (-1.0, 0.0),
    (-D, -D),
    (0.0, -1.0),
    (D, -D),
];

fn gradient(seed: u64, ix: i64, iy: i64) -> (f64, f64) {
    let h = mix(seed ^ mix((ix as u64) ^ mix(iy as u64).rotate_left(17)));
    GRADIENTS[(h >> 61) as usize]
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Gradient noise at (x, y) in lattice units, about −1..1.
pub fn gradient_noise(seed: u64, x: f64, y: f64) -> f64 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (ix, iy) = (x0 as i64, y0 as i64);
    let dot = |dx: i64, dy: i64| {
        let g = gradient(seed, ix + dx, iy + dy);
        g.0 * (fx - dx as f64) + g.1 * (fy - dy as f64)
    };
    let (u, v) = (fade(fx), fade(fy));
    let a = dot(0, 0) + (dot(1, 0) - dot(0, 0)) * u;
    let b = dot(0, 1) + (dot(1, 1) - dot(0, 1)) * u;
    // 2D gradient noise with unit gradients stays within ±√½.
    ((a + (b - a) * v) * std::f64::consts::SQRT_2).clamp(-1.0, 1.0)
}

/// Octave offsets so octaves do not share the lattice origin.
fn offset(seed: u64, o: u32) -> (f64, f64) {
    let h = mix(seed ^ u64::from(o).wrapping_mul(0xd6e8_feb8_6659_fd93));
    (
        (h & 0xffff) as f64 / 65536.0 * 1000.0,
        ((h >> 16) & 0xffff) as f64 / 65536.0 * 1000.0,
    )
}

/// fBm at a point given in km, normalised to 0..1.
pub fn fbm(seed: u64, c: &NoiseConfig, x_km: f64, y_km: f64) -> f64 {
    let (mut f, mut amp, mut sum, mut norm) = (1.0 / c.scale_km, 1.0, 0.0, 0.0);
    for o in 0..c.octaves.max(1) {
        let (ox, oy) = offset(seed, o);
        sum += amp * gradient_noise(channel(seed, u64::from(o)), x_km * f + ox, y_km * f + oy);
        norm += amp;
        amp *= c.gain;
        f *= c.lacunarity;
    }
    0.5 + 0.5 * sum / norm
}

/// Ridged fBm (sharp crests where the noise crosses zero), 0..1.
pub fn ridged(seed: u64, c: &NoiseConfig, x_km: f64, y_km: f64) -> f64 {
    let (mut f, mut amp, mut sum, mut norm) = (1.0 / c.scale_km, 1.0, 0.0, 0.0);
    for o in 0..c.octaves.max(1) {
        let (ox, oy) = offset(seed, o);
        let n = gradient_noise(channel(seed, u64::from(o)), x_km * f + ox, y_km * f + oy);
        let r = 1.0 - n.abs();
        sum += amp * r * r;
        norm += amp;
        amp *= c.gain;
        f *= c.lacunarity;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let c = NoiseConfig {
            scale_km: 20.0,
            octaves: 4,
            lacunarity: 2.0,
            gain: 0.5,
        };
        let mut seen_low = false;
        let mut seen_high = false;
        for i in 0..400 {
            let (x, y) = (f64::from(i) * 1.7, f64::from(i % 37) * 3.1);
            let a = fbm(7, &c, x, y);
            assert_eq!(a, fbm(7, &c, x, y));
            assert!((0.0..=1.0).contains(&a));
            let r = ridged(7, &c, x, y);
            assert!((0.0..=1.0).contains(&r));
            seen_low |= a < 0.4;
            seen_high |= a > 0.6;
        }
        assert!(seen_low && seen_high, "the noise varies");
        assert_ne!(fbm(7, &c, 10.0, 10.0), fbm(8, &c, 10.0, 10.0));
    }
}
