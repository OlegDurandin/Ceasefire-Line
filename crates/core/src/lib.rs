//! Ceasefire Line rules core.
//!
//! Pure and deterministic: no I/O, no system time, no global state.
//! Game data arrives as JSON strings parsed by [`data::GameData`].

pub mod air;
pub mod combat;
pub mod data;
pub mod error;
pub mod game;
pub mod hex;
pub mod map;
pub mod math;
pub mod movement;
pub mod prob;
pub mod rng;
pub mod scale;
pub mod scenario;
pub mod side;
pub mod supply;
pub mod tactical;
pub mod tech;
pub mod unit;

pub use error::{AttackError, BridgeError, EconomyError, MoveError, RuleError, TransferError};
