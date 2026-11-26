//! Ceasefire Line computer opponent (R-AI-01, R-AI-02).
//!
//! A policy, not a plan for the whole phase: given the state, the AI picks
//! the next command; the core applies it and the AI sees the real result
//! (dice included) before choosing again. Pure and deterministic like the
//! core: no I/O, no clock, no global state; ties are broken by ids, and work
//! budgets are counted in operations, never in time.

pub mod greedy;
pub mod runner;

pub use greedy::greedy_next;
pub use runner::{play_phase, PhaseReport};
