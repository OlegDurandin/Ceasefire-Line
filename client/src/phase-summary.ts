// R-UI-04 «Итоги фазы противника» (key L): what a finished phase did —
// steps lost by each side, units destroyed, hexes changing hands,
// objectives and national morale. Built from the core's events.

import { axialToOffset, type Axial } from "./hex";

export type Cell = [number, number];
type Ev = { type: string } & Record<string, unknown>;

export interface PhaseSummary {
  /** Whose phase it was. */
  side: number;
  turn: number;
  /** Steps lost per side (ground and air). */
  losses: Map<number, number>;
  /** Units destroyed: id and side (names are looked up when shown). */
  destroyed: { id: number; side: number }[];
  /** Hexes changing hands: where, who took it, from whom. */
  captured: { cell: Cell; by: number; from: number | null }[];
  objectives: { cell: Cell; from: number | null; to: number | null }[];
  /** Net NM change per side and its value at the end. */
  nm: Map<number, { change: number; value: number }>;
}

const cell = (h: unknown): Cell => {
  const o = axialToOffset(h as Axial);
  return [o.col, o.row];
};

function empty(side: number, turn: number): PhaseSummary {
  return { side, turn, losses: new Map(), destroyed: [], captured: [], objectives: [], nm: new Map() };
}

/** Collects events phase by phase; a new phase closes the previous summary. */
export class PhaseLog {
  private now: PhaseSummary | null = null;
  /** The last finished phase of each side. */
  readonly finished = new Map<number, PhaseSummary>();

  /** `sideOf`: side of a unit or aircraft id, as before the command. */
  add(events: Ev[], sideOf: (id: number) => number | undefined) {
    const lose = (id: unknown, steps: unknown) => {
      const side = sideOf(id as number);
      const n = Number(steps) || 0;
      if (this.now && side !== undefined && n > 0) this.now.losses.set(side, (this.now.losses.get(side) ?? 0) + n);
    };
    for (const e of events) {
      if (e.type === "phase_started") {
        if (this.now) this.finished.set(this.now.side, this.now);
        this.now = empty(e.side as number, e.turn as number);
        continue;
      }
      const s = this.now;
      if (!s) continue;
      switch (e.type) {
        case "combat_resolved":
          lose(e.attacker, e.attacker_losses);
          lose(e.defender, e.defender_losses);
          break;
        case "bombarded":
          lose(e.target, e.losses);
          break;
        case "air_strike":
          lose(e.target, e.losses);
          lose(e.unit, e.return_losses);
          break;
        case "air_battle":
          lose(e.a, e.a_losses);
          lose(e.b, e.b_losses);
          break;
        case "attrition":
        case "deserted":
          lose(e.unit, e.steps);
          break;
        case "retreat_blocked":
          lose(e.unit, e.extra_losses);
          break;
        case "unit_destroyed":
        case "aircraft_destroyed": {
          const side = sideOf(e.unit as number);
          if (side !== undefined) s.destroyed.push({ id: e.unit as number, side });
          break;
        }
        case "hex_captured":
          s.captured.push({ cell: cell(e.hex), by: e.side as number, from: (e.from as number | null) ?? null });
          break;
        case "objective_changed":
          s.objectives.push({ cell: cell(e.hex), from: (e.from as number | null) ?? null, to: (e.to as number | null) ?? null });
          break;
        case "national_morale": {
          const side = e.side as number;
          const was = s.nm.get(side)?.change ?? 0;
          s.nm.set(side, { change: was + (e.change as number), value: e.value as number });
          break;
        }
      }
    }
  }

  /** The last finished phase of a side other than `viewer` (the opponent's). */
  lastOpponent(viewer: number): PhaseSummary | null {
    let best: PhaseSummary | null = null;
    for (const [side, s] of this.finished) {
      if (side !== viewer && (!best || s.turn > best.turn || (s.turn === best.turn && s.side > best.side))) best = s;
    }
    return best;
  }
}
