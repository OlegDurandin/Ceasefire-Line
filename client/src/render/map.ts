// Map layer (GDD R-UI-02): terrain as world-anchored texture sheets blended
// softly across type borders, a grid that stays nearly invisible until a unit
// is selected, roads as smooth lines through hex centres, rivers/canals and
// bridges on hex sides.
//
// Terrain blending: the most common type is one opaque sheet under the whole
// map; every other type is a sheet covering the map drawn over it through a
// blurred mask of that type's hexes, so borders become soft transitions with
// no per-hex edges. (Additive blending of all masked sheets would be exact,
// but Pixi does not apply blend modes through alpha masks.) Hexes just
// outside the map edge are claimed by one inside neighbour each so types
// running off the edge do not fade there.

import {
  BlurFilter,
  Container,
  FillPattern,
  Graphics,
  Matrix,
  Rectangle,
  Sprite,
  type Renderer,
  type Texture,
} from "pixi.js";

import { axialToOffset, hexCenter, hexCorners, offsetToAxial, sharedSide, type Point } from "../hex";
import type { ScenarioView } from "../view";
import { chaikin, roadChains } from "./roads";

export interface MapTextures {
  terrain: Map<string, Texture>;
  bridge?: Texture;
  bridgeDestroyed?: Texture;
}

/** Texture pixels per world pixel for terrain patterns. */
const TERRAIN_TILE_SCALE = 0.25;
const FALLBACK_COLOR = 0x6b5d45;
/** Blur of the terrain masks, in hex radii (world units; the filter works in
 * mask pixels, hence × resolution): how wide a type border blends. */
const BLEND_BLUR = 0.18;
/** Largest mask texture side in pixels (big maps get coarser masks). */
const MASK_MAX_PX = 2048;
/** Grid opacity: idle and while a unit is selected. */
export const GRID_IDLE = 0.05;
export const GRID_ACTIVE = 0.3;
const GRID_LABEL = "grid";

const ROAD = 0xd8c39a;
const ROAD_EDGE = 0x3a2e22;
const TRACK = 0xd9c08a;
const WATER = 0x4a7a9a;

const flat = (pts: Point[]) => pts.flatMap((p) => [p.x, p.y]);
const DIRS: [number, number][] = [
  [1, 0],
  [1, -1],
  [0, -1],
  [-1, 0],
  [-1, 1],
  [0, 1],
];

function terrainLayer(view: ScenarioView, tex: MapTextures, size: number, renderer: Renderer): Container {
  const layer = new Container();
  const inside = new Set(view.hexes.map((h) => `${h.col},${h.row}`));
  // Each type's hexes plus the outside ring hexes it claims first.
  const byType = new Map<string, Point[]>();
  const claimed = new Set<string>();
  const add = (t: string, p: Point) => {
    let list = byType.get(t);
    if (!list) byType.set(t, (list = []));
    list.push(p);
  };
  for (const h of view.hexes) add(h.terrain, hexCenter(h, size));
  for (const h of view.hexes) {
    for (const [dq, dr] of DIRS) {
      const n = { q: h.q + dq, r: h.r + dr };
      const o = axialToOffset(n);
      const key = `${o.col},${o.row}`;
      if (inside.has(key) || claimed.has(key)) continue;
      claimed.add(key);
      add(h.terrain, hexCenter(n, size));
    }
  }
  const all = [...byType.values()].flat();
  const pad = size * 2;
  const x0 = Math.min(...all.map((p) => p.x)) - pad;
  const y0 = Math.min(...all.map((p) => p.y)) - pad;
  const x1 = Math.max(...all.map((p) => p.x)) + pad;
  const y1 = Math.max(...all.map((p) => p.y)) + pad;
  const frame = new Rectangle(x0, y0, x1 - x0, y1 - y0);
  const resolution = Math.min(0.5, MASK_MAX_PX / Math.max(frame.width, frame.height));

  const sheetOf = (id: string) => {
    const t = tex.terrain.get(id);
    let fill: FillPattern | { color: number } = { color: FALLBACK_COLOR };
    if (t) {
      const pattern = new FillPattern({ texture: t, repetition: "repeat" });
      pattern.setTransform(new Matrix().scale(TERRAIN_TILE_SCALE, TERRAIN_TILE_SCALE));
      fill = pattern;
    }
    return new Graphics().rect(frame.x, frame.y, frame.width, frame.height).fill(fill);
  };
  // Most common type first (ties by id, for a stable picture), then the rest.
  const order = [...byType.entries()].sort(([a, pa], [b, pb]) => pb.length - pa.length || (a < b ? -1 : 1));
  layer.addChild(sheetOf(order[0]![0]));
  for (const [id, centres] of order.slice(1)) {
    const sheet = sheetOf(id);
    const shape = new Graphics();
    // Slightly oversized hexes so no hairline gaps appear between them.
    for (const c of centres) shape.poly(flat(hexCorners(c, size * 1.01))).fill(0xffffff);
    shape.filters = [new BlurFilter({ strength: size * BLEND_BLUR * resolution, quality: 4 })];
    const holder = new Container();
    holder.addChild(shape);
    const maskTex = renderer.generateTexture({ target: holder, frame, resolution });
    shape.destroy();
    holder.destroy();
    const mask = new Sprite(maskTex);
    // Masks are rebuilt with the map (every editor change): free the textures.
    mask.on("destroyed", () => maskTex.destroy(true));
    mask.position.set(frame.x, frame.y);
    mask.scale.set(frame.width / maskTex.width, frame.height / maskTex.height);
    sheet.mask = mask;
    layer.addChild(mask, sheet);
  }
  const outline = new Graphics();
  for (const h of view.hexes) outline.poly(flat(hexCorners(hexCenter(h, size), size))).fill(0xffffff);
  layer.addChild(outline);
  layer.mask = outline;
  return layer;
}

/** `baked`: terrain, water and roads come from the baked tiles (mapgen bake);
 * only the grid and bridges (their state changes in play) are drawn here. */
export function drawMap(
  view: ScenarioView,
  tex: MapTextures,
  size: number,
  renderer: Renderer,
  baked = false,
  bridges = true,
): Container {
  const root = new Container();
  if (!baked) root.addChild(terrainLayer(view, tex, size, renderer));

  const grid = new Graphics();
  grid.label = GRID_LABEL;
  for (const h of view.hexes) {
    grid.poly(flat(hexCorners(hexCenter(h, size), size))).stroke({ width: 1, color: 0x000000 });
  }
  grid.alpha = GRID_IDLE;
  root.addChild(grid);

  const ax = ([col, row]: [number, number]) => offsetToAxial({ col, row });
  const roads = new Graphics();
  const chains = roadChains(view.roads).map((c) => chaikin(c.map((h) => hexCenter(ax(h), size))));
  const stroke = (width: number, color: number) => {
    for (const pts of chains) {
      roads.moveTo(pts[0]!.x, pts[0]!.y);
      for (const p of pts.slice(1)) roads.lineTo(p.x, p.y);
    }
    roads.stroke({ width, color, cap: "round", join: "round" });
  };
  stroke(size * 0.2, ROAD_EDGE);
  stroke(size * 0.11, ROAD);
  // R-MOVE-01: dirt roads — thinner, dashed, no dark edge.
  for (const pts of roadChains(view.tracks).map((c) => chaikin(c.map((h) => hexCenter(ax(h), size))))) {
    dashPolyline(roads, pts, size * 0.22, size * 0.14);
  }
  roads.stroke({ width: size * 0.07, color: TRACK, cap: "butt" });

  const water = new Graphics();
  for (const e of view.canals) {
    const [p, q] = sharedSide(ax(e.a), ax(e.b), size);
    water.moveTo(p.x, p.y).lineTo(q.x, q.y);
  }
  water.stroke({ width: size * 0.14, color: WATER, cap: "round" });
  if (!baked) root.addChild(water, roads);

  if (bridges) root.addChild(drawBridges(view.bridges, tex, size));
  return root;
}

/** Bridge sprites on hex sides; in play they are redrawn from the live state. */
export function drawBridges(list: ScenarioView["bridges"], tex: MapTextures, size: number): Container {
  const layer = new Container();
  const ax = ([col, row]: [number, number]) => offsetToAxial({ col, row });
  for (const b of list) {
    const [p, q] = sharedSide(ax(b.a), ax(b.b), size);
    // R-MOVE-02: a ferry (supply only) is a dashed line across the side.
    if (b.state === "ferry" || b.state === "ferry_destroyed") {
      const ca = hexCenter(ax(b.a), size);
      const cb = hexCenter(ax(b.b), size);
      const m = { x: (p.x + q.x) / 2, y: (p.y + q.y) / 2 };
      const k = 0.32;
      const from = { x: m.x + (ca.x - m.x) * k, y: m.y + (ca.y - m.y) * k };
      const to = { x: m.x + (cb.x - m.x) * k, y: m.y + (cb.y - m.y) * k };
      const g = new Graphics();
      dashPolyline(g, [from, to], size * 0.09, size * 0.07);
      g.stroke({ width: size * 0.07, color: b.state === "ferry" ? 0xe8d9b0 : 0x6a5a48, alpha: b.state === "ferry" ? 0.95 : 0.6, cap: "round" });
      layer.addChild(g);
      continue;
    }
    const t = b.state === "built" ? tex.bridge : tex.bridgeDestroyed;
    if (!t) continue;
    const s = new Sprite(t);
    s.anchor.set(0.5);
    s.position.set((p.x + q.x) / 2, (p.y + q.y) / 2);
    // The bridge spans the side: its long axis points from hex a to hex b.
    const ca = hexCenter(ax(b.a), size);
    const cb = hexCenter(ax(b.b), size);
    s.rotation = Math.atan2(cb.y - ca.y, cb.x - ca.x);
    s.width = size * 1.2;
    s.height = size * 1.2;
    layer.addChild(s);
  }
  return layer;
}

/** Dashes along a polyline (Pixi has no dashed strokes). */
function dashPolyline(g: Graphics, pts: Point[], dash: number, gap: number) {
  let on = true;
  let left = dash;
  for (let i = 0; i + 1 < pts.length; i++) {
    let a = pts[i]!;
    const b = pts[i + 1]!;
    let len = Math.hypot(b.x - a.x, b.y - a.y);
    while (len > 0) {
      const step = Math.min(left, len);
      const t = step / len;
      const c = { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
      if (on) g.moveTo(a.x, a.y).lineTo(c.x, c.y);
      a = c;
      len -= step;
      left -= step;
      if (left <= 1e-6) {
        on = !on;
        left = on ? dash : gap;
      }
    }
  }
}

/** R-UI-02: the grid shows up while a unit is selected. */
export function setGridActive(world: Container, active: boolean) {
  const g = world.getChildByLabel(GRID_LABEL, true);
  if (g) g.alpha = active ? GRID_ACTIVE : GRID_IDLE;
}
