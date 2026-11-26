// R-UI-04: the attack window at the bottom centre, as on the approved mock
// (board «Окно атаки в контексте карты»): attacker card | red centre
// «Прогноз потерь» | defender card, mirrored. States: a unit selected and no
// target — the left card only; a target hovered or aimed — centre and right
// card; after a combat the centre shows the actual losses for a moment.
// Every number has a tooltip with its formula (the core's own inputs).

import { attrText, fireLines, num, readinessTip, supplyTip, type FireInput, type FireParams, type Rules } from "../explain";
import type { Forecast, LiveUnit } from "../game-view";
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

/** Game.explain(): the core's fire inputs and their parameters. */
export interface Explained {
  kind: string;
  inputs: FireInput[];
  params: FireParams[];
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
        <div class="aw-sub">${c.subtitle}</div>
        ${stars}
        <div class="aw-grid">${grid}</div>
      </div></div>`;
  }

  /** The biggest factor of A or D besides the base value: «(окопы 2: +30%)». */
  private modifier(i: FireInput, which: "a" | "d", terrainName: string): string {
    const c = this.rules.combat;
    const cand: [number, string][] =
      which === "a"
        ? [
            [i.attack_mult, this.t("ui.night_mult")],
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
  private fireLine(head: string, i: FireInput, p: FireParams, terrainName: string): string {
    const full = fireLines(i, p, this.rules, this.ru).join("\n");
    return `<span class="tip" title="${attrText(full)}">${head}: A ${num(p.a, 1)}${this.modifier(i, "a", terrainName)} ${this.t("ui.versus")} D ${num(p.d, 1)}${this.modifier(i, "d", terrainName)} → p ${num(p.p, 2)}</span>`;
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
        this.fireLine(`${this.t("ui.fire_of")} ${att.fireName}`, ex.inputs[0]!, x.exchange.vs_defender, def.terrainName) +
        "<br>" +
        this.fireLine(`${this.t("ui.reply_of")} ${def.fireName}`, ex.inputs[1]!, x.exchange.vs_attacker, att.terrainName);
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
