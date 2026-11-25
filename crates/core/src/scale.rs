//! R-SCALE-01: scenario scale. Ranges are stored in km and converted to hexes.

use crate::math::round_half_up;

/// R-SCALE-01: km → hexes, rounded half up, at least 1.
pub fn range_in_hexes(km: f64, hex_km: f64) -> u32 {
    let hexes = round_half_up(km / hex_km);
    hexes.max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_scale_01_ranges_table() {
        // artillery, mobile SAM, static SAM, interception, heli transfer, aviation
        for (km, at10, at5) in [
            (20.0, 2, 4),
            (20.0, 2, 4),
            (30.0, 3, 6),
            (100.0, 10, 20),
            (80.0, 8, 16),
            (250.0, 25, 50),
        ] {
            assert_eq!(range_in_hexes(km, 10.0), at10);
            assert_eq!(range_in_hexes(km, 5.0), at5);
        }
    }

    #[test]
    fn r_scale_01_minimum_one_hex() {
        assert_eq!(range_in_hexes(2.0, 10.0), 1);
    }
}
