//! R-TECH-02: tech boosts bought for MPP.

use serde::{Deserialize, Serialize};

use crate::error::{EconomyError, RuleError};
use crate::side::SideId;
use crate::tech::TechBranch;

use super::{Event, GameState};

/// A boost offered by the scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechBoost {
    pub id: String,
    pub side: SideId,
    pub branch: TechBranch,
    pub cost: u32,
    /// Delay in turns from purchase to the level-up.
    pub turns: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingBoost {
    pub id: String,
    pub side: SideId,
    pub branch: TechBranch,
    pub ready_turn: u32,
}

fn refuse(reason: EconomyError) -> RuleError {
    RuleError::IllegalEconomy(reason)
}

impl GameState {
    /// R-TECH-02: once per scenario, by its own side, for MPP; the branch may
    /// end at most one level above its scenario start (pending boosts count).
    pub(super) fn buy_tech_boost(&mut self, id: &str) -> Result<Vec<Event>, RuleError> {
        let boost = self
            .tech_boosts
            .iter()
            .find(|b| b.id == id)
            .cloned()
            .ok_or(refuse(EconomyError::UnknownBoost))?;
        let side = self.active_side();
        if boost.side != side {
            return Err(refuse(EconomyError::NotYourBoost));
        }
        if self.bought_boosts.contains(id) {
            return Err(refuse(EconomyError::BoostAlreadyBought));
        }
        let start = self
            .start_tech
            .get(&side)
            .map_or(Ok(1), |t| t.level(boost.branch))?;
        let now = self.side_state(side)?.tech.level(boost.branch)?;
        let pending = self
            .pending_boosts
            .iter()
            .filter(|p| p.side == side && p.branch == boost.branch)
            .count() as u8;
        let above = self.data.rules.economy.boost_max_levels_above_start;
        // Level after this boost must stay within start + `above` and level 3.
        let after = now + pending + 1;
        if after > start.saturating_add(above) || after > 3 {
            return Err(refuse(EconomyError::BoostCap));
        }
        let state = self.side_state_mut(side)?;
        if state.mpp < boost.cost {
            return Err(refuse(EconomyError::NotEnoughMpp));
        }
        state.mpp -= boost.cost;
        let ready_turn = self.turn + boost.turns;
        self.bought_boosts.insert(boost.id.clone());
        self.pending_boosts.push(PendingBoost {
            id: boost.id.clone(),
            side,
            branch: boost.branch,
            ready_turn,
        });
        Ok(vec![Event::TechBoostBought {
            id: boost.id,
            side,
            cost: boost.cost,
            ready_turn,
        }])
    }

    /// R-TECH-02: at the start of the owner's phase on or after `ready_turn`.
    pub(super) fn finish_boosts(
        &mut self,
        side: SideId,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        let turn = self.turn;
        let (due, rest): (Vec<PendingBoost>, Vec<PendingBoost>) =
            std::mem::take(&mut self.pending_boosts)
                .into_iter()
                .partition(|p| p.side == side && p.ready_turn <= turn);
        self.pending_boosts = rest;
        for p in due {
            let level = self.side_state_mut(side)?.tech.raise(p.branch)?;
            events.push(Event::TechAdvanced {
                side,
                branch: p.branch,
                level,
                boost: p.id,
            });
        }
        Ok(())
    }
}
