import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { fireLines, num, readinessTip, turnDate, type Rules } from "./explain";
import { lossText } from "./ui/attack-window";

const ru = JSON.parse(readFileSync(new URL("../../data/locale/ru.json", import.meta.url), "utf8")) as Record<string, string>;
const rules = JSON.parse(readFileSync(new URL("../../data/rules.json", import.meta.url), "utf8")) as Rules;

describe("explanations (R-UI-04)", () => {
  it("formats numbers with a decimal comma", () => {
    expect(num(0.3)).toBe("0,3");
    expect(num(5.5)).toBe("5,5");
    expect(num(1)).toBe("1");
    expect(num(0.318, 3)).toBe("0,318");
    expect(num(100, 0)).toBe("100");
  });

  it("dates turns: day and night share a date", () => {
    expect(turnDate("1967-06-09", 1, 1, ru)).toBe("9 июня 1967");
    expect(turnDate("1967-06-09", 1, 2, ru)).toBe("9 июня 1967");
    expect(turnDate("1967-06-09", 1, 3, ru)).toBe("10 июня 1967");
    expect(turnDate(undefined, 1, 3, ru)).toBeNull();
  });

  it("R-COMBAT-02 1967 example: tanks on dug-in infantry, A 5.5, D 5.2, p 0.318", () => {
    const lines = fireLines(
      { strength: 10, readiness_pct: 90, shot_fraction: 1, attack: 5, shooter_exp: 1, attack_mult: 1, defense: 4, target_exp: 0, terrain_mult: 1, entrenchment: 2, defense_mult: 1, p_shift: 0 },
      { n: 9, a: 5.5, d: 5.2, p: 0.318 },
      rules,
      ru,
    );
    expect(lines).toEqual([
      "n = сила 10 × готовность 90% = 9",
      "A = атака 5 × (1 + 0,1 × опыт 1) = 5,5",
      "D = защита 4 × (1 + 0,15 × окопы 2) = 5,2",
      "p = 0,3 + 0,06 × (A − D), в пределах 0,05…0,9 = 0,318",
    ]);
  });

  it("R-UNIT-01 example: supply 8, morale 85%, 1 star → 80%", () => {
    const tip = readinessTip(
      { readiness_pct: 75, base_readiness_pct: 80, readiness_basis: { supply: 8, morale_pct: 85, exp_stars: 1, hq_pct: 0 }, attacks_made: 1, attacks_received: 0, crossing_penalty_pct: 0 },
      rules,
      ru,
    );
    expect(tip).toContain("(0,5 + 0,05 × снабжение 8) × мораль 85% × (1 + 0,05 × опыт 1)");
    expect(tip).toContain("на начало фазы: 80%");
    expect(tip).toContain("усталость: −5 п. п.");
    expect(tip).toContain("сейчас: 75%");
  });
});

describe("losses in the attack window (R-UI-04)", () => {
  it("zero has no minus", () => {
    expect(lossText(0, 0)).toBe("0");
    expect(lossText(0)).toBe("0");
    expect(lossText(0.04)).toBe("0");
    expect(lossText(2, 0)).toBe("−2");
    expect(lossText(0.7)).toBe("−0,7");
  });
});
