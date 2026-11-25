//! Axial hex coordinates, flat-top orientation (Red Blob Games conventions).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

/// Axial neighbor offsets, starting east-south-east and going counter-clockwise.
pub const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

impl Hex {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Third cube coordinate.
    pub const fn s(self) -> i32 {
        -self.q - self.r
    }

    pub fn neighbors(self) -> [Hex; 6] {
        DIRECTIONS.map(|(dq, dr)| Hex::new(self.q + dq, self.r + dr))
    }

    pub fn distance(self, other: Hex) -> u32 {
        let dq = (self.q - other.q).unsigned_abs();
        let dr = (self.r - other.r).unsigned_abs();
        let ds = (self.s() - other.s()).unsigned_abs();
        (dq + dr + ds) / 2
    }

    pub fn is_adjacent(self, other: Hex) -> bool {
        self.distance(other) == 1
    }
}

/// Side of a hex shared by two adjacent hexes, stored in canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HexEdge {
    a: Hex,
    b: Hex,
}

impl HexEdge {
    /// Returns `None` if the hexes are not adjacent.
    pub fn new(a: Hex, b: Hex) -> Option<Self> {
        if !a.is_adjacent(b) {
            return None;
        }
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        Some(Self { a, b })
    }

    pub fn hexes(self) -> (Hex, Hex) {
        (self.a, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbors_are_at_distance_one() {
        let h = Hex::new(2, -1);
        for n in h.neighbors() {
            assert_eq!(h.distance(n), 1);
        }
    }

    #[test]
    fn distance_matches_cube_metric() {
        assert_eq!(Hex::new(0, 0).distance(Hex::new(3, -1)), 3);
        assert_eq!(Hex::new(0, 0).distance(Hex::new(-2, 4)), 4);
        assert_eq!(Hex::new(1, 1).distance(Hex::new(1, 1)), 0);
    }

    #[test]
    fn edge_is_canonical() {
        let a = Hex::new(0, 0);
        let b = Hex::new(1, 0);
        assert_eq!(HexEdge::new(a, b), HexEdge::new(b, a));
        assert!(HexEdge::new(a, Hex::new(2, 0)).is_none());
    }
}
