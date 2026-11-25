//! Seeded PRNG with a fixed algorithm: PCG32 (XSH-RR, 64-bit state).
//! Same seed and stream give the same sequence on native and WASM.

use serde::{Deserialize, Serialize};

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;
/// 2^-53, maps 53 random bits to [0, 1).
const F64_SCALE: f64 = 1.0 / 9_007_199_254_740_992.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    /// Same seeding procedure as the reference `pcg32_srandom_r(seed, stream)`.
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.step();
        rng.state = rng.state.wrapping_add(seed);
        rng.step();
        rng
    }

    fn step(&mut self) {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.step();
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, 1) with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        let hi = u64::from(self.next_u32()) << 21;
        let lo = u64::from(self.next_u32() >> 11);
        (hi | lo) as f64 * F64_SCALE
    }

    /// Bernoulli trial with success probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_pcg32_vector() {
        // pcg32-global-demo / pcg32_srandom_r(42, 54)
        let mut rng = Pcg32::new(42, 54);
        let expected = [
            0xa15c_02b7,
            0x7b47_f409,
            0xba1d_3330,
            0x83d2_f293,
            0xbfa4_784b,
            0xcbed_606e,
        ];
        for e in expected {
            assert_eq!(rng.next_u32(), e);
        }
    }

    #[test]
    fn f64_in_unit_interval() {
        let mut rng = Pcg32::new(1, 1);
        for _ in 0..10_000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
