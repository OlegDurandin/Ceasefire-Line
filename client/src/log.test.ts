import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { fill, formatEvent, type LogContext } from "./log";

const ru = JSON.parse(readFileSync(new URL("../../data/locale/ru.json", import.meta.url), "utf8")) as Record<string, string>;
const ctx: LogContext = {
  ru,
  units: new Map([
    [0, { class: "tank", side: 0 }],
    [7, { class: "infantry", side: 1 }],
  ]),
  sideName: (id) => (id === null ? "—" : ["Израиль", "Сирия"][id]!),
};

describe("event log (R-UI-04)", () => {
  it("fills templates and leaves unknown fields", () => {
    expect(fill("{a} и {b}", { a: 1 })).toBe("1 и {b}");
  });

  it("names units, sides and offset hexes", () => {
    expect(formatEvent({ type: "combat_resolved", attacker: 0, defender: 7, attacker_losses: 1, defender_losses: 3 }, ctx)).toBe(
      "Танковая бригада (Израиль) атакует Пехотная бригада (Сирия): −1 | −3",
    );
    // Axial (7, 3) is offset [7, 6] (Kuneitra).
    expect(formatEvent({ type: "objective_changed", hex: { q: 7, r: 3 }, from: 1, to: 0 }, ctx)).toBe("Цель [7, 6]: Сирия → Израиль");
  });

  it("shows scenario messages and hides routine events", () => {
    expect(formatEvent({ type: "message", key: "event.golan67.kuneitra_falls" }, ctx)).toBe(ru["event.golan67.kuneitra_falls"]);
    expect(formatEvent({ type: "unit_moved", unit: 0, path: [] }, ctx)).toBeNull();
  });

  it("has a Russian line for every key the log uses", () => {
    const src = readFileSync(new URL("./log.ts", import.meta.url), "utf8");
    const keys = [...new Set(src.match(/"(log|ui)\.[a-z_]+"/g)!.map((k) => k.slice(1, -1)))];
    expect(keys.length).toBeGreaterThan(25);
    for (const k of keys) expect(ru[k], k).toBeTypeOf("string");
  });
});
