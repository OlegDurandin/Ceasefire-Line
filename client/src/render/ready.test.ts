import { describe, expect, it } from "vitest";

import { readiness } from "./units";

const u = { acted: false, can_move: true, ap_halves: 10, strikes_left: 1 };

describe("«может ещё действовать» (SC UI item 1)", () => {
  it("fresh until the unit acts", () => {
    expect(readiness(u)).toBe("fresh");
  });
  it("partial after acting while it can still move or strike", () => {
    expect(readiness({ ...u, acted: true })).toBe("partial");
    expect(readiness({ ...u, acted: true, can_move: false })).toBe("partial");
    expect(readiness({ ...u, acted: true, strikes_left: 0 })).toBe("partial");
  });
  it("none when it can neither move nor strike", () => {
    expect(readiness({ ...u, acted: true, ap_halves: 0 })).toBeNull();
    expect(readiness({ ...u, acted: true, can_move: false, strikes_left: 0 })).toBeNull();
  });
});
