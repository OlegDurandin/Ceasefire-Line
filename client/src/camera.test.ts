import { describe, expect, it } from "vitest";

import { MAX_SCALE, MIN_SCALE, clampView, fitView, freeArea, zoomAt } from "./camera";

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

describe("camera bounds", () => {
  const map = { x: -50, y: -50, w: 1000, h: 500 };
  const area = freeArea(1200, 800, { top: 70, right: 50, bottom: 230, left: 0 });

  it("free area is the screen minus the HUD", () => {
    expect(area).toEqual({ x: 0, y: 70, w: 1150, h: 500 });
  });

  it("fits the map into the free area and centres it", () => {
    const v = fitView(map, area);
    expect(v.scale).toBe(1);
    expect(v.x + map.x * v.scale).toBe(75); // (1150 - 1000) / 2
    expect(v.y + map.y * v.scale).toBe(70);
  });

  it("a bigger map cannot leave a gap; a smaller one stays inside", () => {
    // zoomed in (map 2000 × 1000 on screen): its left edge may not come past the area's left edge
    const big = clampView({ scale: 2, x: 500, y: 0 }, map, area);
    expect(big.x + map.x * 2).toBe(0);
    expect(clampView({ scale: 2, x: -5000, y: 0 }, map, area).x + (map.x + map.w) * 2).toBe(1150);
    // zoomed out (map 500 × 250): its edges stay within the area
    const small = clampView({ scale: 0.5, x: -900, y: 900 }, map, area);
    expect(small.x + map.x * 0.5).toBe(0);
    expect(small.y + (map.y + map.h) * 0.5).toBe(570);
  });
});
