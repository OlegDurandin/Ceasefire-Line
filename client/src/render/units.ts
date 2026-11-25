// Unit medallions (GDD R-UI-03): a thick ring and a small pennant in the
// side's colour (scenario.json `sides[].color`), the picture on a light
// backing, strength in large figures on its own plate, experience stars
// above the plate. Units without a sprite get a plain frame until APP-6
// symbols (stage 6).

import { Container, Graphics, Sprite, Text, type Texture } from "pixi.js";

import { hexCenter, offsetToAxial } from "../hex";

export interface Token {
  key: number;
  side: number;
  col: number;
  row: number;
  /** Strength, or `★rating` for a headquarters. */
  label: string;
  /** Experience stars (0–3). */
  stars: number;
  /** R-UI-05: units that have already acted are dimmed. */
  dim: boolean;
}

/** Ring colour when a scenario gives none for the side. */
const FALLBACK_SIDE = 0xb08d57;
const BACKING = 0xd9cfb8;
const DARK = 0x1c1a17;
const PLATE_TEXT = 0xf4efe2;
const STAR = 0xe8c25a;
const DIM_ALPHA = 0.55;

/** "#rrggbb" → 0xrrggbb; anything else → null. */
export function parseColor(s: unknown): number | null {
  return typeof s === "string" && /^#[0-9a-fA-F]{6}$/.test(s) ? parseInt(s.slice(1), 16) : null;
}

/** Side colours indexed by side id, from scenario.json. */
export function sideColors(sides: { id: number; color?: unknown }[]): number[] {
  const out: number[] = [];
  for (const s of sides) out[s.id] = parseColor(s.color) ?? FALLBACK_SIDE;
  return out;
}

export function drawUnits(tokens: Token[], textures: Map<number, Texture | null>, size: number, colors: number[]): Container {
  const layer = new Container();
  const radius = size * 0.66;
  const ring = size * 0.15;
  for (const u of tokens) {
    const color = colors[u.side] ?? FALLBACK_SIDE;
    const c = hexCenter(offsetToAxial({ col: u.col, row: u.row }), size);
    const token = new Container();
    token.position.set(c.x, c.y);
    token.alpha = u.dim ? DIM_ALPHA : 1;

    // Pennant behind the ring: staff and a triangle at the upper right.
    const sx = radius * 0.62;
    const top = -radius * 1.42;
    token.addChild(
      new Graphics()
        .moveTo(sx, -radius * 0.5)
        .lineTo(sx, top)
        .stroke({ width: size * 0.05, color: DARK })
        .poly([sx, top, sx + radius * 0.62, top + radius * 0.2, sx, top + radius * 0.4])
        .fill(color)
        .stroke({ width: size * 0.03, color: DARK }),
    );

    // Light backing for contrast with the dark silhouettes, then the ring.
    token.addChild(
      new Graphics()
        .circle(0, 0, radius + ring / 2 + size * 0.02)
        .fill(DARK)
        .circle(0, 0, radius)
        .fill(BACKING),
    );
    const tex = textures.get(u.key);
    if (tex) {
      const pic = new Sprite(tex);
      pic.anchor.set(0.5);
      pic.width = radius * 2.2;
      pic.height = radius * 2.2;
      const mask = new Graphics().circle(0, 0, radius).fill(0xffffff);
      token.addChild(mask);
      pic.mask = mask;
      token.addChild(pic);
    } else {
      token.addChild(new Graphics().rect(-radius * 0.6, -radius * 0.4, radius * 1.2, radius * 0.8).stroke({ width: 2, color: DARK }));
    }
    token.addChild(new Graphics().circle(0, 0, radius).stroke({ width: ring, color }));

    // Strength plate overlapping the bottom of the ring.
    const label = new Text({
      text: u.label,
      style: { fill: PLATE_TEXT, fontSize: size * 0.46, fontWeight: "800", fontFamily: "Arial, sans-serif" },
    });
    label.anchor.set(0.5);
    const plateH = size * 0.5;
    const plateW = Math.max(label.width + size * 0.24, plateH * 1.15);
    const plateY = radius * 0.98;
    label.position.set(0, plateY);
    token.addChild(
      new Graphics()
        .roundRect(-plateW / 2, plateY - plateH / 2, plateW, plateH, size * 0.1)
        .fill({ color: DARK, alpha: 0.95 })
        .stroke({ width: size * 0.04, color }),
    );
    token.addChild(label);

    if (u.stars > 0) {
      const r = size * 0.1;
      const gap = r * 2.1;
      const y = plateY - plateH / 2 - r * 1.05;
      const stars = new Graphics();
      for (let i = 0; i < u.stars; i++) stars.star((i - (u.stars - 1) / 2) * gap, y, 5, r, r * 0.45);
      stars.fill(STAR).stroke({ width: size * 0.02, color: DARK });
      token.addChild(stars);
    }
    layer.addChild(token);
  }
  return layer;
}

/** R-UNIT-01 experience in hundredths (rules.experience.max = 300) → whole stars. */
export function expStars(exp: number): number {
  return Math.max(0, Math.floor(exp / 100));
}
