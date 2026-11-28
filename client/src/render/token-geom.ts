// R-UI-03: geometry of a unit token (a miniature on a perspective base), as
// on the approved mock (board «Фишка юнита»). All in px for a token of width
// `w`, in a box w × 1.38·w; (cx, cy) is the centre of the base's top ellipse,
// which sits on the hex centre on the map. Shared by the map (Pixi) and the
// attack window's portrait (SVG) so both look the same.

import { parseFlag, type FlagSpec } from "./flag";

export interface TokenGeom {
  w: number;
  h: number;
  /** Rim (side band) height and the top ellipse. */
  t: number;
  rx: number;
  ry: number;
  cx: number;
  cy: number;
  /** Selection / target halo around the base. */
  gRx: number;
  gRy: number;
  gW: number;
  rimW: number;
  ringW: number;
  /** Shadow box (blurred ellipse) and blur radius. */
  shL: number;
  shT: number;
  shW: number;
  shH: number;
  shB: number;
  /** The model: width, and where its contact point (manifest anchor) stands. */
  mW: number;
  contactY: number;
  /** Plate: top, flag size, font size. */
  plT: number;
  fW: number;
  fH: number;
  fs: number;
  /** Grain on the ground: offsets in units of rx/ry, radius, colour. */
  specks: { x: number; y: number; r: number; c: string }[];
}

const SPECKS: [number, number][] = [
  [-0.5, -0.2],
  [0.35, -0.45],
  [0.1, 0.3],
  [-0.25, 0.55],
  [0.6, 0.15],
  [-0.7, 0.25],
  [0.2, -0.15],
  [-0.15, -0.6],
  [0.75, -0.25],
  [0.45, 0.6],
];

export function tokenGeom(w: number): TokenGeom {
  const h = Math.round(w * 1.38);
  const t = w * 0.13;
  const rx = w * 0.47;
  const ry = w * 0.29;
  const cx = w / 2;
  const cy = h - t - ry - 3;
  const fs = Math.round(w * 0.17);
  return {
    w,
    h,
    t,
    rx,
    ry,
    cx,
    cy,
    gRx: rx + w * 0.06,
    gRy: ry + w * 0.05,
    gW: w * 0.1,
    rimW: Math.max(1.5, w * 0.035),
    ringW: Math.max(2, w * 0.045),
    shL: cx - rx + w * 0.14,
    shT: cy - ry * 0.4 + w * 0.08,
    shW: rx * 2.05,
    shH: ry * 2 + t,
    shB: Math.max(2, w * 0.05),
    mW: w,
    // The model stands on the ground a little below the ellipse's centre.
    contactY: cy + ry * 0.12,
    plT: cy + ry + t / 2 - fs * 0.62,
    fW: Math.round(w * 0.15),
    fH: Math.round(w * 0.1),
    fs,
    specks: SPECKS.map(([px, py], i) => ({
      x: cx + px * rx * 0.85,
      y: cy + py * ry * 0.85,
      r: Math.max(0.8, w * (i % 3 === 0 ? 0.018 : 0.011)),
      c: i % 2 === 0 ? "#8f7a4e" : "#f0e2bf",
    })),
  };
}

/** Side band colours from the side's colour: lighter on top, darker below. */
export function sideGradient(color: number): [number, number] {
  const ch = (s: number) => (color >> s) & 0xff;
  const mix = (k: number, to: number) => {
    const m = (s: number) => Math.round(ch(s) * (1 - k) + to * k);
    return (m(16) << 16) | (m(8) << 8) | m(0);
  };
  return [mix(0.35, 255), mix(0.38, 0)];
}

/** Ground of the base: the mock's sandy radial gradient. */
export const GROUND_STOPS: [number, string][] = [
  [0, "#e2cfa2"],
  [0.7, "#c7ad7a"],
  [1, "#a88e5c"],
];

export const HALO = { selected: "#ffd76a", target: "#e0483c" };

/** The side band as an SVG path (front half of a cylinder). */
export function sidePath(g: TokenGeom): string {
  const { cx, cy, rx, ry, t } = g;
  return `M ${cx - rx} ${cy} L ${cx - rx} ${cy + t} A ${rx} ${ry} 0 0 0 ${cx + rx} ${cy + t} L ${cx + rx} ${cy} A ${rx} ${ry} 0 0 1 ${cx - rx} ${cy} Z`;
}

/** A side's flag (scenario.json `sides[].flag`), see ./flag. */
export type FlagStripes = FlagSpec;
export { parseFlag };
