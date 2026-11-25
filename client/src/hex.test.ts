import { describe, expect, it } from "vitest";

import { axialToOffset, distance, hexCenter, offsetToAxial, pixelToAxial, sharedSide } from "./hex";

describe("hex geometry (same conventions as crates/core)", () => {
  it("offset ↔ axial matches the Rust loader", () => {
    // Values from crates/core/src/scenario.rs offset_to_axial (odd-q).
    expect(offsetToAxial({ col: 2, row: 1 })).toEqual({ q: 2, r: 0 });
    expect(offsetToAxial({ col: 3, row: 2 })).toEqual({ q: 3, r: 1 });
    for (let col = 0; col < 8; col++)
      for (let row = 0; row < 8; row++) expect(axialToOffset(offsetToAxial({ col, row }))).toEqual({ col, row });
  });

  it("same row next column and same column next row are neighbours", () => {
    for (let col = 0; col < 6; col++)
      for (let row = 0; row < 6; row++) {
        const h = offsetToAxial({ col, row });
        expect(distance(h, offsetToAxial({ col: col + 1, row }))).toBe(1);
        expect(distance(h, offsetToAxial({ col, row: row + 1 }))).toBe(1);
      }
  });

  it("pixel → hex finds the hex whose centre was given", () => {
    for (let q = -3; q <= 3; q++)
      for (let r = -3; r <= 3; r++) expect(pixelToAxial(hexCenter({ q, r }, 48), 48)).toEqual({ q, r });
  });

  it("shared side lies halfway between neighbour centres", () => {
    const a = { q: 0, r: 0 };
    const b = { q: 1, r: 0 };
    const [p, q] = sharedSide(a, b, 48);
    const ca = hexCenter(a, 48);
    const cb = hexCenter(b, 48);
    const mid = { x: (p.x + q.x) / 2, y: (p.y + q.y) / 2 };
    expect(mid.x).toBeCloseTo((ca.x + cb.x) / 2);
    expect(mid.y).toBeCloseTo((ca.y + cb.y) / 2);
  });
});
