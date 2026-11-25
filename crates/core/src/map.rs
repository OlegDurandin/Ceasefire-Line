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

/// R-MOVE-02: bridge state on a hex side (absent = no bridge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeState {
    Built,
    Destroyed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HexMap {
    cells: BTreeMap<Hex, HexCell>,
    roads: BTreeSet<HexEdge>,
    canals: BTreeSet<HexEdge>,
    bridges: BTreeMap<HexEdge, BridgeState>,
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

    /// R-MOVE-02: a built bridge across a canal edge (also a rebuilt one).
    pub fn add_bridge(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        self.bridges.insert(edge(a, b)?, BridgeState::Built);
        Ok(())
    }

    /// R-MOVE-02: the bridge is destroyed; its remains stay until rebuilt.
    pub fn destroy_bridge(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        self.bridges.insert(edge(a, b)?, BridgeState::Destroyed);
        Ok(())
    }

    pub fn bridge_state(&self, a: Hex, b: Hex) -> Option<BridgeState> {
        HexEdge::new(a, b).and_then(|e| self.bridges.get(&e).copied())
    }

    /// Hex sides with a road (for rendering and editing).
    pub fn roads(&self) -> impl Iterator<Item = HexEdge> + '_ {
        self.roads.iter().copied()
    }

    /// Hex sides with a canal or major river.
    pub fn canals(&self) -> impl Iterator<Item = HexEdge> + '_ {
        self.canals.iter().copied()
    }

    /// Every bridge on the map with its state (for rendering).
    pub fn bridges(&self) -> impl Iterator<Item = (HexEdge, BridgeState)> + '_ {
        self.bridges.iter().map(|(&e, &s)| (e, s))
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

    /// Only a built bridge carries traffic and supply.
    pub fn has_bridge(&self, a: Hex, b: Hex) -> bool {
        self.bridge_state(a, b) == Some(BridgeState::Built)
    }
}
