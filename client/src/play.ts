// Hotseat play on a scenario (stage 3): select, move, attack, forecast,
// reinforce, transfer, HQ pins, bridges, purchases, tech boosts, reports,
// end phase, outcome. All rules run in the core (WASM); actions are enabled
// by the core's own dry run (`Game.check`).

import { Assets, Container, Graphics, type Texture } from "pixi.js";

import type { Camera } from "./camera";
import type { BridgeTarget, EconomyView, Forecast, GameStateView, LiveUnit, Reach, Target, TransferTargets } from "./game-view";
import { axialToOffset, offsetToAxial, pixelToAxial } from "./hex";
import { formatEvent, type UnitRef } from "./log";
import { drawControl } from "./render/control";
import { drawBridges, setGridActive, type MapTextures } from "./render/map";
import { drawOverlays } from "./render/overlays";
import { drawUnits, expStars, type Token } from "./render/units";
import { pickSprite, type SpritesFile } from "./sprites";
import type { Game } from "./wasm/ceasefire_wasm.js";

type Locale = Record<string, string>;

interface Selection {
  unit: LiveUnit;
  reach: Reach[];
  targets: Target[];
  /** Target picked for the forecast; a second tap or the button confirms. */
  aimed: Target | null;
}

type Panel = "purchases" | "tech" | "reports" | null;

const NIGHT_TINT = 0x8796c4;
const LOG_LIMIT = 300;
const TOAST_MS = 7000;
/** Command JSON for the core: hexes are axial. */
const hex = ([col, row]: [number, number]) => offsetToAxial({ col, row });
const attr = (v: unknown) => encodeURIComponent(JSON.stringify(v));

export class Play {
  private state!: GameStateView;
  private selection: Selection | null = null;
  private unitsLayer = new Container();
  private overlay = new Graphics();
  private control = new Graphics();
  private bridges = new Container();
  private textures = new Map<string, Texture>();
  /** Units ever seen, so the log can name destroyed ones. */
  private known = new Map<number, UnitRef>();
  private log: string[] = [];
  private panel: Panel = null;
  /** R-MOVE-05: transfer mode for the selected unit. */
  private transfer: TransferTargets | null = null;

  constructor(
    private game: Game,
    private world: Container,
    private camera: Camera,
    private sprites: SpritesFile,
    private ru: Locale,
    private sideKeys: string[],
    private colors: number[],
    private size: number,
    private mapTex: MapTextures,
  ) {
    world.addChild(this.bridges);
    world.addChild(this.control);
    world.addChild(this.overlay);
    world.addChild(this.unitsLayer);
    camera.onTap = (x, y) => this.tap(x, y);
  }

  private t(key: string, fallback = key): string {
    return this.ru[key] ?? fallback;
  }

  private sideName(id: number | null): string {
    if (id === null) return this.t("ui.nobody", "—");
    const key = this.sideKeys[id];
    return key ? this.t(`side.${key}`, key) : String(id);
  }

  async refresh() {
    this.state = JSON.parse(this.game.state()) as GameStateView;
    for (const u of this.state.units) this.known.set(u.id, { class: u.class, side: u.side });
    this.bridges.removeChildren().forEach((c) => c.destroy({ children: true }));
    this.bridges.addChild(drawBridges(this.state.bridges, this.mapTex, this.size));
    await this.drawUnits();
    const owners = new Map(this.state.control.map(([[c, r], s]) => [`${c},${r}`, s] as const));
    drawControl(this.control, this.size, owners, this.colors);
    this.drawTop();
    this.world.tint = this.state.night ? NIGHT_TINT : 0xffffff;
    if (this.selection) {
      const still = this.state.units.find((u) => u.id === this.selection!.unit.id);
      if (still && still.side === this.state.active_side) this.select(still);
      else this.deselect();
    }
    this.drawPanel();
    if (this.state.outcome) this.showOutcome();
  }

  private async drawUnits() {
    const tokens: Token[] = [];
    const tex = new Map<number, Texture | null>();
    for (const u of this.state.units) {
      const tech = this.state.sides.find((s) => s.id === u.side)?.tech ?? {};
      const ref = pickSprite(this.sprites, u.class, u.nation, tech);
      if (ref && !this.textures.has(ref.url)) this.textures.set(ref.url, await Assets.load<Texture>(ref.url));
      tex.set(u.id, ref ? (this.textures.get(ref.url) ?? null) : null);
      const mine = u.side === this.state.active_side;
      const spent = !(u.can_move && u.ap_halves > 0) && (u.strikes_left === 0 || u.ap_halves === 0);
      tokens.push({
        key: u.id,
        side: u.side,
        col: u.col,
        row: u.row,
        label: u.hq_rating !== null ? `★${u.hq_rating}` : String(u.strength),
        stars: expStars(u.exp),
        dim: mine && spent,
      });
    }
    this.unitsLayer.removeChildren().forEach((c) => c.destroy({ children: true }));
    this.unitsLayer.addChild(drawUnits(tokens, tex, this.size, this.colors));
  }

  private drawTop() {
    const s = this.state;
    const side = s.sides.find((x) => x.id === s.active_side);
    const left = s.last_turn === null ? "—" : String(Math.max(0, s.last_turn - s.turn));
    const top = document.getElementById("top")!;
    top.innerHTML = `
      <span><b>${this.sideName(s.active_side)}</b></span>
      <span>${this.t("ui.turn")} ${s.turn} · ${s.night ? this.t("ui.night") : this.t("ui.day")}</span>
      <span>MPP ${side?.mpp ?? 0}</span>
      <span>NM ${s.sides.map((x) => `${this.sideName(x.id)} ${x.national_morale_pct}%`).join(" · ")}</span>
      <span class="muted">${this.t("ui.to_ceasefire")}: ${left}</span>
      <span class="tabs">${(["map", "purchases", "tech", "reports"] as const)
        .map((p) => `<button data-panel="${p}" class="${(this.panel ?? "map") === p ? "on" : ""}">${this.t(`ui.${p}`)}</button>`)
        .join("")}</span>
      <button id="end-phase">${this.t("ui.end_phase")}</button>`;
    document.getElementById("end-phase")!.onclick = () => this.run(() => this.game.endPhase(), true);
    for (const b of top.querySelectorAll<HTMLButtonElement>("[data-panel]")) {
      b.onclick = () => {
        const p = b.dataset.panel!;
        this.panel = p === "map" || this.panel === p ? null : (p as Panel);
        this.drawTop();
        this.drawPanel();
      };
    }
  }

  /** R-UI-04 panels: purchases, technologies, reports. */
  private drawPanel() {
    const el = document.getElementById("panel")!;
    if (!this.panel) {
      el.style.display = "none";
      return;
    }
    el.style.display = "block";
    const e = JSON.parse(this.game.economy()) as EconomyView;
    const button = (label: string, cmd: object, refused: string | null) =>
      `<button data-cmd="${attr(cmd)}"${refused ? ` disabled title="${refused.replace(/"/g, "&quot;")}"` : ""}>${label}</button>`;
    let html = "";
    if (this.panel === "purchases") {
      html = `<b>${this.t("ui.purchases")}</b> · ${this.sideName(e.side)}<br>
        <span class="muted">${this.t("ui.mpp")} ${e.mpp} · ${this.t("ui.income")} ${e.base_income}</span>`;
      html += e.purchasable.length
        ? `<table>${e.purchasable
            .map(
              (p) => `<tr><td>${this.t(`unit.${p.class}`, p.class)}</td><td>${p.cost} MPP</td>
                <td class="muted">${this.t("ui.left")} ${p.left}</td><td>${button(this.t("ui.buy"), { type: "purchase", class: p.class }, p.refused)}</td></tr>`,
            )
            .join("")}</table>`
        : `<p class="muted">${this.t("ui.nothing_to_buy")}</p>`;
      if (e.arrivals.length) {
        html += `<b>${this.t("ui.arrivals")}</b><ul>${e.arrivals
          .map(([c, turn]) => `<li>${this.t(`unit.${c}`, c)} — ${this.t("ui.arrives_turn")} ${turn}</li>`)
          .join("")}</ul>`;
      }
    } else if (this.panel === "tech") {
      const levels = this.state.sides.find((x) => x.id === e.side)?.tech ?? {};
      html = `<b>${this.t("ui.tech")}</b> · ${this.sideName(e.side)} · ${this.t("ui.mpp")} ${e.mpp}<table>${["armor", "antitank", "air_defense", "aviation", "ew"]
        .map((b) => `<tr><td>${this.t(`tech.${b}`)}</td><td>${this.t("ui.level")} ${levels[b] ?? 1}</td></tr>`)
        .join("")}</table>`;
      if (e.boosts.length) {
        html += `<b>${this.t("ui.boosts")}</b><table>${e.boosts
          .map((b) => {
            const state = b.ready_turn !== null ? `${this.t("ui.ready_turn")} ${b.ready_turn}` : b.bought ? this.t("ui.bought") : "";
            return `<tr><td>${this.t(`boost.${b.id}`, b.id)}<br><span class="muted">${this.t(`tech.${b.branch}`)} · ${b.cost} MPP · ${b.turns} ${this.t("ui.turns")}</span></td>
              <td>${state || button(this.t("ui.buy"), { type: "buy_tech_boost", id: b.id }, b.refused)}</td></tr>`;
          })
          .join("")}</table>`;
      }
    } else {
      html = `<b>${this.t("ui.reports")}</b>${
        this.log.length ? `<ul class="log">${[...this.log].reverse().map((l) => `<li>${l}</li>`).join("")}</ul>` : `<p class="muted">${this.t("ui.no_reports")}</p>`
      }`;
    }
    el.innerHTML = html;
    this.bindCommands(el);
  }

  /** Buttons carrying a command (`data-cmd`) run it through the core. */
  private bindCommands(el: HTMLElement) {
    for (const b of el.querySelectorAll<HTMLButtonElement>("[data-cmd]")) {
      b.onclick = () => {
        const cmd = decodeURIComponent(b.dataset.cmd!);
        void this.run(() => this.game.apply(cmd));
      };
    }
  }

  private unitAt(col: number, row: number): LiveUnit | undefined {
    return this.state.units.find((u) => u.col === col && u.row === row);
  }

  private tap(x: number, y: number) {
    if (this.state.outcome) return;
    const w = this.camera.toWorld(x, y);
    const { col, row } = axialToOffset(pixelToAxial(w, this.size));
    const there = this.unitAt(col, row);
    const sel = this.selection;
    if (sel && this.transfer) {
      const ok = this.transfer.hexes.some(([c, r]) => c === col && r === row);
      this.transfer = null;
      if (ok) {
        void this.run(() => this.game.apply(JSON.stringify({ type: "transfer", unit: sel.unit.id, to: hex([col, row]) })));
        return;
      }
      this.redrawOverlay();
    }
    if (sel) {
      const target = there ? sel.targets.find((t) => t.target === there.id) : undefined;
      if (target) {
        if (sel.aimed?.target === target.target) this.strike(sel.unit, target);
        else this.aim(target);
        return;
      }
      if (sel.reach.some((h) => h.col === col && h.row === row)) {
        void this.run(() => this.game.moveTo(sel.unit.id, col, row));
        return;
      }
    }
    if (there && there.side === this.state.active_side) this.select(there);
    else this.deselect();
  }

  private select(unit: LiveUnit) {
    const reach = JSON.parse(this.game.reachable(unit.id)) as Reach[];
    const targets = JSON.parse(this.game.targets(unit.id)) as Target[];
    const keepAim = this.selection?.unit.id === unit.id ? this.selection.aimed : null;
    const aimed = keepAim && targets.some((t) => t.target === keepAim.target) ? keepAim : null;
    if (this.selection?.unit.id !== unit.id) this.transfer = null;
    this.selection = { unit, reach, targets, aimed };
    this.redrawOverlay();
    this.showInfo(unit);
    if (aimed) this.aim(aimed);
    else this.showAdvanceOrHide();
  }

  private deselect() {
    this.selection = null;
    this.transfer = null;
    this.redrawOverlay();
    document.getElementById("info")!.style.display = "none";
    this.showAdvanceOrHide();
  }

  private redrawOverlay() {
    const sel = this.selection;
    const targetHexes = (sel?.targets ?? [])
      .map((t) => this.state.units.find((u) => u.id === t.target))
      .filter((u): u is LiveUnit => !!u);
    const transfer = (this.transfer?.hexes ?? []).map(([col, row]) => ({ col, row }));
    drawOverlays(this.overlay, this.size, this.transfer ? [] : (sel?.reach ?? []), targetHexes, sel ? sel.unit : null, transfer);
    setGridActive(this.world, !!sel);
  }

  private showInfo(u: LiveUnit) {
    const info = document.getElementById("info")!;
    info.style.display = "block";
    info.innerHTML = `<b>${this.t(`unit.${u.class}`, u.class)}</b> · ${this.sideName(u.side)}<br>
      ${this.t("ui.strength")} ${u.strength} · ${this.t("ui.readiness")} ${u.readiness_pct}% · ${this.t("ui.morale")} ${u.morale_pct}%<br>
      AP ${u.ap_halves / 2} · ${this.t("ui.strikes")} ${u.strikes_left} · ${this.t("ui.supply")} ${u.supply} · ${this.t("ui.entrench")} ${u.entrenchment}
      ${this.actions(u)}`;
    this.bindActions(info, u);
  }

  /** Unit name with its hex, for HQ lists. */
  private unitLabel(u: LiveUnit): string {
    return `${this.t(`unit.${u.class}`, u.class)}${u.hq_rating !== null ? ` ★${u.hq_rating}` : ""} [${u.col}, ${u.row}]`;
  }

  /** R-ECO-02 reinforce, R-MOVE-05 transfer, R-HQ-01 pin, R-MOVE-02 bridges. */
  private actions(u: LiveUnit): string {
    if (u.side !== this.state.active_side || this.state.outcome) return "";
    let html = `<div class="actions">`;
    if (u.strength < 10) {
      const steps = Array.from({ length: 10 - u.strength }, (_, k) => k + 1);
      html += `<div><select id="rf-steps">${steps.map((n) => `<option value="${n}">${n}</option>`).join("")}</select> ${this.t("ui.steps")}
        <label><input type="checkbox" id="rf-elite"> ${this.t("ui.elite")}</label>
        <button id="rf-go">${this.t("ui.reinforce")}</button> <span id="rf-cost" class="muted"></span></div>`;
    }
    const tt = JSON.parse(this.game.transferTargets(u.id)) as TransferTargets;
    if (tt.cost !== null && tt.hexes.length) {
      html += `<div><button id="tr-go" class="${this.transfer ? "on" : ""}">${this.t("ui.transfer")} · ${tt.cost} MPP</button>${
        this.transfer ? ` <span class="muted">${this.t("ui.transfer_hint")} ${tt.cost} MPP</span>` : ""
      }</div>`;
    }
    if (u.class !== "hq") {
      const hqs = this.state.units.filter((h) => h.side === u.side && h.hq_rating !== null);
      const attached = hqs.find((h) => h.id === u.attached_hq);
      html += `<div>${this.t("ui.hq")}: <select id="hq-pin"><option value="auto">${this.t("ui.hq_auto")}</option>${hqs
        .map((h) => `<option value="${h.id}"${u.pinned_hq === h.id ? " selected" : ""}>${this.unitLabel(h)}</option>`)
        .join("")}</select><br><span class="muted">${
        attached ? `${this.t("ui.attached_to")}: ${this.unitLabel(attached)}` : this.t("ui.not_attached")
      }</span></div>`;
    }
    const bridges = JSON.parse(this.game.bridgeTargets(u.id)) as BridgeTarget[];
    for (const b of bridges) {
      const cmd =
        b.kind === "bombard"
          ? { type: "bombard_bridge", unit: u.id, a: hex(b.a), b: hex(b.b) }
          : { type: "build_bridge", unit: u.id, toward: hex(b.b) };
      html += `<div><button data-cmd="${attr(cmd)}">${this.t(b.kind === "bombard" ? "ui.bombard_bridge" : "ui.build_bridge")} [${b.a.join(", ")}]–[${b.b.join(", ")}]</button></div>`;
    }
    return html + `</div>`;
  }

  private bindActions(info: HTMLElement, u: LiveUnit) {
    this.bindCommands(info);
    const steps = info.querySelector<HTMLSelectElement>("#rf-steps");
    const elite = info.querySelector<HTMLInputElement>("#rf-elite");
    const go = info.querySelector<HTMLButtonElement>("#rf-go");
    if (steps && elite && go) {
      const cmd = () => JSON.stringify({ type: "reinforce", unit: u.id, steps: Number(steps.value), elite: elite.checked });
      const update = () => {
        const refused = this.game.check(cmd());
        let cost = "";
        try {
          cost = `${this.game.reinforceCost(u.id, Number(steps.value), elite.checked)} MPP`;
        } catch {
          cost = "";
        }
        info.querySelector("#rf-cost")!.textContent = refused ? `${cost} · ${refused}` : cost;
        go.disabled = !!refused;
      };
      steps.onchange = update;
      elite.onchange = update;
      go.onclick = () => void this.run(() => this.game.apply(cmd()));
      update();
    }
    const tr = info.querySelector<HTMLButtonElement>("#tr-go");
    if (tr) {
      tr.onclick = () => {
        this.transfer = this.transfer ? null : (JSON.parse(this.game.transferTargets(u.id)) as TransferTargets);
        this.redrawOverlay();
        this.showInfo(u);
      };
    }
    const pin = info.querySelector<HTMLSelectElement>("#hq-pin");
    if (pin) {
      if (u.pinned_hq === null) pin.value = "auto";
      pin.onchange = () => {
        const cmd = JSON.stringify({ type: "pin_to_hq", unit: u.id, hq: pin.value === "auto" ? null : Number(pin.value) });
        void this.run(() => this.game.apply(cmd));
      };
    }
  }

  private aim(target: Target) {
    if (!this.selection) return;
    this.selection.aimed = target;
    const f = JSON.parse(this.game.forecast(this.selection.unit.id, target.target)) as Forecast;
    const box = document.getElementById("combat")!;
    const range = (l: { low: number; high: number }) => `${l.low}–${l.high}`;
    let body: string;
    if (f.kind === "attack") {
      const x = f.forecast;
      const a = x.attacker_losses;
      const d = x.defender_losses;
      body = `<div class="row"><span>${this.t("ui.attacker")}</span><span>${this.t("ui.defender")}</span></div>
        <div class="big">−${a.expected.toFixed(1)} | −${d.expected.toFixed(1)}</div>
        <div class="row detail"><span>${range(a)}</span><span>${range(d)}</span></div>
        <div class="detail">A ${x.exchange.vs_defender.a.toFixed(1)} · D ${x.exchange.vs_defender.d.toFixed(1)} · p ${x.exchange.vs_defender.p.toFixed(2)} · n ${x.exchange.vs_defender.n}
          &nbsp;|&nbsp; ${this.t("ui.return_fire")}: p ${x.exchange.vs_attacker.p.toFixed(2)} · n ${x.exchange.vs_attacker.n}</div>`;
    } else {
      const d = f.defender_losses;
      body = `<div>${this.t("ui.bombardment")}</div><div class="big">−0 | −${d.expected.toFixed(1)}</div>
        <div class="detail">${range(d)}</div>`;
    }
    box.innerHTML = `${body}<div class="actions"><button id="strike">${this.t(target.kind === "attack" ? "ui.attack" : "ui.bombard")}</button>
      <button id="cancel">${this.t("ui.cancel")}</button></div>`;
    box.style.display = "block";
    document.getElementById("strike")!.onclick = () => this.selection && this.strike(this.selection.unit, target);
    document.getElementById("cancel")!.onclick = () => {
      if (this.selection) this.selection.aimed = null;
      this.showAdvanceOrHide();
    };
  }

  private strike(unit: LiveUnit, target: Target) {
    void this.run(() => (target.kind === "attack" ? this.game.attack(unit.id, target.target) : this.game.bombard(unit.id, target.target))).then(
      (events) => {
        if (this.selection) this.selection.aimed = null;
        this.showResult(events);
      },
    );
  }

  /** Last combat result and, if the attacker may occupy the hex, the button for it. */
  private showResult(events: unknown[]) {
    const box = document.getElementById("combat")!;
    const combat = events.find((e) => (e as { type: string }).type === "combat_resolved") as
      | { attacker_losses: number; defender_losses: number }
      | undefined;
    const bombard = events.find((e) => (e as { type: string }).type === "bombarded") as { losses: number } | undefined;
    const text = combat
      ? `${this.t("ui.result")}: −${combat.attacker_losses} | −${combat.defender_losses}`
      : bombard
        ? `${this.t("ui.result")}: −0 | −${bombard.losses}`
        : "";
    box.innerHTML = `<div class="big">${text}</div><div class="actions" id="after"></div>`;
    box.style.display = text ? "block" : "none";
    this.showAdvanceOrHide(true);
  }

  private showAdvanceOrHide(keepResult = false) {
    const box = document.getElementById("combat")!;
    const p = this.state?.pending_advance;
    if (p) {
      if (!keepResult) box.innerHTML = `<div class="actions" id="after"></div>`;
      const after = document.getElementById("after") ?? box;
      after.innerHTML = `<button id="advance">${this.t("ui.advance")}</button>`;
      box.style.display = "block";
      document.getElementById("advance")!.onclick = () => void this.run(() => this.game.advance(p[0]));
    } else if (!keepResult) {
      box.style.display = "none";
    }
  }

  /** Runs a core command; on success refreshes the view and returns its events. */
  private async run(command: () => string, endPhase = false): Promise<unknown[]> {
    try {
      const events = JSON.parse(command()) as unknown[];
      this.record(events);
      if (endPhase) this.deselect();
      await this.refresh();
      if (!this.selection) this.showAdvanceOrHide();
      return events;
    } catch (e) {
      const box = document.getElementById("combat")!;
      box.innerHTML = `<div class="bad">${String(e)}</div>`;
      box.style.display = "block";
      return [];
    }
  }

  /** Adds events to the reports; scenario messages also pop up. */
  private record(events: unknown[]) {
    const ctx = { ru: this.ru, units: this.known, sideName: (id: number | null) => this.sideName(id) };
    const messages: string[] = [];
    for (const e of events as ({ type: string } & Record<string, unknown>)[]) {
      const line = formatEvent(e, ctx);
      if (!line) continue;
      this.log.push(line);
      if (e.type === "message") messages.push(line);
    }
    if (this.log.length > LOG_LIMIT) this.log.splice(0, this.log.length - LOG_LIMIT);
    if (messages.length) this.toast(messages.join("<br>"));
  }

  private toast(html: string) {
    let el = document.getElementById("toast");
    if (!el) {
      el = document.createElement("div");
      el.id = "toast";
      el.className = "panel";
      document.body.appendChild(el);
    }
    el.innerHTML = html;
    el.style.display = "block";
    const shown = el;
    window.setTimeout(() => {
      if (shown.innerHTML === html) shown.style.display = "none";
    }, TOAST_MS);
  }

  private showOutcome() {
    const o = this.state.outcome!;
    const modal = document.getElementById("modal")!;
    const winner = o.winner === null ? this.t("ui.outcome.draw") : `${this.t(`ui.outcome.${o.level}`)}: ${this.sideName(o.winner)}`;
    modal.querySelector(".panel")!.innerHTML = `<h2>${winner}</h2>
      <p>${this.t(`ui.reason.${o.reason}`)}</p>
      <p>${o.vp.map(([id, vp]) => `${this.sideName(id)}: ${vp}`).join(" · ")} ${this.t("ui.vp")}</p>
      <button id="again">${this.t("ui.new_game")}</button>`;
    modal.style.display = "flex";
    document.getElementById("again")!.onclick = () => location.reload();
  }
}
