//! Serializable commands and events: the basis for replays, AI, undo and online play.

use serde::{Deserialize, Serialize};

use crate::combat::{Exchange, FireParams};
use crate::hex::Hex;
use crate::side::SideId;

use super::UnitId;

/// The only way to change a [`super::GameState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// R-MOVE-01..04: move along `path` (hexes after the start, in order).
    Move { unit: UnitId, path: Vec<Hex> },
    /// R-COMBAT-02, R-MOVE-04: ground attack on an adjacent enemy unit.
    Attack { unit: UnitId, target: UnitId },
    /// R-COMBAT-04: artillery fire at range, no return fire.
    Bombard { unit: UnitId, target: UnitId },
    /// R-COMBAT-06: tank or mech brigade occupies the hex its target vacated.
    Advance { unit: UnitId },
    /// R-ECO-02, R-SUP-03: add steps to a unit that has not acted this phase.
    /// `elite` costs more and keeps experience.
    Reinforce {
        unit: UnitId,
        steps: u8,
        elite: bool,
    },
    /// R-ECO-02, R-ECO-03: buy a unit; it arrives after a delay.
    Purchase { class: String },
    /// R-MOVE-05: strategic transfer by road to `to`; the route is found by the engine.
    Transfer { unit: UnitId, to: Hex },
    /// R-HQ-01: pin a unit to an HQ (`None` removes the pin); applied at the
    /// start of the next own phase.
    PinToHq { unit: UnitId, hq: Option<UnitId> },
    /// R-MOVE-02: an engineer on the bank bridges the canal edge towards `toward`
    /// (also rebuilds a destroyed bridge).
    BuildBridge { unit: UnitId, toward: Hex },
    /// R-MOVE-02, R-COMBAT-04: artillery fires at the bridge on edge `a`–`b`.
    BombardBridge { unit: UnitId, a: Hex, b: Hex },
    /// R-TECH-02: buy a scenario tech boost.
    BuyTechBoost { id: String },
    /// Tactical battles: declare an attack and queue it for a result; the
    /// snapshot for the tactical level comes in `CombatDeclared`.
    DeclareAttack { unit: UnitId, target: UnitId },
    /// Apply a queued combat's result: `outcome` from the tactical level, or
    /// `None` to calculate automatically.
    ResolveCombat {
        combat: u32,
        outcome: Option<crate::tactical::TacticalOutcome>,
    },
    /// R-TURN-01: end of the active side's phase.
    EndPhase,
}

/// R-NM-01: why national morale changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NmReason {
    ObjectiveLost {
        hex: Hex,
    },
    ObjectiveTaken {
        hex: Hex,
    },
    UnitDestroyed {
        unit: UnitId,
        encircled: bool,
    },
    StepsLost {
        total: u32,
    },
    KeyHexHeld {
        hex: Hex,
    },
    HqDestroyed {
        hq: UnitId,
    },
    /// R-EVT-01.
    ScenarioEvent {
        id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VictoryLevel {
    Decisive,
    Victory,
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// R-NM-02: a side's NM fell to the sudden-death threshold.
    SuddenDeath,
    /// R-NM-02: last turn played; decided by victory points.
    Date,
}

/// R-NM-02: result of the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// `None` for a draw.
    pub winner: Option<SideId>,
    pub level: VictoryLevel,
    pub reason: EndReason,
    /// Victory points per side: objectives held + NM.
    pub vp: Vec<(SideId, i32)>,
}

/// Scenario problems that do not stop loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupWarning {
    /// R-HQ-02: line units but no HQ; all of them get the unattached penalty.
    NoHq,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Emitted by `GameState::new` only.
    SetupWarning {
        side: SideId,
        warning: SetupWarning,
    },
    PhaseStarted {
        turn: u32,
        side: SideId,
        night: bool,
    },
    /// R-SUP-01: captured source recovers towards its base value.
    SourceRecovered {
        hex: Hex,
        value: u8,
    },
    /// Start-of-phase recalculation (R-SUP-02, R-SUP-03, R-UNIT-01, R-UNIT-02).
    UnitRefreshed {
        unit: UnitId,
        supply: u8,
        morale_pct: u8,
        readiness_pct: u8,
        entrenchment: u8,
        ap_halves: u32,
    },
    /// R-SUP-03: steps lost to attrition at supply 0.
    Attrition {
        unit: UnitId,
        steps: u8,
    },
    UnitMoved {
        unit: UnitId,
        path: Vec<Hex>,
        ap_spent_halves: u32,
        /// R-MOVE-02: the last step crossed a canal by boat.
        by_boat: bool,
    },
    /// R-MOVE-06.
    HexCaptured {
        hex: Hex,
        side: SideId,
        from: Option<SideId>,
    },
    CombatResolved {
        attacker: UnitId,
        defender: UnitId,
        exchange: Exchange,
        attacker_losses: u8,
        defender_losses: u8,
        source: crate::tactical::CombatSource,
    },
    /// Tactical battles: a combat waits for its result.
    CombatDeclared {
        combat: u32,
        attacker: UnitId,
        defender: UnitId,
        snapshot: Box<crate::tactical::TacticalScenario>,
    },
    Bombarded {
        unit: UnitId,
        target: UnitId,
        params: FireParams,
        losses: u8,
    },
    /// R-COMBAT-06.
    Retreated {
        unit: UnitId,
        from: Hex,
        to: Hex,
    },
    /// R-COMBAT-06: no retreat hex, extra losses.
    RetreatBlocked {
        unit: UnitId,
        extra_losses: u8,
    },
    UnitDestroyed {
        unit: UnitId,
    },
    Advanced {
        unit: UnitId,
        to: Hex,
        by_boat: bool,
    },
    /// R-ECO-01.
    IncomeReceived {
        side: SideId,
        amount: u32,
        total: u32,
    },
    Reinforced {
        unit: UnitId,
        steps: u8,
        elite: bool,
        cost: u32,
    },
    Purchased {
        side: SideId,
        class: String,
        cost: u32,
        arrives_turn: u32,
    },
    UnitArrived {
        unit: UnitId,
        class: String,
        hex: Hex,
    },
    /// No free source to arrive in; tried again next own phase.
    ArrivalDelayed {
        side: SideId,
        class: String,
    },
    /// R-HQ-01: attachments of the active side at the start of its phase.
    HqAttached {
        links: Vec<(UnitId, UnitId)>,
    },
    /// R-HQ-02 (its NM loss comes as a `NationalMorale` event).
    HqDestroyed {
        hq: UnitId,
    },
    /// R-NM-01.
    NationalMorale {
        side: SideId,
        change: i32,
        value: u8,
        reason: NmReason,
    },
    /// R-NM-01: checked at the end of each phase.
    ObjectiveChanged {
        hex: Hex,
        from: Option<SideId>,
        to: Option<SideId>,
    },
    /// R-NM-02.
    GameOver {
        outcome: Outcome,
    },
    Transferred {
        unit: UnitId,
        path: Vec<Hex>,
        cost: u32,
    },
    Pinned {
        unit: UnitId,
        hq: Option<UnitId>,
    },
    /// R-MOVE-02: work started; the bridge is ready next own phase.
    BridgeStarted {
        unit: UnitId,
        a: Hex,
        b: Hex,
    },
    /// R-MOVE-02: start of the builder's phase; `completed` is false if the
    /// engineer was lost or left the bank.
    BridgeBuilt {
        unit: UnitId,
        a: Hex,
        b: Hex,
        completed: bool,
    },
    BridgeStrike {
        unit: UnitId,
        a: Hex,
        b: Hex,
        chance: f64,
        destroyed: bool,
    },
    /// R-EVT-01.
    ScenarioEventFired {
        id: String,
    },
    MppChanged {
        side: SideId,
        amount: i32,
        total: u32,
    },
    /// R-EVT-01: the ceasefire (last turn) moved.
    CeasefireMoved {
        last_turn: u32,
    },
    /// Localisation key of a message for the players.
    Message {
        key: String,
    },
    /// R-TECH-02.
    TechBoostBought {
        id: String,
        side: SideId,
        cost: u32,
        ready_turn: u32,
    },
    TechAdvanced {
        side: SideId,
        branch: crate::tech::TechBranch,
        level: u8,
        boost: String,
    },
    PhaseEnded {
        turn: u32,
        side: SideId,
    },
}
