// Hex control (R-MOVE-06, R-UI-02): a translucent wash of the owner's colour
// over its hexes and a front line along sides where two sides' hexes meet;
// neutral closed territory under a grey veil (not a side colour).

import { Graphics } from "pixi.js";

import { axialToOffset, hexCenter, hexCorners, offsetToAxial, sharedSide, type Point } from "../hex";

const WASH_ALPHA = 0.14;
const NEUTRAL = 0x8a8a86;
const NEUTRAL_ALPHA = 0.5;
const NEUTRAL_EDGE = 0xd9d6cf;
const FRONT = 0xf4efe2;
const FRONT_EDGE = 0x1c1a17;

const flat = (pts: Point[]) => pts.flatMap((p) => [p.x, p.y]);
const DIRS: [number, number][] = [
  [1, 0],
  [1, -1],
  [0, -1],
  [-1, 0],
  [-1, 1],
  [0, 1],
];

/** Sides between hexes owned by different sides, each pair once. */
export function frontSides(owners: Map<string, number>): [[number, number], [number, number]][] {
  const out: [[number, number], [number, number]][] = [];
  for (const [key, side] of owners) {
    const [col, row] = key.split(",").map(Number) as [number, number];
    const ax = offsetToAxial({ col, row });
    for (const [dq, dr] of DIRS) {
      const n = axialToOffset({ q: ax.q + dq, r: ax.r + dr });
      const nk = `${n.col},${n.row}`;
      const other = owners.get(nk);
      if (other !== undefined && other !== side && key < nk) out.push([[col, row], [n.col, n.row]]);
    }
  }
  return out;
}

export function drawControl(g: Graphics, size: number, owners: Map<string, number>, colors: number[], neutral: [number, number][] = []) {
  g.clear();
  const closed = new Set(neutral.map(([c, r]) => `${c},${r}`));
  for (const [col, row] of neutral) {
    g.poly(flat(hexCorners(hexCenter(offsetToAxial({ col, row }), size), size))).fill({ color: NEUTRAL, alpha: NEUTRAL_ALPHA });
  }
  // Border of the neutral territory: sides towards hexes that are not neutral.
  for (const [col, row] of neutral) {
    const ax = offsetToAxial({ col, row });
    for (const [dq, dr] of DIRS) {
      const n = axialToOffset({ q: ax.q + dq, r: ax.r + dr });
      if (closed.has(`${n.col},${n.row}`)) continue;
      const [p, q] = sharedSide(ax, { q: ax.q + dq, r: ax.r + dr }, size);
      g.moveTo(p.x, p.y).lineTo(q.x, q.y);
    }
  }
  if (neutral.length) g.stroke({ width: size * 0.05, color: NEUTRAL_EDGE, alpha: 0.8, cap: "round" });
  for (const [key, side] of owners) {
    const [col, row] = key.split(",").map(Number) as [number, number];
    const color = colors[side];
    if (color === undefined) continue;
    g.poly(flat(hexCorners(hexCenter(offsetToAxial({ col, row }), size), size))).fill({ color, alpha: WASH_ALPHA });
  }
  const front = frontSides(owners);
  const seg = (w: number, color: number) => {
    for (const [a, b] of front) {
      const [p, q] = sharedSide(offsetToAxial({ col: a[0], row: a[1] }), offsetToAxial({ col: b[0], row: b[1] }), size);
      g.moveTo(p.x, p.y).lineTo(q.x, q.y);
    }
    g.stroke({ width: w, color, cap: "round", join: "round" });
  };
  seg(size * 0.16, FRONT_EDGE);
  seg(size * 0.08, FRONT);
}
