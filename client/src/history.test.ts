import { describe, expect, it } from "vitest";
import { effectLine, fill, historyDifferences, paragraphs, victoryLines, type ScenarioFacts } from "./history";

const ru = {
  "effect.reinforcements": "подкрепления: {list}",
  "effect.mpp": "{side}: {amount:+} MPP",
  "effect.nm": "{side}: NM {change}",
  "effect.shift_last_turn": "конец сценария сдвигается на {turns} ход.",
  "effect.units_morale": "{side}: мораль наземных частей {change}",
  "effect.units_entrenchment": "{side}: окопы {change}",
  "effect.units_readiness_penalty": "{side}: готовность −{pct} п. п. до следующей фазы",
  "effect.open_exits": "{side}: выходы открыты",
  "effect.destroy_bridge": "мост разрушен",
  "ui.no_effect": "без игрового эффекта",
  "ui.alt_purchase": "{class} — альтернатива",
  "ui.option_differs": "Опция «{option}»: {value} (исторически — {history})",
  "ui.victory_line": "{side}: победа — {victory}, решительная — {decisive}",
  "ui.victory_line_hold": "{side}: победа, если противник впереди не больше чем на {lead}; решительная — {decisive}",
  "ui.victory_steps": "{side}: {vp} за {per} шагов",
  "ui.victory_last_turn": "Конец — ход {turn}.",
  "ui.victory_supply": "{side}: {vp} ВО, если в конце снабжение в гексе «{name}» не ниже {min}",
  "ui.objective_line": "{name}: {vp} ВО, {nm} NM",
  "ui.sudden_death_line": "NM ≤ {pct}%",
  "unit.commando": "Коммандос",
  "unit.mech": "Мех. бригада",
};
const side = (id: number) => (id === 0 ? "Израиль" : "Сирия");

describe("R-UI-07 effect line", () => {
  it("r_ui_07_effects_of_the_withdrawal_order", () => {
    const line = effectLine(
      [
        { kind: "units_morale", side: 1, change: -20 },
        { kind: "units_entrenchment", side: 1, change: -2 },
        { kind: "units_readiness_penalty", side: 1, pct: 15 },
        { kind: "message", key: "event.golan67.withdrawal_order" },
      ],
      ru,
      side,
      () => "",
    );
    expect(line).toBe("Сирия: мораль наземных частей −20; Сирия: окопы −2; Сирия: готовность −15 п. п. до следующей фазы");
  });

  it("r_ui_07_reinforcements_mpp_and_ceasefire", () => {
    const line = effectLine(
      [
        { kind: "reinforcements", units: [{ name: "a" }, { name: "b" }] },
        { kind: "mpp", side: 0, amount: 20 },
        { kind: "shift_last_turn", turns: -1 },
      ],
      ru,
      side,
      (u) => String(u.name),
    );
    expect(line).toBe("подкрепления: a, b; Израиль: +20 MPP; конец сценария сдвигается на −1 ход.");
  });

  it("r_ui_07_message_only_has_no_effect", () => {
    expect(effectLine([{ kind: "message", key: "x" }], ru, side, () => "")).toBe("без игрового эффекта");
  });
});

describe("R-UI-07 briefing from data", () => {
  const sc: ScenarioFacts = {
    last_turn: 4,
    options: { first_strike: "historical" },
    sides: [
      { id: 0, key: "isr", purchase_limits: { commando: { count: 1, alt: true }, artillery: 1 } },
      { id: 1, key: "syr" },
    ],
    victory: [
      { side: 0, victory: 40, decisive: 70 },
      { side: 1, victory: -20, decisive: 30 },
    ],
    vp_per_enemy_step: [{ side: 0, vp: 1, per_steps: 2 }],
    objectives: [{ hex: [7, 6], vp: 20, nm_value: 10 }],
    sudden_death: [{ side: 1, threshold: null }],
  };

  it("r_ui_07_differences_list_alt_units_and_options", () => {
    expect(historyDifferences(sc, ru, side)).toEqual(["Израиль: Коммандос — альтернатива"]);
    const free = { ...sc, options: { first_strike: "free" } };
    expect(historyDifferences(free, ru, side)).toEqual(["Израиль: Коммандос — альтернатива", "Опция «first_strike»: free (исторически — historical)"]);
  });

  it("r_ui_07_victory_conditions_from_scenario", () => {
    expect(victoryLines(sc, ru, side, () => "Кунейтра", 20)).toEqual([
      "Конец — ход 4.",
      "Израиль: победа — 40, решительная — 70",
      "Сирия: победа, если противник впереди не больше чем на 20; решительная — 30",
      "Израиль: 1 за 2 шагов",
      "Кунейтра: 20 ВО, 10 NM",
      "Израиль: NM ≤ 20%",
    ]);
  });

  it("r_nm_02_supply_corridor_vp_line", () => {
    const pocket: ScenarioFacts = { sides: sc.sides, supply_vp: [{ side: 0, hex: [19, 11], min_supply: 4, vp: 15 }] };
    expect(victoryLines(pocket, ru, side, () => "Велен", 20)).toEqual([
      "Израиль: 15 ВО, если в конце снабжение в гексе «Велен» не ниже 4",
      "Израиль: NM ≤ 20%",
      "Сирия: NM ≤ 20%",
    ]);
  });

  it("r_ui_07_missing_text_shows_the_key", () => {
    expect(paragraphs({}, "briefing.x.essay")).toBe("<p>briefing.x.essay</p>");
    expect(paragraphs({ k: "a\n\nb" }, "k")).toBe("<p>a</p><p>b</p>");
    expect(fill("{a:+}", { a: 3 })).toBe("+3");
  });
});
