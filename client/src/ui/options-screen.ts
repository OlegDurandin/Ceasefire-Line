// Options screen, as in Strategic Command: three columns of switches (Вид,
// Предпочтения, Искусственный интеллект), a row of style pickers with a
// preview and ▲ ▼ (unit counters, AI turn speed), «ОК» and «Отмена». Only
// what the game does today; fog of war (R-MODE-01, stage 6) is shown off.

import type { Locale } from "../history";
import { AI_SPEEDS, isOn, type AiSpeed, type Settings, type Switch } from "../settings";
import type { UnitStyle } from "../sprites";

export const UNIT_STYLES: UnitStyle[] = ["miniature", "medallion"];

/** The options being edited (applied on «ОК»). */
export interface Options {
  style: UnitStyle;
  aiSpeed: AiSpeed;
  switches: Record<Switch, boolean>;
}

export function optionsFrom(s: Settings, style: UnitStyle, aiSpeed: AiSpeed): Options {
  return {
    style,
    aiSpeed,
    switches: { windows: isOn(s, "windows"), summary: isOn(s, "summary"), warnUnused: isOn(s, "warnUnused"), trail: isOn(s, "trail") },
  };
}

/** A switch row: id, locale key, value; `null` — not available yet. */
type Row = { id: string; key: string; on: boolean | null };

/** Column → rows. «Показывать ходы и бои» is AI speed ≠ «мгновенно». */
export function columns(o: Options): { key: string; rows: Row[] }[] {
  return [
    {
      key: "ui.opt.view",
      rows: [
        { id: "windows", key: "ui.opt.windows", on: o.switches.windows },
        { id: "summary", key: "ui.opt.summary", on: o.switches.summary },
      ],
    },
    {
      key: "ui.opt.prefs",
      rows: [
        { id: "fog", key: "ui.opt.fog", on: null },
        { id: "warnUnused", key: "ui.opt.warn_unused", on: o.switches.warnUnused },
      ],
    },
    {
      key: "ui.opt.ai",
      rows: [
        { id: "showAi", key: "ui.opt.show_ai", on: o.aiSpeed !== "instant" },
        { id: "trail", key: "ui.opt.trail", on: o.switches.trail },
      ],
    },
  ];
}

/** Flips one switch; «Показывать ходы и бои» goes to ×1 or «мгновенно». */
export function toggle(o: Options, id: string): Options {
  if (id === "showAi") return { ...o, aiSpeed: o.aiSpeed === "instant" ? "x1" : "instant" };
  if (id in o.switches) return { ...o, switches: { ...o.switches, [id]: !o.switches[id as Switch] } };
  return o;
}

/** ▲ / ▼ of a picker: the previous / next value, stopping at the ends. */
export function step<T>(values: readonly T[], now: T, dir: -1 | 1): T {
  const i = Math.max(0, values.indexOf(now));
  return values[Math.min(values.length - 1, Math.max(0, i + dir))]!;
}

export function stepStyle(o: Options, dir: -1 | 1): Options {
  return { ...o, style: step(UNIT_STYLES, o.style, dir) };
}

export function stepSpeed(o: Options, dir: -1 | 1): Options {
  return { ...o, aiSpeed: step(AI_SPEEDS, o.aiSpeed, dir) };
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function picker(id: string, title: string, preview: string, caption: string, first: boolean, last: boolean, upLabel: string, downLabel: string): string {
  return `<div class="op-picker"><div class="op-title">${esc(title)}</div>
    <div class="op-pick"><div class="op-preview">${preview}</div>
      <div class="op-arrows"><button type="button" data-step="${id}:-1" aria-label="${esc(upLabel)}"${first ? " disabled" : ""}>▲</button><button type="button" data-step="${id}:1" aria-label="${esc(downLabel)}"${last ? " disabled" : ""}>▼</button></div></div>
    <div class="op-caption">${esc(caption)}</div></div>`;
}

/** The screen. `stylePreview` — an <img> (or other markup) of a unit in that style. */
export function optionsHtml(o: Options, ru: Locale, stylePreview: (s: UnitStyle) => string): string {
  const t = (k: string) => ru[k] ?? k;
  const cols = columns(o)
    .map(
      (c) => `<div class="op-col"><div class="op-title">${esc(t(c.key))}</div>${c.rows
        .map((r) => {
          const off = r.on === null;
          const label = off ? `${t(r.key)} · ${t("ui.opt.later")}` : t(r.key);
          return `<button type="button" class="op-switch${r.on ? " on" : ""}" data-switch="${r.id}" role="switch" aria-checked="${!!r.on}"${off ? " disabled" : ""}><span class="op-dot"></span><span class="op-label">${esc(label)}</span></button>`;
        })
        .join("")}</div>`,
    )
    .join("");
  const si = UNIT_STYLES.indexOf(o.style);
  const ai = AI_SPEEDS.indexOf(o.aiSpeed);
  const up = t("ui.opt.prev");
  const down = t("ui.opt.next");
  return `<div class="op-panel" role="dialog" aria-label="${esc(t("ui.opt.title"))}">
    <h2 class="op-head">${esc(t("ui.opt.title"))}</h2>
    <div class="op-cols">${cols}</div>
    <div class="op-pickers">
      ${picker("style", t("ui.opt.counters"), stylePreview(o.style), t(`ui.settings.${o.style}`), si <= 0, si >= UNIT_STYLES.length - 1, up, down)}
      ${picker("speed", t("ui.opt.ai_speed"), `<span class="op-speed">${esc(t(`ui.opt.speed.${o.aiSpeed}`))}</span>`, t(`ui.opt.speed_caption.${o.aiSpeed}`), ai <= 0, ai >= AI_SPEEDS.length - 1, up, down)}
    </div>
    <div class="op-buttons"><button type="button" id="op-ok">${esc(t("ui.opt.ok"))}</button><button type="button" id="op-cancel" class="op-cancel">${esc(t("ui.opt.cancel"))}</button></div>
  </div>`;
}
