// Roads as smooth lines through hex centres (R-UI-02): road edges are joined
// into chains between junctions and ends, then rounded by Chaikin's corner
// cutting. Ends and junctions stay on the hex centres so chains meet there.

import type { Point } from "../hex";

type Off = [number, number];

const k = ([c, r]: Off) => `${c},${r}`;

/** Splits a road graph into polylines of hexes; each edge is used once. */
export function roadChains(edges: { a: Off; b: Off }[]): Off[][] {
  const adj = new Map<string, { hex: Off; next: Off[] }>();
  const node = (h: Off) => {
    let n = adj.get(k(h));
    if (!n) adj.set(k(h), (n = { hex: h, next: [] }));
    return n;
  };
  for (const e of edges) {
    node(e.a).next.push(e.b);
    node(e.b).next.push(e.a);
  }
  const used = new Set<string>();
  const ek = (a: Off, b: Off) => (k(a) < k(b) ? `${k(a)}|${k(b)}` : `${k(b)}|${k(a)}`);
  const walk = (start: Off, first: Off): Off[] => {
    const chain = [start];
    let cur = first;
    used.add(ek(start, cur));
    for (;;) {
      chain.push(cur);
      const n = adj.get(k(cur))!;
      if (n.next.length !== 2) break;
      const nxt = n.next.find((h) => !used.has(ek(cur, h)));
      if (!nxt) break;
      used.add(ek(cur, nxt));
      cur = nxt;
    }
    return chain;
  };
  const chains: Off[][] = [];
  // Deterministic order: nodes sorted, ends and junctions first, then loops.
  const nodes = [...adj.values()].sort((x, y) => (k(x.hex) < k(y.hex) ? -1 : 1));
  for (const n of nodes.filter((n) => n.next.length !== 2)) {
    for (const h of n.next) if (!used.has(ek(n.hex, h))) chains.push(walk(n.hex, h));
  }
  for (const n of nodes) {
    for (const h of n.next) if (!used.has(ek(n.hex, h))) chains.push(walk(n.hex, h));
  }
  return chains;
}

/** Chaikin corner cutting; the first and last points are kept. A closed chain
 * (first == last) is smoothed through its start too. */
export function chaikin(pts: Point[], iterations = 2): Point[] {
  let p = pts;
  for (let it = 0; it < iterations && p.length > 2; it++) {
    const out: Point[] = [p[0]!];
    for (let i = 0; i < p.length - 1; i++) {
      const a = p[i]!;
      const b = p[i + 1]!;
      out.push({ x: 0.75 * a.x + 0.25 * b.x, y: 0.75 * a.y + 0.25 * b.y });
      out.push({ x: 0.25 * a.x + 0.75 * b.x, y: 0.25 * a.y + 0.75 * b.y });
    }
    out.push(p[p.length - 1]!);
    p = out;
  }
  return p;
}
