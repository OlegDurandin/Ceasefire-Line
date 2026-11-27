//! R-NM-02 «выход» (designer decision 2026-11-27): steps of a side that
//! enter an open exit hex leave the map and give that side VP.

use super::{Event, GameState, UnitId};
use crate::side::SideId;

impl GameState {
    /// The unit leaves through an open exit of its side, if it stands on one.
    pub(super) fn exit_if_due(&mut self, id: UnitId, events: &mut Vec<Event>) {
        let Some(u) = self.units.get(&id) else { return };
        let (side, hex, steps) = (u.side, u.hex, u.strength);
        let Some(vp) = self
            .conditions
            .exits
            .iter()
            .find(|z| z.open && z.side == side && z.hexes.contains(&hex))
            .map(|z| z.vp_per_step)
        else {
            return;
        };
        self.units.remove(&id);
        let gained = vp * i32::from(steps);
        *self.exited_vp.entry(side).or_insert(0) += gained;
        events.push(Event::UnitExited {
            unit: id,
            hex,
            steps,
            vp: gained,
        });
    }

    /// R-EVT-01 effect: the exits of `side` open.
    pub(super) fn open_exits(&mut self, side: SideId) {
        for z in self.conditions.exits.iter_mut().filter(|z| z.side == side) {
            z.open = true;
        }
    }

    /// R-NM-02: the scenario's victory conditions (exit zones with their state).
    pub fn conditions(&self) -> &super::VictoryConditions {
        &self.conditions
    }

    /// R-NM-02: VP a side earned by steps that left through exits.
    pub fn exited_vp(&self, side: SideId) -> i32 {
        self.exited_vp.get(&side).copied().unwrap_or(0)
    }
}
