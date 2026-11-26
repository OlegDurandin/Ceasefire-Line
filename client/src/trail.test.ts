import { describe, expect, it } from "vitest";

import { offsetToAxial } from "./hex";
import { aiTiming, parseSettings } from "./settings";
import { focusOf, trailOf } from "./trail";

const ax = (col: number, row: number) => offsetToAxial({ col, row });
const units = [
  { id: 1, side: 0, col: 2, row: 3 },
  { id: 2, side: 1, col: 4, row: 3 },
];

describe("following the AI turn (R-UI-05)", () => {
  it("looks at the end of a move and at the target of an attack", () => {
    expect(focusOf({ type: "move", unit: 1, path: [ax(3, 3), ax(3, 4)] }, units)).toEqual([3, 4]);
    expect(focusOf({ type: "attack", unit: 1, target: 2 }, units)).toEqual([4, 3]);
    expect(focusOf({ type: "transfer", unit: 1, to: ax(9, 9) }, units)).toEqual([9, 9]);
    expect(focusOf({ type: "end_phase" }, units)).toBeNull();
  });

  it("leaves trail marks for moves, attacks and retreats", () => {
    const marks = trailOf(
      [
        { type: "unit_moved", unit: 1, path: [ax(3, 3)] },
        { type: "combat_resolved", attacker: 1, defender: 2 },
        { type: "retreated", unit: 2, from: ax(4, 3), to: ax(5, 3) },
      ],
      units,
    );
    expect(marks).toEqual([
      { kind: "move", side: 0, path: [[2, 3], [3, 3]] },
      { kind: "attack", side: 0, from: [2, 3], to: [4, 3] },
      { kind: "retreat", side: 1, path: [[4, 3], [5, 3]] },
    ]);
  });

  it("speeds: ×1 / ×2 / ×4 / instant, kept with other settings", () => {
    expect(aiTiming("x1")).toEqual({ stepMs: 600, resultMs: 1100, follow: true });
    expect(aiTiming("x4").stepMs).toBe(150);
    expect(aiTiming("instant")).toEqual({ stepMs: 0, resultMs: 0, follow: false });
    expect(parseSettings('{"units":"medallion","aiSpeed":"x2"}')).toEqual({ units: "medallion", aiSpeed: "x2" });
    expect(parseSettings('{"aiSpeed":"x9"}')).toEqual({});
    expect(parseSettings("not json")).toEqual({});
  });
});
