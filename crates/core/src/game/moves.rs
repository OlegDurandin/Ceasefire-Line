//! R-MOVE-01..04, R-MOVE-06: movement along a path.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::data::UnitTag;
use crate::error::{MoveError, RuleError};
use crate::hex::Hex;
use crate::movement::{enemy_zoc, entry_cost_halves, Mobility};
use crate::side::SideId;

use super::{Event, GameState, Unit, UnitId};

struct Step {
    cost: u32,
    enters_zoc: bool,
    /// R-MOVE-02: across a canal without a bridge; costs all remaining AP.
    boat: bool,
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
        let map = &self.state.map;
        if map.has_canal(from, to) && !map.has_bridge(from, to) {
            // R-MOVE-02: only infantry, by boat; the far hex must be enterable.
            let class = self.state.class(self.unit)?;
            let cell = map
                .cell(to)
                .ok_or_else(|| self.illegal(MoveError::Impassable))?;
            let terrain = self.state.data.terrain.get(&cell.terrain)?;
            if !class.has_tag(UnitTag::BoatCrossing) || (terrain.def.requires_pass && !cell.pass) {
                return Err(self.illegal(MoveError::Impassable));
            }
            return Ok(Step {
                cost: 0,
                enters_zoc: to_zoc,
                boat: true,
            });
        }
        let cost = entry_cost_halves(
            map,
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
            boat: false,
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

    /// Validates a path; returns its AP cost, whether it ends in enemy ZOC and
    /// whether its last step is a boat crossing.
    fn plan_move(&self, id: UnitId, path: &[Hex]) -> Result<(u32, bool, bool), RuleError> {
        let unit = self.movable_unit(id)?;
        let view = MoveView::new(self, unit);
        let Some(&last) = path.last() else {
            return Err(view.illegal(MoveError::EmptyPath));
        };
        let mut from = unit.hex;
        let mut total = 0u32;
        let mut in_zoc_stop = false;
        let mut boat = false;
        for (i, &to) in path.iter().enumerate() {
            if in_zoc_stop {
                return Err(view.illegal(MoveError::ZocStop));
            }
            if boat {
                return Err(view.illegal(MoveError::BoatCrossingNotLast));
            }
            let step = view.step(from, to, i == 0)?;
            total = total.saturating_add(step.cost);
            in_zoc_stop = step.enters_zoc;
            boat = step.boat;
            from = to;
        }
        if !view.can_end_at(last) {
            return Err(view.illegal(MoveError::Occupied));
        }
        // A boat crossing needs at least the minimum AP left, then takes all.
        let needed = if boat {
            total.saturating_add(self.ap.min)
        } else {
            total
        };
        if needed > unit.ap_halves {
            return Err(view.illegal(MoveError::NotEnoughAp));
        }
        let cost = if boat { unit.ap_halves } else { total };
        Ok((cost, in_zoc_stop, boat))
    }

    /// R-MOVE-01..04, R-MOVE-06.
    pub(super) fn move_unit(&mut self, id: UnitId, path: &[Hex]) -> Result<Vec<Event>, RuleError> {
        let (cost, ends_in_zoc, by_boat) = self.plan_move(id, path)?;
        let side = self.get_unit(id)?.side;
        let boat_penalty = self.data.rules.movement.boat_crossing_readiness_pct;
        let mut events = vec![Event::UnitMoved {
            unit: id,
            path: path.to_vec(),
            ap_spent_halves: cost,
            by_boat,
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
        if by_boat {
            unit.can_move = false;
            unit.crossing_penalty_pct = boat_penalty;
        }
        Ok(events)
    }

    /// Dijkstra over legal steps: best cost and predecessor per hex. Boat
    /// crossings are terminal and cost all remaining AP.
    fn search(&self, unit: &Unit) -> Result<BTreeMap<Hex, (u32, Hex)>, RuleError> {
        let view = MoveView::new(self, unit);
        let start = unit.hex;
        let mut best: BTreeMap<Hex, (u32, Hex)> = BTreeMap::from([(start, (0, start))]);
        let mut heap = BinaryHeap::from([Reverse((0u32, start))]);
        let mut boats: BTreeMap<Hex, Hex> = BTreeMap::new();
        while let Some(Reverse((cost, hex))) = heap.pop() {
            if best.get(&hex).is_some_and(|&(b, _)| cost > b) {
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
                if next == start {
                    continue;
                }
                if step.boat {
                    if cost.saturating_add(self.ap.min) <= unit.ap_halves {
                        boats.entry(next).or_insert(hex);
                    }
                    continue;
                }
                let next_cost = cost.saturating_add(step.cost);
                if next_cost > unit.ap_halves {
                    continue;
                }
                if best.get(&next).is_none_or(|&(b, _)| next_cost < b) {
                    best.insert(next, (next_cost, hex));
                    heap.push(Reverse((next_cost, next)));
                }
            }
        }
        for (hex, from) in boats {
            best.entry(hex).or_insert((unit.ap_halves, from));
        }
        Ok(best)
    }

    /// R-MOVE-01..03: hexes the unit can end its move in, with AP cost (half-points).
    pub fn reachable(&self, id: UnitId) -> Result<BTreeMap<Hex, u32>, RuleError> {
        let unit = self.get_unit(id)?;
        if !unit.can_move || unit.side != self.active_side() {
            return Ok(BTreeMap::new());
        }
        let view = MoveView::new(self, unit);
        Ok(self
            .search(unit)?
            .into_iter()
            .filter(|&(hex, _)| hex != unit.hex && view.can_end_at(hex))
            .map(|(hex, (cost, _))| (hex, cost))
            .collect())
    }

    /// Cheapest legal path to `to` (hexes after the start), ready for
    /// [`super::Command::Move`]; `None` if `to` is not reachable.
    pub fn path_to(&self, id: UnitId, to: Hex) -> Result<Option<Vec<Hex>>, RuleError> {
        if !self.reachable(id)?.contains_key(&to) {
            return Ok(None);
        }
        let unit = self.get_unit(id)?;
        let best = self.search(unit)?;
        let mut path = vec![to];
        let mut cur = to;
        while let Some(&(_, prev)) = best.get(&cur) {
            if prev == unit.hex {
                break;
            }
            path.push(prev);
            cur = prev;
        }
        path.reverse();
        Ok(Some(path))
    }
}
