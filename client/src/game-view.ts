// JSON returned by Game.state() and friends (crates/wasm).

export interface LiveUnit {
  id: number;
  class: string;
  side: number;
  col: number;
  row: number;
  strength: number;
  readiness_pct: number;
  morale_pct: number;
  exp: number;
  entrenchment: number;
  supply: number;
  ap_halves: number;
  strikes_left: number;
  can_move: boolean;
  acted: boolean;
  hq_rating: number | null;
  nation: string | null;
  formation: string | null;
  /** R-HQ-01: HQ the unit is attached to (set at the start of its phase). */
  attached_hq: number | null;
  /** R-HQ-01: manual pin, applied from the next own phase. */
  pinned_hq: number | null;
}

export interface Outcome {
  winner: number | null;
  level: "decisive" | "victory" | "draw";
  reason: "sudden_death" | "date";
  vp: [number, number][];
}

export interface GameStateView {
  turn: number;
  active_side: number;
  night: boolean;
  last_turn: number | null;
  sides: { id: number; mpp: number; national_morale_pct: number; tech: Record<string, number> }[];
  units: LiveUnit[];
  control: [[number, number], number][];
  pending_advance: [number, [number, number]] | null;
  outcome: Outcome | null;
  bridges: { a: [number, number]; b: [number, number]; state: "built" | "destroyed" }[];
  /** R-MOVE-06: neutral closed territory. */
  neutral: [number, number][];
}

/** Game.economy(): what the active side can buy (R-ECO-02, R-ECO-03, R-TECH-02). */
export interface EconomyView {
  side: number;
  mpp: number;
  base_income: number;
  purchasable: { class: string; cost: number; left: number; refused: string | null }[];
  arrivals: [string, number][];
  boosts: { id: string; branch: string; cost: number; turns: number; ready_turn: number | null; bought: boolean; refused: string | null }[];
}

/** Game.bridgeTargets() (R-MOVE-02). */
export interface BridgeTarget {
  kind: "bombard" | "build";
  a: [number, number];
  b: [number, number];
}

/** Game.transferTargets() (R-MOVE-05). */
export interface TransferTargets {
  cost: number | null;
  hexes: [number, number][];
}

export interface Reach {
  col: number;
  row: number;
  cost_halves: number;
}

export interface Target {
  target: number;
  kind: "attack" | "bombard";
}

export interface LossForecast {
  expected: number;
  low: number;
  high: number;
}

export interface FireParams {
  n: number;
  a: number;
  d: number;
  p: number;
}

export type Forecast =
  | {
      kind: "attack";
      forecast: {
        exchange: { vs_defender: FireParams; vs_attacker: FireParams };
        defender_losses: LossForecast;
        attacker_losses: LossForecast;
      };
    }
  | { kind: "bombard"; defender_losses: LossForecast };
