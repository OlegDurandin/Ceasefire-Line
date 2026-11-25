//! Browser bridge. The client passes JSON texts in and gets JSON texts back,
//! so no extra serialization crates are needed. All rules stay in the core.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::hex::{Hex, HexEdge};
use ceasefire_core::map::BridgeState;
use ceasefire_core::scenario::{axial_to_offset, load_map, load_scenario, validate, ScenarioIssue};
use ceasefire_core::RuleError;
use serde::Serialize;
use wasm_bindgen::prelude::*;

fn js(e: RuleError) -> JsError {
    JsError::new(&e.to_string())
}

#[derive(Serialize)]
struct HexView {
    col: i32,
    row: i32,
    q: i32,
    r: i32,
    terrain: String,
    pass: bool,
    owner: Option<u8>,
}

#[derive(Serialize)]
struct EdgeView {
    a: [i32; 2],
    b: [i32; 2],
}

#[derive(Serialize)]
struct BridgeView {
    a: [i32; 2],
    b: [i32; 2],
    state: BridgeState,
}

#[derive(Serialize)]
struct UnitView {
    index: usize,
    class: String,
    side: u8,
    col: i32,
    row: i32,
    strength: u8,
    exp: u16,
    hq_rating: Option<u8>,
    nation: Option<String>,
}

#[derive(Serialize)]
struct SideView {
    id: u8,
    tech: ceasefire_core::tech::TechLevels,
}

#[derive(Serialize)]
struct ScenarioView {
    hex_km: Option<f64>,
    hexes: Vec<HexView>,
    roads: Vec<EdgeView>,
    canals: Vec<EdgeView>,
    bridges: Vec<BridgeView>,
    sides: Vec<SideView>,
    units: Vec<UnitView>,
    /// `validate()` output, `{kind: {...fields}}` as Debug text for now.
    issues: Vec<String>,
}

fn edge(e: HexEdge) -> EdgeView {
    let (a, b) = e.hexes();
    EdgeView {
        a: axial_to_offset(a),
        b: axial_to_offset(b),
    }
}

/// Static game data loaded once.
#[wasm_bindgen]
pub struct Core {
    data: Arc<GameData>,
}

#[wasm_bindgen]
impl Core {
    #[wasm_bindgen(constructor)]
    pub fn new(rules: &str, terrain: &str, units: &str, tech: &str) -> Result<Core, JsError> {
        let data = GameData::from_json(GameDataJson {
            rules,
            terrain,
            units,
            tech,
        })
        .map_err(js)?;
        Ok(Core {
            data: Arc::new(data),
        })
    }

    /// Everything the map view needs, as JSON. Hexes in offset coordinates.
    #[wasm_bindgen(js_name = scenarioView)]
    pub fn scenario_view(
        &self,
        scenario: &str,
        map: &str,
        events: Option<String>,
    ) -> Result<String, JsError> {
        let setup = load_scenario(scenario, map, events.as_deref()).map_err(js)?;
        let issues: Vec<String> = validate(&self.data, &setup)
            .iter()
            .map(issue_text)
            .collect();
        let (hex_map, control) = load_map(map).map_err(js)?;
        let control: BTreeMap<Hex, u8> = control.into_iter().map(|(h, s)| (h, s.0)).collect();
        let view = ScenarioView {
            hex_km: setup.hex_km,
            hexes: hex_map
                .hexes()
                .filter_map(|h| {
                    hex_map.cell(h).map(|c| {
                        let [col, row] = axial_to_offset(h);
                        HexView {
                            col,
                            row,
                            q: h.q,
                            r: h.r,
                            terrain: c.terrain.clone(),
                            pass: c.pass,
                            owner: control.get(&h).copied(),
                        }
                    })
                })
                .collect(),
            roads: hex_map.roads().map(edge).collect(),
            canals: hex_map.canals().map(edge).collect(),
            bridges: hex_map
                .bridges()
                .map(|(e, state)| {
                    let EdgeView { a, b } = edge(e);
                    BridgeView { a, b, state }
                })
                .collect(),
            sides: setup
                .sides
                .iter()
                .map(|s| SideView {
                    id: s.id.0,
                    tech: s.tech.clone(),
                })
                .collect(),
            units: setup
                .units
                .iter()
                .enumerate()
                .map(|(index, u)| {
                    let [col, row] = axial_to_offset(u.hex);
                    UnitView {
                        index,
                        class: u.class.clone(),
                        side: u.side.0,
                        col,
                        row,
                        strength: u.strength,
                        exp: u.exp,
                        hq_rating: u.hq_rating,
                        nation: u.nation.clone(),
                    }
                })
                .collect(),
            issues,
        };
        serde_json::to_string(&view).map_err(|e| JsError::new(&e.to_string()))
    }
}

fn issue_text(i: &ScenarioIssue) -> String {
    format!("{i:?}")
}
