import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { parseFlag } from "../render/flag";
import { flagCss, topBarHtml, turnLine } from "./top-bar";

const ru = JSON.parse(readFileSync(new URL("../../../data/locale/ru.json", import.meta.url), "utf8")) as Record<string, string>;

describe("top bar (R-UI-04)", () => {
  it("turn line: «ход N из M, до перемирия K»", () => {
    expect(turnLine(3, 10, ru)).toBe("Ход 3 из 10 · до перемирия 7");
    expect(turnLine(12, 10, ru)).toBe("Ход 12 из 10 · до перемирия 0");
    expect(turnLine(3, null, ru)).toBe("Ход 3");
  });

  it("flag stripes as a CSS gradient", () => {
    expect(flagCss(parseFlag([["#ffffff", 0.5], ["#2b59c3", 0.5]]), "#999")).toBe("linear-gradient(#ffffff 0% 50%, #2b59c3 50% 100%)");
    expect(flagCss(null, "#999")).toBe("#999");
  });

  it("has the five tabs, the layers and the tools; no end of phase after the game", () => {
    const d = {
      side: "Израиль",
      flag: "#fff",
      mpp: 60,
      mppTip: "",
      nm: 85,
      nmTip: "",
      bars: [],
      tab: "map" as const,
      date: "9 июня 1967",
      night: false,
      turn: 1,
      lastTurn: 4,
      ceasefireTip: "",
      layers: new Set(["objectives" as const]),
      over: false,
    };
    const html = topBarHtml(d, ru);
    for (const tab of ["Карта", "Закупки", "Подкрепления", "Технологии", "Отчёты"]) expect(html).toContain(tab);
    for (const id of ["layer-supply", "layer-objectives", "settings", "help", "save-game", "to-menu", "end-phase"]) expect(html).toContain(`id="${id}"`);
    expect(html).toContain("Ход 1 из 4");
    expect(topBarHtml({ ...d, over: true }, ru)).not.toContain('id="end-phase"');
  });
});
