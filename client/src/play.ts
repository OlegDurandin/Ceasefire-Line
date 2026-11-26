// Hotseat play on a scenario (stage 3): select, move, attack, forecast,
// reinforce, transfer, HQ pins, bridges, purchases, tech boosts, reports,
// end phase, outcome. All rules run in the core (WASM); actions are enabled
// by the core's own dry run (`Game.check`).

import { Assets, Container, Graphics, type Texture } from "pixi.js";

import type { Camera } from "./camera";
import type { BridgeTarget, EconomyView, Forecast, GameStateView, LiveUnit, Reach, Target, TransferTargets } from "./game-view";
import { axialToOffset, hexCenter, offsetToAxial, pixelToAxial } from "./hex";
import { attrText, turnDate, type Rules } from "./explain";
import { applyFacing, type Facing } from "./facing";
import { fill, formatEvent, type UnitRef } from "./log";
import { drawControl } from "./render/control";
import { drawHqLinks, drawObjectives, drawSupply, drawUmbrellas, drawZoc } from "./render/layers";
import { AttackWindow, type CardData, type Explained } from "./ui/attack-window";
import { drawBridges, setGridActive, type MapTextures } from "./render/map";
import { drawOverlays } from "./render/overlays";
import { drawUnits, expStars, type Token } from "./render/units";
import { pickSprite, styledSprite, type MiniIndex, type SpritesFile, type UnitStyle } from "./sprites";
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
type Layer = "supply" | "zoc" | "hq" | "umbrella" | "objectives";

/** Data the panels explain numbers with (R-UI-04). */
export interface PlayContext {
  rules: Rules;
  /** Terrain id → max entrenchment (data/terrain.json). */
  maxEntrench: Map<string, number>;
  startDate?: string;
  startTurn: number;
  suddenDeathPct: number;
  /** R-UI-03: unit pictures — miniatures (default) or medallions. */
  style: UnitStyle;
  /** Key of the autosave in localStorage. */
  saveKey: string;
  /** R-UI-03: default facing per side id (scenario.json `sides[].facing`). */
  sideFacing: Facing[];
  minis: MiniIndex;
}

const NIGHT_TINT = 0x8796c4;
const LOG_LIMIT = 300;
/** R-UI-04: how long the actual losses stay in the attack window. */
const RESULT_MS = 1100;
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
  /** R-UI-03: where each unit faces after its last move or attack. */
  private facing = new Map<number, Facing>();
  /** R-UI-02 map layers the player switched on. */
  private layers = new Set<Layer>(["objectives"]);
  private aw: AttackWindow;
  /** The target the pointer is over (preview without aiming). */
  private hovered: number | null = null;
  private layerBox = new Container();

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
    private ctx: PlayContext,
  ) {
    world.addChild(this.bridges);
    world.addChild(this.control);
    world.addChild(this.layerBox);
    world.addChild(this.overlay);
    world.addChild(this.unitsLayer);
    camera.onTap = (x, y) => this.tap(x, y);
    window.addEventListener("keydown", (e) => this.key(e));
    this.aw = new AttackWindow(document.getElementById("attack")!, ru, ctx.rules);
    camera.onHover = (x, y) => this.hover(x, y);
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
    drawControl(this.control, this.size, owners, this.colors, this.state.neutral);
    this.drawLayers();
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
      const base = pickSprite(this.sprites, u.class, u.nation, tech);
      const ref = base ? styledSprite(base, this.ctx.style, this.sprites, this.ctx.minis) : null;
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
        facing: this.facing.get(u.id) ?? this.ctx.sideFacing[u.side] ?? "left",
        lowSupply: u.supply <= this.ctx.rules.supply.low_supply_max,
        encircled: u.hex_supply === 0,
        style: ref?.style ?? "medallion",
        anchor: ref?.anchor ?? [0.5, 0.5],
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
    const date = turnDate(this.ctx.startDate, this.ctx.startTurn, s.turn, this.ru);
    const income = JSON.parse(this.game.economy()) as EconomyView;
    const tip = (key: string, args: Record<string, string | number>) => attrText(fill(this.t(key), args));
    top.innerHTML = `
      <span><b>${this.sideName(s.active_side)}</b></span>
      <span>${date ? `${date} · ` : ""}${this.t("ui.turn")} ${s.turn} · ${s.night ? this.t("ui.night") : this.t("ui.day")}</span>
      <span title="${tip("ui.mpp_tip", { income: income.base_income })}">MPP ${side?.mpp ?? 0}</span>
      <span title="${tip("ui.nm_tip", { death: this.ctx.suddenDeathPct })}">NM ${s.sides.map((x) => `${this.sideName(x.id)} ${x.national_morale_pct}%`).join(" · ")}</span>
      <span class="muted" title="${tip("ui.ceasefire_tip", { turn: s.last_turn ?? "—" })}">${this.t("ui.to_ceasefire")}: ${left}</span>
      <span class="tabs">${(["map", "purchases", "tech", "reports"] as const)
        .map((p) => `<button data-panel="${p}" class="${(this.panel ?? "map") === p ? "on" : ""}">${this.t(`ui.${p}`)}</button>`)
        .join("")}</span>
      <button id="to-menu" title="${attrText(this.t("ui.menu_tip"))}">${this.t("ui.menu")}</button>`;
    document.getElementById("to-menu")!.onclick = () => this.menu();
    this.drawToolbar();
    for (const b of top.querySelectorAll<HTMLButtonElement>("[data-panel]")) {
      b.onclick = () => {
        const p = b.dataset.panel!;
        this.panel = p === "map" || this.panel === p ? null : (p as Panel);
        this.drawTop();
        this.drawPanel();
      };
    }
  }

  /** R-UI-04: the strip of icons above the attack window — layers, next unit, undo, end of phase. */
  private drawToolbar() {
    const bar = document.getElementById("toolbar")!;
    const layer = (l: Layer, icon: string) =>
      `<button data-layer="${l}" class="icon${this.layers.has(l) ? " on" : ""}" title="${attrText(this.t(`ui.layer.${l}`))}">${icon}</button>`;
    bar.innerHTML = `<span class="group">${(["supply", "zoc", "hq", "umbrella", "objectives"] as const).map((l) => layer(l, this.t(`ui.layer_short.${l}`))).join("")}</span>
      <span class="group"><button id="next-unit" class="icon" title="${attrText(this.t("ui.next_unit"))}">N</button>
      <button id="undo" title="${attrText(this.t("ui.undo_tip"))}"${this.game.canUndo() ? "" : " disabled"}>↶ ${this.t("ui.undo")}</button>
      <button id="end-phase" title="Enter">${this.t("ui.end_phase")} ⏎</button></span>`;
    bar.style.display = this.state.outcome ? "none" : "flex";
    for (const b of bar.querySelectorAll<HTMLButtonElement>("[data-layer]")) {
      b.onclick = () => {
        const l = b.dataset.layer as Layer;
        if (this.layers.has(l)) this.layers.delete(l);
        else this.layers.add(l);
        this.drawToolbar();
        this.drawLayers();
      };
    }
    document.getElementById("next-unit")!.onclick = () => this.nextUnit();
    document.getElementById("undo")!.onclick = () => void this.undo();
    document.getElementById("end-phase")!.onclick = () => this.endPhase();
  }

  /** R-UI-02: supply heat map and enemy ZOC of the active side, HQ links, AA umbrellas, objectives. */
  private drawLayers() {
    this.layerBox.removeChildren().forEach((c) => c.destroy({ children: true }));
    const side = this.state.active_side;
    if (this.layers.has("supply")) {
      this.layerBox.addChild(drawSupply(this.size, JSON.parse(this.game.supplyMap(side)) as [number, number, number][]));
    }
    if (this.layers.has("zoc")) this.layerBox.addChild(drawZoc(this.size, JSON.parse(this.game.zoc(side)) as [number, number][]));
    if (this.layers.has("hq")) this.layerBox.addChild(drawHqLinks(this.size, this.state.units, this.colors));
    if (this.layers.has("umbrella")) this.layerBox.addChild(drawUmbrellas(this.size, this.state.units, this.colors));
    if (this.layers.has("objectives")) this.layerBox.addChild(drawObjectives(this.size, this.state.objectives, this.colors));
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
    this.hovered = null;
    this.redrawOverlay();
    this.showInfo(unit);
    if (aimed) this.aim(aimed);
    else {
      this.aw.showUnit(this.card(unit));
      this.showAdvanceOrHide();
    }
  }

  private deselect() {
    this.selection = null;
    this.transfer = null;
    this.hovered = null;
    this.aw.hide();
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

  /** Orders of the selected unit (top left); its numbers are in the attack window's card. */
  private showInfo(u: LiveUnit) {
    const info = document.getElementById("info")!;
    const actions = this.actions(u);
    info.style.display = actions.includes("<button") || actions.includes("<select") ? "block" : "none";
    info.innerHTML = `<b class="caps">${this.t("ui.orders")}</b> · ${this.t(`unit.${u.class}`, u.class)}${actions}`;
    this.bindActions(info, u);
  }

  /** R-UI-04 card data of a unit for the attack window. */
  private card(u: LiveUnit): CardData {
    const tech = this.state.sides.find((x) => x.id === u.side)?.tech ?? {};
    const base = pickSprite(this.sprites, u.class, u.nation, tech);
    const ref = base ? styledSprite(base, this.ctx.style, this.sprites, this.ctx.minis) : null;
    const nation = u.nation ? this.t(`nation.${u.nation}`, "") : "";
    return {
      unit: u,
      title: `${this.t(`unit.${u.class}`, u.class)} (${nation || this.sideName(u.side)})`,
      portrait: ref?.url ?? null,
      mirrored: (this.facing.get(u.id) ?? this.ctx.sideFacing[u.side] ?? "left") === "right",
      stars: expStars(u.exp),
      maxEntrench: this.ctx.maxEntrench.get(u.terrain) ?? null,
      terrainName: this.t(`terrain.${u.terrain}`, u.terrain),
    };
  }

  /** Direction of an attack on the screen, degrees clockwise from east. */
  private attackDeg(a: LiveUnit, d: LiveUnit): number {
    const pa = hexCenter(offsetToAxial({ col: a.col, row: a.row }), 1);
    const pd = hexCenter(offsetToAxial({ col: d.col, row: d.row }), 1);
    return (Math.atan2(pd.y - pa.y, pd.x - pa.x) * 180) / Math.PI;
  }

  /** Forecast for a target: hover (preview) or aim (with the buttons). */
  private showForecast(target: Target, aimed: boolean) {
    const sel = this.selection;
    const defender = this.state.units.find((u) => u.id === target.target);
    if (!sel || !defender) return;
    const f = JSON.parse(this.game.forecast(sel.unit.id, target.target)) as Forecast;
    const ex = JSON.parse(this.game.explain(sel.unit.id, target.target)) as Explained;
    const actions = aimed
      ? `<button id="strike">${this.t(target.kind === "attack" ? "ui.attack" : "ui.bombard")}</button> <button id="cancel">${this.t("ui.cancel")}</button>`
      : "";
    this.aw.showTarget(this.card(sel.unit), this.card(defender), f, ex, this.attackDeg(sel.unit, defender), actions);
    if (aimed) {
      document.getElementById("strike")!.onclick = () => this.selection && this.strike(this.selection.unit, target);
      document.getElementById("cancel")!.onclick = () => {
        if (!this.selection) return;
        this.selection.aimed = null;
        this.aw.showUnit(this.card(this.selection.unit));
      };
    }
  }

  /** R-UI-04: the pointer over a target shows its forecast without aiming. */
  private hover(x: number, y: number) {
    const sel = this.selection;
    if (!sel || sel.aimed || this.state.outcome) return;
    const { col, row } = axialToOffset(pixelToAxial(this.camera.toWorld(x, y), this.size));
    const there = this.unitAt(col, row);
    const target = there ? sel.targets.find((t) => t.target === there.id) : undefined;
    const id = target?.target ?? null;
    if (id === this.hovered) return;
    this.hovered = id;
    if (target) this.showForecast(target, false);
    else this.aw.showUnit(this.card(sel.unit));
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
    this.showForecast(target, true);
  }

  private strike(unit: LiveUnit, target: Target) {
    const before = this.card(unit);
    const defBefore = this.state.units.find((u) => u.id === target.target);
    const defCard = defBefore ? this.card(defBefore) : null;
    void this.run(() => (target.kind === "attack" ? this.game.attack(unit.id, target.target) : this.game.bombard(unit.id, target.target))).then(
      (events) => {
        if (this.selection) this.selection.aimed = null;
        this.showResult(events, before, defCard);
      },
    );
  }

  /** R-UI-04: the actual losses in the window's centre for ~1 s, then the unit card again. */
  private showResult(events: unknown[], att: CardData, def: CardData | null) {
    const combat = events.find((e) => (e as { type: string }).type === "combat_resolved") as
      | { attacker_losses: number; defender_losses: number }
      | undefined;
    const bombard = events.find((e) => (e as { type: string }).type === "bombarded") as { losses: number } | undefined;
    if (!combat && !bombard) return;
    // Cards after the combat (a destroyed unit keeps its last card).
    const now = (c: CardData | null) => {
      const u = c && this.state.units.find((x) => x.id === c.unit.id);
      return u ? this.card(u) : c;
    };
    this.aw.showResult(now(att), now(def), combat?.attacker_losses ?? 0, combat?.defender_losses ?? bombard!.losses, RESULT_MS, () => {
      if (this.selection) this.aw.showUnit(this.card(this.selection.unit));
      else this.aw.hide();
    });
    this.showAdvanceOrHide();
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

  /** R-UI-05 keys: N next unit, Enter end of phase, Ctrl/Cmd+Z undo, U unit pictures. */
  private key(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || this.state?.outcome) return;
    const k = e.key.toLowerCase();
    if (k === "n" || k === "т") this.nextUnit();
    else if (e.key === "Enter") this.endPhase();
    else if (k === "z" && (e.ctrlKey || e.metaKey)) void this.undo();
    else if (k === "u" || k === "г") {
      // R-UI-03: miniatures ↔ medallions.
      this.ctx.style = this.ctx.style === "miniature" ? "medallion" : "miniature";
      void this.drawUnits();
    } else return;
    e.preventDefault();
  }

  /** Own units that can still move or strike (the same test as the dimming). */
  private ready(): LiveUnit[] {
    return this.state.units.filter(
      (u) => u.side === this.state.active_side && ((u.can_move && u.ap_halves > 0) || (u.strikes_left > 0 && u.ap_halves > 0)),
    );
  }

  /** R-UI-05: N — the next unit that has not acted, in id order, cycling. */
  private nextUnit() {
    const ready = this.ready();
    if (!ready.length) return;
    const cur = this.selection?.unit.id ?? -1;
    const next = ready.find((u) => u.id > cur) ?? ready[0]!;
    this.select(next);
    const c = hexCenter(offsetToAxial({ col: next.col, row: next.row }), this.size);
    this.camera.centerOn(c.x, c.y);
  }

  /** R-UI-05: warn before ending the phase with strikes left on units that have a target. */
  private endPhase() {
    const unused = this.state.units.filter(
      (u) => u.side === this.state.active_side && u.strikes_left > 0 && (JSON.parse(this.game.targets(u.id)) as Target[]).length > 0,
    );
    if (!unused.length) {
      void this.run(() => this.game.endPhase(), true);
      return;
    }
    const box = document.getElementById("combat")!;
    box.innerHTML = `<div>${fill(this.t("ui.unused_strikes"), { n: unused.length })}</div>
      <div class="detail">${unused.map((u) => this.unitLabel(u)).join(" · ")}</div>
      <div class="actions"><button id="end-anyway">${this.t("ui.end_phase")}</button><button id="end-back">${this.t("ui.back")}</button></div>`;
    box.style.display = "block";
    document.getElementById("end-anyway")!.onclick = () => void this.run(() => this.game.endPhase(), true);
    document.getElementById("end-back")!.onclick = () => this.showAdvanceOrHide();
  }

  /** «В меню»: the game is saved (it saves after every command anyway), then the menu. */
  private menu() {
    this.save();
    const modal = document.getElementById("modal")!;
    const box = modal.querySelector(".panel")!;
    box.innerHTML = `<h2>${this.t("ui.menu")}</h2><p class="muted">${this.t("ui.menu_saved")}</p>
      <div class="menu"><button id="m-continue">${this.t("ui.continue")}</button>
      <button id="m-new">${this.t("ui.new_game")}</button>
      <button id="m-editor">${this.t("ui.open_editor")}</button></div>`;
    modal.style.display = "flex";
    document.getElementById("m-continue")!.onclick = () => (modal.style.display = "none");
    document.getElementById("m-new")!.onclick = () => {
      try {
        localStorage.removeItem(this.ctx.saveKey);
      } catch {
        // no storage: nothing to clear
      }
      location.reload();
    };
    document.getElementById("m-editor")!.onclick = () => {
      const p = new URLSearchParams(location.search);
      p.set("mode", "edit");
      location.search = p.toString();
    };
  }

  /** R-UI-05: take back the last movement (until anything else happens). */
  private async undo() {
    if (!this.game.undoLast()) return;
    this.log.push(this.t("ui.undone"));
    this.save();
    await this.refresh();
    if (!this.selection) this.showAdvanceOrHide();
  }

  /** Autosave: the command log replays the game (deterministic core). */
  private save() {
    try {
      if (this.state?.outcome) localStorage.removeItem(this.ctx.saveKey);
      else localStorage.setItem(this.ctx.saveKey, this.game.commands());
    } catch {
      // Storage may be unavailable (private mode); the game goes on without saves.
    }
  }

  /** Runs a core command; on success refreshes the view and returns its events. */
  private async run(command: () => string, endPhase = false): Promise<unknown[]> {
    try {
      const colBefore = new Map(this.state.units.map((u) => [u.id, u.col] as const));
      const sideOf = new Map(this.state.units.map((u) => [u.id, u.side] as const));
      const events = JSON.parse(command()) as unknown[];
      applyFacing(
        this.facing,
        events as ({ type: string } & Record<string, unknown>)[],
        (id) => colBefore.get(id),
        (id) => this.ctx.sideFacing[sideOf.get(id) ?? 0] ?? "left",
      );
      this.record(events);
      if (endPhase) this.deselect();
      await this.refresh();
      this.save();
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
    this.save();
    document.getElementById("again")!.onclick = () => location.reload();
  }
}
