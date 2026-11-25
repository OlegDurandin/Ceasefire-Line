import { describe, expect, it } from "vitest";

import { cycleEdge, edgeState, paintControl, paintTerrain, placeUnit, terrainAt, type MapDoc, type ScenarioDoc } from "./model";

const map = (): MapDoc => ({
  width: 3,
  height: 2,
  legend: { p: { terrain: "plain" }, h: { terrain: "hills" } },
  terrain: ["ppp", "ppp"],
  control_legend: { "0": 0 },
  control: ["0..", "0.."],
  roads: [[[0, 0], [1, 0], [2, 0]]],
  canals: [],
  bridges: [],
});

describe("editor model", () => {
  it("paints terrain, adding legend characters when needed", () => {
    const m = map();
    paintTerrain(m, [1, 1], "hills");
    paintTerrain(m, [2, 1], "water");
    paintTerrain(m, [0, 1], "mountains", true);
    expect(m.terrain[1]).toBe(`${Object.keys(m.legend).find((k) => m.legend[k]!.terrain === "mountains")}hw`);
    expect(terrainAt(m, [2, 1])).toEqual({ terrain: "water", pass: false });
    expect(terrainAt(m, [0, 1])).toEqual({ terrain: "mountains", pass: true });
  });

  it("paints control and nobody", () => {
    const m = map();
    paintControl(m, [2, 0], 1);
    paintControl(m, [0, 0], null);
    expect(m.control[0]).toBe("..1");
    expect(m.control_legend["1"]).toBe(1);
  });

  it("cycles a hex side through road, canal, bridge, destroyed bridge", () => {
    const m = map();
    const a: [number, number] = [0, 0];
    const b: [number, number] = [1, 0];
    expect(edgeState(m, a, b)).toBe("road");
    expect(cycleEdge(m, a, b)).toBe("canal");
    expect(cycleEdge(m, a, b)).toBe("bridge");
    expect(m.bridges).toHaveLength(1);
    expect(cycleEdge(m, a, b)).toBe("destroyed");
    expect(m.destroyed_bridges).toHaveLength(1);
    expect(m.bridges).toHaveLength(0);
    expect(cycleEdge(m, a, b)).toBe("none");
    expect(m.canals).toHaveLength(0);
    // The other road segment of the original polyline survived normalisation.
    expect(edgeState(m, [1, 0], [2, 0])).toBe("road");
  });

  it("places, replaces and removes units", () => {
    const scn: ScenarioDoc = { sides: [{ id: 0, key: "isr" }], units: [] };
    placeUnit(scn, [1, 1], { class: "tank", side: 0, hex: [0, 0], morale_pct: 85 });
    placeUnit(scn, [1, 1], { class: "infantry", side: 0, hex: [0, 0], morale_pct: 85 });
    expect(scn.units).toEqual([{ class: "infantry", side: 0, hex: [1, 1], morale_pct: 85 }]);
    placeUnit(scn, [1, 1], null);
    expect(scn.units).toHaveLength(0);
  });
});
