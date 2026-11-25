// R-UI-02: reachable hexes in green, attack targets with orange hatching,
// the selected unit's hex outlined.

import { Graphics } from "pixi.js";

import { hexCenter, hexCorners, offsetToAxial, type Point } from "../hex";

const REACH = 0x4caf50;
const TARGET = 0xf08c2c;
const SELECT = 0xf2e3b3;

const flat = (pts: Point[]) => pts.flatMap((p) => [p.x, p.y]);
const center = (col: number, row: number, size: number) => hexCenter(offsetToAxial({ col, row }), size);

export function drawOverlays(
  g: Graphics,
  size: number,
  reach: { col: number; row: number }[],
  targets: { col: number; row: number }[],
  selected: { col: number; row: number } | null,
) {
  g.clear();
  for (const h of reach) {
    g.poly(flat(hexCorners(center(h.col, h.row, size), size * 0.94))).fill({ color: REACH, alpha: 0.28 });
  }
  for (const h of targets) {
    const c = center(h.col, h.row, size);
    g.poly(flat(hexCorners(c, size * 0.94))).fill({ color: TARGET, alpha: 0.12 }).stroke({ width: 3, color: TARGET });
    // Diagonal hatching inside the inscribed circle.
    const r = size * 0.8;
    for (const d of [-0.6, -0.3, 0, 0.3, 0.6].map((k) => k * r)) {
      const h2 = Math.sqrt(r * r - d * d);
      const ox = c.x + d / Math.SQRT2;
      const oy = c.y + d / Math.SQRT2;
      g.moveTo(ox - h2 / Math.SQRT2, oy + h2 / Math.SQRT2).lineTo(ox + h2 / Math.SQRT2, oy - h2 / Math.SQRT2);
    }
    g.stroke({ width: 2, color: TARGET, alpha: 0.8 });
  }
  if (selected) {
    g.poly(flat(hexCorners(center(selected.col, selected.row, size), size * 0.96))).stroke({ width: 3, color: SELECT });
  }
}
