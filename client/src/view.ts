// JSON returned by the core's Core.scenarioView (crates/wasm).

export interface HexView {
  col: number;
  row: number;
  q: number;
  r: number;
  terrain: string;
  pass: boolean;
  owner: number | null;
}

export interface EdgeView {
  a: [number, number];
  b: [number, number];
}

export interface BridgeView extends EdgeView {
  state: "built" | "destroyed" | "ferry" | "ferry_destroyed";
}

export interface UnitView {
  index: number;
  class: string;
  side: number;
  col: number;
  row: number;
  strength: number;
  exp: number;
  hq_rating: number | null;
  nation: string | null;
  model?: string | null;
}

export interface ScenarioView {
  hex_km: number | null;
  hexes: HexView[];
  roads: EdgeView[];
  /** R-MOVE-01: dirt roads. */
  tracks: EdgeView[];
  canals: EdgeView[];
  bridges: BridgeView[];
  sides: { id: number; tech: Record<string, number> }[];
  units: UnitView[];
  /** `validate()` output: `{kind, ...fields}`, hexes in offset coordinates. */
  issues: Record<string, unknown>[];
  /** R-MOVE-06: neutral closed territory. */
  neutral: [number, number][];
}
