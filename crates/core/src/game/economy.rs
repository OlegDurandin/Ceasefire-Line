//! R-ECO-01..03, R-SUP-03: income, reinforcement, purchases.

use crate::error::{EconomyError, RuleError};
use crate::side::SideId;
use crate::supply::SupplyMap;

use super::{Arrival, Event, GameState, Unit, UnitId};

fn refuse(reason: EconomyError) -> RuleError {
    RuleError::IllegalEconomy(reason)
}

/// Integer division rounded half up.
fn div_half_up(num: u64, den: u64) -> u64 {
    (2 * num + den) / (2 * den)
}

impl GameState {
    /// R-ECO-01: `base + Σ value × supply / max + scheduled`, cities summed
    /// before rounding (half up).
    pub(super) fn income(&self, side: SideId, supply: &SupplyMap) -> u32 {
        let Ok(state) = self.side_state(side) else {
            return 0;
        };
        let cities: u64 = self
            .cities
            .iter()
            .filter(|c| self.control.get(&c.hex) == Some(&side))
            .map(|c| u64::from(c.value) * u64::from(supply.get(c.hex)))
            .sum();
        let city_income = div_half_up(cities, u64::from(self.supply_params.max.max(1)));
        let scheduled: u64 = self
            .scheduled_income
            .iter()
            .filter(|e| e.side == side && e.from_turn <= self.turn)
            .filter(|e| e.until_turn.is_none_or(|u| self.turn <= u))
            .map(|e| u64::from(e.amount))
            .sum();
        let total = u64::from(state.base_income) + city_income + scheduled;
        u32::try_from(total).unwrap_or(u32::MAX)
    }

    /// R-ECO-02 + R-SUP-03: price of `steps` steps for this unit now.
    pub fn reinforce_cost(&self, id: UnitId, steps: u8, elite: bool) -> Result<u32, RuleError> {
        let unit = self.get_unit(id)?;
        self.check_reinforce(unit, steps).map(|(_, cost_pct)| {
            let e = &self.data.rules.economy;
            let class_cost = self.class(unit).map_or(0, |c| c.cost_mpp);
            price(
                class_cost,
                steps,
                e.reinforce_cost_pct,
                cost_pct,
                elite.then_some(e.elite_cost_pct),
            )
        })
    }

    /// Returns the band's steps limit and cost percent.
    fn check_reinforce(&self, unit: &Unit, steps: u8) -> Result<(u8, u32), RuleError> {
        if steps == 0 {
            return Err(refuse(EconomyError::ZeroSteps));
        }
        if unit.acted {
            return Err(refuse(EconomyError::UnitActed));
        }
        let band = self
            .data
            .rules
            .economy
            .band(unit.supply)
            .ok_or(refuse(EconomyError::NoSupply))?;
        if unit.reinforced_steps.saturating_add(steps) > band.max_steps {
            return Err(refuse(EconomyError::StepLimit));
        }
        if unit.strength.saturating_add(steps) > self.data.rules.unit.max_strength {
            return Err(refuse(EconomyError::FullStrength));
        }
        Ok((band.max_steps, band.cost_pct))
    }

    /// R-ECO-02: elite steps keep experience; green steps dilute it
    /// (`exp × old / new`, floor, in quarter stars).
    pub(super) fn reinforce(
        &mut self,
        id: UnitId,
        steps: u8,
        elite: bool,
    ) -> Result<Vec<Event>, RuleError> {
        let unit = self.own_unit(id)?;
        let side = unit.side;
        let cost = self.reinforce_cost(id, steps, elite)?;
        let state = self.side_state_mut(side)?;
        if state.mpp < cost {
            return Err(refuse(EconomyError::NotEnoughMpp));
        }
        state.mpp -= cost;
        let unit = self
            .units
            .get_mut(&id)
            .ok_or(RuleError::UnknownUnit(id.0))?;
        let old = u32::from(unit.strength);
        let new = old + u32::from(steps);
        if !elite {
            unit.exp_quarters = (u32::from(unit.exp_quarters) * old / new) as u8;
        }
        unit.strength = new as u8;
        unit.reinforced_steps += steps;
        Ok(vec![Event::Reinforced {
            unit: id,
            steps,
            elite,
            cost,
        }])
    }

    /// R-ECO-02, R-ECO-03.
    pub(super) fn purchase(&mut self, class_id: &str) -> Result<Vec<Event>, RuleError> {
        let class = self.data.units.get(class_id)?;
        if !class.target_class.is_ground() {
            return Err(RuleError::Unsupported(format!(
                "`{class_id}`: buying aircraft and ships (R-TURN-03)"
            )));
        }
        let cost = class.cost_mpp;
        let delay = self.data.rules.economy.purchase_delay_turns;
        let side = self.active_side();
        let arrives_turn = self.turn + delay;
        let state = self.side_state_mut(side)?;
        let limit = state.purchase_limits.get(class_id).copied().unwrap_or(0);
        let bought = state.purchased.get(class_id).copied().unwrap_or(0);
        if bought >= limit {
            return Err(refuse(EconomyError::PurchaseLimit));
        }
        if state.mpp < cost {
            return Err(refuse(EconomyError::NotEnoughMpp));
        }
        state.mpp -= cost;
        state.purchased.insert(class_id.to_owned(), bought + 1);
        self.arrivals.push(Arrival {
            side,
            class: class_id.to_owned(),
            turn: arrives_turn,
        });
        Ok(vec![Event::Purchased {
            side,
            class: class_id.to_owned(),
            cost,
            arrives_turn,
        }])
    }

    /// R-ECO-02: due arrivals appear in free own sources of value ≥
    /// `arrival_source_value` (lowest coordinates first); otherwise they wait.
    pub(super) fn deliver_arrivals(&mut self, side: SideId, nm: u8, events: &mut Vec<Event>) {
        let min_value = self.data.rules.economy.arrival_source_value;
        let max_strength = self.data.rules.unit.max_strength;
        let (due, waiting): (Vec<Arrival>, Vec<Arrival>) = std::mem::take(&mut self.arrivals)
            .into_iter()
            .partition(|a| a.side == side && a.turn <= self.turn);
        self.arrivals = waiting;
        for arrival in due {
            let occupied = self.occupancy();
            let spot = self
                .sources
                .iter()
                .filter(|(h, s)| {
                    self.control.get(h) == Some(&side)
                        && s.current >= min_value
                        && !occupied.contains_key(h)
                })
                .map(|(&h, _)| h)
                .next();
            let Some(hex) = spot else {
                events.push(Event::ArrivalDelayed {
                    side,
                    class: arrival.class.clone(),
                });
                self.arrivals.push(arrival);
                continue;
            };
            let id = UnitId(self.next_unit_id);
            self.next_unit_id += 1;
            self.units.insert(
                id,
                Unit {
                    id,
                    class: arrival.class.clone(),
                    side,
                    hex,
                    strength: max_strength,
                    morale_pct: nm.min(100),
                    exp_quarters: 0,
                    entrenchment: 0,
                    supply: 0,
                    zero_supply_phases: 0,
                    base_readiness_pct: 0,
                    attacks_made: 0,
                    attacks_received: 0,
                    ap_halves: 0,
                    strikes_left: 0,
                    can_move: false,
                    started_in_zoc: false,
                    moved: false,
                    fought: false,
                    acted: false,
                    reinforced_steps: 0,
                    hq_rating: None,
                },
            );
            events.push(Event::UnitArrived {
                unit: id,
                class: arrival.class,
                hex,
            });
        }
    }
}

/// `class_cost × step_pct% × band_pct% × elite_pct% × steps`, rounded half up.
fn price(class_cost: u32, steps: u8, step_pct: u32, band_pct: u32, elite_pct: Option<u32>) -> u32 {
    let num = u64::from(class_cost)
        * u64::from(step_pct)
        * u64::from(band_pct)
        * u64::from(elite_pct.unwrap_or(100))
        * u64::from(steps);
    u32::try_from(div_half_up(num, 100 * 100 * 100)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_examples() {
        // infantry 60: 6 per step at 100%, 9 at 150%, 12 at 200%
        assert_eq!(price(60, 3, 10, 100, None), 18);
        assert_eq!(price(60, 1, 10, 150, None), 9);
        assert_eq!(price(60, 1, 10, 200, None), 12);
        // elite +20%: 7.2 per step → 2 steps 14.4 → 14
        assert_eq!(price(60, 2, 10, 100, Some(120)), 14);
        // 7.5 rounds half up
        assert_eq!(price(50, 1, 10, 150, None), 8);
    }
}
