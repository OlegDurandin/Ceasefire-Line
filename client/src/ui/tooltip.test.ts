import { describe, expect, it } from "vitest";
import { placeTip } from "./tooltip";

const view = { width: 1600, height: 1000 };
const tip = { width: 200, height: 40 };

describe("placeTip (R-UI-04)", () => {
  it("puts the bubble centred under an icon", () => {
    expect(placeTip({ left: 500, top: 760, right: 540, bottom: 800 }, tip, view)).toEqual({ left: 420, top: 808 });
  });
  it("flips above when there is no room below", () => {
    expect(placeTip({ left: 500, top: 950, right: 540, bottom: 990 }, tip, view).top).toBe(902);
  });
  it("goes to the left of the right-hand layer column", () => {
    const p = placeTip({ left: 1552, top: 116, right: 1588, bottom: 152 }, tip, view);
    expect(p).toEqual({ left: 1344, top: 114 });
  });
  it("stays inside the viewport", () => {
    const p = placeTip({ left: 0, top: 0, right: 20, bottom: 20 }, tip, view);
    expect(p.left).toBeGreaterThanOrEqual(8);
  });
});
