//! R-HQ-01, R-HQ-02: automatic HQ attachment and its effects.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::error::RuleError;
use crate::hex::Hex;
use crate::scale::range_in_hexes;
use crate::side::SideId;

use super::{Event, GameState, Unit, UnitId};

struct HqSlot {
    id: UnitId,
    hex: Hex,
    rating: u8,
    formation: Option<String>,
    nation: Option<String>,
    free: u32,
}

impl GameState {
    /// R-HQ-01, at the start of `side`'s phase: pins first, then the HQ's own
    /// formation (nearest first), then the nearest HQ of the same nation with a
    /// free slot (ties: higher rating, then lower id). Capacity = rating,
    /// radius from `hq.attach_radius_km`. Units whose HQ was destroyed skip
    /// one phase.
    pub(super) fn attach_hqs(&mut self, side: SideId) -> Vec<(UnitId, UnitId)> {
        let radius = range_in_hexes(self.data.rules.hq.attach_radius_km, self.hex_km);
        let mut hqs: Vec<HqSlot> = self
            .units
            .values()
            .filter(|u| u.side == side)
            .filter_map(|u| {
                u.hq_rating.map(|rating| HqSlot {
                    id: u.id,
                    hex: u.hex,
                    rating,
                    formation: u.formation.clone(),
                    nation: u.nation.clone(),
                    free: u32::from(rating),
                })
            })
            .collect();
        let members: Vec<Unit> = self
            .units
            .values()
            .filter(|u| u.side == side && u.hq_rating.is_none())
            .cloned()
            .collect();
        let fits =
            |u: &Unit, hq: &HqSlot| u.nation == hq.nation && u.hex.distance(hq.hex) <= radius;
        let mut assigned: BTreeMap<UnitId, UnitId> = BTreeMap::new();

        // 1. Pins.
        for u in members.iter().filter(|u| !u.hq_lost) {
            let Some(pin) = u.pinned_hq else { continue };
            if let Some(hq) = hqs.iter_mut().find(|h| h.id == pin) {
                if hq.free > 0 && fits(u, hq) {
                    hq.free -= 1;
                    assigned.insert(u.id, hq.id);
                }
            }
        }
        // 2. Own formation, nearest first.
        for hq in hqs.iter_mut() {
            let Some(formation) = hq.formation.clone() else {
                continue;
            };
            let mut own: Vec<&Unit> = members
                .iter()
                .filter(|u| !u.hq_lost && !assigned.contains_key(&u.id))
                .filter(|u| u.formation.as_deref() == Some(formation.as_str()) && fits(u, hq))
                .collect();
            own.sort_by_key(|u| (u.hex.distance(hq.hex), u.id));
            for u in own {
                if hq.free == 0 {
                    break;
                }
                hq.free -= 1;
                assigned.insert(u.id, hq.id);
            }
        }
        // 3. Nearest HQ with a free slot.
        for u in members.iter().filter(|u| !u.hq_lost) {
            if assigned.contains_key(&u.id) {
                continue;
            }
            let best = hqs
                .iter_mut()
                .filter(|h| h.free > 0 && fits(u, h))
                .min_by_key(|h| (u.hex.distance(h.hex), Reverse(h.rating), h.id));
            if let Some(hq) = best {
                hq.free -= 1;
                assigned.insert(u.id, hq.id);
            }
        }

        for u in self
            .units
            .values_mut()
            .filter(|u| u.side == side && u.hq_rating.is_none())
        {
            u.attached_hq = assigned.get(&u.id).copied();
            u.hq_lost = false;
        }
        assigned.into_iter().collect()
    }

    /// R-HQ-02: readiness modifier in percentage points, applied after the
    /// R-UNIT-01 formula: +(rating − base) × per_rating attached, −unattached
    /// otherwise. Morale is not changed. HQs themselves get nothing.
    pub(super) fn hq_effect(&self, unit: &Unit) -> i32 {
        if unit.hq_rating.is_some() {
            return 0;
        }
        let h = &self.data.rules.hq;
        match unit
            .attached_hq
            .and_then(|id| self.units.get(&id))
            .and_then(|hq| hq.hq_rating)
        {
            Some(rating) => i32::from(
                rating
                    .saturating_sub(h.rating_base)
                    .saturating_mul(h.bonus_per_rating_pct),
            ),
            None => -i32::from(h.unattached_readiness_pct),
        }
    }

    /// R-HQ-02: experience grows faster under an HQ of rating ≥ `fast_exp_rating`.
    pub(super) fn exp_gain(&self, id: UnitId, gain: u16) -> u16 {
        let h = &self.data.rules.hq;
        let fast = self
            .units
            .get(&id)
            .and_then(|u| u.attached_hq)
            .and_then(|hq| self.units.get(&hq))
            .and_then(|hq| hq.hq_rating)
            .is_some_and(|r| r >= h.fast_exp_rating);
        if fast {
            u16::try_from(u32::from(gain) * u32::from(h.fast_exp_pct) / 100).unwrap_or(u16::MAX)
        } else {
            gain
        }
    }

    /// R-HQ-02: NM −3 for the owner; its units stay unattached for one phase.
    pub(super) fn hq_destroyed(&mut self, hq: &Unit, events: &mut Vec<Event>) {
        let loss = self.data.rules.hq.destroyed_nm_loss;
        let mut nm = 0;
        if let Ok(side) = self.side_state_mut(hq.side) {
            side.national_morale_pct = side.national_morale_pct.saturating_sub(loss);
            nm = side.national_morale_pct;
        }
        for u in self.units.values_mut() {
            if u.attached_hq == Some(hq.id) {
                u.attached_hq = None;
                u.hq_lost = true;
            }
            if u.pinned_hq == Some(hq.id) {
                u.pinned_hq = None;
            }
        }
        events.push(Event::HqDestroyed {
            hq: hq.id,
            nm_loss: loss,
            national_morale_pct: nm,
        });
    }

    /// R-HQ-01: optional manual pin; takes effect at the next own phase start.
    pub(super) fn pin_to_hq(
        &mut self,
        id: UnitId,
        hq: Option<UnitId>,
    ) -> Result<Vec<Event>, RuleError> {
        let unit = self.own_unit(id)?;
        let side = unit.side;
        let bad = |h: UnitId| RuleError::IllegalPin {
            unit: id.0,
            hq: h.0,
        };
        if unit.hq_rating.is_some() {
            return Err(bad(hq.unwrap_or(id)));
        }
        if let Some(h) = hq {
            let target = self.units.get(&h).ok_or_else(|| bad(h))?;
            if target.side != side || target.hq_rating.is_none() {
                return Err(bad(h));
            }
        }
        if let Some(u) = self.units.get_mut(&id) {
            u.pinned_hq = hq;
        }
        Ok(vec![Event::Pinned { unit: id, hq }])
    }
}
