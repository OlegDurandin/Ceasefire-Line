// R-UI-02, as on the approved mock (board «Окно атаки в контексте карты»):
// reachable hexes green with the range border, the selected unit's hex
// yellow, attack targets hatched red-orange (the tokens draw their own
// halos); R-MOVE-05 transfer targets in blue.

import { Graphics } from "pixi.js";

import { hexCenter, hexCorners, offsetToAxial, type Point } from "../hex";
import { reachBorder } from "./layers";

const REACH = 0x6caa48;
const REACH_EDGE = 0x468c28;
const TARGET = 0xc4401e;
const HATCH = 0xd65c28;
const SELECT = 0xffd76a;
const SELECT_EDGE = 0xe8b93c;
const TRANSFER = 0x4a90d9;

const flat = (pts: Point[]) => pts.flatMap((p) => [p.x, p.y]);
const center = (col: number, row: number, size: number) => hexCenter(offsetToAxial({ col, row }), size);

/** SC UI item 2: the path of the move under the pointer, from the unit's hex. */
export function drawPath(g: Graphics, size: number, from: { col: number; row: number } | null, path: [number, number][]) {
  g.clear();
  if (!from || !path.length) return;
  const pts = [center(from.col, from.row, size), ...path.map(([c, r]) => center(c, r, size))];
  g.moveTo(pts[0]!.x, pts[0]!.y);
  for (const p of pts.slice(1)) g.lineTo(p.x, p.y);
  g.stroke({ width: size * 0.16, color: 0x1c1a17, alpha: 0.45, cap: "round", join: "round" });
  g.moveTo(pts[0]!.x, pts[0]!.y);
  for (const p of pts.slice(1)) g.lineTo(p.x, p.y);
  g.stroke({ width: size * 0.08, color: SELECT, cap: "round", join: "round" });
  const end = pts[pts.length - 1]!;
  g.circle(end.x, end.y, size * 0.14).fill(SELECT);
}

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
    g.poly(flat(hexCorners(center(h.col, h.row, size), size))).fill({ color: REACH, alpha: 0.3 }).stroke({ width: 1.5, color: REACH_EDGE, alpha: 0.75 });
  }
  // R-UI-02: the border of the movement range.
  if (reach.length) reachBorder(g, size, selected ? [...reach, selected] : reach, REACH_EDGE);
  if (selected) {
    g.poly(flat(hexCorners(center(selected.col, selected.row, size), size))).fill({ color: SELECT, alpha: 0.3 }).stroke({ width: 2.5, color: SELECT_EDGE });
  }
  for (const h of targets) {
    const c = center(h.col, h.row, size);
    const corners = hexCorners(c, size);
    g.poly(flat(corners)).fill({ color: HATCH, alpha: 0.22 });
    // Diagonal stripes clipped to the hex (inscribed circle).
    const r = size * 0.86;
    for (let d = -r; d <= r; d += size * 0.2) {
      const half = Math.sqrt(Math.max(0, r * r - d * d));
      const ox = c.x + d / Math.SQRT2;
      const oy = c.y + d / Math.SQRT2;
      g.moveTo(ox - half / Math.SQRT2, oy + half / Math.SQRT2).lineTo(ox + half / Math.SQRT2, oy - half / Math.SQRT2);
    }
    g.stroke({ width: size * 0.08, color: TARGET, alpha: 0.75 });
    g.poly(flat(corners)).stroke({ width: 2.5, color: TARGET });
  }
}
