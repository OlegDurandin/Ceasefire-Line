import { describe, expect, it } from "vitest";

import { cardHtml, formatDate, modes, scenarioScreenHtml, type ScenarioCard } from "./scenario-screen";

const ru = {
  "ui.month.gen.jun": "июня",
  "ui.ss.mode.vs_ai": "За {side} против ИИ",
  "ui.ss.n_turns": "{n} ходов",
  "ui.ss.km": "{km} км",
  "side.isr": "Израиль",
  "side.syr": "Сирия",
};

const golan: ScenarioCard = {
  id: "golan67",
  name: "Голаны-67",
  startDate: "1967-06-09",
  turns: 4,
  hexKm: 5,
  sideKeys: ["isr", "syr"],
  image: "briefing_golan67",
  dates: "9–10 июня 1967",
  question: "Возьмёт ли Израиль Кунейтру?",
  victory: ["Кунейтра — 40 очков"],
};
const sinai: ScenarioCard = { ...golan, id: "sinai67", name: "Синай-67", startDate: "1967-06-05", image: null };
const url = (id: string) => `/assets/briefing/${id}.webp`;

describe("scenario selection screen", () => {
  it("dates read as day, month (genitive) and year", () => {
    expect(formatDate("1967-06-05", ru)).toBe("5 июня 1967");
    expect(formatDate(null, ru)).toBe("—");
  });

  it("offers hotseat and each side against the AI", () => {
    expect(modes(golan)).toEqual([{ kind: "hotseat" }, { kind: "vs_ai", side: "isr" }, { kind: "vs_ai", side: "syr" }]);
  });

  it("lists every scenario with its start date and marks the selected one", () => {
    const html = scenarioScreenHtml([golan, sinai], "sinai67", { kind: "vs_ai", side: "syr" }, "about", ru, url, false);
    expect(html).toContain('data-scenario="golan67"');
    expect(html).toMatch(/class="ss-row on" data-scenario="sinai67"/);
    expect(html).toContain("5 июня 1967");
    expect(html).toMatch(/class="ss-mode on" data-mode="syr"/);
    expect(html).not.toContain("ss-cancel");
  });

  it("the card shows the illustration, facts and the design question; or the victory conditions", () => {
    const about = cardHtml(golan, ru, "about", url);
    expect(about).toContain('src="/assets/briefing/briefing_golan67.webp"');
    expect(about).toContain("4 ходов");
    expect(about).toContain("5 км");
    expect(about).toContain("Израиль");
    expect(about).toContain("Возьмёт ли Израиль Кунейтру?");
    const victory = cardHtml(golan, ru, "victory", url);
    expect(victory).toContain("<li>Кунейтра — 40 очков</li>");
    expect(victory).not.toContain("Возьмёт ли");
    // No illustration yet: a dark placeholder with the name.
    expect(cardHtml(sinai, ru, "about", url)).toContain('<div class="ss-noimg">Синай-67</div>');
  });
});
