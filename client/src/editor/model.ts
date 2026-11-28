// Editing map.json and scenario.json documents in place (stage 2 editor).
// Pure functions on parsed JSON; the core validates the result.

export type Off = [number, number];
export type Edge = [Off, Off];

export interface MapDoc {
  _comment?: string;
  width: number;
  height: number;
  legend: Record<string, { terrain: string; pass?: boolean }>;
  terrain: string[];
  control_legend: Record<string, number>;
  control: string[];
  roads: Off[][];
  /** R-MOVE-01: dirt roads, hex polylines like `roads`. */
  tracks?: Off[][];
  canals: Edge[];
  bridges: Edge[];
  destroyed_bridges?: Edge[];
  geo?: { bbox: [number, number, number, number]; hex_km: number; size_km?: [number, number]; [k: string]: unknown };
}

export interface UnitDoc {
  class: string;
  side: number;
  hex: Off;
  strength?: number;
  morale_pct: number;
  exp?: number;
  entrenchment?: number;
  hq_rating?: number | null;
  formation?: string | null;
  nation?: string | null;
  model?: string | null;
  [k: string]: unknown;
}

export interface ScenarioDoc {
  sides: { id: number; key: string; [k: string]: unknown }[];
  units: UnitDoc[];
  [k: string]: unknown;
}

const key = ([c, r]: Off) => `${c},${r}`;
export const edgeKey = (a: Off, b: Off) => {
  const [x, y] = key(a) < key(b) ? [a, b] : [b, a];
  return `${key(x)}|${key(y)}`;
};
const hasEdge = (list: Edge[] | undefined, a: Off, b: Off) => (list ?? []).some(([x, y]) => edgeKey(x, y) === edgeKey(a, b));
const withoutEdge = (list: Edge[] | undefined, a: Off, b: Off) => (list ?? []).filter(([x, y]) => edgeKey(x, y) !== edgeKey(a, b));

/** Road polylines → one 2-point polyline per hex side (what the editor writes). */
export function normalizeRoads(map: MapDoc) {
  map.roads = pairsOf(map.roads);
  if (map.tracks) map.tracks = pairsOf(map.tracks);
}

function pairsOf(lines: Off[][]): Off[][] {
  const seen = new Set<string>();
  const pairs: Off[][] = [];
  for (const line of lines) {
    for (let i = 0; i + 1 < line.length; i++) {
      const a = line[i]!;
      const b = line[i + 1]!;
      const k = edgeKey(a, b);
      if (!seen.has(k)) {
        seen.add(k);
        pairs.push([a, b]);
      }
    }
  }
  return pairs;
}

/** Legend character for a terrain (+ pass), adding one if needed. */
function legendChar(map: MapDoc, terrain: string, pass: boolean): string {
  for (const [ch, e] of Object.entries(map.legend)) if (e.terrain === terrain && !!e.pass === pass) return ch;
  const used = new Set(Object.keys(map.legend));
  const pool = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
  const preferred = pass ? terrain[0]!.toUpperCase() : terrain[0]!;
  const ch = !used.has(preferred) ? preferred : [...pool].find((c) => !used.has(c))!;
  map.legend[ch] = pass ? { terrain, pass: true } : { terrain };
  return ch;
}

const setChar = (row: string, col: number, ch: string) => row.slice(0, col) + ch + row.slice(col + 1);

export function terrainAt(map: MapDoc, [col, row]: Off): { terrain: string; pass: boolean } | null {
  const e = map.legend[map.terrain[row]?.[col] ?? ""];
  return e ? { terrain: e.terrain, pass: !!e.pass } : null;
}

export function paintTerrain(map: MapDoc, [col, row]: Off, terrain: string, pass = false) {
  if (row < 0 || row >= map.height || col < 0 || col >= map.width) return;
  map.terrain[row] = setChar(map.terrain[row]!, col, legendChar(map, terrain, pass));
}

/** R-MOVE-06: owner of a hex, `null` = nobody ('.'). */
export function paintControl(map: MapDoc, [col, row]: Off, side: number | null) {
  if (row < 0 || row >= map.height || col < 0 || col >= map.width) return;
  let ch = ".";
  if (side !== null) {
    const found = Object.entries(map.control_legend).find(([, s]) => s === side)?.[0];
    ch = found ?? String(side);
    map.control_legend[ch] = side;
  }
  map.control[row] = setChar(map.control[row]!, col, ch);
}

export type EdgeState = "none" | "road" | "track" | "canal" | "bridge" | "destroyed";

const onLines = (lines: Off[][] | undefined, a: Off, b: Off) =>
  (lines ?? []).some((l) => l.some((p, i) => i + 1 < l.length && edgeKey(p, l[i + 1]!) === edgeKey(a, b)));

export function edgeState(map: MapDoc, a: Off, b: Off): EdgeState {
  if (hasEdge(map.destroyed_bridges, a, b)) return "destroyed";
  if (hasEdge(map.bridges, a, b)) return "bridge";
  if (hasEdge(map.canals, a, b)) return "canal";
  if (onLines(map.roads, a, b)) return "road";
  return onLines(map.tracks, a, b) ? "track" : "none";
}

/** Next state of a hex side: none → road → dirt road → canal → bridge → destroyed bridge → none.
 *  A road and a canal on one side means a bridged crossing, so roads are kept on bridges. */
export function cycleEdge(map: MapDoc, a: Off, b: Off): EdgeState {
  normalizeRoads(map);
  const order: EdgeState[] = ["none", "road", "track", "canal", "bridge", "destroyed"];
  const next = order[(order.indexOf(edgeState(map, a, b)) + 1) % order.length]!;
  map.roads = map.roads.filter((l) => edgeKey(l[0]!, l[1]!) !== edgeKey(a, b));
  map.tracks = (map.tracks ?? []).filter((l) => edgeKey(l[0]!, l[1]!) !== edgeKey(a, b));
  map.canals = withoutEdge(map.canals, a, b);
  map.bridges = withoutEdge(map.bridges, a, b);
  map.destroyed_bridges = withoutEdge(map.destroyed_bridges, a, b);
  if (next === "road") map.roads.push([a, b]);
  if (next === "track") map.tracks.push([a, b]);
  if (next === "canal" || next === "bridge" || next === "destroyed") map.canals.push([a, b]);
  if (next === "bridge") map.bridges.push([a, b]);
  if (next === "destroyed") map.destroyed_bridges.push([a, b]);
  return next;
}

export function unitAt(scn: ScenarioDoc, [col, row]: Off): number {
  return scn.units.findIndex((u) => u.hex[0] === col && u.hex[1] === row);
}

/** Places a unit (replacing whatever stood there) or removes it with `null`. */
export function placeUnit(scn: ScenarioDoc, at: Off, unit: UnitDoc | null) {
  const i = unitAt(scn, at);
  if (i >= 0) scn.units.splice(i, 1);
  if (unit) scn.units.push({ ...unit, hex: [at[0], at[1]] });
}
