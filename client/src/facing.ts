// R-UI-03: which way a unit's model faces. Pictures face left; a side's
// default (scenario.json `sides[].facing`) points at the enemy. After a move
// the unit faces along its last step, after an attack or bombardment towards
// its target — by the sign of dx. Flat-top hexes: screen x = 1.5·size·q, so
// the sign of dx is the sign of Δq (axial q = offset column).

import { distance, offsetToAxial } from "./hex";

export type Facing = "left" | "right";

/** Enemies farther than this (hexes) do not turn a unit. */
const FACE_RADIUS = 6;

/**
 * R-UI-03: a unit faces the enemy — the side (left or right) where the enemy
 * units near it are, the nearer ones weighing more (1/d²); with no enemy within
 * `FACE_RADIUS`, or as many on both sides, the side's default. Δq = Δcolumn
 * gives the screen direction on flat-top hexes.
 */
export function faceEnemy(
  u: { col: number; row: number; side: number },
  units: { col: number; row: number; side: number }[],
  fallback: Facing,
): Facing {
  const at = offsetToAxial(u);
  let pull = 0;
  for (const e of units) {
    if (e.side === u.side) continue;
    const d = distance(at, offsetToAxial(e));
    if (d === 0 || d > FACE_RADIUS) continue;
    pull += Math.sign(e.col - u.col) / (d * d);
  }
  if (pull > 1e-9) return "right";
  if (pull < -1e-9) return "left";
  return fallback;
}

type Ev = { type: string } & Record<string, unknown>;
type Hex = { q: number; r: number };

export function facingFrom(dq: number, current: Facing): Facing {
  if (dq > 0) return "right";
  if (dq < 0) return "left";
  return current;
}

/**
 * Updates `facing` from a command's events. `colBefore` gives each unit's
 * column before the command (start of a move, attacker and target positions).
 */
export function applyFacing(facing: Map<number, Facing>, events: Ev[], colBefore: (id: number) => number | undefined, fallback: (id: number) => Facing) {
  const turn = (id: number, from: number | undefined, to: number | undefined) => {
    if (from === undefined || to === undefined) return;
    facing.set(id, facingFrom(to - from, facing.get(id) ?? fallback(id)));
  };
  for (const e of events) {
    const unit = e.unit as number;
    switch (e.type) {
      case "unit_moved":
      case "transferred": {
        const path = e.path as Hex[];
        if (!path.length) break;
        const last = path[path.length - 1]!;
        const prev = path.length > 1 ? path[path.length - 2]!.q : colBefore(unit);
        turn(unit, prev, last.q);
        break;
      }
      case "advanced":
        turn(unit, colBefore(unit), (e.to as Hex).q);
        break;
      case "retreated":
        turn(unit, (e.from as Hex).q, (e.to as Hex).q);
        break;
      case "combat_resolved":
        turn(e.attacker as number, colBefore(e.attacker as number), colBefore(e.defender as number));
        break;
      case "bombarded":
        turn(unit, colBefore(unit), colBefore(e.target as number));
        break;
    }
  }
}
