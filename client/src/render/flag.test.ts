import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { flagCss, flagShapes, flagSvg, parseFlag, type FlagBook } from "./flag";

const book = JSON.parse(readFileSync(new URL("../../../data/flags.json", import.meta.url), "utf8")) as FlagBook;

describe("flags drawn by code (R-UI-03, GDD section 14)", () => {
  it("the old stripe list still parses as horizontal stripes", () => {
    const f = parseFlag([["#ffffff", 1], ["#2b59c3", 3]])!;
    expect(f).toEqual({ stripes: [["#ffffff", 0.25], ["#2b59c3", 0.75]], vertical: false, emblem: null });
    expect(flagCss(f, "#999")).toBe("linear-gradient(#ffffff 0% 25%, #2b59c3 25% 100%)");
  });

  it("the three fictional nations have flags with emblems", () => {
    for (const id of ["merid", "salvera", "tarvia"]) {
      const f = parseFlag(id, book);
      expect(f, id).not.toBeNull();
      expect(f!.emblem, id).not.toBeNull();
    }
    expect(parseFlag("tarvia", book)!.vertical).toBe(true);
    expect(parseFlag("nowhere", book)).toBeNull();
  });

  it("vertical stripes run left to right; emblems sit in the centre", () => {
    const shapes = flagShapes(parseFlag("tarvia", book)!, 30, 20);
    expect(shapes.slice(0, 3).map((s) => (s.kind === "rect" ? [s.x, s.w] : null))).toEqual([[0, 9], [9, 12], [21, 9]]);
    expect(shapes[3]).toMatchObject({ kind: "circle", cx: 15, cy: 10 });
    const star = flagShapes({ stripes: [["#000000", 1]], vertical: false, emblem: { shape: "star", color: "#ffffff", size: 0.5 } }, 30, 20)[1]!;
    expect(star.kind).toBe("poly");
    if (star.kind === "poly") {
      expect(star.pts).toHaveLength(24);
      expect(star.pts[0]).toBeCloseTo(15);
      expect(star.pts[1]).toBeCloseTo(5);
    }
  });

  it("an emblem flag becomes an SVG background safe inside a style attribute", () => {
    const f = parseFlag("salvera", book)!;
    expect(flagSvg(f)).toContain('stroke="#e0b648"');
    const css = flagCss(f, "#999");
    expect(css.startsWith("url('data:image/svg+xml,")).toBe(true);
    expect(css).not.toContain('"');
  });
});
