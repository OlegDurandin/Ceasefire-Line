// Units (GDD R-UI-03). Default: a painted miniature on a base drawn in code —
// the base rim in the side's colour (scenario.json `sides[].color`) with light
// ground on top, the model standing on its contact point (manifest anchor)
// and rising above the base, a shadow to the south-east, the strength plate
// on the front edge with experience stars above it, a small pennant; the
// token lifts when hovered. Fallback (no miniature, or `?units=medallion`):
// the medallion — picture in a ring, strength plate below. Units without a
// picture get a plain frame until APP-6 symbols (stage 6).

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
  style: "miniature" | "medallion";
  /** Contact point of the picture (share of its size). */
  anchor: [number, number];
  /** R-UI-03: supply ≤ 3 (R-SUP-03) and encircled (no supply in the hex, R-NM-01). */
  lowSupply?: boolean;
  encircled?: boolean;
}

/** Ring colour when a scenario gives none for the side. */
const FALLBACK_SIDE = 0xb08d57;
const BACKING = 0xd9cfb8;
const GROUND = 0xcdbb94;
const DARK = 0x1c1a17;
const PLATE_TEXT = 0xf4efe2;
const STAR = 0xe8c25a;
const DIM_ALPHA = 0.55;
/** Base shape (in hex radii): half-width, height of the ellipse, thickness. */
const BASE_RX = 0.68;
const BASE_RY = 0.32;
const BASE_H = 0.13;
/** The model's width on screen, in hex radii (pictures are square). */
const MODEL_SIZE = 1.85;
/** Where the base sits below the hex centre, in hex radii. */
const BASE_DROP = 0.28;
/** Hover lift, in hex radii. */
const LIFT = 0.1;

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

/** A darker shade of a colour (the shaded side of the base rim). */
function shade(c: number, k: number): number {
  const ch = (shift: number) => Math.round(((c >> shift) & 0xff) * k);
  return (ch(16) << 16) | (ch(8) << 8) | ch(0);
}

function plate(token: Container, label: string, y: number, size: number, color: number): number {
  const text = new Text({
    text: label,
    style: { fill: PLATE_TEXT, fontSize: size * 0.42, fontWeight: "800", fontFamily: "Arial, sans-serif" },
  });
  text.anchor.set(0.5);
  const h = size * 0.46;
  const w = Math.max(text.width + size * 0.22, h * 1.15);
  text.position.set(0, y);
  token.addChild(
    new Graphics()
      .roundRect(-w / 2, y - h / 2, w, h, size * 0.1)
      .fill({ color: DARK, alpha: 0.95 })
      .stroke({ width: size * 0.04, color }),
  );
  token.addChild(text);
  return y - h / 2;
}

/** R-UI-03 status icons left of the strength plate: supply ≤ 3, encircled. */
function statusIcons(token: Container, u: Token, y: number, size: number) {
  const icons: Graphics[] = [];
  if (u.encircled) {
    const r = size * 0.15;
    icons.push(
      new Graphics()
        .circle(0, 0, r)
        .fill(0x1c1a17)
        .arc(0, 0, r * 0.62, Math.PI * 0.35, Math.PI * 2.15)
        .stroke({ width: size * 0.06, color: 0xe0453a }),
    );
  } else if (u.lowSupply) {
    const r = size * 0.15;
    icons.push(
      new Graphics()
        .circle(0, 0, r)
        .fill(0x1c1a17)
        .roundRect(-r * 0.45, -r * 0.55, r * 0.9, r * 1.1, r * 0.2)
        .fill(0xd9a23a),
    );
  }
  icons.forEach((g, k) => {
    g.position.set(-size * (0.5 + 0.34 * k), y);
    token.addChild(g);
  });
}

function stars(token: Container, n: number, top: number, size: number) {
  if (n <= 0) return;
  const r = size * 0.1;
  const gap = r * 2.1;
  const y = top - r * 1.05;
  const g = new Graphics();
  for (let i = 0; i < n; i++) g.star((i - (n - 1) / 2) * gap, y, 5, r, r * 0.45);
  g.fill(STAR).stroke({ width: size * 0.02, color: DARK });
  token.addChild(g);
}

function pennant(x: number, bottom: number, top: number, size: number, color: number): Graphics {
  return new Graphics()
    .moveTo(x, bottom)
    .lineTo(x, top)
    .stroke({ width: size * 0.045, color: DARK })
    .poly([x, top, x + size * 0.38, top + size * 0.12, x, top + size * 0.24])
    .fill(color)
    .stroke({ width: size * 0.025, color: DARK });
}

function drawMiniature(u: Token, tex: Texture | null, size: number, color: number): Container {
  const token = new Container();
  const rx = BASE_RX * size;
  const ry = BASE_RY * size;
  const h = BASE_H * size;
  // Shadow to the south-east (light from the north-west, R-UI-06); it stays put when the token lifts.
  token.addChild(new Graphics().ellipse(size * 0.14, h + size * 0.1, rx * 1.05, ry * 1.1).fill({ color: 0x000000, alpha: 0.38 }));
  const body = new Container();
  token.addChild(body);
  // Base: dark side band, rim in the side's colour, light ground on top.
  body.addChild(
    new Graphics()
      .ellipse(0, h, rx, ry)
      .fill(shade(color, 0.6))
      .rect(-rx, 0, rx * 2, h)
      .fill(shade(color, 0.75))
      .ellipse(0, 0, rx, ry)
      .fill(color)
      .ellipse(0, 0, rx * 0.86, ry * 0.8)
      .fill(GROUND)
      .ellipse(0, h, rx, ry)
      .stroke({ width: size * 0.025, color: DARK, alpha: 0.6 }),
  );
  body.addChild(pennant(rx * 0.78, -ry * 0.2, -size * 1.05, size, color));
  if (tex) {
    const model = new Sprite(tex);
    model.anchor.set(u.anchor[0], u.anchor[1]);
    model.width = MODEL_SIZE * size;
    model.height = MODEL_SIZE * size;
    // Contact point a little behind the base's centre, so the model stands on the ground.
    model.position.set(0, ry * 0.25);
    body.addChild(model);
  } else {
    body.addChild(new Graphics().rect(-rx * 0.6, -size * 0.7, rx * 1.2, size * 0.6).stroke({ width: 2, color: DARK }));
  }
  const top = plate(body, u.label, ry + h * 0.6, size, color);
  stars(body, u.stars, top, size);
  statusIcons(body, u, ry + h * 0.6, size);
  // R-UI-03: lift on hover.
  body.eventMode = "static";
  body.cursor = "pointer";
  body.on("pointerover", () => (body.y = -LIFT * size));
  body.on("pointerout", () => (body.y = 0));
  return token;
}

function drawMedallion(u: Token, tex: Texture | null, size: number, color: number): Container {
  const token = new Container();
  const radius = size * 0.66;
  const ring = size * 0.15;
  token.addChild(pennant(radius * 0.62, -radius * 0.5, -radius * 1.42, size, color));
  token.addChild(
    new Graphics()
      .circle(0, 0, radius + ring / 2 + size * 0.02)
      .fill(DARK)
      .circle(0, 0, radius)
      .fill(BACKING),
  );
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
  const top = plate(token, u.label, radius * 0.98, size, color);
  stars(token, u.stars, top, size);
  statusIcons(token, u, radius * 0.98, size);
  return token;
}

export function drawUnits(tokens: Token[], textures: Map<number, Texture | null>, size: number, colors: number[]): Container {
  const layer = new Container();
  // Painter's order: units lower on the screen are drawn over those above.
  const placed = tokens.map((u) => ({ u, c: hexCenter(offsetToAxial({ col: u.col, row: u.row }), size) })).sort((a, b) => a.c.y - b.c.y || a.c.x - b.c.x);
  for (const { u, c } of placed) {
    const color = colors[u.side] ?? FALLBACK_SIDE;
    const tex = textures.get(u.key) ?? null;
    const mini = u.style === "miniature";
    const token = mini ? drawMiniature(u, tex, size, color) : drawMedallion(u, tex, size, color);
    token.position.set(c.x, c.y + (mini ? BASE_DROP * size : 0));
    token.alpha = u.dim ? DIM_ALPHA : 1;
    layer.addChild(token);
  }
  return layer;
}

/** R-UNIT-01 experience in hundredths (rules.experience.max = 300) → whole stars. */
export function expStars(exp: number): number {
  return Math.max(0, Math.floor(exp / 100));
}
