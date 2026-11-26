//! The baseline player («жадина», stage 4 step 1): the yardstick the real AI
//! must beat. Each call returns one command for the active side:
//!
//! 1. take a vacated hex when the core offers it (R-COMBAT-06);
//! 2. the attack or bombardment with the best expected exchange from the
//!    core's own forecast (R-COMBAT-03), if it is favourable;
//! 3. move one unit that is not yet in contact towards the nearest enemy
//!    unit or enemy-held objective;
//! 4. end the phase.
//!
//! Every command it returns changes the state (strikes, AP or the `moved`
//! flag), so a phase always ends.

use ceasefire_core::game::{Command, GameState, Unit};
use ceasefire_core::hex::Hex;

/// One command for the active side.
pub fn greedy_next(s: &GameState) -> Command {
    let side = s.active_side();
    if let Some(p) = s.pending_advance() {
        if s.unit(p.unit).is_some_and(|u| u.side == side) {
            return Command::Advance { unit: p.unit };
        }
    }
    if let Some(cmd) = best_strike(s) {
        return cmd;
    }
    if let Some(cmd) = best_move(s) {
        return cmd;
    }
    Command::EndPhase
}

fn own_units(s: &GameState) -> Vec<&Unit> {
    let side = s.active_side();
    let mut v: Vec<&Unit> = s.units().filter(|u| u.side == side).collect();
    v.sort_by_key(|u| u.id);
    v
}

fn enemy_units(s: &GameState) -> Vec<&Unit> {
    let side = s.active_side();
    let mut v: Vec<&Unit> = s.units().filter(|u| u.side != side).collect();
    v.sort_by_key(|u| u.id);
    v
}

/// Expected enemy steps destroyed minus own steps lost; only favourable strikes.
fn best_strike(s: &GameState) -> Option<Command> {
    let mut best: Option<(f64, Command)> = None;
    for u in own_units(s).into_iter().filter(|u| u.strikes_left > 0) {
        for e in enemy_units(s) {
            let (score, cmd) = if let Ok(f) = s.forecast_attack(u.id, e.id) {
                (
                    f.defender_losses.expected - f.attacker_losses.expected,
                    Command::Attack {
                        unit: u.id,
                        target: e.id,
                    },
                )
            } else if let Ok(f) = s.forecast_bombard(u.id, e.id) {
                (
                    f.expected,
                    Command::Bombard {
                        unit: u.id,
                        target: e.id,
                    },
                )
            } else {
                continue;
            };
            // Strictly better only: the first (lowest ids) wins ties.
            if score > 0.0 && best.as_ref().is_none_or(|(b, _)| score > *b) {
                best = Some((score, cmd));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// Where a unit is heading: enemy units and objectives the side does not hold.
fn goals(s: &GameState) -> Vec<Hex> {
    let side = s.active_side();
    let mut g: Vec<Hex> = enemy_units(s).iter().map(|e| e.hex).collect();
    g.extend(
        s.objectives()
            .iter()
            .filter(|o| s.controller(o.hex) != Some(side))
            .map(|o| o.hex),
    );
    g.sort();
    g.dedup();
    g
}

fn distance_to(goals: &[Hex], h: Hex) -> u32 {
    goals
        .iter()
        .map(|g| h.distance(*g))
        .min()
        .unwrap_or(u32::MAX)
}

/// The first unit (by id) that can get closer to a goal, moved as close as it can.
fn best_move(s: &GameState) -> Option<Command> {
    let goals = goals(s);
    if goals.is_empty() {
        return None;
    }
    let enemies: Vec<Hex> = enemy_units(s).iter().map(|e| e.hex).collect();
    for u in own_units(s) {
        if u.moved || !u.can_move || u.ap_halves == 0 || u.hq_rating.is_some() {
            continue;
        }
        // In contact: hold the line.
        if enemies.iter().any(|e| e.distance(u.hex) == 1) {
            continue;
        }
        let Ok(reach) = s.reachable(u.id) else {
            continue;
        };
        let here = distance_to(&goals, u.hex);
        let target = reach
            .iter()
            .map(|(h, cost)| (distance_to(&goals, *h), *cost, *h))
            .min();
        let Some((d, _, to)) = target else {
            continue;
        };
        if d >= here {
            continue;
        }
        if let Ok(Some(path)) = s.path_to(u.id, to) {
            return Some(Command::Move { unit: u.id, path });
        }
    }
    None
}
