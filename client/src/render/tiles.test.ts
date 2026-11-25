import { describe, expect, it } from "vitest";

import { pickLevel, worldPerBakePx, type TilesMeta } from "./tiles";

const meta: TilesMeta = {
  format: "webp",
  tile_px: 512,
  px_per_km: 24,
  hex_radius_km: 2.886751345948129,
  top_left_from_hex00_km: [-2.886751345948129, -2.5],
  width_px: 2350,
  height_px: 2269,
  levels: [0, 1, 2, 3].map((z) => ({ z, width: 1, height: 1, cols: 1, rows: 1 })),
};

describe("baked tiles (R-UI-01)", () => {
  it("maps level-0 pixels to world pixels", () => {
    // 48 px hex radius / 2.887 km = 16.63 world px per km; 24 bake px per km.
    expect(worldPerBakePx(meta, 48)).toBeCloseTo(0.6928, 4);
  });

  it("picks finer levels when zooming in and clamps to the pyramid", () => {
    expect(pickLevel(meta, 48, 3)).toBe(0);
    expect(pickLevel(meta, 48, 1)).toBe(0);
    expect(pickLevel(meta, 48, 0.6)).toBe(1);
    expect(pickLevel(meta, 48, 0.25)).toBe(2);
    expect(pickLevel(meta, 48, 0.01)).toBe(3);
  });
});
