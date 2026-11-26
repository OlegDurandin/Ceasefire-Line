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
    /// R-MOVE-02: a ferry carries supply only (R-SUP-02), no movement or ZOC.
    Ferry,
    FerryDestroyed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HexMap {
    cells: BTreeMap<Hex, HexCell>,
    roads: BTreeSet<HexEdge>,
    /// R-MOVE-01: dirt roads (tracks).
    tracks: BTreeSet<HexEdge>,
    canals: BTreeSet<HexEdge>,
    bridges: BTreeMap<HexEdge, BridgeState>,
    /// R-MOVE-06: neutral closed territory of the scenario (no ground units,
    /// no supply); set by `GameState::new` from `ScenarioSetup::neutral`.
    closed: BTreeSet<Hex>,
    /// R-AIR-01: airfields (aircraft may be based there, as in a city).
    airfields: BTreeSet<Hex>,
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

    pub fn add_track(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        self.tracks.insert(edge(a, b)?);
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

    /// R-MOVE-02: a ferry across a canal edge (supply only).
    pub fn add_ferry(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        self.bridges.insert(edge(a, b)?, BridgeState::Ferry);
        Ok(())
    }

    /// R-MOVE-02: the bridge (or ferry) is destroyed; its remains stay until rebuilt.
    pub fn destroy_bridge(&mut self, a: Hex, b: Hex) -> Result<(), RuleError> {
        let e = edge(a, b)?;
        let state = if self.bridges.get(&e) == Some(&BridgeState::Ferry) {
            BridgeState::FerryDestroyed
        } else {
            BridgeState::Destroyed
        };
        self.bridges.insert(e, state);
        Ok(())
    }

    /// R-MOVE-06: closes a hex of neutral territory.
    pub fn close(&mut self, h: Hex) {
        self.closed.insert(h);
    }

    pub fn is_closed(&self, h: Hex) -> bool {
        self.closed.contains(&h)
    }

    pub fn closed(&self) -> impl Iterator<Item = Hex> + '_ {
        self.closed.iter().copied()
    }

    pub fn bridge_state(&self, a: Hex, b: Hex) -> Option<BridgeState> {
        HexEdge::new(a, b).and_then(|e| self.bridges.get(&e).copied())
    }

    /// Hex sides with a road (for rendering and editing).
    pub fn roads(&self) -> impl Iterator<Item = HexEdge> + '_ {
        self.roads.iter().copied()
    }

    /// Hex sides with a dirt road (track).
    pub fn tracks(&self) -> impl Iterator<Item = HexEdge> + '_ {
        self.tracks.iter().copied()
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

    pub fn has_track(&self, a: Hex, b: Hex) -> bool {
        Self::has(&self.tracks, a, b)
    }

    pub fn has_canal(&self, a: Hex, b: Hex) -> bool {
        Self::has(&self.canals, a, b)
    }

    /// Only a built bridge carries traffic and supply.
    pub fn has_bridge(&self, a: Hex, b: Hex) -> bool {
        self.bridge_state(a, b) == Some(BridgeState::Built)
    }

    /// R-MOVE-02: a working ferry (supply only).
    pub fn has_ferry(&self, a: Hex, b: Hex) -> bool {
        self.bridge_state(a, b) == Some(BridgeState::Ferry)
    }

    /// A built bridge or a working ferry: what strikes can destroy.
    pub fn has_crossing(&self, a: Hex, b: Hex) -> bool {
        self.has_bridge(a, b) || self.has_ferry(a, b)
    }

    /// R-AIR-01.
    pub fn add_airfield(&mut self, hex: Hex) {
        self.airfields.insert(hex);
    }

    pub fn is_airfield(&self, hex: Hex) -> bool {
        self.airfields.contains(&hex)
    }

    pub fn airfields(&self) -> impl Iterator<Item = Hex> + '_ {
        self.airfields.iter().copied()
    }
}
