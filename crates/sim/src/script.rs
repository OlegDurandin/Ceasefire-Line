//! Scripted orders for `sim ai-vs-ai --script` (rebalancing, Sinai-67 tempo
//! diagnosis): on the listed turns one side's formations go for given hexes
//! the way the history did; everything else (other formations, aircraft,
//! economy, leftovers) is played by the side's AI. Sim only: the scripted
//! player is a measuring tool, not an opponent.
//!
//! For a scripted unit, in order:
//! 1. take a vacated hex when the core offers it (R-COMBAT-06);
//! 2. bombard, then attack the enemy in the target hex, else the best one
//!    next to it — whatever the odds, as ordered (only if the forecast
//!    expects any enemy loss);
//! 3. step into the target hex if it is empty;
//! 4. move as close to the target as the unit can.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ceasefire_core::game::{Command, GameState, Unit};
use ceasefire_core::hex::Hex;
use ceasefire_core::scenario::offset_to_axial;
use ceasefire_core::side::SideId;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderFile {
    formation: String,
    /// Offset `[c, r]`, as in the scenario.
    target: [i32; 2],
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhaseFile {
    turn: u32,
    orders: Vec<OrderFile>,
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptFile {
    side: u8,
    phases: Vec<PhaseFile>,
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
}

/// One formation ordered to one hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub formation: String,
    pub target: Hex,
}

#[derive(Debug, Clone)]
pub struct Script {
    pub side: SideId,
    /// Turn → orders of that turn's phase of `side`.
    pub turns: BTreeMap<u32, Vec<Order>>,
}

impl Script {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let f: ScriptFile =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let turns = f
            .phases
            .into_iter()
            .map(|p| {
                let orders = p
                    .orders
                    .into_iter()
                    .map(|o| Order {
                        formation: o.formation,
                        target: offset_to_axial(o.target),
                    })
                    .collect();
                (p.turn, orders)
            })
            .collect();
        Ok(Self {
            side: SideId(f.side),
            turns,
        })
    }

    /// Orders for the active phase, if it is scripted.
    pub fn orders(&self, s: &GameState) -> Option<&[Order]> {
        if s.active_side() != self.side {
            return None;
        }
        self.turns.get(&s.turn()).map(Vec::as_slice)
    }
}

/// The scripted command for the active phase, or `None` when no scripted
/// unit has anything left to do (the AI plays on).
pub fn next(s: &GameState, orders: &[Order]) -> Option<Command> {
    let side = s.active_side();
    if let Some(p) = s.pending_advance() {
        if s.unit(p.unit).is_some_and(|u| u.side == side) {
            return Some(Command::Advance { unit: p.unit });
        }
    }
    for o in orders {
        let mut units: Vec<&Unit> = s
            .units()
            .filter(|u| {
                u.side == side
                    && u.hq_rating.is_none()
                    && u.formation.as_deref() == Some(o.formation.as_str())
            })
            .collect();
        units.sort_by_key(|u| u.id);
        // Fire first (bombard / attack), then movement, so that the
        // whole formation strikes the defender before anyone steps in.
        for u in &units {
            if let Some(c) = strike(s, u, o.target) {
                return Some(c);
            }
        }
        for u in &units {
            if let Some(c) = approach(s, u, o.target) {
                return Some(c);
            }
        }
    }
    None
}

/// Enemy units in the target hex first, then those next to it, by id.
fn targets(s: &GameState, side: SideId, target: Hex) -> Vec<&Unit> {
    let mut v: Vec<&Unit> = s
        .units()
        .filter(|e| e.side != side && e.hex.distance(target) <= 1)
        .collect();
    v.sort_by_key(|e| (e.hex.distance(target), e.id));
    v
}

fn strike(s: &GameState, u: &Unit, target: Hex) -> Option<Command> {
    if u.strikes_left == 0 {
        return None;
    }
    for e in targets(s, u.side, target) {
        if let Ok(f) = s.forecast_bombard(u.id, e.id) {
            if f.expected > 0.0 {
                return Some(Command::Bombard {
                    unit: u.id,
                    target: e.id,
                });
            }
        }
        if let Ok(f) = s.forecast_attack(u.id, e.id) {
            if f.defender_losses.expected > 0.0 {
                return Some(Command::Attack {
                    unit: u.id,
                    target: e.id,
                });
            }
        }
    }
    None
}

fn approach(s: &GameState, u: &Unit, target: Hex) -> Option<Command> {
    if u.moved || !u.can_move || u.ap_halves == 0 || u.hex == target {
        return None;
    }
    // Artillery keeps out of contact: it stops two hexes short.
    let stop = if u.class == "artillery" { 2 } else { 0 };
    let here = u.hex.distance(target);
    if here <= stop {
        return None;
    }
    let reach = s.reachable(u.id).ok()?;
    let (d, _, to) = reach
        .iter()
        .filter(|(h, _)| s.unit_at(**h).is_none() && h.distance(target) >= stop)
        .map(|(h, cost)| (h.distance(target), *cost, *h))
        .min()?;
    if d >= here {
        return None;
    }
    let path = s.path_to(u.id, to).ok()??;
    Some(Command::Move { unit: u.id, path })
}
