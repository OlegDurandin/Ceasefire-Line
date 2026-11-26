//! Playing a phase with a policy: ask for a command, apply it, repeat until
//! the policy ends the phase. A refused command or the step budget ends the
//! phase too, so a weak policy can never hang a game.

use ceasefire_core::error::RuleError;
use ceasefire_core::game::{Command, Event, GameState};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PhaseReport {
    /// Commands applied, the closing `EndPhase` included.
    pub commands: Vec<Command>,
    pub events: Vec<Event>,
    /// Commands the core refused (each ends the phase early).
    pub refused: u32,
}

/// Plays the active side's phase. `max_commands` bounds the work (R-AI-02:
/// a budget in operations, not in time).
pub fn play_phase(
    s: &mut GameState,
    mut policy: impl FnMut(&GameState) -> Command,
    max_commands: u32,
) -> Result<PhaseReport, RuleError> {
    let mut report = PhaseReport::default();
    for _ in 0..max_commands {
        if s.outcome().is_some() {
            return Ok(report);
        }
        let cmd = policy(s);
        let end = cmd == Command::EndPhase;
        match s.apply(cmd.clone()) {
            Ok(ev) => {
                report.commands.push(cmd);
                report.events.extend(ev);
                if end {
                    return Ok(report);
                }
            }
            Err(_) if !end => {
                report.refused += 1;
                break;
            }
            Err(e) => return Err(e),
        }
    }
    if s.outcome().is_none() {
        report.events.extend(s.apply(Command::EndPhase)?);
        report.commands.push(Command::EndPhase);
    }
    Ok(report)
}
