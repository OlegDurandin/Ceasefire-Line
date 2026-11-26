// R-UI-04: the attack window at the bottom centre, after Strategic Command
// (same layout and information, own art): attacker card | red centre with
// the forecast | defender card (mirrored). States: a unit selected and no
// target — the left card only; a target hovered or aimed — centre and right
// card; after a combat the centre shows the actual losses for a moment.
// Every number has a tooltip with its formula (the core's own inputs).

import { attrText, fireLines, num, readinessTip, supplyTip, type FireInput, type FireParams, type Rules } from "../explain";
import type { Forecast, LiveUnit } from "../game-view";
import { fill } from "../log";

type Locale = Record<string, string>;

export interface CardData {
  unit: LiveUnit;
  /** «Название (страна)». */
  title: string;
  portrait: string | null;
  /** Mirror the portrait (the unit faces right). */
  mirrored: boolean;
  stars: number;
  maxEntrench: number | null;
  terrainName: string;
}

/** Game.explain(): the core's fire inputs and their parameters. */
export interface Explained {
  kind: string;
  inputs: FireInput[];
  params: FireParams[];
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
    this.el.innerHTML = `<div class="aw">${this.card(u, "left")}</div>`;
    this.el.style.display = "block";
  }

  /**
   * Forecast against a target (hover or aim). `arrowDeg` is the direction of
   * the attack on the map (0° = east, clockwise); `actions` is HTML for the
   * buttons (aimed only).
   */
  showTarget(att: CardData, def: CardData, f: Forecast, ex: Explained, arrowDeg: number, actions: string) {
    window.clearTimeout(this.resultTimer);
    this.el.innerHTML = `<div class="aw">${this.card(att, "left")}${this.center(f, ex, def, arrowDeg, actions)}${this.card(def, "right")}</div>`;
    this.el.style.display = "block";
  }

  /** The actual losses in the centre for `ms`, then `after()`. */
  showResult(att: CardData | null, def: CardData | null, attackerLoss: number, defenderLoss: number, ms: number, after: () => void) {
    window.clearTimeout(this.resultTimer);
    const center = `<div class="aw-center result">
      <div class="aw-head">${this.t("ui.result")}</div>
      <div class="aw-big">−${attackerLoss} | −${defenderLoss}</div></div>`;
    this.el.innerHTML = `<div class="aw">${att ? this.card(att, "left") : ""}${center}${def ? this.card(def, "right") : ""}</div>`;
    this.el.style.display = "block";
    this.resultTimer = window.setTimeout(after, ms);
  }

  private card(c: CardData, side: "left" | "right"): string {
    const u = c.unit;
    const r = this.rules;
    const tip = (s: string) => `class="tip" title="${attrText(s)}"`;
    const row = (label: string, value: string, title: string) => `<dt>${label}</dt><dd ${tip(title)}>${value}</dd>`;
    const expTip = fill(this.t("ui.exp_tip"), {
      atk: num(r.combat.exp_attack_bonus * 100, 0),
      def: num(r.combat.exp_defense_bonus * 100, 0),
      rdy: num(r.readiness.exp_bonus * 100, 0),
    });
    const stars = c.stars ? `<div class="aw-stars" ${tip(expTip)}>${"★".repeat(c.stars)}</div>` : "";
    const left = [
      row(this.t("ui.col.strength"), String(u.strength), fill(this.t("ui.strength_tip"), { n: u.strength })),
      row(this.t("ui.col.supply"), String(u.supply), supplyTip(u.supply, r, this.ru)),
      row("AP", num(u.ap_halves / 2, 1), fill(this.t("ui.ap_tip"), { night: r.movement.night_ap_penalty, low: r.supply.low_supply_max, supply: r.movement.low_supply_ap_penalty })),
    ].join("");
    const right = [
      row(this.t("ui.col.strikes"), String(u.strikes_left), this.t("ui.strikes_tip")),
      row(this.t("ui.col.readiness"), `${u.readiness_pct}%`, readinessTip(u, r, this.ru)),
      row(this.t("ui.col.morale"), `${u.morale_pct}%`, this.t("ui.morale_tip")),
      row(
        this.t("ui.col.entrench"),
        `${u.entrenchment}/${c.maxEntrench ?? "—"}`,
        fill(this.t("ui.entrench_tip"), { terrain: c.terrainName, max: c.maxEntrench ?? "—", min: r.supply.entrench_min_supply }),
      ),
    ].join("");
    const portrait = c.portrait
      ? `<div class="aw-portrait"><img src="${c.portrait}" alt=""${c.mirrored ? ' class="mirror"' : ""}></div>`
      : `<div class="aw-portrait"></div>`;
    return `<div class="aw-card ${side}">${portrait}<div class="aw-info">
      <div class="aw-title">${c.title}</div>${stars}
      <div class="aw-cols"><dl>${left}</dl><dl>${right}</dl></div></div></div>`;
  }

  /** The biggest factor of A or D besides the base value, with its name. */
  private mainModifier(i: FireInput, which: "a" | "d", terrainName: string): string {
    const c = this.rules.combat;
    const cand: [number, string][] =
      which === "a"
        ? [
            [i.attack_mult, this.t("ui.night_mult")],
            [1 + c.exp_attack_bonus * i.shooter_exp, this.t("ui.exp")],
          ]
        : [
            [i.terrain_mult, terrainName],
            [1 + c.entrench_defense_bonus * i.entrenchment, this.t("ui.entrench")],
            [i.defense_mult, this.t("ui.canal_mult")],
            [1 + c.exp_defense_bonus * i.target_exp, this.t("ui.exp")],
          ];
    const [k, name] = cand.reduce((m, x) => (Math.abs(x[0] - 1) > Math.abs(m[0] - 1) ? x : m), [1, ""] as [number, string]);
    return k === 1 ? "" : ` <span class="aw-mod">(${name} ×${num(k)})</span>`;
  }

  private center(f: Forecast, ex: Explained, def: CardData, arrowDeg: number, actions: string): string {
    const tip = (s: string) => `class="tip" title="${attrText(s)}"`;
    const range = (l: { low: number; high: number }) => `${l.low}–${l.high}`;
    const arrow = `<div class="aw-arrow" style="transform: rotate(${Math.round(arrowDeg)}deg)" title="${attrText(this.t("ui.attack_dir"))}">➜</div>`;
    let big: string;
    let ad: string;
    let spread: string;
    if (f.kind === "attack") {
      const x = f.forecast;
      const toDef = x.exchange.vs_defender;
      const toAtt = x.exchange.vs_attacker;
      const lines = (i: FireInput, p: FireParams) => fireLines(i, p, this.rules, this.ru).join("\n");
      const fwd = lines(ex.inputs[0]!, toDef);
      const back = lines(ex.inputs[1]!, toAtt);
      big = `<span ${tip(`${this.t("ui.return_fire")}:\n${back}`)}>−${num(x.attacker_losses.expected, 1)}</span> | <span ${tip(`${this.t("ui.attacker")} → ${this.t("ui.defender")}:\n${fwd}`)}>−${num(x.defender_losses.expected, 1)}</span>`;
      ad = `<span ${tip(fwd)}>A ${num(toDef.a, 1)}${this.mainModifier(ex.inputs[0]!, "a", def.terrainName)}</span> · <span ${tip(fwd)}>D ${num(toDef.d, 1)}${this.mainModifier(ex.inputs[0]!, "d", def.terrainName)}</span>`;
      spread = `<span ${tip(this.t("ui.range_tip"))}>${range(x.attacker_losses)} | ${range(x.defender_losses)}</span>`;
    } else {
      const d = f.defender_losses;
      const i = ex.inputs[0]!;
      const text = fireLines(i, ex.params[0]!, this.rules, this.ru).join("\n");
      big = `−0 | <span ${tip(this.t("ui.bombardment"))}>−${num(d.expected, 1)}</span>`;
      ad = `<span ${tip(text)}>${this.t("ui.bombardment")}${this.mainModifier(i, "d", def.terrainName)}</span>`;
      spread = `<span ${tip(this.t("ui.range_tip"))}>0 | ${range(d)}</span>`;
    }
    return `<div class="aw-center">${arrow}
      <div class="aw-big">${big}</div>
      <div class="aw-ad">${ad}</div>
      <div class="aw-range">${spread}</div>
      ${actions ? `<div class="aw-actions">${actions}</div>` : ""}</div>`;
  }
}
