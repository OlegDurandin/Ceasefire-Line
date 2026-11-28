// R-UI-03, GDD section 14: side flags drawn by code. A flag is stripes
// (horizontal or vertical) plus an optional emblem; the same shapes feed the
// SVG (HTML panels) and the PixiJS plate on the unit token.
//
// scenario.json `sides[].flag` is either the old stripe list
// `[[colour, share], …]`, an object `{stripes, vertical?, emblem?}`, or the id
// of a flag in data/flags.json (fictional nations, «Срединные земли»).

export type Stripe = [string, number];

export type EmblemShape = "diamond" | "disc" | "ring" | "star";

export interface Emblem {
  shape: EmblemShape;
  color: string;
  /** Emblem height as a share of the flag height (0..1). */
  size: number;
}

export interface FlagSpec {
  stripes: Stripe[];
  vertical: boolean;
  emblem: Emblem | null;
}

/** data/flags.json: flag id → flag (same object form as in scenario.json). */
export type FlagBook = Record<string, unknown>;

const COLOR = /^#[0-9a-fA-F]{6}$/;
const SHAPES: EmblemShape[] = ["diamond", "disc", "ring", "star"];

function stripes(v: unknown): Stripe[] | null {
  if (!Array.isArray(v)) return null;
  const out: Stripe[] = [];
  for (const s of v) {
    if (Array.isArray(s) && typeof s[0] === "string" && COLOR.test(s[0]) && typeof s[1] === "number" && s[1] > 0) out.push([s[0], s[1]]);
  }
  const sum = out.reduce((a, [, k]) => a + k, 0);
  return out.length && sum > 0 ? out.map(([c, k]) => [c, k / sum] as Stripe) : null;
}

function emblem(v: unknown): Emblem | null {
  if (!v || typeof v !== "object") return null;
  const e = v as { shape?: unknown; color?: unknown; size?: unknown };
  if (!SHAPES.includes(e.shape as EmblemShape) || typeof e.color !== "string" || !COLOR.test(e.color)) return null;
  const size = typeof e.size === "number" && e.size > 0 && e.size <= 1 ? e.size : 0.5;
  return { shape: e.shape as EmblemShape, color: e.color, size };
}

/** Parses a flag from scenario.json (`sides[].flag`); a string is looked up in `book`. */
export function parseFlag(v: unknown, book: FlagBook = {}): FlagSpec | null {
  if (typeof v === "string") return typeof book[v] === "string" ? null : parseFlag(book[v], book);
  if (Array.isArray(v)) {
    const s = stripes(v);
    return s ? { stripes: s, vertical: false, emblem: null } : null;
  }
  if (!v || typeof v !== "object") return null;
  const o = v as { stripes?: unknown; vertical?: unknown; emblem?: unknown };
  const s = stripes(o.stripes);
  return s ? { stripes: s, vertical: o.vertical === true, emblem: emblem(o.emblem) } : null;
}

/** A flag of one colour (sides without a flag). */
export function plainFlag(color: string): FlagSpec {
  return { stripes: [[color, 1]], vertical: false, emblem: null };
}

export type Shape =
  | { kind: "rect"; x: number; y: number; w: number; h: number; color: string }
  | { kind: "poly"; pts: number[]; color: string }
  | { kind: "circle"; cx: number; cy: number; r: number; color: string }
  | { kind: "ring"; cx: number; cy: number; r: number; width: number; color: string };

/** Shapes of the flag in a w × h box, bottom to top. */
export function flagShapes(f: FlagSpec, w: number, h: number): Shape[] {
  const out: Shape[] = [];
  let at = 0;
  for (const [color, k] of f.stripes) {
    out.push(f.vertical ? { kind: "rect", x: at * w, y: 0, w: k * w, h, color } : { kind: "rect", x: 0, y: at * h, w, h: k * h, color });
    at += k;
  }
  const e = f.emblem;
  if (!e) return out;
  const [cx, cy, r] = [w / 2, h / 2, (e.size * h) / 2];
  switch (e.shape) {
    case "disc":
      out.push({ kind: "circle", cx, cy, r, color: e.color });
      break;
    case "ring":
      out.push({ kind: "ring", cx, cy, r: r * 0.82, width: r * 0.36, color: e.color });
      break;
    case "diamond":
      out.push({ kind: "poly", pts: [cx, cy - r, cx + r * 0.75, cy, cx, cy + r, cx - r * 0.75, cy], color: e.color });
      break;
    case "star": {
      // Six-pointed star drawn as one outline (12 vertices).
      const pts: number[] = [];
      for (let i = 0; i < 12; i++) {
        const rad = i % 2 === 0 ? r : r * 0.5;
        const [dx, dy] = STAR12[i]!;
        pts.push(cx + dx * rad, cy + dy * rad);
      }
      out.push({ kind: "poly", pts, color: e.color });
      break;
    }
  }
  return out;
}

/** Unit directions every 30°, starting straight up (no trigonometry at run time). */
const H = Math.sqrt(3) / 2;
const STAR12: [number, number][] = [
  [0, -1], [0.5, -H], [H, -0.5], [1, 0], [H, 0.5], [0.5, H],
  [0, 1], [-0.5, H], [-H, 0.5], [-1, 0], [-H, -0.5], [-0.5, -H],
];

const r2 = (v: number) => Math.round(v * 100) / 100;

/** The flag as an SVG document (viewBox w × h). */
export function flagSvg(f: FlagSpec, w = 30, h = 20): string {
  const body = flagShapes(f, w, h)
    .map((s) => {
      switch (s.kind) {
        case "rect":
          return `<rect x="${r2(s.x)}" y="${r2(s.y)}" width="${r2(s.w)}" height="${r2(s.h)}" fill="${s.color}"/>`;
        case "poly":
          return `<polygon points="${s.pts.map(r2).join(" ")}" fill="${s.color}"/>`;
        case "circle":
          return `<circle cx="${r2(s.cx)}" cy="${r2(s.cy)}" r="${r2(s.r)}" fill="${s.color}"/>`;
        case "ring":
          return `<circle cx="${r2(s.cx)}" cy="${r2(s.cy)}" r="${r2(s.r)}" fill="none" stroke="${s.color}" stroke-width="${r2(s.width)}"/>`;
      }
    })
    .join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">${body}</svg>`;
}

/** CSS `background` value drawing the flag (or `fallback` without one). */
export function flagCss(f: FlagSpec | null, fallback: string): string {
  if (!f) return fallback;
  if (!f.emblem) {
    // Plain stripes stay a gradient (top to bottom, or left to right).
    let at = 0;
    const parts = f.stripes.map(([c, k]) => {
      const from = at;
      at += k * 100;
      return `${c} ${Math.round(from * 10) / 10}% ${Math.round(at * 10) / 10}%`;
    });
    return `linear-gradient(${f.vertical ? "to right, " : ""}${parts.join(", ")})`;
  }
  // Single quotes: the value goes into style="…" attributes.
  return `url('data:image/svg+xml,${encodeURIComponent(flagSvg(f))}') center / 100% 100% no-repeat`;
}
