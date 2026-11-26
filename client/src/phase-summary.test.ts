import { describe, expect, it } from "vitest";

import { offsetToAxial } from "./hex";
import { PhaseLog } from "./phase-summary";

const ax = (col: number, row: number) => offsetToAxial({ col, row });
const sides = new Map([
  [1, 0],
  [2, 1],
  [3, 1],
]);
const sideOf = (id: number) => sides.get(id);

describe("enemy phase summary (R-UI-04, key L)", () => {
  it("sums losses, destroyed units, hexes, objectives and NM of a phase", () => {
    const log = new PhaseLog();
    log.add([{ type: "phase_started", turn: 1, side: 1 }], sideOf);
    log.add(
      [
        { type: "combat_resolved", attacker: 2, defender: 1, attacker_losses: 1, defender_losses: 3 },
        { type: "bombarded", unit: 3, target: 1, losses: 2 },
        { type: "unit_destroyed", unit: 1 },
        { type: "hex_captured", hex: ax(4, 5), side: 1, from: 0 },
        { type: "objective_changed", hex: ax(4, 5), from: 0, to: 1 },
        { type: "national_morale", side: 0, change: -3, value: 90 },
        { type: "national_morale", side: 0, change: -2, value: 88 },
      ],
      sideOf,
    );
    expect(log.lastOpponent(0)).toBeNull(); // not finished yet
    log.add([{ type: "phase_started", turn: 2, side: 0 }], sideOf);
    const s = log.lastOpponent(0)!;
    expect(s.side).toBe(1);
    expect(s.losses).toEqual(new Map([[1, 1], [0, 5]]));
    expect(s.destroyed).toEqual([{ id: 1, side: 0 }]);
    expect(s.captured).toEqual([{ cell: [4, 5], by: 1, from: 0 }]);
    expect(s.objectives).toEqual([{ cell: [4, 5], from: 0, to: 1 }]);
    expect(s.nm.get(0)).toEqual({ change: -5, value: 88 });
    expect(log.lastOpponent(1)).toBeNull();
  });
});
