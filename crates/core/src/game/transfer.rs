//! R-MOVE-05: strategic transfer by road through own territory.

use std::collections::{BTreeMap, VecDeque};

use crate::error::{RuleError, TransferError};
use crate::hex::Hex;
use crate::movement::enemy_zoc;

use super::economy::div_half_up;
use super::{Event, GameState, UnitId};

fn refuse(unit: UnitId, reason: TransferError) -> RuleError {
    RuleError::IllegalTransfer {
        unit: unit.0,
        reason,
    }
}

impl GameState {
    /// R-MOVE-05: shortest road route (in hexes) to `to`: road edges only
    /// (a canal needs a bridge), own hexes only, never into enemy ZOC or
    /// enemy units; friendly units may be passed; the destination must be free.
    pub fn transfer_route(&self, id: UnitId, to: Hex) -> Result<Vec<Hex>, RuleError> {
        let unit = self.own_unit(id)?;
        if unit.acted {
            return Err(refuse(id, TransferError::Acted));
        }
        let occupancy = self.occupancy();
        let zoc = enemy_zoc(&self.map, &occupancy, unit.side);
        if zoc.contains(&unit.hex) {
            return Err(refuse(id, TransferError::StartsInZoc));
        }
        let side = unit.side;
        let passable = |from: Hex, next: Hex| {
            self.map.has_road(from, next)
                && (!self.map.has_canal(from, next) || self.map.has_bridge(from, next))
                && self.control.get(&next) == Some(&side)
                && !zoc.contains(&next)
                && occupancy.get(&next).is_none_or(|o| o.side == side)
        };
        let mut parent: BTreeMap<Hex, Hex> = BTreeMap::new();
        let mut queue = VecDeque::from([unit.hex]);
        while let Some(hex) = queue.pop_front() {
            if hex == to {
                break;
            }
            for next in hex.neighbors() {
                if next != unit.hex && !parent.contains_key(&next) && passable(hex, next) {
                    parent.insert(next, hex);
                    queue.push_back(next);
                }
            }
        }
        if to == unit.hex || !parent.contains_key(&to) || occupancy.contains_key(&to) {
            return Err(refuse(id, TransferError::NoRoute));
        }
        let mut path = vec![to];
        let mut cur = to;
        while let Some(&prev) = parent.get(&cur) {
            if prev == unit.hex {
                break;
            }
            path.push(prev);
            cur = prev;
        }
        path.reverse();
        Ok(path)
    }

    /// R-MOVE-05: price = `transfer_cost_pct` of the unit price, rounded half up.
    pub fn transfer_cost(&self, id: UnitId) -> Result<u32, RuleError> {
        let unit = self.get_unit(id)?;
        let pct = self.data.rules.economy.transfer_cost_pct;
        let num = u64::from(self.class(unit)?.cost_mpp) * u64::from(pct);
        Ok(u32::try_from(div_half_up(num, 100)).unwrap_or(u32::MAX))
    }

    /// R-MOVE-05: the unit ends its turn after the transfer.
    pub(super) fn transfer(&mut self, id: UnitId, to: Hex) -> Result<Vec<Event>, RuleError> {
        let path = self.transfer_route(id, to)?;
        let cost = self.transfer_cost(id)?;
        let side = self.get_unit(id)?.side;
        let state = self.side_state_mut(side)?;
        if state.mpp < cost {
            return Err(refuse(id, TransferError::NotEnoughMpp));
        }
        state.mpp -= cost;
        let unit = self
            .units
            .get_mut(&id)
            .ok_or(RuleError::UnknownUnit(id.0))?;
        unit.hex = to;
        unit.ap_halves = 0;
        unit.strikes_left = 0;
        unit.can_move = false;
        unit.acted = true;
        unit.moved = true;
        unit.entrenchment = 0;
        Ok(vec![Event::Transferred {
            unit: id,
            path,
            cost,
        }])
    }
}
