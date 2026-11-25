//! Hex map: terrain per hex, roads / canals / bridges on hex edges.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::RuleError;
use crate::hex::{Hex, HexEdge};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexCell {
    pub terrain: String,
    /// R-MOVE-01: mountain hex that can be entered (Mitla, Giddi).
    pub pass: bool,
}

impl HexCell {
    pub fn new(terrain: &str) -> Self {
        Self {
            terrain: terrain.to_owned(),
            pass: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HexMap {
    cells: BTreeMap<Hex, HexCell>,
    roads: BTreeSet<HexEdge>,
    canals: BTreeSet<HexEdge>,
    bridges: BTreeSet<HexEdge>,
}

fn edge(a: Hex, b: Hex) -> Result<HexEdge, RuleError> {
    HexEdge::new(a, b).ok_or(RuleError::NotAdjacent(a, b))
}

impl HexMap {
    pub fn insert(&mut self, hex: Hex, cell: HexCell) {
        self.cells.insert(hex, cell);
    }

    pub fn cell(&self, hex: Hex) -> Option<&HexCell> {
        self.cells.get(&hex)
    }

    pub fn hexes(&self) -> impl Iterator<Item = Hex> + '_ {
        self.cells.keys().copied()
    }

    pub fn add_road(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        self.roads.insert(edge(a, b)?);
        Ok(())
    }

    pub fn add_canal(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        self.canals.insert(edge(a, b)?);
        Ok(())
    }

    /// R-MOVE-02: bridge across a canal edge.
    pub fn add_bridge(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        self.bridges.insert(edge(a, b)?);
        Ok(())
    }

    /// R-MOVE-02: a destroyed bridge disappears; engineers rebuild it.
    pub fn remove_bridge(&mut self, a: Hex, b: Hex) {
        if let Some(e) = HexEdge::new(a, b) {
            self.bridges.remove(&e);
        }
    }

    fn has(set: &BTreeSet<HexEdge>, a: Hex, b: Hex) -> bool {
        HexEdge::new(a, b).is_some_and(|e| set.contains(&e))
    }

    pub fn has_road(&self, a: Hex, b: Hex) -> bool {
        Self::has(&self.roads, a, b)
    }

    pub fn has_canal(&self, a: Hex, b: Hex) -> bool {
        Self::has(&self.canals, a, b)
    }

    pub fn has_bridge(&self, a: Hex, b: Hex) -> bool {
        Self::has(&self.bridges, a, b)
    }
}
