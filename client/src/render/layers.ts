// Map layers (GDD R-UI-02): supply heat map 0–10, enemy ZOC, HQ subordination
// lines, AA umbrellas, scenario objectives; the border of the movement range.

import { Container, Graphics, Text } from "pixi.js";

import { axialToOffset, hexCenter, hexCorners, offsetToAxial, sharedSide, type Point } from "../hex";
import type { TrailMark } from "../trail";

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

/** R-AIR-03: SAM umbrellas — a circle of each SAM unit's radius (after technology). */
export function drawUmbrellas(
  size: number,
  units: { col: number; row: number; side: number; sam_radius_hexes: number | null }[],
  colors: number[],
): Graphics {
  const g = new Graphics();
  // Neighbouring hex centres are √3·size apart.
  const perHex = Math.sqrt(3) * size;
  for (const u of units) {
    if (!u.sam_radius_hexes) continue;
    const c = center(u.col, u.row, size);
    const color = colors[u.side] ?? 0xb08d57;
    g.circle(c.x, c.y, u.sam_radius_hexes * perHex).fill({ color, alpha: 0.1 }).stroke({ width: size * 0.06, color, alpha: 0.9 });
  }
  return g;
}

/** R-UI-05 «следы хода»: moves (lines with a dot at the end), attacks (red
 * arrows) and retreats (dashed) of the last phase, in the side's colour. */
export function drawTrail(size: number, marks: TrailMark[], colors: number[]): Graphics {
  const g = new Graphics();
  const pt = ([c, r]: [number, number]) => center(c, r, size);
  for (const m of marks) {
    const color = colors[m.side] ?? 0xb08d57;
    if (m.kind === "attack") {
      const a = pt(m.from);
      const b = pt(m.to);
      const k = 0.62; // stop short of the target's centre
      const tip = { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
      g.moveTo(a.x, a.y).lineTo(tip.x, tip.y).stroke({ width: size * 0.1, color: 0x1c1a17, alpha: 0.6, cap: "round" });
      g.moveTo(a.x, a.y).lineTo(tip.x, tip.y).stroke({ width: size * 0.06, color: 0xc4401e, cap: "round" });
      const ang = Math.atan2(b.y - a.y, b.x - a.x);
      const h = size * 0.22;
      g.poly([
        tip.x + Math.cos(ang) * h, tip.y + Math.sin(ang) * h,
        tip.x + Math.cos(ang + 2.4) * h, tip.y + Math.sin(ang + 2.4) * h,
        tip.x + Math.cos(ang - 2.4) * h, tip.y + Math.sin(ang - 2.4) * h,
      ]).fill(0xc4401e);
      continue;
    }
    const pts = m.path.map(pt);
    if (pts.length < 2) continue;
    const dashed = m.kind === "retreat";
    for (let i = 1; i < pts.length; i++) {
      const a = pts[i - 1]!;
      const b = pts[i]!;
      if (dashed) {
        for (let t = 0; t < 1; t += 0.25) {
          const u = Math.min(1, t + 0.14);
          g.moveTo(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t).lineTo(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u);
        }
        g.stroke({ width: size * 0.06, color, alpha: 0.9, cap: "round" });
      } else {
        g.moveTo(a.x, a.y).lineTo(b.x, b.y).stroke({ width: size * 0.09, color: 0x1c1a17, alpha: 0.5, cap: "round" });
        g.moveTo(a.x, a.y).lineTo(b.x, b.y).stroke({ width: size * 0.05, color, alpha: 0.95, cap: "round" });
      }
    }
    const end = pts[pts.length - 1]!;
    g.circle(end.x, end.y, size * 0.09).fill(color);
  }
  return g;
}

/** R-AI-01 «Намерения ИИ»: the AI's last plan. */
export interface AiPlanView {
  pockets: { hexes: [number, number][]; value: number; cut: number[] }[];
  envelop: [number, [number, number]][];
  hold: number[];
  pin: [number, [number, number]][];
}

/** Chokepoints (yellow rings with V), arrows from the units sent there,
 * holders (yellow dot), pinning units (thin lines) and cut-off units (red rings). */
export function drawIntent(size: number, plan: AiPlanView, units: { id: number; col: number; row: number }[]): Container {
  const layer = new Container();
  const g = new Graphics();
  layer.addChild(g);
  const at = new Map(units.map((u) => [u.id, center(u.col, u.row, size)] as const));
  for (const p of plan.pockets) {
    for (const id of p.cut) {
      const c = at.get(id);
      if (c) g.circle(c.x, c.y, size * 0.62).stroke({ width: size * 0.06, color: 0xd2402a, alpha: 0.9 });
    }
    for (const [col, row] of p.hexes) {
      const c = center(col, row, size);
      g.circle(c.x, c.y, size * 0.7).stroke({ width: size * 0.1, color: 0x1c1a17, alpha: 0.6 });
      g.circle(c.x, c.y, size * 0.7).stroke({ width: size * 0.06, color: 0xf2c94c });
      const label = new Text({ text: String(Math.round(p.value)), style: { fontFamily: "sans-serif", fontSize: size * 0.32, fill: 0xf2c94c, stroke: { color: 0x1c1a17, width: size * 0.06 } } });
      label.anchor.set(0.5);
      label.position.set(c.x, c.y - size * 0.95);
      layer.addChild(label);
    }
  }
  for (const [id, [col, row]] of plan.envelop) {
    const a = at.get(id);
    if (!a) continue;
    const b = center(col, row, size);
    g.moveTo(a.x, a.y).lineTo(b.x, b.y).stroke({ width: size * 0.09, color: 0x1c1a17, alpha: 0.5, cap: "round" });
    g.moveTo(a.x, a.y).lineTo(b.x, b.y).stroke({ width: size * 0.05, color: 0xf2c94c, cap: "round" });
    g.circle(b.x, b.y, size * 0.12).fill(0xf2c94c);
  }
  for (const [id, [col, row]] of plan.pin) {
    const a = at.get(id);
    if (!a) continue;
    const b = center(col, row, size);
    g.moveTo(a.x, a.y).lineTo(b.x, b.y).stroke({ width: size * 0.03, color: 0xe8d9b0, alpha: 0.7 });
  }
  for (const id of plan.hold) {
    const a = at.get(id);
    if (a) g.circle(a.x, a.y - size * 0.5, size * 0.12).fill(0xf2c94c);
  }
  return layer;
}
