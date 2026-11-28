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
  model?: string | null;
  formation: string | null;
  /** R-UI-04: the formation's name, a locale key (`formation.*`); null — show the class. */
  name: string | null;
  /** R-HQ-01: HQ the unit is attached to (set at the start of its phase). */
  attached_hq: number | null;
  /** R-HQ-01: manual pin, applied from the next own phase. */
  pinned_hq: number | null;
  /** R-UNIT-01 explanation. */
  base_readiness_pct: number;
  readiness_basis: { supply: number; morale_pct: number; exp_stars: number; hq_pct: number };
  attacks_made: number;
  attacks_received: number;
  crossing_penalty_pct: number;
  /** R-EVT-01: readiness penalty from a scenario event, until the next own phase. */
  event_penalty_pct: number;
  terrain: string;
  /** Supply of the unit's side in its hex now; 0 = encircled. */
  hex_supply: number;
  /** R-SUP-03 variant `pocket_desertion`: may lose a step at its next phase start. */
  desertion_risk?: boolean;
  /** R-AIR-03 umbrella radius in hexes, for SAM units. */
  sam_radius_hexes: number | null;
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
  bridges: { a: [number, number]; b: [number, number]; state: "built" | "destroyed" | "ferry" | "ferry_destroyed" }[];
  /** R-MOVE-06: neutral closed territory. */
  neutral: [number, number][];
  objectives: { hex: [number, number]; vp: number; nm_value: number; owner: number | null }[];
  /** R-AIR-01: aircraft, airfields and off-map bases. */
  air: AirUnitView[];
  airfields: [number, number][];
  offmap_bases: { id: string; side: number; edge: "north" | "south" | "east" | "west"; distance_km: number }[];
}

export type BaseView = { kind: "hex"; hex: [number, number] } | { kind: "off_map"; id: string };

export interface AirUnitView {
  id: number;
  class: string;
  side: number;
  base: BaseView;
  strength: number;
  readiness_pct: number;
  exp: number;
  fatigue_pct: number;
  sorties: number;
  rebased: boolean;
  nation: string | null;
  model?: string | null;
  name: string | null;
  radius_hexes: number;
}

/** R-AIR-02: what an aircraft can strike now. */
export type AirTarget = { kind: "unit"; target: number } | { kind: "airfield"; hex: [number, number] } | { kind: "bridge"; a: [number, number]; b: [number, number] };

export interface AirDuel {
  a: number;
  b: number;
  a_fire: FireParamsView;
  b_fire: FireParamsView;
}

export interface FireParamsView {
  n: number;
  a: number;
  d: number;
  p: number;
}

export interface AirForecast {
  escorts: number[];
  interceptors: number[];
  escort_duels: AirDuel[];
  group_duels: AirDuel[];
  strike: FireParamsView;
  strike_losses: { expected: number; low: number; high: number };
  return_fire: FireParamsView | null;
}

/** Game.economy(): what the active side can buy (R-ECO-02, R-ECO-03, R-TECH-02). */
export interface EconomyView {
  side: number;
  mpp: number;
  base_income: number;
  purchasable: { class: string; cost: number; left: number; refused: string | null }[];
  /** (class, turn, chosen source [col,row] or null). */
  arrivals: [string, number, [number, number] | null][];
  /** Variant `arrival_choice`: sources a purchase may arrive at (empty when off). */
  arrival_choices: [number, number][];
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
