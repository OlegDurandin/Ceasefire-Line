import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { ACTIONS, actionBarHtml, type ActionId, type ActionState } from "./action-bar";
import { hexCardHtml, hexRows, type HexFacts } from "./hex-info";
import { fitTransform, fromMini, toMini } from "./minimap";

const ru = JSON.parse(readFileSync(new URL("../../../data/locale/ru.json", import.meta.url), "utf8")) as Record<string, string>;

const facts: HexFacts = {
  col: 17,
  row: 8,
  terrain: "hills",
  terrainName: "Холмы",
  texture: null,
  closed: false,
  info: { ap: 2, defense_mult: 1.3, max_entrench: 3 },
  control: "Сирия",
  supply: null,
  forecast: null,
  road: "track",
  enemyZoc: false,
  occupant: null,
};

describe("hex mode (R-UI-04)", () => {
  it("rows as on the mock", () => {
    expect(hexRows(facts, ru).map((r) => `${r.label} ${r.value}`)).toEqual([
      "Ход 2 AP",
      "Защита ×1,3",
      "Макс. окопы 3",
      "Снабжение —",
      "Дорога грунтовая",
      "ZOC врага нет",
    ]);
  });

  it("R-SUP-04: supply after entering a hex that is not own", () => {
    const row = hexRows({ ...facts, forecast: 6 }, ru)[3]!;
    expect(row.value).toBe("— → 6");
    expect(row.tip).toBe("forecast");
    expect(hexRows({ ...facts, supply: 2, forecast: 5 }, ru)[3]!.value).toBe("2 → 5");
  });

  it("closed and impassable hexes", () => {
    expect(hexRows({ ...facts, closed: true }, ru)[0]!.value).toContain("закрыт");
    expect(hexRows({ ...facts, info: { ap: 0, defense_mult: 1, max_entrench: 0 } }, ru)[0]!.value).toBe("непроходим");
  });

  it("card: where, coordinates, control, occupant", () => {
    const html = hexCardHtml(facts, "cursor", "right", ru, { roadAp: 0.5, trackAp: 1, entrenchMinSupply: 5 });
    expect(html).toContain("Под курсором (17, 8) · контроль: Сирия");
    expect(html).toContain("В гексе: пусто");
    expect(hexCardHtml(null, "selected", "left", ru, { roadAp: 0.5, trackAp: 1, entrenchMinSupply: 5 })).toContain("Выберите гекс");
  });
});

describe("action bar (R-UI-04)", () => {
  it("eight hexagonal actions in the mock's order, disabled ones marked", () => {
    const states = Object.fromEntries(ACTIONS.map((a) => [a, { enabled: a === "undo", tip: `tip ${a}` }])) as Record<ActionId, ActionState>;
    const html = actionBarHtml(states, "unit", ru);
    expect([...html.matchAll(/data-action="(\w+)"/g)].map((m) => m[1])).toEqual(["reinforce", "elite", "transfer", "pin", "bombard", "bridge", "heli", "undo"]);
    expect(html.match(/aria-disabled="true"/g)).toHaveLength(7);
    expect(html).toContain('title="tip transfer"');
    expect(html).toContain('id="info-mode"');
  });
});

describe("minimap", () => {
  it("fits the world into the canvas and maps back", () => {
    const t = fitTransform({ x: -10, y: -10, w: 200, h: 100 }, 100, 100);
    expect(t.scale).toBe(0.5);
    expect(toMini(t, { x: -10, y: -10 })).toEqual({ x: 0, y: 25 });
    expect(fromMini(t, toMini(t, { x: 42, y: 17 }))).toEqual({ x: 42, y: 17 });
  });
});
