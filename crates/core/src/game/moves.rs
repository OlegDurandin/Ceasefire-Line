//! R-MOVE-01..04, R-MOVE-06: movement along a path.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::error::{MoveError, RuleError};
use crate::hex::Hex;
use crate::movement::{enemy_zoc, entry_cost_halves, Mobility};
use crate::side::SideId;

use super::{Event, GameState, Unit, UnitId};

struct Step {
    cost: u32,
    enters_zoc: bool,
}

/// Snapshot of what matters for one unit's movement.
struct MoveView<'a> {
    state: &'a GameState,
    unit: &'a Unit,
    zoc: BTreeSet<Hex>,
    ground: BTreeMap<Hex, SideId>,
}

impl<'a> MoveView<'a> {
    fn new(state: &'a GameState, unit: &'a Unit) -> Self {
        let occupancy = state.occupancy();
        Self {
            state,
            unit,
            zoc: enemy_zoc(&state.map, &occupancy, unit.side),
            ground: occupancy.iter().map(|(&h, o)| (h, o.side)).collect(),
        }
    }

    fn illegal(&self, reason: MoveError) -> RuleError {
        RuleError::IllegalMove {
            unit: self.unit.id.0,
            reason,
        }
    }

    /// R-MOVE-01..03: one step. Friendly units may be passed through.
    fn step(&self, from: Hex, to: Hex, first: bool) -> Result<Step, RuleError> {
        if !from.is_adjacent(to) {
            return Err(self.illegal(MoveError::NotAdjacent));
        }
        if self.ground.get(&to).is_some_and(|&s| s != self.unit.side) {
            return Err(self.illegal(MoveError::Occupied));
        }
        let from_zoc = self.zoc.contains(&from);
        let to_zoc = self.zoc.contains(&to);
        if from_zoc && to_zoc {
            return Err(self.illegal(MoveError::ZocToZoc));
        }
        let cost = entry_cost_halves(
            &self.state.map,
            &self.state.data.terrain,
            from,
            to,
            Mobility::Tracked,
            to_zoc,
        )?
        .ok_or_else(|| self.illegal(MoveError::Impassable))?;
        // Exit surcharge only for units that began the phase in ZOC.
        let surcharge = if first && from_zoc && self.unit.started_in_zoc {
            self.state.ap.zoc_exit
        } else {
            0
        };
        Ok(Step {
            cost: cost + surcharge,
            enters_zoc: to_zoc,
        })
    }

    fn can_end_at(&self, hex: Hex) -> bool {
        !self.ground.contains_key(&hex)
    }
}

impl GameState {
    fn movable_unit(&self, id: UnitId) -> Result<&Unit, RuleError> {
        let unit = self.own_unit(id)?;
        if !unit.can_move || unit.ap_halves == 0 {
            return Err(RuleError::IllegalMove {
                unit: id.0,
                reason: MoveError::CannotMove,
            });
        }
        Ok(unit)
    }

    /// Validates a path; returns its AP cost and whether it ends in enemy ZOC.
    fn plan_move(&self, id: UnitId, path: &[Hex]) -> Result<(u32, bool), RuleError> {
        let unit = self.movable_unit(id)?;
        let view = MoveView::new(self, unit);
        let Some(&last) = path.last() else {
            return Err(view.illegal(MoveError::EmptyPath));
        };
        let mut from = unit.hex;
        let mut total = 0u32;
        let mut in_zoc_stop = false;
        for (i, &to) in path.iter().enumerate() {
            if in_zoc_stop {
                return Err(view.illegal(MoveError::ZocStop));
            }
            let step = view.step(from, to, i == 0)?;
            total = total.saturating_add(step.cost);
            in_zoc_stop = step.enters_zoc;
            from = to;
        }
        if !view.can_end_at(last) {
            return Err(view.illegal(MoveError::Occupied));
        }
        if total > unit.ap_halves {
            return Err(view.illegal(MoveError::NotEnoughAp));
        }
        Ok((total, in_zoc_stop))
    }

    /// R-MOVE-01..04, R-MOVE-06.
    pub(super) fn move_unit(&mut self, id: UnitId, path: &[Hex]) -> Result<Vec<Event>, RuleError> {
        let (cost, ends_in_zoc) = self.plan_move(id, path)?;
        let side = self.get_unit(id)?.side;
        let mut events = vec![Event::UnitMoved {
            unit: id,
            path: path.to_vec(),
            ap_spent_halves: cost,
        }];
        for &hex in path {
            self.take_hex(hex, side, &mut events);
        }
        let unit = self
            .units
            .get_mut(&id)
            .ok_or(RuleError::UnknownUnit(id.0))?;
        if let Some(&last) = path.last() {
            unit.hex = last;
        }
        unit.ap_halves -= cost;
        unit.entrenchment = 0;
        unit.moved = true;
        unit.acted = true;
        if ends_in_zoc {
            unit.can_move = false;
        }
        Ok(events)
    }

    /// R-MOVE-01..03: hexes the unit can end its move in, with AP cost (half-points).
    pub fn reachable(&self, id: UnitId) -> Result<BTreeMap<Hex, u32>, RuleError> {
        let unit = self.get_unit(id)?;
        let mut result = BTreeMap::new();
        if !unit.can_move || unit.side != self.active_side() {
            return Ok(result);
        }
        let view = MoveView::new(self, unit);
        let start = unit.hex;
        let mut best: BTreeMap<Hex, u32> = BTreeMap::from([(start, 0)]);
        let mut heap = BinaryHeap::from([Reverse((0u32, start))]);
        while let Some(Reverse((cost, hex))) = heap.pop() {
            if best.get(&hex).is_some_and(|&b| cost > b) {
                continue;
            }
            if hex != start && view.zoc.contains(&hex) {
                continue;
            }
            for next in hex.neighbors() {
                let step = match view.step(hex, next, hex == start) {
                    Ok(s) => s,
                    Err(RuleError::IllegalMove { .. }) => continue,
                    Err(e) => return Err(e),
                };
                let next_cost = cost.saturating_add(step.cost);
                if next_cost > unit.ap_halves || next == start {
                    continue;
                }
                if best.get(&next).is_none_or(|&b| next_cost < b) {
                    best.insert(next, next_cost);
                    heap.push(Reverse((next_cost, next)));
                }
            }
        }
        for (hex, cost) in best {
            if hex != start && view.can_end_at(hex) {
                result.insert(hex, cost);
            }
        }
        Ok(result)
    }
}
