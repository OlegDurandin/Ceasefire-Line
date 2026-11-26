// Units (GDD R-UI-03), as on the approved mock (board «Фишка юнита»): a
// painted miniature on a perspective base drawn in code — the top ellipse
// (rx ≈ 0.47·w, ry ≈ 0.29·w) with a rim in the side's colour, a side band
// ≈ 0.13·w high with a vertical gradient of that colour, sandy ground with a
// radial gradient and grain; the model (manifest anchor) stands on the
// ground a little below the ellipse's centre, about w wide, rising above it;
// a plate (flag + strength) centred on the band's front edge; a blurred
// shadow to the south-east. Selected — a yellow outline with a translucent
// halo around the base, target — red; no AP and no strikes — the whole token
// is muted. The base's centre is the hex centre. Fallback (no miniature, or
// `?units=medallion`): the medallion. Experience stars only in the attack
// window's card.

import { ColorMatrixFilter, Container, FillGradient, Graphics, Sprite, Text, type Texture } from "pixi.js";

import { hexCenter, offsetToAxial } from "../hex";
import { GROUND_STOPS, HALO, sideGradient, tokenGeom, type FlagStripes, type TokenGeom } from "./token-geom";

export interface Token {
  key: number;
  side: number;
  col: number;
  row: number;
  /** Strength, or `★rating` for a headquarters. */
  label: string;
  /** Experience stars (0–3); shown in the attack window, not on the map. */
  stars: number;
  /** R-UI-05: units with no AP and no strikes left are muted. */
  dim: boolean;
  style: "miniature" | "medallion";
  /** Contact point of the picture (share of its size). */
  anchor: [number, number];
  /** R-UI-03: supply ≤ 3 (R-SUP-03) and encircled (no supply in the hex, R-NM-01). */
  lowSupply?: boolean;
  encircled?: boolean;
  /** R-UI-03: the pictures face left; `right` mirrors the model (not its shadow). */
  facing?: "left" | "right";
  selected?: boolean;
  target?: boolean;
}

/** Token width in hex radii (on the mock: 72 px tokens on hexes of radius 48). */
export const TOKEN_W = 1.5;

/** Ring colour when a scenario gives none for the side. */
const FALLBACK_SIDE = 0xb08d57;
const BACKING = 0xd9cfb8;
const DARK = 0x1c1a17;
const PLATE_BG = 0x1c1e21;
const PLATE_EDGE = 0xc9a46a;
const PLATE_TEXT = 0xffffff;
const DIM_ALPHA = 0.55;
const ELLIPSE_STEPS = 28;

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

/** Points of the lower half of an ellipse, left to right. */
function lowerArc(cx: number, cy: number, rx: number, ry: number): number[] {
  const pts: number[] = [];
  for (let i = 0; i <= ELLIPSE_STEPS; i++) {
    const a = Math.PI - (Math.PI * i) / ELLIPSE_STEPS;
    pts.push(cx + rx * Math.cos(a), cy + ry * Math.sin(a));
  }
  return pts;
}

/** The side band: the lower half of the top ellipse shifted down by t, joined to it. */
function sideBand(g: TokenGeom): number[] {
  const bottom = lowerArc(g.cx, g.cy + g.t, g.rx, g.ry);
  const top = lowerArc(g.cx, g.cy, g.rx, g.ry);
  const back: number[] = [];
  for (let i = top.length - 2; i >= 0; i -= 2) back.push(top[i]!, top[i + 1]!);
  return [...bottom, ...back];
}

/** Plate: flag + strength on a dark tag with a bronze edge, centred at x = 0, top at y = 0. */
function drawPlate(label: string, g: TokenGeom, flag: FlagStripes | null, color: number): Container {
  const plate = new Container();
  const text = new Text({ text: label, style: { fill: PLATE_TEXT, fontSize: g.fs, fontWeight: "700", fontFamily: "'PT Sans', Arial, sans-serif" } });
  const h = Math.round(g.fs * 1.2) + 2;
  const w = 4 + g.fW + 4 + text.width + 6;
  plate.addChild(new Graphics().roundRect(-w / 2, 0, w, h, 3).fill(PLATE_BG).stroke({ width: 1, color: PLATE_EDGE }));
  const fx = -w / 2 + 4;
  const fy = (h - g.fH) / 2;
  const f = new Graphics();
  let y = fy;
  const stripes: FlagStripes = flag ?? [[`#${color.toString(16).padStart(6, "0")}`, 1]];
  for (const [c, k] of stripes) {
    f.rect(fx, y, g.fW, g.fH * k).fill(parseInt(c.slice(1), 16));
    y += g.fH * k;
  }
  f.rect(fx, fy, g.fW, g.fH).stroke({ width: 1, color: 0xffffff, alpha: 0.7 });
  plate.addChild(f);
  text.position.set(fx + g.fW + 4, (h - text.height) / 2);
  plate.addChild(text);
  return plate;
}

/** R-UI-03 status icons right of the plate: supply ≤ 3, encircled. */
function statusIcons(token: Container, u: Token, x: number, y: number, size: number) {
  const r = size * 0.14;
  const g = new Graphics();
  if (u.encircled) {
    g.circle(0, 0, r).fill(DARK).arc(0, 0, r * 0.62, Math.PI * 0.35, Math.PI * 2.15).stroke({ width: size * 0.055, color: 0xe0453a });
  } else if (u.lowSupply) {
    g.circle(0, 0, r).fill(DARK).roundRect(-r * 0.45, -r * 0.55, r * 0.9, r * 1.1, r * 0.2).fill(0xd9a23a);
  } else return;
  g.position.set(x, y);
  token.addChild(g);
}

function drawMiniature(u: Token, tex: Texture | null, size: number, color: number, flag: FlagStripes | null): Container {
  const g = tokenGeom(TOKEN_W * size);
  // Origin of the token = the base's centre = the hex centre.
  const token = new Container();
  const root = new Container();
  root.position.set(-g.cx, -g.cy);
  token.addChild(root);

  // Blurred shadow to the south-east: concentric ellipses fading out.
  const sh = new Graphics();
  const scx = g.shL + g.shW / 2;
  const scy = g.shT + g.shH / 2;
  const steps = 5;
  for (let i = steps; i >= 1; i--) {
    const grow = (g.shB * i) / steps;
    sh.ellipse(scx, scy, g.shW / 2 + grow, g.shH / 2 + grow).fill({ color: 0x281a08, alpha: 0.42 / steps });
  }
  sh.ellipse(scx, scy, g.shW / 2 - g.shB / 2, g.shH / 2 - g.shB / 2).fill({ color: 0x281a08, alpha: 0.2 });
  root.addChild(sh);

  const body = new Container();
  root.addChild(body);
  const halo = u.selected ? HALO.selected : u.target ? HALO.target : null;
  const haloColor = halo ? parseInt(halo.slice(1), 16) : 0;
  if (halo) body.addChild(new Graphics().ellipse(g.cx, g.cy, g.gRx, g.gRy).stroke({ width: g.gW, color: haloColor, alpha: 0.45 }));
  const [top, bottom] = sideGradient(color);
  const band = new FillGradient({
    type: "linear",
    start: { x: 0, y: 0 },
    end: { x: 0, y: 1 },
    colorStops: [
      { offset: 0, color: top },
      { offset: 1, color: bottom },
    ],
    textureSpace: "local",
  });
  const ground = new FillGradient({
    type: "radial",
    center: { x: 0.4, y: 0.35 },
    innerRadius: 0,
    outerCenter: { x: 0.4, y: 0.35 },
    outerRadius: 0.75,
    colorStops: GROUND_STOPS.map(([offset, c]) => ({ offset, color: c })),
    textureSpace: "local",
  });
  const base = new Graphics()
    .poly(sideBand(g))
    .fill(band)
    .stroke({ width: 1, color: 0x141414, alpha: 0.55 })
    .ellipse(g.cx, g.cy, g.rx, g.ry)
    .fill(ground)
    .stroke({ width: g.rimW, color });
  for (const s of g.specks) base.circle(s.x, s.y, s.r).fill(parseInt(s.c.slice(1), 16));
  body.addChild(base);
  if (halo) {
    // The front half of the outline, along the middle of the band.
    const arc = lowerArc(g.cx, g.cy + g.t / 2, g.rx, g.ry);
    const ring = new Graphics().moveTo(arc[0]!, arc[1]!);
    for (let i = 2; i < arc.length; i += 2) ring.lineTo(arc[i]!, arc[i + 1]!);
    body.addChild(ring.stroke({ width: g.ringW, color: haloColor }));
  }
  if (tex) {
    const model = new Sprite(tex);
    model.anchor.set(u.anchor[0], u.anchor[1]);
    model.width = g.mW;
    model.height = g.mW;
    if (u.facing === "right") model.scale.x = -Math.abs(model.scale.x);
    model.position.set(g.cx, g.contactY);
    body.addChild(model);
  } else {
    body.addChild(new Graphics().rect(g.cx - g.rx * 0.5, g.cy - g.w * 0.6, g.rx, g.w * 0.5).stroke({ width: 2, color: DARK }));
  }
  const plate = drawPlate(u.label, g, flag, color);
  plate.position.set(g.cx, g.plT);
  body.addChild(plate);
  statusIcons(body, u, g.cx + plate.width / 2 + size * 0.18, g.plT + plate.height / 2, size);
  if (u.dim) {
    // As on the mock: saturate(0.35) brightness(0.78).
    const cm = new ColorMatrixFilter();
    cm.saturate(-0.65, false);
    cm.brightness(0.78, true);
    body.filters = [cm];
  }
  return token;
}

function drawMedallion(u: Token, tex: Texture | null, size: number, color: number, flag: FlagStripes | null): Container {
  const token = new Container();
  const radius = size * 0.66;
  const ring = size * 0.15;
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
    if (u.facing === "right") pic.scale.x = -Math.abs(pic.scale.x);
    const mask = new Graphics().circle(0, 0, radius).fill(0xffffff);
    token.addChild(mask);
    pic.mask = mask;
    token.addChild(pic);
  } else {
    token.addChild(new Graphics().rect(-radius * 0.6, -radius * 0.4, radius * 1.2, radius * 0.8).stroke({ width: 2, color: DARK }));
  }
  const halo = u.selected ? HALO.selected : u.target ? HALO.target : null;
  token.addChild(new Graphics().circle(0, 0, radius).stroke({ width: ring, color: halo ? parseInt(halo.slice(1), 16) : color }));
  const g = tokenGeom(TOKEN_W * size);
  const plate = drawPlate(u.label, g, flag, color);
  plate.position.set(0, radius * 0.7);
  token.addChild(plate);
  statusIcons(token, u, plate.width / 2 + size * 0.18, radius * 0.7 + plate.height / 2, size);
  token.alpha = u.dim ? DIM_ALPHA : 1;
  return token;
}

export function drawUnits(
  tokens: Token[],
  textures: Map<number, Texture | null>,
  size: number,
  colors: number[],
  flags: (FlagStripes | null)[] = [],
): Container {
  const layer = new Container();
  // Painter's order: units lower on the screen are drawn over those above.
  const placed = tokens
    .map((u) => ({ u, c: hexCenter(offsetToAxial({ col: u.col, row: u.row }), size) }))
    .sort((a, b) => a.c.y - b.c.y || a.c.x - b.c.x);
  for (const { u, c } of placed) {
    const color = colors[u.side] ?? FALLBACK_SIDE;
    const tex = textures.get(u.key) ?? null;
    const flag = flags[u.side] ?? null;
    const token = u.style === "miniature" ? drawMiniature(u, tex, size, color, flag) : drawMedallion(u, tex, size, color, flag);
    token.position.set(c.x, c.y);
    layer.addChild(token);
  }
  return layer;
}

/** R-UNIT-01 experience in hundredths (rules.experience.max = 300) → whole stars. */
export function expStars(exp: number): number {
  return Math.max(0, Math.floor(exp / 100));
}

/** R-AIR-01: aircraft as small medallions in the upper right corner of their base's hex. */
export interface AirToken extends Token {
  /** Position among the aircraft at the same base (0, 1, …). */
  slot: number;
}

const AIR_SCALE = 0.5;

/** World position of an aircraft token (also for hit tests). */
export function airTokenCenter(col: number, row: number, slot: number, size: number): { x: number; y: number } {
  const c = hexCenter(offsetToAxial({ col, row }), size);
  return { x: c.x + size * (0.55 - slot * 0.55), y: c.y - size * 0.62 };
}

/** Radius of an aircraft token, for hit tests. */
export const airTokenRadius = (size: number) => size * AIR_SCALE * 0.8;

export function drawAirTokens(tokens: AirToken[], textures: Map<number, Texture | null>, size: number, colors: number[], flags: (FlagStripes | null)[] = []): Container {
  const layer = new Container();
  for (const u of tokens) {
    const color = colors[u.side] ?? FALLBACK_SIDE;
    const token = drawMedallion(u, textures.get(u.key) ?? null, size * AIR_SCALE, color, flags[u.side] ?? null);
    const p = airTokenCenter(u.col, u.row, u.slot, size);
    token.position.set(p.x, p.y);
    layer.addChild(token);
  }
  return layer;
}
