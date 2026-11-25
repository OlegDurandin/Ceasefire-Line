import { describe, expect, it } from "vitest";

import { hexCenter, offsetToAxial } from "./hex";
import { hillshadePlacement } from "./hillshade";

describe("hillshade placement", () => {
  it("puts the image corner half a hex left of and above hex (0,0)", () => {
    const r = 5 / Math.sqrt(3);
    const meta = { png: "h.png", width_px: 100, height_px: 100, px_per_km: 20, hex_radius_km: r, top_left_from_hex00_km: [-r, (-r * Math.sqrt(3)) / 2] as [number, number] };
    const p = hillshadePlacement(meta, 48);
    expect(p.x).toBeCloseTo(-48);
    expect(p.y).toBeCloseTo((-48 * Math.sqrt(3)) / 2);
    // 5 km between neighbour centres = 100 image px → same distance in world px.
    const c0 = hexCenter(offsetToAxial({ col: 0, row: 0 }), 48);
    const c1 = hexCenter(offsetToAxial({ col: 0, row: 1 }), 48);
    expect(100 * p.scale).toBeCloseTo(Math.hypot(c1.x - c0.x, c1.y - c0.y));
  });
});
