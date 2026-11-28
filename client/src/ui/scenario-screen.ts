// Scenario selection screen, as in Strategic Command («Campaign name / Start
// date» list, a card with the illustration, facts and the design question,
// buttons «В бой», «Условия победы», «Отмена»). Texts are locale keys; the
// card is built from scenario.json and briefing.json (R-UI-07).

import { fill, type Locale } from "../history";

/** What the screen shows about one scenario. */
export interface ScenarioCard {
  id: string;
  name: string;
  /** ISO date, e.g. "1967-06-05". */
  startDate: string | null;
  turns: number | null;
  hexKm: number | null;
  /** Side keys in id order; side 0 moves first (R-TURN-01). */
  sideKeys: string[];
  image: string | null;
  dates: string | null;
  question: string | null;
  /** Victory conditions, ready lines (history.victoryLines). */
  victory: string[];
}

/** Game mode picked on the screen: hotseat, or the side the player takes against the AI. */
export type Mode = { kind: "hotseat" } | { kind: "vs_ai"; side: string };

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/** «5 июня 1967» from "1967-06-05" (month names are locale keys `ui.month.gen.*`). */
export function formatDate(iso: string | null, ru: Locale): string {
  const m = iso ? /^(\d{4})-(\d{2})-(\d{2})/.exec(iso) : null;
  if (!m) return "—";
  const month = ru[`ui.month.gen.${MONTHS[Number(m[2]) - 1]}`] ?? m[2]!;
  return `${Number(m[3])} ${month} ${m[1]}`;
}

/** Mode buttons: hotseat, then «за <сторону>» for each side (two-sided scenarios). */
export function modes(card: ScenarioCard): Mode[] {
  const out: Mode[] = [{ kind: "hotseat" }];
  if (card.sideKeys.length === 2) for (const side of card.sideKeys) out.push({ kind: "vs_ai", side });
  return out;
}

export const modeKey = (m: Mode) => (m.kind === "hotseat" ? "hotseat" : m.side);

/** The list row of a scenario. */
function row(c: ScenarioCard, selected: boolean, ru: Locale): string {
  return `<button type="button" class="ss-row${selected ? " on" : ""}" data-scenario="${esc(c.id)}" role="option" aria-selected="${selected}">
      <span class="ss-name">${esc(c.name)}</span><span class="ss-date">${esc(formatDate(c.startDate, ru))}</span></button>`;
}

/** The right-hand card: illustration, facts, then title, dates and the question (or the victory conditions). */
export function cardHtml(c: ScenarioCard, ru: Locale, view: "about" | "victory", imageUrl: (id: string) => string): string {
  const t = (k: string) => ru[k] ?? k;
  const side = (k: string | undefined) => (k ? (ru[`side.${k}`] ?? k) : "—");
  const fact = (key: string, value: string) => `<div><b>${esc(t(key))}:</b> ${esc(value)}</div>`;
  const facts = [
    fact("ui.ss.turn_length", t("ui.ss.turn_12h")),
    fact("ui.ss.turns", c.turns === null ? "—" : String(c.turns)),
    fact("ui.ss.hex", c.hexKm === null ? "—" : fill(t("ui.ss.km"), { km: c.hexKm })),
    fact("ui.ss.first", side(c.sideKeys[0])),
  ].join("");
  const picture = c.image
    ? `<img src="${esc(imageUrl(c.image))}" alt="">`
    : `<div class="ss-noimg">${esc(c.name)}</div>`;
  const body =
    view === "victory"
      ? `<b>${esc(t("ui.ss.victory"))}</b><ul>${c.victory.map((l) => `<li>${esc(l)}</li>`).join("")}</ul>`
      : `<b>${esc(c.name)}</b>${c.dates ? `<i>${esc(c.dates)}${c.turns === null ? "" : ` (${esc(fill(t("ui.ss.n_turns"), { n: c.turns }))})`}</i>` : ""}${c.question ? `<p>${esc(c.question)}</p>` : ""}`;
  return `<div class="ss-photo">${picture}</div><div class="ss-facts">${facts}</div><div class="ss-text">${body}</div>`;
}

/** The whole screen (inside the title backdrop). */
export function scenarioScreenHtml(cards: ScenarioCard[], selected: string, mode: Mode, view: "about" | "victory", ru: Locale, imageUrl: (id: string) => string, cancel: boolean): string {
  const t = (k: string) => ru[k] ?? k;
  const card = cards.find((c) => c.id === selected) ?? cards[0]!;
  const empty = Math.max(0, 8 - cards.length);
  const rows = cards.map((c) => row(c, c.id === card.id, ru)).join("") + `<div class="ss-empty"></div>`.repeat(empty);
  const modeButtons = modes(card)
    .map((m) => {
      const label = m.kind === "hotseat" ? t("ui.ss.mode.hotseat") : fill(t("ui.ss.mode.vs_ai"), { side: ru[`side.${m.side}`] ?? m.side });
      const on = modeKey(m) === modeKey(mode);
      return `<button type="button" class="ss-mode${on ? " on" : ""}" data-mode="${esc(modeKey(m))}" aria-pressed="${on}">${esc(label)}</button>`;
    })
    .join("");
  return `<div class="ss-panel" role="dialog" aria-label="${esc(t("ui.ss.title"))}">
    <div class="ss-list">
      <div class="ss-head"><span>${esc(t("ui.ss.col_name"))}</span><span>${esc(t("ui.ss.col_date"))}</span></div>
      <div class="ss-rows" role="listbox">${rows}</div>
    </div>
    <div class="ss-card">${cardHtml(card, ru, view, imageUrl)}</div>
    <div class="ss-modes" role="group" aria-label="${esc(t("ui.ss.mode"))}">${modeButtons}</div>
    <div class="ss-buttons">
      <button type="button" id="ss-play">${esc(t("ui.ss.play"))}</button>
      <button type="button" id="ss-victory" aria-pressed="${view === "victory"}">${esc(t(view === "victory" ? "ui.ss.about" : "ui.ss.victory"))}</button>
      ${cancel ? `<button type="button" id="ss-cancel" class="ss-cancel">${esc(t("ui.ss.cancel"))}</button>` : ""}
    </div>
  </div>`;
}
