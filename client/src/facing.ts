// R-UI-03: which way a unit's model faces. Pictures face left; a side's
// default (scenario.json `sides[].facing`) points at the enemy. After a move
// the unit faces along its last step, after an attack or bombardment towards
// its target — by the sign of dx. Flat-top hexes: screen x = 1.5·size·q, so
// the sign of dx is the sign of Δq (axial q = offset column).

export type Facing = "left" | "right";

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
