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
    #[error("not supported yet: {0}")]
    Unsupported(String),
    #[error("unknown unit {0}")]
    UnknownUnit(u32),
    #[error("unit {0} does not belong to the active side")]
    NotYourUnit(u32),
    #[error("illegal move of unit {unit}: {reason:?}")]
    IllegalMove { unit: u32, reason: MoveError },
    #[error("illegal attack by unit {unit}: {reason:?}")]
    IllegalAttack { unit: u32, reason: AttackError },
    #[error("unit {0} has no pending advance")]
    NoPendingAdvance(u32),
    #[error("illegal economy action: {0:?}")]
    IllegalEconomy(EconomyError),
    #[error("illegal strategic transfer of unit {unit}: {reason:?}")]
    IllegalTransfer { unit: u32, reason: TransferError },
    #[error("unit {unit} cannot be pinned to {hq}")]
    IllegalPin { unit: u32, hq: u32 },
    #[error("the game is over")]
    GameOver,
}

/// Why a strategic transfer is rejected (R-MOVE-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferError {
    Acted,
    StartsInZoc,
    /// No road route through own hexes outside enemy ZOC to a free destination.
    NoRoute,
    NotEnoughMpp,
}

/// Why a reinforcement or purchase is rejected (R-ECO-02, R-ECO-03, R-SUP-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EconomyError {
    ZeroSteps,
    /// The unit already moved, attacked or fired this phase.
    UnitActed,
    /// Supply below every reinforcement band.
    NoSupply,
    /// Over the band's steps per phase.
    StepLimit,
    FullStrength,
    NotEnoughMpp,
    /// Scenario force limit for this class reached (destroyed units count).
    PurchaseLimit,
}

/// Why a move is rejected (R-MOVE-01..04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    EmptyPath,
    /// Entered enemy ZOC this phase, attacked without exploitation, or no AP.
    CannotMove,
    NotAdjacent,
    Impassable,
    Occupied,
    /// Path continues after entering enemy ZOC.
    ZocStop,
    /// Direct step from one enemy ZOC hex to another.
    ZocToZoc,
    NotEnoughAp,
}

/// Why an attack or bombardment is rejected (R-MOVE-04, R-COMBAT-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackError {
    NoStrikes,
    NoAp,
    NotAdjacent,
    OutOfRange,
    NotEnemy,
    NotGroundTarget,
    /// Artillery attacks only by bombardment.
    UseBombard,
    CannotBombard,
}
