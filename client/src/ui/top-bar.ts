// R-UI-04 top bar, as on the approved mock (component «Верхняя панель»):
// left — the active side's flag, name, MPP and NM; centre — NM bars of the
// sides and the section tabs; right — date, day or night, «ход N из M, до
// перемирия K» and the round red «Конец фазы» (Enter); under it a column of
// round buttons: map layers, settings, help, save, menu.

import { attrText } from "../explain";
import { fill } from "../log";
import { icon } from "./icons";

type Locale = Record<string, string>;

export type Tab = "map" | "purchases" | "reinforcements" | "tech" | "reports";
export const TABS: Tab[] = ["map", "purchases", "reinforcements", "tech", "reports"];
export type Layer = "supply" | "zoc" | "hq" | "umbrella" | "objectives" | "trail" | "intent";
export const LAYERS: Layer[] = ["supply", "zoc", "hq", "umbrella", "objectives", "trail", "intent"];

export interface TopBarData {
  side: string;
  /** CSS background of the side's flag. */
  flag: string;
  mpp: number;
  mppTip: string;
  nm: number;
  nmTip: string;
  bars: { name: string; flag: string; pct: number; color: string }[];
  tab: Tab;
  date: string | null;
  night: boolean;
  turn: number;
  lastTurn: number | null;
  ceasefireTip: string;
  layers: Set<Layer>;
  /** The game is over: no end of phase. */
  over: boolean;
}

/** «Ход 3 из 10 · до перемирия 7»; without a last turn just «Ход 3». */
export function turnLine(turn: number, lastTurn: number | null, ru: Locale): string {
  if (lastTurn === null) return fill(ru["ui.turn_n"] ?? "Turn {n}", { n: turn });
  const of = fill(ru["ui.turn_of"] ?? "Turn {n} of {m}", { n: turn, m: lastTurn });
  return `${of} · ${fill(ru["ui.to_ceasefire_n"] ?? "{k}", { k: Math.max(0, lastTurn - turn) })}`;
}

/** CSS stripes of a flag, top to bottom (scenario.json `sides[].flag`). */
export function flagCss(stripes: [string, number][] | null, fallback: string): string {
  if (!stripes) return fallback;
  let y = 0;
  const parts = stripes.map(([c, k]) => {
    const from = y;
    y += k * 100;
    return `${c} ${Math.round(from * 10) / 10}% ${Math.round(y * 10) / 10}%`;
  });
  return `linear-gradient(${parts.join(", ")})`;
}

export function topBarHtml(d: TopBarData, ru: Locale): string {
  const t = (k: string) => ru[k] ?? k;
  const round = (id: string, name: string, label: string, attrs = "") =>
    `<button type="button" class="hud-round" ${attrs} id="${id}" aria-label="${attrText(label)}" title="${attrText(label)}">${icon(name)}</button>`;
  const left = `<div class="tb-left">
      <div class="tb-emblem" aria-hidden="true"><span style="background: ${d.flag}"></span></div>
      <span class="tb-side">${d.side}</span>
      <span class="tb-mpp" title="${attrText(d.mppTip)}">${d.mpp} MPP</span>
      <span class="tb-nm" title="${attrText(d.nmTip)}">NM ${d.nm}%</span>
    </div>`;
  const bars = d.bars
    .map(
      (b) => `<div class="tb-bar"><span class="tb-flag" aria-hidden="true" style="background: ${b.flag}"></span>
        <div class="tb-meter" role="img" aria-label="${attrText(`${b.name}, NM ${b.pct}%`)}" title="${attrText(`${b.name}: NM ${b.pct}%`)}"><div style="width: ${b.pct}%; background: ${b.color}"></div></div></div>`,
    )
    .join("");
  const tabs = TABS.map(
    (tab) =>
      `<button type="button" class="tb-tab${d.tab === tab ? " on" : ""}" data-tab="${tab}" aria-pressed="${d.tab === tab}"><span aria-hidden="true">▼</span>${t(`ui.${tab}`)}</button>`,
  ).join("");
  const center = `<div class="tb-center">
      <div class="tb-bars" role="group" aria-label="${attrText(t("ui.nm_bars"))}">${bars}</div>
      <nav class="tb-tabs" aria-label="${attrText(t("ui.sections"))}">${tabs}</nav>
    </div>`;
  const right = `<div class="tb-right">
      <span class="tb-date">${d.date ? `${d.date} · ` : ""}${t(d.night ? "ui.night" : "ui.day")}</span>
      <span class="tb-turn" title="${attrText(d.ceasefireTip)}">${turnLine(d.turn, d.lastTurn, ru)}</span>
    </div>`;
  const end = d.over
    ? ""
    : `<button type="button" id="end-phase" class="tb-end" aria-label="${attrText(t("ui.end_phase_key"))}" title="${attrText(t("ui.end_phase_key"))}">${icon("end", 26, 3)}</button>`;
  const layers = LAYERS.map((l) =>
    round(`layer-${l}`, l, t(`ui.layer.${l}`), `data-layer="${l}" aria-pressed="${d.layers.has(l)}"${d.layers.has(l) ? ' data-on=""' : ""}`),
  ).join("");
  const tools = `<div class="tb-tools" role="toolbar" aria-label="${attrText(t("ui.layers_tools"))}">${layers}
      <div class="tb-sep" aria-hidden="true"></div>
      ${round("settings", "settings", t("ui.settings"))}${round("help", "help", t("ui.help"))}${round("save-game", "save", t("ui.save"))}${round("to-menu", "menu", t("ui.menu_tip"))}
    </div>`;
  return left + center + right + end + tools;
}
