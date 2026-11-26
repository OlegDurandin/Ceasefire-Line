// R-UI-04: the attack window at the bottom centre, as on the approved mock
// (board «Окно атаки в контексте карты»): attacker card | red centre
// «Прогноз потерь» | defender card, mirrored. States: a unit selected and no
// target — the left card only; a target hovered or aimed — centre and right
// card; after a combat the centre shows the actual losses for a moment.
// Every number has a tooltip with its formula (the core's own inputs).

import { attrText, fireLines, num, readinessTip, supplyTip, type AttackModifiers, type FireInput, type FireParams, type Rules } from "../explain";
import type { AirForecast, FireParamsView, Forecast, LiveUnit } from "../game-view";
import { fill } from "../log";

type Locale = Record<string, string>;

export interface CardData {
  unit: LiveUnit;
  /** Class name (the mock's upper-case title). */
  title: string;
  /** «Страна · модель · штаб». */
  subtitle: string;
  /** The token as HTML (ui/token-svg.ts). */
  portrait: string;
  stars: number;
  apMax: number | null;
  maxEntrench: number | null;
  terrainName: string;
  /** Genitive name for the fire lines («Огонь танков»). */
  fireName: string;
}

/** R-AIR-01: an aircraft's card. */
export interface AirCardData {
  title: string;
  subtitle: string;
  portrait: string;
  stars: number;
  strength: number;
  canSortie: boolean;
  readiness: number;
  fatigue: number;
  radius: number;
  base: string;
}

/** The right card of an air strike on an airfield. */
export interface AirfieldCardData {
  title: string;
  subtitle: string;
  parked: number;
}

/** Game.explain(): the core's fire inputs and their parameters. */
export interface Explained {
  kind: string;
  inputs: FireInput[];
  params: FireParams[];
  /** Attacks only: rule variants in this attack. */
  modifiers?: AttackModifiers;
}

const MAX_STARS = 3;

/** Losses as shown: «−2», «−0,7»; zero without the minus («0»). */
export function lossText(x: number, digits = 1): string {
  const t = num(x, digits);
  return /^0([,.]0*)?$/.test(t) ? "0" : `−${t}`;
}

export class AttackWindow {
  private resultTimer: number | undefined;

  constructor(
    private el: HTMLElement,
    private ru: Locale,
    private rules: Rules,
  ) {}

  private t(k: string, fallback = k) {
    return this.ru[k] ?? fallback;
  }

  hide() {
    window.clearTimeout(this.resultTimer);
    this.el.style.display = "none";
    this.el.innerHTML = "";
  }

  /** A unit selected, no target: the left card only. */
  showUnit(u: CardData) {
    window.clearTimeout(this.resultTimer);
    this.el.innerHTML = `<div class="aw single">${this.card(u, "left")}</div>`;
    this.el.style.display = "block";
  }

  /** Forecast against a target (hover or aim); `arrowDeg`: the attack's direction on the map, clockwise from east. */
  showTarget(att: CardData, def: CardData, f: Forecast, ex: Explained, arrowDeg: number, actions: string) {
    window.clearTimeout(this.resultTimer);
    this.el.innerHTML = `<div class="aw">${this.card(att, "left")}${this.center(att, def, f, ex, arrowDeg, actions)}${this.card(def, "right")}</div>`;
    this.el.style.display = "block";
  }

  /** The actual losses in the centre for `ms`, then `after()`. */
  showResult(att: CardData | null, def: CardData | null, attackerLoss: number, defenderLoss: number, ms: number, after: () => void) {
    window.clearTimeout(this.resultTimer);
    const center = `<div class="aw-center">
      <div class="aw-kicker">${this.t("ui.result")}</div>
      <div class="aw-pair">
        <div class="aw-col"><div class="aw-big">${lossText(attackerLoss, 0)}</div><div class="aw-cap">${this.t("ui.attacker_l")}</div></div>
        <div class="aw-div" aria-hidden="true"></div>
        <div class="aw-col"><div class="aw-big">${lossText(defenderLoss, 0)}</div><div class="aw-cap">${this.t("ui.defender_l")}</div></div>
      </div></div>`;
    this.el.innerHTML = `<div class="aw">${att ? this.card(att, "left") : "<div></div>"}${center}${def ? this.card(def, "right") : "<div></div>"}</div>`;
    this.el.style.display = "block";
    this.resultTimer = window.setTimeout(after, ms);
  }

  /** R-AIR-01: an aircraft selected, no target. */
  showAirUnit(a: AirCardData) {
    window.clearTimeout(this.resultTimer);
    this.el.innerHTML = `<div class="aw single">${this.airCard(a, "left")}</div>`;
    this.el.style.display = "block";
  }

  /** R-AIR-02: forecast of a strike on a unit (`def`) or an airfield. */
  showAirTarget(att: AirCardData, def: CardData | AirfieldCardData, f: AirForecast, arrowDeg: number, actions: string) {
    window.clearTimeout(this.resultTimer);
    const right = "unit" in def ? this.card(def, "right") : this.airfieldCard(def);
    this.el.innerHTML = `<div class="aw">${this.airCard(att, "left")}${this.airCenter(att, f, arrowDeg, actions)}${right}</div>`;
    this.el.style.display = "block";
  }

  private airCard(c: AirCardData, side: "left" | "right"): string {
    const pair = (label: string, value: string, title = "") =>
      `<span class="aw-l">${label}</span><span class="aw-v${title ? " tip" : ""}"${title ? ` title="${attrText(title)}"` : ""}>${value}</span>`;
    const grid = [
      pair(this.t("ui.col.strength"), String(c.strength)),
      pair(this.t("ui.air.sortie"), this.t(c.canSortie ? "ui.air.ready" : "ui.air.used")),
      pair(this.t("ui.col.readiness"), `${c.readiness}%`, fill(this.t("ui.air.readiness_tip"), { fatigue: c.fatigue })),
      pair(this.t("ui.air.fatigue"), `${c.fatigue}%`),
      pair(this.t("ui.air.radius"), `${c.radius} ${this.t("ui.air.hexes")}`),
      pair(this.t("ui.air.base"), c.base),
    ].join("");
    const stars = `<div class="aw-stars">${"★".repeat(c.stars)}<span class="aw-star-off">${"★".repeat(Math.max(0, MAX_STARS - c.stars))}</span></div>`;
    return `<div class="aw-card ${side}">
      <div class="aw-portrait">${c.portrait}</div>
      <div class="aw-info">
        <div class="aw-title">${c.title}</div>
        <div class="aw-sub" title="${attrText(c.subtitle)}">${c.subtitle}</div>
        ${stars}
        <div class="aw-grid">${grid}</div>
      </div></div>`;
  }

  private airfieldCard(c: AirfieldCardData): string {
    return `<div class="aw-card right"><div class="aw-info">
      <div class="aw-title">${c.title}</div>
      <div class="aw-sub">${c.subtitle}</div>
      <div class="aw-grid"><span class="aw-l">${this.t("ui.air.parked")}</span><span class="aw-v">${c.parked}</span></div>
    </div></div>`;
  }

  private airCenter(att: AirCardData, f: AirForecast, arrowDeg: number, actions: string): string {
    const e = (p: FireParamsView, cap: number) => Math.min(cap, p.n * p.p);
    const line = (head: string, p: FireParamsView) => `${head}: A ${num(p.a, 1)} ${this.t("ui.versus")} D ${num(p.d, 1)} → p ${num(p.p, 2)}`;
    const group = f.group_duels.reduce((s, d) => s + e(d.a_fire, att.strength), 0);
    const ret = f.return_fire ? e(f.return_fire, att.strength) : 0;
    const lines = [line(this.t("ui.air.strike"), f.strike)];
    for (const d of f.escort_duels) lines.push(`${this.t("ui.air.escort_duel")}: p ${num(d.a_fire.p, 2)} / ${num(d.b_fire.p, 2)}`);
    for (const d of f.group_duels) lines.push(line(this.t("ui.air.group_duel"), d.a_fire));
    if (f.return_fire) lines.push(line(this.t("ui.air.return"), f.return_fire));
    if (!f.interceptors.length) lines.push(this.t("ui.air.no_interception"));
    const arrow = `<svg class="aw-arrow" width="34" height="34" viewBox="0 0 34 34" style="transform: rotate(${Math.round(arrowDeg)}deg)" role="img" aria-label="${attrText(this.t("ui.attack_dir"))}"><path d="M3 13 H19 V6 L31 17 L19 28 V21 H3 Z" fill="#c4401e" stroke="#f0b8a8" stroke-width="1.5" stroke-linejoin="round"></path></svg>`;
    const s = f.strike_losses;
    return `<div class="aw-center">${arrow}
      <div class="aw-kicker">${this.t("ui.loss_forecast")}</div>
      <div class="aw-pair">
        <div class="aw-col"><div class="aw-big">${lossText(Math.min(att.strength, group + ret))}</div><div class="aw-cap">${this.t("ui.attacker_l")}</div></div>
        <div class="aw-div" aria-hidden="true"></div>
        <div class="aw-col"><div class="aw-big">${lossText(s.expected)}</div><div class="aw-cap">${this.t("ui.defender_l")} · ${s.low}–${s.high}</div></div>
      </div>
      <div class="aw-lines">${lines.join("<br>")}</div>
      ${actions ? `<div class="aw-actions">${actions}</div>` : ""}</div>`;
  }

  private card(c: CardData, side: "left" | "right"): string {
    const u = c.unit;
    const r = this.rules;
    const tip = (s: string) => `class="tip" title="${attrText(s)}"`;
    const pair = (label: string, value: string, title: string) => `<span class="aw-l">${label}</span><span class="aw-v" ${tip(title)}>${value}</span>`;
    const empty = `<span class="aw-l"></span><span></span>`;
    const expTip = fill(this.t("ui.exp_tip"), {
      atk: num(r.combat.exp_attack_bonus * 100, 0),
      def: num(r.combat.exp_defense_bonus * 100, 0),
      rdy: num(r.readiness.exp_bonus * 100, 0),
    });
    const stars = `<div class="aw-stars" ${tip(expTip)} aria-label="${attrText(fill(this.t("ui.stars_label"), { n: c.stars, max: MAX_STARS }))}">${"★".repeat(c.stars)}<span class="aw-star-off">${"★".repeat(Math.max(0, MAX_STARS - c.stars))}</span></div>`;
    const ap = `${num(u.ap_halves / 2, 1)}${c.apMax !== null ? ` / ${num(c.apMax, 1)}` : ""}`;
    const apTip = fill(this.t("ui.ap_tip"), { night: r.movement.night_ap_penalty, low: r.supply.low_supply_max, supply: r.movement.low_supply_ap_penalty });
    const entrTip = fill(this.t("ui.entrench_tip"), { terrain: c.terrainName, max: c.maxEntrench ?? "—", min: r.supply.entrench_min_supply });
    const grid = [
      pair(this.t("ui.col.strength"), String(u.strength), fill(this.t("ui.strength_tip"), { n: u.strength })),
      pair(this.t("ui.col.strikes"), String(u.strikes_left), this.t("ui.strikes_tip")),
      pair(this.t("ui.col.supply"), String(u.supply), supplyTip(u.supply, r, this.ru)),
      pair(this.t("ui.col.readiness"), `${u.readiness_pct}%`, readinessTip(u, r, this.ru)),
      pair("AP", ap, apTip),
      pair(this.t("ui.col.morale"), `${u.morale_pct}%`, this.t("ui.morale_tip")),
      pair(this.t("ui.col.entrench"), `${u.entrenchment} / ${c.maxEntrench ?? "—"}`, entrTip),
      side === "right" ? pair(this.t("ui.col.terrain"), c.terrainName.toLowerCase(), this.t("ui.terrain_tip")) : empty,
    ].join("");
    return `<div class="aw-card ${side}">
      <div class="aw-portrait">${c.portrait}</div>
      <div class="aw-info">
        <div class="aw-title">${c.title}</div>
        <div class="aw-sub" title="${attrText(c.subtitle)}">${c.subtitle}</div>
        ${stars}
        <div class="aw-grid">${grid}</div>
      </div></div>`;
  }

  /** The biggest factor of A or D besides the base value: «(окопы 2: +30%)». */
  private modifier(i: FireInput, which: "a" | "d", terrainName: string, mods?: AttackModifiers): string {
    const c = this.rules.combat;
    const unsupplied = mods?.unsupplied_target_mult ?? 1;
    const cand: [number, string][] =
      which === "a"
        ? [
            [i.attack_mult / unsupplied, this.t("ui.night_mult")],
            [unsupplied, this.t("ui.unsupplied_target").toLowerCase()],
            [1 + c.exp_attack_bonus * i.shooter_exp, `${this.t("ui.exp")} ${i.shooter_exp}`],
          ]
        : [
            [i.terrain_mult, terrainName.toLowerCase()],
            [1 + c.entrench_defense_bonus * i.entrenchment, `${this.t("ui.entrench")} ${i.entrenchment}`],
            [i.defense_mult, this.t("ui.canal_mult")],
            [1 + c.exp_defense_bonus * i.target_exp, `${this.t("ui.exp")} ${i.target_exp}`],
          ];
    const [k, name] = cand.reduce((m, x) => (Math.abs(x[0] - 1) > Math.abs(m[0] - 1) ? x : m), [1, ""] as [number, string]);
    if (k === 1) return "";
    const pct = Math.round((k - 1) * 100);
    return ` (${name}: ${pct > 0 ? "+" : "−"}${Math.abs(pct)}%)`;
  }

  /** «Огонь танков: A 5,5 против D 5,2 (окопы 2: +30%) → p 0,32». */
  private fireLine(head: string, i: FireInput, p: FireParams, terrainName: string, mods?: AttackModifiers): string {
    const full = fireLines(i, p, this.rules, this.ru, mods).join("\n");
    return `<span class="tip" title="${attrText(full)}">${head}: A ${num(p.a, 1)}${this.modifier(i, "a", terrainName, mods)} ${this.t("ui.versus")} D ${num(p.d, 1)}${this.modifier(i, "d", terrainName)} → p ${num(p.p, 2)}</span>`;
  }

  /** SC-alignment variants of this attack, one line each (R-COMBAT-07, -05, -02). */
  private variantLines(m: AttackModifiers | undefined): string {
    if (!m) return "";
    const line = (text: string, tip: string) => `<br><span class="tip aw-variant" title="${attrText(tip)}">${text}</span>`;
    let out = "";
    if (m.prepared_pct) out += line(`${this.t("ui.prepared_attack")}: ${this.t("ui.readiness")} +${m.prepared_pct} ${this.t("ui.pp")}`, fill(this.t("ui.prepared_attack_tip"), { pct: m.prepared_pct }));
    if (m.unsupplied_target_mult !== 1) out += line(`${this.t("ui.unsupplied_target")}: A × ${num(m.unsupplied_target_mult)}`, fill(this.t("ui.unsupplied_target_tip"), { mult: num(m.unsupplied_target_mult) }));
    if (m.entrench_strip) out += line(`${this.t("ui.strips_entrench")}: −${m.entrench_strip}`, this.t("ui.strips_entrench_tip"));
    return out;
  }

  private center(att: CardData, def: CardData, f: Forecast, ex: Explained, arrowDeg: number, actions: string): string {
    const tip = (s: string) => `class="tip" title="${attrText(s)}"`;
    const range = (l: { low: number; high: number }) => `${l.low}–${l.high}`;
    const big = (x: number) => lossText(x);
    // An arrow (the mock's colours) turned to the attack's direction on the map.
    const arrow = `<svg class="aw-arrow" width="34" height="34" viewBox="0 0 34 34" style="transform: rotate(${Math.round(arrowDeg)}deg)" role="img" aria-label="${attrText(this.t("ui.attack_dir"))}"><path d="M3 13 H19 V6 L31 17 L19 28 V21 H3 Z" fill="#c4401e" stroke="#f0b8a8" stroke-width="1.5" stroke-linejoin="round"></path></svg>`;
    let aLoss: string;
    let dLoss: string;
    let lines: string;
    if (f.kind === "attack") {
      const x = f.forecast;
      aLoss = `<div class="aw-big" ${tip(this.t("ui.expected_tip"))}>${big(x.attacker_losses.expected)}</div><div class="aw-cap" ${tip(this.t("ui.range_tip"))}>${this.t("ui.attacker_l")} · ${range(x.attacker_losses)}</div>`;
      dLoss = `<div class="aw-big" ${tip(this.t("ui.expected_tip"))}>${big(x.defender_losses.expected)}</div><div class="aw-cap" ${tip(this.t("ui.range_tip"))}>${this.t("ui.defender_l")} · ${range(x.defender_losses)}</div>`;
      lines =
        this.fireLine(`${this.t("ui.fire_of")} ${att.fireName}`, ex.inputs[0]!, x.exchange.vs_defender, def.terrainName, ex.modifiers) +
        "<br>" +
        this.fireLine(`${this.t("ui.reply_of")} ${def.fireName}`, ex.inputs[1]!, x.exchange.vs_attacker, att.terrainName) +
        this.variantLines(ex.modifiers);
    } else {
      const d = f.defender_losses;
      aLoss = `<div class="aw-big">0</div><div class="aw-cap">${this.t("ui.attacker_l")} · 0–0</div>`;
      dLoss = `<div class="aw-big" ${tip(this.t("ui.expected_tip"))}>${big(d.expected)}</div><div class="aw-cap" ${tip(this.t("ui.range_tip"))}>${this.t("ui.defender_l")} · ${range(d)}</div>`;
      lines = this.fireLine(`${this.t("ui.bombardment")} ${att.fireName}`, ex.inputs[0]!, ex.params[0]!, def.terrainName);
    }
    return `<div class="aw-center">${arrow}
      <div class="aw-kicker">${this.t("ui.loss_forecast")}</div>
      <div class="aw-pair"><div class="aw-col">${aLoss}</div><div class="aw-div" aria-hidden="true"></div><div class="aw-col">${dLoss}</div></div>
      <div class="aw-lines">${lines}</div>
      ${actions ? `<div class="aw-actions">${actions}</div>` : ""}</div>`;
  }
}
