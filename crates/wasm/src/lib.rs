//! Browser bridge. The client passes JSON texts in and gets JSON texts back,
//! so no extra serialization crates are needed. All rules stay in the core.

use std::collections::BTreeMap;
use std::sync::Arc;

use ceasefire_core::data::{GameData, GameDataJson};
use ceasefire_core::game::{Command, GameState, Outcome, UnitId};
use ceasefire_core::hex::{Hex, HexEdge};
use ceasefire_core::map::BridgeState;
use ceasefire_core::scenario::offset_to_axial;
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
    /// `validate()` output: `{kind, ...fields}`, hexes in offset coordinates.
    issues: Vec<ScenarioIssue>,
    /// R-MOVE-06: neutral closed territory, offset hexes.
    neutral: Vec<[i32; 2]>,
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
        let issues = validate(&self.data, &setup);
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
            neutral: setup
                .neutral
                .iter()
                .flat_map(|z| z.hexes.iter().map(|&h| axial_to_offset(h)))
                .collect(),
        };
        serde_json::to_string(&view).map_err(|e| JsError::new(&e.to_string()))
    }
}

fn to_json<T: Serialize>(v: &T) -> Result<String, JsError> {
    serde_json::to_string(v).map_err(|e| JsError::new(&e.to_string()))
}

#[derive(Serialize)]
struct SideStateView {
    id: u8,
    mpp: u32,
    national_morale_pct: u8,
    tech: ceasefire_core::tech::TechLevels,
}

#[derive(Serialize)]
struct LiveUnitView {
    id: u32,
    class: String,
    side: u8,
    col: i32,
    row: i32,
    strength: u8,
    readiness_pct: u8,
    morale_pct: u8,
    exp: u16,
    entrenchment: u8,
    supply: u8,
    ap_halves: u32,
    strikes_left: u32,
    can_move: bool,
    acted: bool,
    hq_rating: Option<u8>,
    nation: Option<String>,
    formation: Option<String>,
    /// R-HQ-01: HQ the unit is attached to (set at the start of its phase).
    attached_hq: Option<u32>,
    /// R-HQ-01: manual pin, applied from the next own phase.
    pinned_hq: Option<u32>,
    /// R-UNIT-01 explanation: phase-start readiness, its inputs, fatigue.
    base_readiness_pct: u8,
    readiness_basis: ceasefire_core::game::ReadinessBasis,
    attacks_made: u32,
    attacks_received: u32,
    crossing_penalty_pct: u8,
    terrain: String,
    /// R-SUP-02 now in the unit's hex for its side; 0 = encircled (R-NM-01).
    hex_supply: u8,
}

#[derive(Serialize)]
struct GameView<'a> {
    turn: u32,
    active_side: u8,
    night: bool,
    last_turn: Option<u32>,
    sides: Vec<SideStateView>,
    units: Vec<LiveUnitView>,
    control: Vec<([i32; 2], u8)>,
    pending_advance: Option<(u32, [i32; 2])>,
    outcome: Option<&'a Outcome>,
    /// R-MOVE-02: bridge states change in play.
    bridges: Vec<BridgeView>,
    /// R-MOVE-06: neutral closed territory, offset hexes.
    neutral: Vec<[i32; 2]>,
    /// R-NM-01: objectives and who holds them.
    objectives: Vec<ObjectiveView>,
}

#[derive(Serialize)]
struct ObjectiveView {
    hex: [i32; 2],
    vp: i32,
    nm_value: u8,
    owner: Option<u8>,
}

#[derive(Serialize)]
struct Purchasable {
    class: String,
    cost: u32,
    /// Still allowed above the schedule (R-ECO-03).
    left: u32,
    /// Why it cannot be bought now, if so.
    refused: Option<String>,
}

#[derive(Serialize)]
struct BoostView {
    id: String,
    branch: ceasefire_core::tech::TechBranch,
    cost: u32,
    turns: u32,
    /// Turn the level-up comes, once bought.
    ready_turn: Option<u32>,
    bought: bool,
    refused: Option<String>,
}

#[derive(Serialize)]
struct EconomyView {
    side: u8,
    mpp: u32,
    base_income: u32,
    purchasable: Vec<Purchasable>,
    arrivals: Vec<(String, u32)>,
    boosts: Vec<BoostView>,
}

#[derive(Serialize)]
struct BridgeTarget {
    kind: &'static str,
    a: [i32; 2],
    b: [i32; 2],
}

#[derive(Serialize)]
struct Reach {
    col: i32,
    row: i32,
    cost_halves: u32,
}

#[derive(Serialize)]
struct Target {
    target: u32,
    kind: &'static str,
}

/// A running game (hotseat): the client calls commands and re-reads `state()`.
#[wasm_bindgen]
pub struct Game {
    state: GameState,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new(
        core: &Core,
        scenario: &str,
        map: &str,
        events: Option<String>,
    ) -> Result<Game, JsError> {
        let setup = load_scenario(scenario, map, events.as_deref()).map_err(js)?;
        let (state, _) = GameState::new(Arc::clone(&core.data), setup).map_err(js)?;
        Ok(Game { state })
    }

    /// Turn, sides, units and outcome as JSON (offset coordinates).
    pub fn state(&self) -> Result<String, JsError> {
        let s = &self.state;
        let mut supply = BTreeMap::new();
        for side in [
            ceasefire_core::side::SideId(0),
            ceasefire_core::side::SideId(1),
        ] {
            if s.side(side).is_some() {
                supply.insert(side, s.hex_supply(side).map_err(js)?);
            }
        }
        let rules = &s.data().rules.readiness;
        let sides = [
            ceasefire_core::side::SideId(0),
            ceasefire_core::side::SideId(1),
        ];
        let view = GameView {
            turn: s.turn(),
            active_side: s.active_side().0,
            night: s.is_night(),
            last_turn: s.last_turn(),
            sides: sides
                .iter()
                .filter_map(|&id| s.side(id))
                .map(|side| SideStateView {
                    id: side.id.0,
                    mpp: side.mpp,
                    national_morale_pct: side.national_morale_pct,
                    tech: side.tech.clone(),
                })
                .collect(),
            units: s
                .units()
                .map(|u| {
                    let [col, row] = axial_to_offset(u.hex);
                    LiveUnitView {
                        id: u.id.0,
                        class: u.class.clone(),
                        side: u.side.0,
                        col,
                        row,
                        strength: u.strength,
                        readiness_pct: u.readiness_pct(rules),
                        morale_pct: u.morale_pct,
                        exp: u.exp,
                        entrenchment: u.entrenchment,
                        supply: u.supply,
                        ap_halves: u.ap_halves,
                        strikes_left: u.strikes_left,
                        can_move: u.can_move,
                        acted: u.acted,
                        hq_rating: u.hq_rating,
                        nation: u.nation.clone(),
                        formation: u.formation.clone(),
                        attached_hq: u.attached_hq.map(|h| h.0),
                        pinned_hq: u.pinned_hq.map(|h| h.0),
                        base_readiness_pct: u.base_readiness_pct,
                        readiness_basis: u.readiness_basis,
                        attacks_made: u.attacks_made,
                        attacks_received: u.attacks_received,
                        crossing_penalty_pct: u.crossing_penalty_pct,
                        terrain: s
                            .map()
                            .cell(u.hex)
                            .map(|c| c.terrain.clone())
                            .unwrap_or_default(),
                        hex_supply: supply.get(&u.side).map_or(0, |m| m.get(u.hex)),
                    }
                })
                .collect(),
            control: s
                .map()
                .hexes()
                .filter_map(|h| s.controller(h).map(|side| (axial_to_offset(h), side.0)))
                .collect(),
            pending_advance: s
                .pending_advance()
                .map(|p| (p.unit.0, axial_to_offset(p.hex))),
            outcome: s.outcome(),
            bridges: s
                .map()
                .bridges()
                .map(|(e, state)| {
                    let EdgeView { a, b } = edge(e);
                    BridgeView { a, b, state }
                })
                .collect(),
            neutral: s.map().closed().map(axial_to_offset).collect(),
            objectives: s
                .objectives()
                .iter()
                .map(|o| ObjectiveView {
                    hex: axial_to_offset(o.hex),
                    vp: o.vp,
                    nm_value: o.nm_value,
                    owner: s.controller(o.hex).map(|c| c.0),
                })
                .collect(),
        };
        to_json(&view)
    }

    /// Hexes the unit can end a move in (R-MOVE-01..03).
    pub fn reachable(&self, unit: u32) -> Result<String, JsError> {
        let reach: Vec<Reach> = self
            .state
            .reachable(UnitId(unit))
            .map_err(js)?
            .into_iter()
            .map(|(h, cost_halves)| {
                let [col, row] = axial_to_offset(h);
                Reach {
                    col,
                    row,
                    cost_halves,
                }
            })
            .collect();
        to_json(&reach)
    }

    /// Enemy units the unit may attack (adjacent) or bombard (in range) now.
    pub fn targets(&self, unit: u32) -> Result<String, JsError> {
        let s = &self.state;
        let id = UnitId(unit);
        let side = s.unit(id).map(|u| u.side);
        let mut out = Vec::new();
        for enemy in s.units().filter(|e| Some(e.side) != side) {
            if s.forecast_attack(id, enemy.id).is_ok() {
                out.push(Target {
                    target: enemy.id.0,
                    kind: "attack",
                });
            } else if s.forecast_bombard(id, enemy.id).is_ok() {
                out.push(Target {
                    target: enemy.id.0,
                    kind: "bombard",
                });
            }
        }
        to_json(&out)
    }

    /// R-COMBAT-03: forecast with the same code as the resolution.
    pub fn forecast(&self, unit: u32, target: u32) -> Result<String, JsError> {
        let (u, t) = (UnitId(unit), UnitId(target));
        match self.state.forecast_attack(u, t) {
            Ok(f) => to_json(&serde_json::json!({ "kind": "attack", "forecast": f })),
            Err(_) => {
                let f = self.state.forecast_bombard(u, t).map_err(js)?;
                to_json(&serde_json::json!({ "kind": "bombard", "defender_losses": f }))
            }
        }
    }

    /// R-UI-02 supply layer: `[col, row, value]` for hexes with supply > 0.
    #[wasm_bindgen(js_name = supplyMap)]
    pub fn supply_map(&self, side: u8) -> Result<String, JsError> {
        let m = self
            .state
            .hex_supply(ceasefire_core::side::SideId(side))
            .map_err(js)?;
        let cells: Vec<[i32; 3]> = m
            .iter()
            .map(|(h, v)| {
                let [c, r] = axial_to_offset(h);
                [c, r, i32::from(v)]
            })
            .collect();
        to_json(&cells)
    }

    /// R-UI-02 ZOC layer: hexes in the ZOC of `side`'s enemies.
    pub fn zoc(&self, side: u8) -> Result<String, JsError> {
        let hexes: Vec<[i32; 2]> = self
            .state
            .zoc_against(ceasefire_core::side::SideId(side))
            .into_iter()
            .map(axial_to_offset)
            .collect();
        to_json(&hexes)
    }

    /// R-UI-04: the formula inputs of an attack (`[to defender, to attacker]`)
    /// or a bombardment, from the same code as the resolution.
    pub fn explain(&self, unit: u32, target: u32) -> Result<String, JsError> {
        let (u, t) = (UnitId(unit), UnitId(target));
        match self.state.explain_attack(u, t) {
            Ok(inputs) => to_json(&serde_json::json!({ "kind": "attack", "inputs": inputs })),
            Err(_) => {
                let input = self.state.explain_bombard(u, t).map_err(js)?;
                to_json(&serde_json::json!({ "kind": "bombard", "inputs": [input] }))
            }
        }
    }

    /// Moves along the core's cheapest path to [col, row].
    #[wasm_bindgen(js_name = moveTo)]
    pub fn move_to(&mut self, unit: u32, col: i32, row: i32) -> Result<String, JsError> {
        let to = offset_to_axial([col, row]);
        let path = self
            .state
            .path_to(UnitId(unit), to)
            .map_err(js)?
            .ok_or_else(|| JsError::new("hex not reachable"))?;
        self.run(Command::Move {
            unit: UnitId(unit),
            path,
        })
    }

    pub fn attack(&mut self, unit: u32, target: u32) -> Result<String, JsError> {
        self.run(Command::Attack {
            unit: UnitId(unit),
            target: UnitId(target),
        })
    }

    pub fn bombard(&mut self, unit: u32, target: u32) -> Result<String, JsError> {
        self.run(Command::Bombard {
            unit: UnitId(unit),
            target: UnitId(target),
        })
    }

    pub fn advance(&mut self, unit: u32) -> Result<String, JsError> {
        self.run(Command::Advance { unit: UnitId(unit) })
    }

    #[wasm_bindgen(js_name = endPhase)]
    pub fn end_phase(&mut self) -> Result<String, JsError> {
        self.run(Command::EndPhase)
    }

    /// Any command as JSON (`Command`, hexes axial `{q, r}`); returns the events.
    pub fn apply(&mut self, command: &str) -> Result<String, JsError> {
        let c: Command = serde_json::from_str(command).map_err(|e| JsError::new(&e.to_string()))?;
        self.run(c)
    }

    /// Dry run on a copy: `undefined` if the command would be accepted,
    /// otherwise the rule error. The UI enables actions with this, so it
    /// never disagrees with the rules.
    pub fn check(&self, command: &str) -> Result<Option<String>, JsError> {
        let c: Command = serde_json::from_str(command).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(self.refusal(c))
    }

    fn refusal(&self, c: Command) -> Option<String> {
        self.state.clone().apply(c).err().map(|e| e.to_string())
    }

    /// R-ECO-02: MPP for `steps` steps of reinforcement.
    #[wasm_bindgen(js_name = reinforceCost)]
    pub fn reinforce_cost(&self, unit: u32, steps: u8, elite: bool) -> Result<u32, JsError> {
        self.state
            .reinforce_cost(UnitId(unit), steps, elite)
            .map_err(js)
    }

    /// R-MOVE-05: hexes the unit can be transferred to now, with the cost.
    #[wasm_bindgen(js_name = transferTargets)]
    pub fn transfer_targets(&self, unit: u32) -> Result<String, JsError> {
        let s = &self.state;
        let id = UnitId(unit);
        let Ok(cost) = s.transfer_cost(id) else {
            return to_json(&serde_json::json!({ "cost": null, "hexes": [] }));
        };
        let hexes: Vec<[i32; 2]> = s
            .map()
            .hexes()
            .filter(|&h| s.unit_at(h).is_none() && s.transfer_route(id, h).is_ok())
            .map(axial_to_offset)
            .collect();
        to_json(&serde_json::json!({ "cost": cost, "hexes": hexes }))
    }

    /// R-ECO-02, R-ECO-03, R-TECH-02 for the active side: what can be bought.
    pub fn economy(&self) -> Result<String, JsError> {
        let s = &self.state;
        let side_id = s.active_side();
        let side = s
            .side(side_id)
            .ok_or_else(|| JsError::new("no active side"))?;
        let purchasable = side
            .purchase_limits
            .iter()
            .map(|(class, limit)| {
                let cost = s.data().units.get(class).map(|c| c.cost_mpp).unwrap_or(0);
                Purchasable {
                    class: class.clone(),
                    cost,
                    left: limit.saturating_sub(side.purchased.get(class).copied().unwrap_or(0)),
                    refused: self.refusal(Command::Purchase {
                        class: class.clone(),
                    }),
                }
            })
            .collect();
        let arrivals = s
            .arrivals()
            .iter()
            .filter(|a| a.side == side_id)
            .map(|a| (a.class.clone(), a.turn))
            .collect();
        let boosts = s
            .tech_boosts()
            .iter()
            .filter(|b| b.side == side_id)
            .map(|b| {
                let pending = s.pending_boosts().iter().find(|p| p.id == b.id);
                let refused = self.refusal(Command::BuyTechBoost { id: b.id.clone() });
                BoostView {
                    id: b.id.clone(),
                    branch: b.branch,
                    cost: b.cost,
                    turns: b.turns,
                    ready_turn: pending.map(|p| p.ready_turn),
                    bought: s.boost_bought(&b.id),
                    refused,
                }
            })
            .collect();
        to_json(&EconomyView {
            side: side_id.0,
            mpp: side.mpp,
            base_income: side.base_income,
            purchasable,
            arrivals,
            boosts,
        })
    }

    /// R-MOVE-02: bridges the unit can fire at, and canal sides it can bridge.
    #[wasm_bindgen(js_name = bridgeTargets)]
    pub fn bridge_targets(&self, unit: u32) -> Result<String, JsError> {
        let s = &self.state;
        let id = UnitId(unit);
        let Some(u) = s.unit(id) else {
            return to_json(&Vec::<BridgeTarget>::new());
        };
        let mut out = Vec::new();
        for (e, _) in s.map().bridges() {
            let (a, b) = e.hexes();
            if self
                .refusal(Command::BombardBridge { unit: id, a, b })
                .is_none()
            {
                out.push(BridgeTarget {
                    kind: "bombard",
                    a: axial_to_offset(a),
                    b: axial_to_offset(b),
                });
            }
        }
        for toward in u.hex.neighbors() {
            if self
                .refusal(Command::BuildBridge { unit: id, toward })
                .is_none()
            {
                out.push(BridgeTarget {
                    kind: "build",
                    a: axial_to_offset(u.hex),
                    b: axial_to_offset(toward),
                });
            }
        }
        to_json(&out)
    }

    fn run(&mut self, command: Command) -> Result<String, JsError> {
        let events = self.state.apply(command).map_err(js)?;
        to_json(&events)
    }
}
