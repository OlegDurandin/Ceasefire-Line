//! R-MOVE-01..04, R-MOVE-06: movement along a path.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::data::UnitTag;
use crate::error::{MoveError, RuleError};
use crate::hex::Hex;
use crate::movement::{enemy_zoc_counts, entry_cost_halves, Mobility};
use crate::side::SideId;

use super::{Event, GameState, Unit, UnitId};

struct Step {
    cost: u32,
    /// The step ends the move: enemy ZOC (R-MOVE-03), or in a breakthrough
    /// ZOC of `breakthrough_stop` enemy units (R-MOVE-07).
    enters_zoc: bool,
    /// R-MOVE-02: across a canal without a bridge; costs all remaining AP.
    boat: bool,
    /// R-TURN-02: along a road or dirt road and not into enemy ZOC — no
    /// night surcharge.
    on_road: bool,
}

/// Snapshot of what matters for one unit's movement.
struct MoveView<'a> {
    state: &'a GameState,
    unit: &'a Unit,
    zoc: BTreeSet<Hex>,
    /// R-MOVE-07: enemy units whose ZOC covers each hex.
    zoc_units: BTreeMap<Hex, u8>,
    ground: BTreeMap<Hex, SideId>,
}

impl<'a> MoveView<'a> {
    fn new(state: &'a GameState, unit: &'a Unit) -> Self {
        let occupancy = state.occupancy();
        let zoc_units = enemy_zoc_counts(&state.map, &occupancy, unit.side);
        Self {
            state,
            unit,
            zoc: zoc_units.keys().copied().collect(),
            zoc_units,
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
        let breakthrough = self.breakthrough();
        if from_zoc && to_zoc && !breakthrough {
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
            if !class.has_tag(UnitTag::BoatCrossing)
                || map.is_closed(to)
                || terrain.def.impassable
                || (terrain.def.requires_pass && !cell.pass)
            {
                return Err(self.illegal(MoveError::Impassable));
            }
            return Ok(Step {
                cost: 0,
                enters_zoc: self.stops_at(to),
                boat: true,
                on_road: false,
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
        // Exit surcharge only for units that began the phase in ZOC; a
        // breakthrough pays per ZOC hex entered instead (R-MOVE-07).
        let surcharge = if breakthrough {
            if to_zoc {
                self.state.ap.breakthrough_zoc
            } else {
                0
            }
        } else if first && from_zoc && self.unit.started_in_zoc {
            self.state.ap.zoc_exit
        } else {
            0
        };
        Ok(Step {
            cost: cost + surcharge,
            enters_zoc: self.stops_at(to),
            boat: false,
            on_road: (map.has_road(from, to) || map.has_track(from, to)) && !to_zoc,
        })
    }

    /// R-TURN-02: the night surcharge for this step when still `due`;
    /// returns the extra cost and whether it is still due after the step.
    fn night(&self, step: &Step, due: bool) -> (u32, bool) {
        if due && !step.on_road {
            (self.state.ap.night_penalty, false)
        } else {
            (0, due)
        }
    }

    /// R-MOVE-07: the unit is in a breakthrough and the variant is on.
    fn breakthrough(&self) -> bool {
        self.unit.breakthrough && self.state.variants.breakthrough
    }

    /// Entering `hex` ends the move: enemy ZOC (R-MOVE-03); in a
    /// breakthrough only the ZOC of `breakthrough_stop` or more enemy units.
    fn stops_at(&self, hex: Hex) -> bool {
        let units = self.zoc_units.get(&hex).copied().unwrap_or(0);
        if self.breakthrough() {
            units >= self.state.ap.breakthrough_stop
        } else {
            units > 0
        }
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

    /// Validates a path; returns its AP cost, whether it ends in enemy ZOC,
    /// whether its last step is a boat crossing and whether the night
    /// surcharge (R-TURN-02) is still due after it.
    fn plan_move(&self, id: UnitId, path: &[Hex]) -> Result<(u32, bool, bool, bool), RuleError> {
        let unit = self.movable_unit(id)?;
        let view = MoveView::new(self, unit);
        let Some(&last) = path.last() else {
            return Err(view.illegal(MoveError::EmptyPath));
        };
        let mut from = unit.hex;
        let mut total = 0u32;
        let mut in_zoc_stop = false;
        let mut boat = false;
        let mut night_due = unit.night_surcharge_due;
        for (i, &to) in path.iter().enumerate() {
            if in_zoc_stop {
                return Err(view.illegal(MoveError::ZocStop));
            }
            if boat {
                return Err(view.illegal(MoveError::BoatCrossingNotLast));
            }
            let step = view.step(from, to, i == 0)?;
            let (night, due) = view.night(&step, night_due);
            night_due = due;
            total = total.saturating_add(step.cost + night);
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
        Ok((cost, in_zoc_stop, boat, night_due))
    }

    /// R-MOVE-01..04, R-MOVE-06.
    pub(super) fn move_unit(&mut self, id: UnitId, path: &[Hex]) -> Result<Vec<Event>, RuleError> {
        let (cost, ends_in_zoc, by_boat, night_due) = self.plan_move(id, path)?;
        let side = self.get_unit(id)?.side;
        // R-MOVE-06: a hex of the path in enemy ZOC (as the move began)
        // does not change hands; the final hex always does.
        let zoc = MoveView::new(self, self.get_unit(id)?).zoc;
        let boat_penalty = self.data.rules.movement.boat_crossing_readiness_pct;
        let mut events = vec![Event::UnitMoved {
            unit: id,
            path: path.to_vec(),
            ap_spent_halves: cost,
            by_boat,
        }];
        for (i, &hex) in path.iter().enumerate() {
            if i + 1 == path.len() || !zoc.contains(&hex) {
                self.take_hex(hex, side, &mut events);
            }
        }
        let unit = self
            .units
            .get_mut(&id)
            .ok_or(RuleError::UnknownUnit(id.0))?;
        if let Some(&last) = path.last() {
            unit.hex = last;
        }
        unit.ap_halves -= cost;
        unit.night_surcharge_due = night_due;
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

    /// Dijkstra over legal steps and the night surcharge state (R-TURN-02):
    /// best cost and predecessor per (hex, surcharge still due). Boat
    /// crossings are terminal and cost all remaining AP.
    fn search(&self, unit: &Unit) -> Result<BTreeMap<Node, (u32, Node)>, RuleError> {
        let view = MoveView::new(self, unit);
        let start: Node = (unit.hex, unit.night_surcharge_due);
        let mut best: BTreeMap<Node, (u32, Node)> = BTreeMap::from([(start, (0, start))]);
        let mut heap = BinaryHeap::from([Reverse((0u32, start))]);
        let mut boats: BTreeMap<Node, Node> = BTreeMap::new();
        while let Some(Reverse((cost, node))) = heap.pop() {
            let (hex, due) = node;
            if best.get(&node).is_some_and(|&(b, _)| cost > b) {
                continue;
            }
            if hex != start.0 && view.stops_at(hex) {
                continue;
            }
            for next in hex.neighbors() {
                let step = match view.step(hex, next, hex == start.0) {
                    Ok(s) => s,
                    Err(RuleError::IllegalMove { .. }) => continue,
                    Err(e) => return Err(e),
                };
                if next == start.0 {
                    continue;
                }
                let (night, next_due) = view.night(&step, due);
                let next_node = (next, next_due);
                if step.boat {
                    if cost.saturating_add(night).saturating_add(self.ap.min) <= unit.ap_halves {
                        boats.entry(next_node).or_insert(node);
                    }
                    continue;
                }
                let next_cost = cost.saturating_add(step.cost + night);
                if next_cost > unit.ap_halves {
                    continue;
                }
                if best.get(&next_node).is_none_or(|&(b, _)| next_cost < b) {
                    best.insert(next_node, (next_cost, node));
                    heap.push(Reverse((next_cost, next_node)));
                }
            }
        }
        for (node, from) in boats {
            best.entry(node).or_insert((unit.ap_halves, from));
        }
        Ok(best)
    }

    /// Cheapest search node per hex (ties: surcharge still due first).
    fn cheapest(best: &BTreeMap<Node, (u32, Node)>) -> BTreeMap<Hex, (u32, Node)> {
        let mut out: BTreeMap<Hex, (u32, Node)> = BTreeMap::new();
        for (&node, &(cost, _)) in best {
            let better = out
                .get(&node.0)
                .is_none_or(|&(c, n)| cost < c || (cost == c && node.1 && !n.1));
            if better {
                out.insert(node.0, (cost, node));
            }
        }
        out
    }

    /// R-MOVE-01..03: hexes the unit can end its move in, with AP cost (half-points).
    pub fn reachable(&self, id: UnitId) -> Result<BTreeMap<Hex, u32>, RuleError> {
        let unit = self.get_unit(id)?;
        if !unit.can_move || unit.side != self.active_side() {
            return Ok(BTreeMap::new());
        }
        let view = MoveView::new(self, unit);
        Ok(Self::cheapest(&self.search(unit)?)
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
        let Some(&(_, mut cur)) = Self::cheapest(&best).get(&to) else {
            return Ok(None);
        };
        let start = (unit.hex, unit.night_surcharge_due);
        let mut path = vec![to];
        while let Some(&(_, prev)) = best.get(&cur) {
            if prev == start || prev.0 == unit.hex {
                break;
            }
            path.push(prev.0);
            cur = prev;
        }
        path.reverse();
        Ok(Some(path))
    }
}

/// A search state: hex and whether the night surcharge is still due.
type Node = (Hex, bool);
