// Unit medallions (GDD R-UI-03): picture inside a ring, strength below.
// Units without a sprite get a plain APP-6-style frame until stage 6.

import { Container, Graphics, Sprite, Text, type Texture } from "pixi.js";

import { hexCenter, offsetToAxial } from "../hex";
import type { UnitView } from "../view";

/** Neutral side colours for rings (R-UI-03 asks for neutrality). */
const SIDE_RING = [0x5d7a99, 0xa08a5c];
const BRONZE = 0xb08d57;

export function drawUnits(units: UnitView[], sprites: Map<number, Texture | null>, size: number): Container {
  const layer = new Container();
  const radius = size * 0.72;
  for (const u of units) {
    const c = hexCenter(offsetToAxial({ col: u.col, row: u.row }), size);
    const token = new Container();
    token.position.set(c.x, c.y);
    const ring = new Graphics()
      .circle(0, 0, radius)
      .fill({ color: 0x1c1a17, alpha: 0.85 })
      .stroke({ width: size * 0.08, color: SIDE_RING[u.side] ?? BRONZE });
    token.addChild(ring);
    const tex = sprites.get(u.index);
    if (tex) {
      const pic = new Sprite(tex);
      pic.anchor.set(0.5);
      pic.width = radius * 2.1;
      pic.height = radius * 2.1;
      const mask = new Graphics().circle(0, 0, radius * 0.92).fill(0xffffff);
      token.addChild(mask);
      pic.mask = mask;
      token.addChild(pic);
    } else {
      // Placeholder frame (APP-6 symbols are drawn in code at stage 6).
      token.addChild(new Graphics().rect(-radius * 0.6, -radius * 0.4, radius * 1.2, radius * 0.8).stroke({ width: 2, color: 0xe8e1d3 }));
    }
    const label = new Text({
      text: u.hq_rating !== null ? `★${u.hq_rating}` : String(u.strength),
      style: { fill: 0xe8e1d3, fontSize: size * 0.34, fontWeight: "700", stroke: { color: 0x000000, width: 3 } },
    });
    label.anchor.set(0.5, 0);
    label.position.set(0, radius * 0.55);
    token.addChild(label);
    layer.addChild(token);
  }
  return layer;
}
