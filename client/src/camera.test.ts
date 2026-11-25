import { describe, expect, it } from "vitest";

import { MAX_SCALE, MIN_SCALE, zoomAt } from "./camera";

describe("camera zoom", () => {
  it("keeps the point under the cursor fixed", () => {
    const v = { x: 100, y: 50, scale: 1 };
    const z = zoomAt(v, 2, 300, 200);
    // World point under (300,200) before: ((300-100)/1, (200-50)/1) = (200,150).
    expect((300 - z.x) / z.scale).toBeCloseTo(200);
    expect((200 - z.y) / z.scale).toBeCloseTo(150);
  });

  it("clamps the scale", () => {
    expect(zoomAt({ x: 0, y: 0, scale: 1 }, 100, 0, 0).scale).toBe(MAX_SCALE);
    expect(zoomAt({ x: 0, y: 0, scale: 1 }, 0.001, 0, 0).scale).toBe(MIN_SCALE);
  });
});
