// Map layers (GDD R-UI-02): supply heat map 0–10, enemy ZOC, HQ subordination
// lines, scenario objectives; the border of the movement range.

import { Container, Graphics, Text } from "pixi.js";

import { axialToOffset, hexCenter, hexCorners, offsetToAxial, sharedSide, type Point } from "../hex";

const flat = (pts: Point[]) => pts.flatMap((p) => [p.x, p.y]);
const center = (col: number, row: number, size: number) => hexCenter(offsetToAxial({ col, row }), size);
const DIRS: [number, number][] = [
  [1, 0],
  [1, -1],
  [0, -1],
  [-1, 0],
  [-1, 1],
  [0, 1],
];

/** R-SUP-02 bands: 0 none, 1–3 low (AP penalty), 4–6 middle, 7–10 good. */
export function supplyColor(v: number): number {
  if (v <= 3) return 0xc4473a;
  if (v <= 6) return 0xd9a23a;
  return 0x5f9e57;
}

export function drawSupply(size: number, cells: [number, number, number][]): Container {
  const layer = new Container();
  const g = new Graphics();
  layer.addChild(g);
  for (const [col, row, v] of cells) {
    g.poly(flat(hexCorners(center(col, row, size), size * 0.97))).fill({ color: supplyColor(v), alpha: 0.32 });
    const t = new Text({ text: String(v), style: { fill: 0xf4efe2, fontSize: size * 0.3, fontWeight: "700", stroke: { color: 0x1c1a17, width: 3 } } });
    t.anchor.set(0.5);
    const c = center(col, row, size);
    t.position.set(c.x, c.y - size * 0.62);
    layer.addChild(t);
  }
  return layer;
}

/** R-MOVE-03: hexes in the enemy's ZOC: red wash and an inner red rim. */
export function drawZoc(size: number, hexes: [number, number][]): Graphics {
  const g = new Graphics();
  for (const [col, row] of hexes) {
    const c = center(col, row, size);
    g.poly(flat(hexCorners(c, size * 0.97))).fill({ color: 0xb83a2e, alpha: 0.2 });
    g.poly(flat(hexCorners(c, size * 0.86))).stroke({ width: size * 0.05, color: 0xe0453a, alpha: 0.75 });
  }
  return g;
}

/** R-HQ-01: a line from each attached unit to its HQ, in the side's colour. */
export function drawHqLinks(
  size: number,
  units: { col: number; row: number; side: number; id: number; attached_hq: number | null }[],
  colors: number[],
): Graphics {
  const g = new Graphics();
  const byId = new Map(units.map((u) => [u.id, u]));
  for (const u of units) {
    const hq = u.attached_hq === null ? undefined : byId.get(u.attached_hq);
    if (!hq) continue;
    const a = center(u.col, u.row, size);
    const b = center(hq.col, hq.row, size);
    g.moveTo(a.x, a.y).lineTo(b.x, b.y).stroke({ width: size * 0.09, color: 0x1c1a17, alpha: 0.6, cap: "round" });
    g.moveTo(a.x, a.y).lineTo(b.x, b.y).stroke({ width: size * 0.05, color: colors[u.side] ?? 0xb08d57, cap: "round" });
    g.circle(a.x, a.y, size * 0.07).fill(colors[u.side] ?? 0xb08d57);
  }
  return g;
}

/** R-NM-01: objective badges — a star in the holder's colour and its VP. */
export function drawObjectives(size: number, objectives: { hex: [number, number]; vp: number; owner: number | null }[], colors: number[]): Container {
  const layer = new Container();
  for (const o of objectives) {
    const c = center(o.hex[0], o.hex[1], size);
    const x = c.x - size * 0.62;
    const y = c.y - size * 0.55;
    const color = o.owner === null ? 0x8a8a86 : (colors[o.owner] ?? 0xb08d57);
    layer.addChild(
      new Graphics()
        .circle(x, y, size * 0.3)
        .fill(0x1c1a17)
        .stroke({ width: size * 0.06, color })
        .star(x, y, 5, size * 0.21, size * 0.1)
        .fill(0xe8c25a),
    );
    const t = new Text({ text: `${o.vp}`, style: { fill: 0xf4efe2, fontSize: size * 0.3, fontWeight: "800", stroke: { color: 0x1c1a17, width: 3 } } });
    t.anchor.set(0.5, 0);
    t.position.set(x, y + size * 0.3);
    layer.addChild(t);
  }
  return layer;
}

/** R-UI-02: the border of the movement range (sides towards hexes outside it). */
export function reachBorder(g: Graphics, size: number, reach: { col: number; row: number }[], color: number) {
  const inside = new Set(reach.map((h) => `${h.col},${h.row}`));
  for (const h of reach) {
    const ax = offsetToAxial(h);
    for (const [dq, dr] of DIRS) {
      const n = { q: ax.q + dq, r: ax.r + dr };
      const o = axialToOffset(n);
      if (inside.has(`${o.col},${o.row}`)) continue;
      const [p, q] = sharedSide(ax, n, size);
      g.moveTo(p.x, p.y).lineTo(q.x, q.y);
    }
  }
  g.stroke({ width: size * 0.06, color, cap: "round" });
}
