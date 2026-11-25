// Flat-top hexes, axial (q, r) and odd-q offset [col, row] — the same
// conventions as crates/core (hex.rs, scenario.rs). Red Blob Games maths.

export const SQRT3 = Math.sqrt(3);

export interface Axial {
  q: number;
  r: number;
}

export interface Offset {
  col: number;
  row: number;
}

export interface Point {
  x: number;
  y: number;
}

export function offsetToAxial({ col, row }: Offset): Axial {
  return { q: col, r: row - (col - (col & 1)) / 2 };
}

export function axialToOffset({ q, r }: Axial): Offset {
  return { col: q, row: r + (q - (q & 1)) / 2 };
}

/** Centre of a hex; `size` is the corner radius in pixels. */
export function hexCenter({ q, r }: Axial, size: number): Point {
  return { x: 1.5 * size * q, y: SQRT3 * size * (r + q / 2) };
}

/** Six corners, flat-top, starting east and going clockwise in screen space. */
export function hexCorners(c: Point, size: number): Point[] {
  return [0, 1, 2, 3, 4, 5].map((i) => {
    const a = (Math.PI / 3) * i;
    return { x: c.x + size * Math.cos(a), y: c.y + size * Math.sin(a) };
  });
}

/** Hex containing a pixel (cube rounding). */
export function pixelToAxial({ x, y }: Point, size: number): Axial {
  const fq = ((2 / 3) * x) / size;
  const fr = ((-1 / 3) * x + (SQRT3 / 3) * y) / size;
  const fs = -fq - fr;
  let q = Math.round(fq);
  let r = Math.round(fr);
  const s = Math.round(fs);
  const dq = Math.abs(q - fq);
  const dr = Math.abs(r - fr);
  const ds = Math.abs(s - fs);
  if (dq > dr && dq > ds) q = -r - s;
  else if (dr > ds) r = -q - s;
  return { q: q + 0, r: r + 0 };
}

export function distance(a: Axial, b: Axial): number {
  const dq = a.q - b.q;
  const dr = a.r - b.r;
  return (Math.abs(dq) + Math.abs(dr) + Math.abs(dq + dr)) / 2;
}

/** The side shared by two adjacent hexes, as a segment between two corners. */
export function sharedSide(a: Axial, b: Axial, size: number): [Point, Point] {
  const ca = hexCenter(a, size);
  const cb = hexCenter(b, size);
  const corners = hexCorners(ca, size);
  const d = (p: Point) => (p.x - cb.x) ** 2 + (p.y - cb.y) ** 2;
  const sorted = [...corners].sort((p, q) => d(p) - d(q));
  return [sorted[0]!, sorted[1]!];
}
