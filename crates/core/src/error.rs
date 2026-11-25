use thiserror::Error;

/// Typed rule and data errors of the core.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RuleError {
    #[error("data parse error: {0}")]
    Parse(String),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("AP value {0} is not a non-negative multiple of 0.5")]
    InvalidAp(f64),
    #[error("unknown unit class `{0}`")]
    UnknownUnitClass(String),
    #[error("unknown terrain `{0}`")]
    UnknownTerrain(String),
    #[error("tech level {level} of branch {branch} is outside 1..=3")]
    InvalidTechLevel { branch: String, level: u8 },
    #[error("unit class `{0}` has no ground defense and cannot be attacked by ground units")]
    NoGroundDefense(String),
    #[error("hexes {0:?} and {1:?} are not adjacent")]
    NotAdjacent(crate::hex::Hex, crate::hex::Hex),
}
