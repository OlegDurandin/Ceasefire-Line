import { describe, expect, it } from "vitest";

import { applyFacing, type Facing } from "./facing";

const cols: Record<number, number> = { 1: 3, 2: 4, 3: 6 };
const before = (id: number) => cols[id];

describe("unit facing (R-UI-03)", () => {
  it("defaults to the side's facing and turns along the last step", () => {
    const f = new Map<number, Facing>();
    applyFacing(f, [{ type: "unit_moved", unit: 1, path: [{ q: 4, r: 0 }, { q: 3, r: 1 }] }], before, () => "right");
    expect(f.get(1)).toBe("left");
    applyFacing(f, [{ type: "unit_moved", unit: 1, path: [{ q: 3, r: 2 }] }], before, () => "right");
    expect(f.get(1), "straight down (dq = 0) keeps the facing").toBe("left");
  });

  it("turns the attacker towards its target, bombardment too", () => {
    const f = new Map<number, Facing>([[3, "right"]]);
    applyFacing(f, [{ type: "combat_resolved", attacker: 3, defender: 2 }], before, () => "right");
    expect(f.get(3)).toBe("left");
    applyFacing(f, [{ type: "bombarded", unit: 1, target: 3 }], before, () => "left");
    expect(f.get(1)).toBe("right");
    expect(f.has(2), "the defender does not turn").toBe(false);
  });
});
