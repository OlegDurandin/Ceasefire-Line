//! R-MOVE-02: bridges over the canal: building, rebuilding, destroying.

use crate::data::UnitTag;
use crate::error::{AttackError, BridgeError, RuleError};
use crate::hex::Hex;
use crate::scale::range_in_hexes;

use crate::side::SideId;

use super::{Event, GameState, PendingBridge, UnitId};

fn refuse(unit: UnitId, reason: BridgeError) -> RuleError {
    RuleError::IllegalBridge {
        unit: unit.0,
        reason,
    }
}

impl GameState {
    /// R-MOVE-02: an engineer that has not acted this phase, standing on the
    /// bank, starts a bridge over the canal edge to `toward` if a friendly
    /// ground unit holds the far bank. The engineer spends its phase; the
    /// bridge is ready at the start of its side's next phase. Bridges serve
    /// both sides.
    pub(super) fn build_bridge(
        &mut self,
        id: UnitId,
        toward: Hex,
    ) -> Result<Vec<Event>, RuleError> {
        let unit = self.own_unit(id)?.clone();
        if !self.class(&unit)?.has_tag(UnitTag::BuildBridge) {
            return Err(refuse(id, BridgeError::NotEngineer));
        }
        if unit.acted {
            return Err(refuse(id, BridgeError::Acted));
        }
        if !unit.hex.is_adjacent(toward) || !self.map.has_canal(unit.hex, toward) {
            return Err(refuse(id, BridgeError::NoCanal));
        }
        let edge_pending = self
            .pending_bridges
            .iter()
            .any(|p| (p.a, p.b) == (unit.hex, toward) || (p.a, p.b) == (toward, unit.hex));
        if self.map.has_bridge(unit.hex, toward) || edge_pending {
            return Err(refuse(id, BridgeError::AlreadyBridged));
        }
        if !self.unit_at(toward).is_some_and(|u| u.side == unit.side) {
            return Err(refuse(id, BridgeError::NoFriendlyOnFarBank));
        }
        self.pending_bridges.push(PendingBridge {
            engineer: id,
            side: unit.side,
            a: unit.hex,
            b: toward,
        });
        if let Some(u) = self.units.get_mut(&id) {
            u.ap_halves = 0;
            u.can_move = false;
            u.acted = true;
        }
        Ok(vec![Event::BridgeStarted {
            unit: id,
            a: unit.hex,
            b: toward,
        }])
    }

    /// R-MOVE-02: at the start of `side`'s phase its bridges are finished if
    /// the engineer is still alive on the same bank; otherwise the work is lost.
    pub(super) fn finish_bridges(
        &mut self,
        side: SideId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let (mine, rest): (Vec<PendingBridge>, Vec<PendingBridge>) =
            std::mem::take(&mut self.pending_bridges)
                .into_iter()
                .partition(|p| p.side == side);
        self.pending_bridges = rest;
        for p in mine {
            let on_site = self.units.get(&p.engineer).is_some_and(|u| u.hex == p.a);
            if on_site {
                self.map.add_bridge(p.a, p.b)?;
            }
            events.push(Event::BridgeBuilt {
                unit: p.engineer,
                a: p.a,
                b: p.b,
                completed: on_site,
            });
        }
        Ok(())
    }

    /// R-MOVE-02 + R-COMBAT-04: artillery within range of either bank fires
    /// at the bridge (1 AP and a strike, no movement after); destroyed with
    /// `bridge.bombard_destroy_chance`.
    pub(super) fn bombard_bridge(
        &mut self,
        id: UnitId,
        a: Hex,
        b: Hex,
    ) -> Result<Vec<Event>, RuleError> {
        let unit = self.own_unit(id)?.clone();
        let class = self.class(&unit)?;
        let attack = |reason| RuleError::IllegalAttack { unit: id.0, reason };
        let range_km = class
            .range_km
            .filter(|_| class.has_tag(UnitTag::Bombard))
            .ok_or(attack(AttackError::CannotBombard))?;
        if unit.strikes_left == 0 {
            return Err(attack(AttackError::NoStrikes));
        }
        if unit.ap_halves < self.ap.attack {
            return Err(attack(AttackError::NoAp));
        }
        if !self.map.has_bridge(a, b) {
            return Err(refuse(id, BridgeError::NoBridge));
        }
        let dist = unit.hex.distance(a).min(unit.hex.distance(b));
        if dist > range_in_hexes(range_km, self.hex_km) {
            return Err(attack(AttackError::OutOfRange));
        }
        let chance = self.data.rules.bridge.bombard_destroy_chance;
        let destroyed = self.rng.chance(chance);
        if destroyed {
            self.map.destroy_bridge(a, b)?;
        }
        if let Some(u) = self.units.get_mut(&id) {
            u.attacks_made += 1;
            u.acted = true;
            u.strikes_left -= 1;
            u.ap_halves = 0;
            u.can_move = false;
        }
        Ok(vec![Event::BridgeStrike {
            unit: id,
            a,
            b,
            chance,
            destroyed,
        }])
    }
}
