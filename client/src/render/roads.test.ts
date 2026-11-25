import { describe, expect, it } from "vitest";

import { chaikin, roadChains } from "./roads";

type Off = [number, number];
const e = (a: Off, b: Off) => ({ a, b });

describe("roadChains (R-UI-02)", () => {
  it("joins a straight road into one chain", () => {
    const chains = roadChains([e([2, 0], [1, 0]), e([0, 0], [1, 0])]);
    expect(chains).toEqual([
      [
        [0, 0],
        [1, 0],
        [2, 0],
      ],
    ]);
  });

  it("splits at a junction and uses every edge once", () => {
    const chains = roadChains([e([0, 0], [1, 0]), e([1, 0], [2, 0]), e([1, 0], [1, 1]), e([1, 1], [1, 2])]);
    expect(chains.length).toBe(3);
    expect(chains.reduce((n, c) => n + c.length - 1, 0)).toBe(4);
    expect(chains.some((c) => c.length === 3 && c[2]![1] === 2)).toBe(true);
  });

  it("keeps a loop as a closed chain", () => {
    const chains = roadChains([e([0, 0], [1, 0]), e([1, 0], [1, 1]), e([1, 1], [0, 0])]);
    expect(chains.length).toBe(1);
    expect(chains[0]!.length).toBe(4);
    expect(chains[0]![0]).toEqual(chains[0]![3]);
  });
});

describe("chaikin", () => {
  it("keeps the ends and cuts corners", () => {
    const out = chaikin(
      [
        { x: 0, y: 0 },
        { x: 4, y: 0 },
        { x: 4, y: 4 },
      ],
      1,
    );
    expect(out[0]).toEqual({ x: 0, y: 0 });
    expect(out[out.length - 1]).toEqual({ x: 4, y: 4 });
    expect(out).not.toContainEqual({ x: 4, y: 0 });
  });

  it("leaves a two-point segment alone", () => {
    const seg = [
      { x: 0, y: 0 },
      { x: 1, y: 1 },
    ];
    expect(chaikin(seg)).toEqual(seg);
  });
});
