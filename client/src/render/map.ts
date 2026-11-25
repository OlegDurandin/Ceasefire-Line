// Map layer: terrain textures per hex, a faint grid, roads, rivers/canals and
// bridges on hex sides (GDD R-UI-02).

import { Container, FillPattern, Graphics, Matrix, Sprite, type Texture } from "pixi.js";

import { hexCenter, hexCorners, offsetToAxial, sharedSide, type Point } from "../hex";
import type { ScenarioView } from "../view";

export interface MapTextures {
  terrain: Map<string, Texture>;
  bridge?: Texture;
  bridgeDestroyed?: Texture;
}

/** Texture pixels per world pixel for terrain patterns. */
const TERRAIN_TILE_SCALE = 0.25;
const GRID_ALPHA = 0.12;
const FALLBACK_COLOR = 0x6b5d45;

const flat = (pts: Point[]) => pts.flatMap((p) => [p.x, p.y]);

export function drawMap(view: ScenarioView, tex: MapTextures, size: number): Container {
  const root = new Container();
  const ground = new Graphics();
  const patterns = new Map<string, FillPattern>();
  for (const [id, t] of tex.terrain) {
    const pattern = new FillPattern({ texture: t, repetition: "repeat" });
    pattern.setTransform(new Matrix().scale(TERRAIN_TILE_SCALE, TERRAIN_TILE_SCALE));
    patterns.set(id, pattern);
  }
  for (const h of view.hexes) {
    const corners = hexCorners(hexCenter(h, size), size);
    const pattern = patterns.get(h.terrain);
    ground.poly(flat(corners)).fill(pattern ?? { color: FALLBACK_COLOR });
  }
  root.addChild(ground);

  const grid = new Graphics();
  for (const h of view.hexes) {
    grid.poly(flat(hexCorners(hexCenter(h, size), size))).stroke({ width: 1, color: 0x000000, alpha: GRID_ALPHA });
  }
  root.addChild(grid);

  const edges = new Graphics();
  const ax = ([col, row]: [number, number]) => offsetToAxial({ col, row });
  for (const e of view.roads) {
    const a = hexCenter(ax(e.a), size);
    const b = hexCenter(ax(e.b), size);
    edges.moveTo(a.x, a.y).lineTo(b.x, b.y);
  }
  edges.stroke({ width: size * 0.09, color: 0x8b7355, cap: "round" });
  for (const e of view.canals) {
    const [p, q] = sharedSide(ax(e.a), ax(e.b), size);
    edges.moveTo(p.x, p.y).lineTo(q.x, q.y);
  }
  edges.stroke({ width: size * 0.14, color: 0x4a7a9a, cap: "round" });
  root.addChild(edges);

  for (const b of view.bridges) {
    const t = b.state === "built" ? tex.bridge : tex.bridgeDestroyed;
    if (!t) continue;
    const [p, q] = sharedSide(ax(b.a), ax(b.b), size);
    const s = new Sprite(t);
    s.anchor.set(0.5);
    s.position.set((p.x + q.x) / 2, (p.y + q.y) / 2);
    // The bridge spans the side: its long axis points from hex a to hex b.
    const ca = hexCenter(ax(b.a), size);
    const cb = hexCenter(ax(b.b), size);
    s.rotation = Math.atan2(cb.y - ca.y, cb.x - ca.x);
    s.width = size * 1.2;
    s.height = size * 1.2;
    root.addChild(s);
  }
  return root;
}
