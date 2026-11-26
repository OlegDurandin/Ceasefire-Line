// R-UI-05: following the AI's turn. Where the camera looks before a command,
// and the «следы хода» layer: moves, attacks and retreats of a phase.

import { axialToOffset, type Axial } from "./hex";

export type Cell = [number, number];

export type TrailMark =
  | { kind: "move"; side: number; path: Cell[] }
  | { kind: "attack"; side: number; from: Cell; to: Cell }
  | { kind: "retreat"; side: number; path: Cell[] };

interface UnitPos {
  id: number;
  side: number;
  col: number;
  row: number;
}

const cell = (h: unknown): Cell => {
  const o = axialToOffset(h as Axial);
  return [o.col, o.row];
};

/** The hex the camera should show before a command (null: nothing to show). */
export function focusOf(command: { type: string } & Record<string, unknown>, units: UnitPos[]): Cell | null {
  const at = (id: unknown) => {
    const u = units.find((x) => x.id === id);
    return u ? ([u.col, u.row] as Cell) : null;
  };
  switch (command.type) {
    case "move": {
      const path = command.path as unknown[] | undefined;
      return path?.length ? cell(path[path.length - 1]) : at(command.unit);
    }
    case "attack":
    case "bombard":
    case "air_strike":
    case "declare_attack":
      return at(command.target);
    case "transfer":
      return command.to ? cell(command.to) : at(command.unit);
    case "air_strike_airfield":
      return command.hex ? cell(command.hex) : null;
    default:
      return null;
  }
}

/** Trail marks of one command's events; `before` — units before the command. */
export function trailOf(events: ({ type: string } & Record<string, unknown>)[], before: UnitPos[]): TrailMark[] {
  const byId = new Map(before.map((u) => [u.id, u] as const));
  const out: TrailMark[] = [];
  for (const e of events) {
    const u = byId.get(e.unit as number);
    if (e.type === "unit_moved" && u) {
      out.push({ kind: "move", side: u.side, path: [[u.col, u.row], ...(e.path as unknown[]).map(cell)] });
    } else if (e.type === "transferred" && u) {
      out.push({ kind: "move", side: u.side, path: [[u.col, u.row], ...(e.path as unknown[]).map(cell)] });
    } else if (e.type === "advanced" && u) {
      // The advancing unit stands where it attacked from.
      out.push({ kind: "move", side: u.side, path: [[u.col, u.row], cell(e.to)] });
    } else if (e.type === "retreated" && u) {
      out.push({ kind: "retreat", side: u.side, path: [cell(e.from), cell(e.to)] });
    } else if (e.type === "combat_resolved" || e.type === "bombarded") {
      const a = byId.get((e.attacker ?? e.unit) as number);
      const d = byId.get((e.defender ?? e.target) as number);
      if (a && d) out.push({ kind: "attack", side: a.side, from: [a.col, a.row], to: [d.col, d.row] });
    }
  }
  return out;
}
