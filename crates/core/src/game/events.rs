//! R-EVT-01: scenario events (trigger → conditions → effects), checked at
//! the start of every phase (R-TURN-01 step 1). Each event fires once.

use serde::{Deserialize, Serialize};

use crate::error::RuleError;
use crate::hex::Hex;
use crate::side::SideId;

use super::{Event, GameState, NmReason, Unit, UnitId, UnitSetup};

/// What an event waits for, and what its conditions test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Predicate {
    /// Turn number reached.
    Turn { turn: u32 },
    /// R-MOVE-06: hex controlled by `side`.
    HexControl { hex: Hex, side: SideId },
    /// R-NM-01: NM of `side` at or below `pct`.
    NmAtMost { side: SideId, pct: u8 },
    /// Steps lost by `side` so far.
    LossesAtLeast { side: SideId, steps: u32 },
    /// A ground unit of `side` within `km` (whole km) of `hex` (R-SCALE-01: km → hexes).
    UnitNear { side: SideId, hex: Hex, km: u32 },
    /// Any of the predicates holds.
    AnyOf { of: Vec<Predicate> },
    /// All of the predicates hold.
    AllOf { of: Vec<Predicate> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    /// Units appear; an occupied hex falls back to the nearest free own hex.
    Reinforcements {
        units: Vec<UnitSetup>,
    },
    Mpp {
        side: SideId,
        amount: i32,
    },
    Nm {
        side: SideId,
        change: i32,
    },
    /// R-EVT-01: the ceasefire date moves (negative = earlier).
    ShiftLastTurn {
        turns: i32,
    },
    SetLastTurn {
        turn: u32,
    },
    /// Localisation key of a message shown to the players.
    Message {
        key: String,
    },
    /// Morale of every ground unit of `side` changes, within 0 and the side's NM.
    UnitsMorale {
        side: SideId,
        change: i32,
    },
    /// R-NM-02: the exits of `side` open (e.g. a withdrawal order).
    OpenExits {
        side: SideId,
    },
    /// Entrenchment of every ground unit of `side` changes, within 0 and the terrain's maximum.
    UnitsEntrenchment {
        side: SideId,
        change: i32,
    },
    /// Readiness of every ground unit of `side` drops by `pct` points until
    /// the start of that side's next phase (like fatigue).
    UnitsReadinessPenalty {
        side: SideId,
        pct: u8,
    },
    /// R-MOVE-02: a bridge (or ferry) on the side `a`|`b` is blown up.
    DestroyBridge {
        a: Hex,
        b: Hex,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioEvent {
    pub id: String,
    /// R-UI-07: shown as a window with the historical text until clicked.
    pub important: bool,
    /// Only checked at the start of this side's phase; `None` = every phase.
    pub phase_of: Option<SideId>,
    pub trigger: Predicate,
    pub conditions: Vec<Predicate>,
    pub effects: Vec<Effect>,
}

impl GameState {
    fn holds(&self, p: &Predicate) -> bool {
        match p {
            Predicate::Turn { turn } => self.turn >= *turn,
            Predicate::HexControl { hex, side } => self.control.get(hex) == Some(side),
            Predicate::NmAtMost { side, pct } => self
                .side(*side)
                .is_some_and(|s| s.national_morale_pct <= *pct),
            Predicate::LossesAtLeast { side, steps } => {
                self.side(*side).is_some_and(|s| s.steps_lost >= *steps)
            }
            Predicate::UnitNear { side, hex, km } => {
                let r = crate::scale::range_in_hexes(f64::from(*km), self.hex_km);
                self.units
                    .values()
                    .any(|u| u.side == *side && u.hex.distance(*hex) <= r)
            }
            Predicate::AnyOf { of } => of.iter().any(|p| self.holds(p)),
            Predicate::AllOf { of } => of.iter().all(|p| self.holds(p)),
        }
    }

    /// R-EVT-01, at the start of the active side's phase, in file order.
    pub(super) fn run_events(&mut self, events: &mut Vec<Event>) -> Result<(), RuleError> {
        let side = self.active_side();
        let due: Vec<ScenarioEvent> = self
            .events
            .iter()
            .filter(|e| !self.fired_events.contains(&e.id))
            .filter(|e| e.phase_of.is_none_or(|s| s == side))
            .filter(|e| self.holds(&e.trigger) && e.conditions.iter().all(|c| self.holds(c)))
            .cloned()
            .collect();
        for e in due {
            self.fired_events.insert(e.id.clone());
            events.push(Event::ScenarioEventFired {
                id: e.id.clone(),
                important: e.important,
                effects: e.effects.clone(),
            });
            for effect in &e.effects {
                self.apply_effect(&e.id, effect, events)?;
            }
        }
        Ok(())
    }

    fn apply_effect(
        &mut self,
        event_id: &str,
        effect: &Effect,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        match effect {
            Effect::Reinforcements { units } => {
                for u in units {
                    self.place_reinforcement(u, events)?;
                }
            }
            Effect::Mpp { side, amount } => {
                let s = self.side_state_mut(*side)?;
                s.mpp = s.mpp.saturating_add_signed(*amount);
                let total = s.mpp;
                events.push(Event::MppChanged {
                    side: *side,
                    amount: *amount,
                    total,
                });
            }
            Effect::Nm { side, change } => {
                self.change_nm(
                    *side,
                    *change,
                    NmReason::ScenarioEvent {
                        id: event_id.to_owned(),
                    },
                    events,
                )?;
            }
            Effect::ShiftLastTurn { turns } => {
                if let Some(last) = self.last_turn {
                    let new = last.saturating_add_signed(*turns).max(self.turn);
                    self.set_last_turn(new, events);
                }
            }
            Effect::SetLastTurn { turn } => self.set_last_turn((*turn).max(self.turn), events),
            Effect::Message { key } => events.push(Event::Message { key: key.clone() }),
            Effect::UnitsMorale { side, change } => {
                let cap = i32::from(self.morale_cap(*side)?);
                for unit in self.ground_units_mut(*side)? {
                    unit.morale_pct = (i32::from(unit.morale_pct) + change).clamp(0, cap) as u8;
                }
                events.push(Event::UnitsMoraleChanged {
                    side: *side,
                    change: *change,
                });
            }
            Effect::UnitsEntrenchment { side, change } => {
                let ids: Vec<UnitId> = self.ground_units_mut(*side)?.iter().map(|u| u.id).collect();
                for id in ids {
                    let Some(hex) = self.units.get(&id).map(|u| u.hex) else {
                        continue;
                    };
                    let max = i32::from(
                        self.data
                            .terrain
                            .get(&self.map_terrain(hex)?)?
                            .def
                            .max_entrench,
                    );
                    if let Some(unit) = self.units.get_mut(&id) {
                        unit.entrenchment =
                            (i32::from(unit.entrenchment) + change).clamp(0, max) as u8;
                    }
                }
                events.push(Event::UnitsEntrenchmentChanged {
                    side: *side,
                    change: *change,
                });
            }
            Effect::OpenExits { side } => {
                self.open_exits(*side);
                events.push(Event::ExitsOpened { side: *side });
            }
            Effect::UnitsReadinessPenalty { side, pct } => {
                for unit in self.ground_units_mut(*side)? {
                    unit.event_penalty_pct = unit.event_penalty_pct.saturating_add(*pct).min(100);
                }
                events.push(Event::UnitsReadinessPenalty {
                    side: *side,
                    pct: *pct,
                });
            }
            Effect::DestroyBridge { a, b } => {
                if self.map.has_crossing(*a, *b) {
                    self.map.destroy_bridge(*a, *b)?;
                    events.push(Event::BridgeBlown { a: *a, b: *b });
                }
            }
        }
        Ok(())
    }

    /// Ground units of `side` (R-EVT-01 effects on units).
    fn ground_units_mut(&mut self, side: SideId) -> Result<Vec<&mut Unit>, RuleError> {
        let data = self.data.clone();
        let mut out = Vec::new();
        for unit in self.units.values_mut().filter(|u| u.side == side) {
            if data.units.get(&unit.class)?.target_class.is_ground() {
                out.push(unit);
            }
        }
        Ok(out)
    }

    fn set_last_turn(&mut self, turn: u32, events: &mut Vec<Event>) {
        self.last_turn = Some(turn);
        events.push(Event::CeasefireMoved { last_turn: turn });
    }

    /// The requested hex if free, else the nearest free hex the side controls
    /// (ties: lower (q, r)). No room at all is a data error.
    fn place_reinforcement(
        &mut self,
        u: &UnitSetup,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let class = self.data.units.get(&u.class)?;
        if !class.target_class.is_ground() {
            return Err(RuleError::Unsupported(format!(
                "`{}`: air and naval reinforcements",
                u.class
            )));
        }
        let occupied = self.occupancy();
        let free = |h: &Hex| self.map.cell(*h).is_some() && !occupied.contains_key(h);
        let hex = if free(&u.hex) {
            u.hex
        } else {
            self.control
                .iter()
                .filter(|(h, s)| **s == u.side && free(h))
                .map(|(h, _)| *h)
                .min_by_key(|h| (h.distance(u.hex), *h))
                .ok_or_else(|| {
                    RuleError::InvalidData(format!("no room for reinforcement `{}`", u.class))
                })?
        };
        let id = UnitId(self.next_unit_id);
        self.next_unit_id += 1;
        let mut unit = Unit::from_setup(id, u, hex);
        unit.supply = u.supply.unwrap_or(0);
        self.units.insert(id, unit);
        events.push(Event::UnitArrived {
            unit: id,
            class: u.class.clone(),
            hex,
        });
        Ok(())
    }
}
