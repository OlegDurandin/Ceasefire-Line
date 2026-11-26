// R-UI-02: reachable hexes in green with the range border, attack targets
// with a red ring (R-UI-03), the selected unit with a bright ring; R-MOVE-05
// transfer targets in blue.

import { Graphics } from "pixi.js";

import { hexCenter, hexCorners, offsetToAxial, type Point } from "../hex";
import { reachBorder } from "./layers";
import { DISK } from "./units";

const REACH = 0x4caf50;
const TARGET = 0xe0453a;
const SELECT = 0xffe27a;
const TRANSFER = 0x4a90d9;

const flat = (pts: Point[]) => pts.flatMap((p) => [p.x, p.y]);
const center = (col: number, row: number, size: number) => hexCenter(offsetToAxial({ col, row }), size);

export function drawOverlays(
  g: Graphics,
  size: number,
  reach: { col: number; row: number }[],
  targets: { col: number; row: number }[],
  selected: { col: number; row: number } | null,
  transfer: { col: number; row: number }[] = [],
) {
  g.clear();
  for (const h of transfer) {
    g.poly(flat(hexCorners(center(h.col, h.row, size), size * 0.94))).fill({ color: TRANSFER, alpha: 0.3 }).stroke({ width: 2, color: TRANSFER });
  }
  for (const h of reach) {
    g.poly(flat(hexCorners(center(h.col, h.row, size), size * 0.94))).fill({ color: REACH, alpha: 0.28 });
  }
  // R-UI-02: the border of the movement range.
  if (reach.length) reachBorder(g, size, selected ? [...reach, selected] : reach, REACH);
  // R-UI-03: a red ring around each target's disk, a bright one around the selected unit.
  for (const h of targets) {
    const c = center(h.col, h.row, size);
    g.poly(flat(hexCorners(c, size * 0.94))).fill({ color: TARGET, alpha: 0.12 });
    diskRing(g, c, size, TARGET);
  }
  if (selected) diskRing(g, center(selected.col, selected.row, size), size, SELECT);
}

/** A ring just outside a unit's disk (drawn under the unit, so it shows around it). */
function diskRing(g: Graphics, c: Point, size: number, color: number) {
  const y = c.y + (DISK.drop + DISK.h / 2) * size;
  g.ellipse(c.x, y, (DISK.rx + 0.1) * size, (DISK.ry + 0.1) * size).stroke({ width: size * 0.08, color });
}
