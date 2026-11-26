// Hotseat play on a scenario (stage 3): select, move, attack, forecast,
// reinforce, transfer, HQ pins, bridges, purchases, tech boosts, reports,
// end phase, outcome. All rules run in the core (WASM); actions are enabled
// by the core's own dry run (`Game.check`). The HUD follows the approved mock
// (R-UI-04): top bar, action bar, bottom window in attack or hex mode.

import { Assets, Container, Graphics, type Texture } from "pixi.js";

import type { Camera } from "./camera";
import type { BridgeTarget, EconomyView, Forecast, GameStateView, LiveUnit, Reach, Target, TransferTargets } from "./game-view";
import { axialToOffset, hexCenter, offsetToAxial, pixelToAxial } from "./hex";
import { attrText, turnDate, type Rules } from "./explain";
import { applyFacing, type Facing } from "./facing";
import { encodeSave } from "./save";
import { fill, formatEvent, type UnitRef } from "./log";
import { drawControl } from "./render/control";
import { drawHqLinks, drawObjectives, drawSupply, drawUmbrellas, drawZoc } from "./render/layers";
import { AttackWindow, type CardData, type Explained } from "./ui/attack-window";
import { ACTIONS, actionBarHtml, type ActionId, type ActionState } from "./ui/action-bar";
import { hexCardHtml, type HexFacts, type TerrainInfo } from "./ui/hex-info";
import { Minimap, type Rect } from "./ui/minimap";
import { refusalText } from "./ui/refusal";
import { flagCss, topBarHtml, type Layer, type Tab } from "./ui/top-bar";
import { drawBridges, setGridActive, type MapTextures } from "./render/map";
import { drawOverlays } from "./render/overlays";
import type { FlagStripes } from "./render/token-geom";
import { drawUnits, expStars, type Token } from "./render/units";
import { tokenHtml } from "./ui/token-svg";
import { pickSprite, styledSprite, type MiniIndex, type SpritesFile, type UnitStyle } from "./sprites";
import type { HexView } from "./view";
import type { Game } from "./wasm/ceasefire_wasm.js";

type Locale = Record<string, string>;

interface Selection {
  unit: LiveUnit;
  reach: Reach[];
  targets: Target[];
  /** Target picked for the forecast; a second tap or the button confirms. */
  aimed: Target | null;
}

type Cell = { col: number; row: number };

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
  /** Key of the client settings in localStorage. */
  settingsKey: string;
  /** R-UI-03: default facing per side id (scenario.json `sides[].facing`). */
  sideFacing: Facing[];
  /** R-UI-03: facing restored from the save (unit id → facing). */
  savedFacing: Map<number, Facing>;
  /** R-UI-03: flag stripes per side id (scenario.json `sides[].flag`). */
  flags: (FlagStripes | null)[];
  /** Class → full AP (data/units.json), for «AP тек. / макс.». */
  classAp: Map<string, number>;
  minis: MiniIndex;
  /** R-UI-04 hex mode: the map's hexes by "col,row", hexes with a road or a track. */
  hexes: Map<string, HexView>;
  roadHexes: Set<string>;
  trackHexes: Set<string>;
  /** data/terrain.json per terrain id, road and track AP. */
  terrain: Map<string, TerrainInfo>;
  roadAp: number;
  trackAp: number;
  terrainTexture: (terrain: string) => string | null;
  /** Minimap: the world rect of the map and the baked overview image. */
  minimap: { world: Rect; image: { url: string; rect: Rect } | null };
}

const NIGHT_TINT = 0x8796c4;
const LOG_LIMIT = 300;
/** R-UI-04: how long the actual losses stay in the attack window. */
const RESULT_MS = 1100;
/** Short notices («Партия сохранена») and scenario messages (story text, longer to read). */
const NOTICE_MS = 2500;
const MESSAGE_MS = 7000;
/** Facing snapshots kept for «Отменить» (only movements are undone, so a few suffice). */
const FACING_UNDO_LIMIT = 50;
/** Minimap size in CSS px (fits the window's centre column). */
const MINIMAP_W = 132;
const MINIMAP_H = 124;
/** Command JSON for the core: hexes are axial. */
const hex = ([col, row]: [number, number]) => offsetToAxial({ col, row });
const attr = (v: unknown) => encodeURIComponent(JSON.stringify(v));
const key = (col: number, row: number) => `${col},${row}`;
const css = (c: number) => `#${c.toString(16).padStart(6, "0")}`;
/** A side colour mixed with white for the NM bars, as on the mock. */
const light = (c: number, k = 0.45) => {
  const m = (s: number) => Math.round(((c >> s) & 0xff) * (1 - k) + 255 * k);
  return css((m(16) << 16) | (m(8) << 8) | m(0));
};

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
  private tab: Tab = "map";
  /** R-MOVE-05: transfer mode for the selected unit. */
  private transfer: TransferTargets | null = null;
  /** R-UI-03: where each unit faces after its last move or attack. */
  private facing = new Map<number, Facing>();
  /** Facing before each command, so «Отменить» turns the unit back too. */
  private facingUndo: Map<number, Facing>[] = [];
  /** R-UI-02 map layers the player switched on. */
  private layers = new Set<Layer>(["objectives"]);
  private aw: AttackWindow;
  /** The target the pointer is over (preview without aiming). */
  private hovered: number | null = null;
  /** Selection and targets the tokens were last drawn with. */
  private haloKey = "";
  private layerBox = new Container();
  /** R-UI-04: the bottom window shows the unit (attack mode) or hexes. */
  private info: "unit" | "hex" = "unit";
  /** Hex mode: the selected hex and the hex under the pointer. */
  private picked: Cell | null = null;
  private pointer: Cell | null = null;
  /** The line bottom left: the hex last hovered, tapped or long-pressed. */
  private lineCell: Cell | null = null;
  /** Own supply and enemy ZOC of the active side, per refresh (hex mode). */
  private supplyHere: Map<string, number> | null = null;
  private zocHere: Set<string> | null = null;
  private minimap: Minimap;
  /** A result is on show in the attack window (the window stays in attack mode). */
  private showingResult = false;

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
    this.facing = new Map(ctx.savedFacing);
    camera.onTap = (x, y) => this.tap(x, y);
    window.addEventListener("keydown", (e) => this.key(e));
    this.aw = new AttackWindow(document.getElementById("attack")!, ru, ctx.rules);
    camera.onHover = (x, y) => this.hover(x, y);
    // Touch: a long press is the hover («гекс под курсором» and a target's forecast).
    camera.onLongPress = (x, y) => this.hover(x, y);
    const mm = ctx.minimap;
    this.minimap = new Minimap(mm.world, mm.image?.rect ?? null, mm.image?.url ?? null, MINIMAP_W, MINIMAP_H, this.t("ui.minimap"), (x, y) => {
      this.camera.centerOn(x, y);
    });
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
    this.supplyHere = null;
    this.zocHere = null;
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
    } else {
      this.drawActionBar();
      this.showWindow();
    }
    this.drawPanel();
    if (this.state.outcome) this.showOutcome();
  }

  /** The camera moved: the minimap's frame follows. */
  onCamera() {
    this.drawMinimap();
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
        selected: this.selection?.unit.id === u.id,
        target: !!this.selection?.targets.some((t) => t.target === u.id),
      });
    }
    this.unitsLayer.removeChildren().forEach((c) => c.destroy({ children: true }));
    this.unitsLayer.addChild(drawUnits(tokens, tex, this.size, this.colors, this.ctx.flags));
  }

  private flag(side: number): string {
    return flagCss(this.ctx.flags[side] ?? null, css(this.colors[side] ?? 0xb08d57));
  }

  /** R-UI-04 top bar (component «Верхняя панель»). */
  private drawTop() {
    const s = this.state;
    const side = s.sides.find((x) => x.id === s.active_side);
    const income = JSON.parse(this.game.economy()) as EconomyView;
    const tip = (k: string, args: Record<string, string | number>) => fill(this.t(k), args);
    const top = document.getElementById("top")!;
    top.innerHTML = topBarHtml(
      {
        side: this.sideName(s.active_side),
        flag: this.flag(s.active_side),
        mpp: side?.mpp ?? 0,
        mppTip: tip("ui.mpp_tip", { income: income.base_income }),
        nm: side?.national_morale_pct ?? 0,
        nmTip: tip("ui.nm_tip", { death: this.ctx.suddenDeathPct }),
        bars: s.sides.map((x) => ({ name: this.sideName(x.id), flag: this.flag(x.id), pct: x.national_morale_pct, color: light(this.colors[x.id] ?? 0xb08d57) })),
        tab: this.tab,
        date: turnDate(this.ctx.startDate, this.ctx.startTurn, s.turn, this.ru),
        night: s.night,
        turn: s.turn,
        lastTurn: s.last_turn,
        ceasefireTip: tip("ui.ceasefire_tip", { turn: s.last_turn ?? "—" }),
        layers: this.layers,
        over: !!s.outcome,
      },
      this.ru,
    );
    for (const b of top.querySelectorAll<HTMLButtonElement>("[data-tab]")) {
      b.onclick = () => {
        const tab = b.dataset.tab as Tab;
        this.tab = this.tab === tab ? "map" : tab;
        this.drawTop();
        this.drawPanel();
      };
    }
    for (const b of top.querySelectorAll<HTMLButtonElement>("[data-layer]")) {
      b.onclick = () => {
        const l = b.dataset.layer as Layer;
        if (this.layers.has(l)) this.layers.delete(l);
        else this.layers.add(l);
        this.drawTop();
        this.drawLayers();
      };
    }
    const on = (id: string, f: () => void) => {
      const b = document.getElementById(id);
      if (b) b.onclick = f;
    };
    on("end-phase", () => this.endPhase());
    on("settings", () => this.settings());
    on("help", () => this.help());
    on("save-game", () => {
      this.save();
      this.toast(this.t("ui.saved"));
    });
    on("to-menu", () => this.menu());
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

  /** R-UI-04 panels under the tabs: purchases, reinforcements on the way, technologies, reports. */
  private drawPanel() {
    const el = document.getElementById("panel")!;
    if (this.tab === "map") {
      el.style.display = "none";
      return;
    }
    el.style.display = "block";
    const e = JSON.parse(this.game.economy()) as EconomyView;
    const button = (label: string, cmd: object, refused: string | null) =>
      `<button data-cmd="${attr(cmd)}"${refused ? ` disabled title="${attrText(refusalText(refused, this.ru))}"` : ""}>${label}</button>`;
    let html = "";
    if (this.tab === "purchases") {
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
    } else if (this.tab === "reinforcements") {
      html = `<b>${this.t("ui.reinforcements")}</b> · ${this.sideName(e.side)}`;
      html += e.arrivals.length
        ? `<ul>${e.arrivals.map(([c, turn]) => `<li>${this.t(`unit.${c}`, c)} — ${this.t("ui.arrives_turn")} ${turn}</li>`).join("")}</ul>`
        : `<p class="muted">${this.t("ui.no_arrivals")}</p>`;
    } else if (this.tab === "tech") {
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

  private cellAt(x: number, y: number): Cell {
    return axialToOffset(pixelToAxial(this.camera.toWorld(x, y), this.size));
  }

  private tap(x: number, y: number) {
    if (this.state.outcome) return;
    const { col, row } = this.cellAt(x, y);
    if (this.ctx.hexes.has(key(col, row))) {
      this.lineCell = { col, row };
      this.drawPointerLine();
    }
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
    this.picked = this.ctx.hexes.has(key(col, row)) ? { col, row } : null;
    if (there && there.side === this.state.active_side) this.select(there);
    else this.deselect();
  }

  private select(unit: LiveUnit) {
    const reach = JSON.parse(this.game.reachable(unit.id)) as Reach[];
    const targets = JSON.parse(this.game.targets(unit.id)) as Target[];
    const keepAim = this.selection?.unit.id === unit.id ? this.selection.aimed : null;
    const aimed = keepAim && targets.some((t) => t.target === keepAim.target) ? keepAim : null;
    if (this.selection?.unit.id !== unit.id) {
      this.transfer = null;
      this.info = "unit";
    }
    this.selection = { unit, reach, targets, aimed };
    this.picked = { col: unit.col, row: unit.row };
    this.hovered = null;
    this.redrawOverlay();
    this.drawActionBar();
    if (aimed) this.aim(aimed);
    else {
      this.showWindow();
      this.showAdvanceOrHide();
    }
  }

  private deselect() {
    this.selection = null;
    this.transfer = null;
    this.hovered = null;
    this.redrawOverlay();
    this.drawActionBar();
    this.showWindow();
    this.showAdvanceOrHide();
  }

  private redrawOverlay() {
    const sel = this.selection;
    const targetHexes = (sel?.targets ?? [])
      .map((t) => this.state.units.find((u) => u.id === t.target))
      .filter((u): u is LiveUnit => !!u);
    const transfer = (this.transfer?.hexes ?? []).map(([col, row]) => ({ col, row }));
    const picked = !sel ? this.picked : null;
    drawOverlays(this.overlay, this.size, this.transfer ? [] : (sel?.reach ?? []), targetHexes, sel ? sel.unit : picked, transfer);
    setGridActive(this.world, !!sel);
    // R-UI-03: the tokens draw the selection and target halos.
    const k = `${sel?.unit.id ?? ""}|${(sel?.targets ?? []).map((t) => t.target).join(",")}`;
    if (k !== this.haloKey) {
      this.haloKey = k;
      void this.drawUnits();
    }
  }

  /** R-UI-04: the bottom window in its mode — the unit (attack) or hexes with the minimap. */
  private showWindow() {
    if (this.state.outcome) {
      this.aw.hide();
      return;
    }
    const sel = this.selection;
    if (sel && this.info === "unit") {
      if (this.showingResult) return;
      this.aw.showUnit(this.card(sel.unit));
      return;
    }
    this.drawHexWindow();
  }

  /** R-UI-04 hex mode (board «Режим гекса»): selected hex | minimap | hex under the pointer. */
  private drawHexWindow() {
    const el = document.getElementById("attack")!;
    let box = el.querySelector<HTMLElement>(".aw.hexmode");
    if (!box) {
      el.innerHTML = `<div class="aw hexmode"><div id="hx-left"></div>
        <div class="aw-center hx-center"><div id="hx-mm"></div><div class="mm-hint">${this.t("ui.minimap_hint")}</div></div>
        <div id="hx-right"></div></div>`;
      box = el.querySelector<HTMLElement>(".aw.hexmode")!;
      el.querySelector("#hx-mm")!.appendChild(this.minimap.canvas);
    }
    el.style.display = "block";
    this.drawHexCard("left");
    this.drawHexCard("right");
    this.drawMinimap();
  }

  private drawHexCard(side: "left" | "right") {
    const cell = document.getElementById(side === "left" ? "hx-left" : "hx-right");
    if (!cell) return;
    const at = side === "left" ? this.picked : this.pointer;
    const rules = { roadAp: this.ctx.roadAp, trackAp: this.ctx.trackAp, entrenchMinSupply: this.ctx.rules.supply.entrench_min_supply };
    const touch = window.matchMedia("(hover: none)").matches;
    cell.outerHTML = hexCardHtml(at ? this.hexFacts(at) : null, side === "left" ? "selected" : "cursor", side, this.ru, rules, touch).replace(
      `class="aw-card ${side}`,
      `id="hx-${side}" class="aw-card ${side}`,
    );
  }

  private drawMinimap() {
    if (!this.state || !this.minimap.canvas.isConnected) return;
    const a = this.camera.toWorld(0, 0);
    const b = this.camera.toWorld(window.innerWidth, window.innerHeight);
    const owners = new Map(this.state.control.map(([[c, r], s]) => [key(c, r), s] as const));
    const hexes = [...this.ctx.hexes.values()].map((h) => {
      const p = hexCenter({ q: h.q, r: h.r }, this.size);
      const o = owners.get(key(h.col, h.row));
      return { x: p.x, y: p.y, color: o === undefined ? "#5a5650" : light(this.colors[o] ?? 0xb08d57, 0.2) };
    });
    const dots = this.state.units.map((u) => {
      const p = hexCenter(offsetToAxial({ col: u.col, row: u.row }), this.size);
      return { x: p.x, y: p.y, color: light(this.colors[u.side] ?? 0xb08d57, 0.1) };
    });
    // The frame is clipped to the map, so it stays visible when the whole map is on screen.
    const m = this.ctx.minimap.world;
    const x0 = Math.max(a.x, m.x);
    const y0 = Math.max(a.y, m.y);
    const x1 = Math.min(b.x, m.x + m.w);
    const y1 = Math.min(b.y, m.y + m.h);
    this.minimap.draw(hexes, dots, x1 > x0 && y1 > y0 ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null, this.size);
  }

  /** What hex mode and the pointer line tell about a hex. */
  private hexFacts(c: Cell): HexFacts | null {
    const k = key(c.col, c.row);
    const h = this.ctx.hexes.get(k);
    if (!h) return null;
    const side = this.state.active_side;
    if (!this.supplyHere) this.supplyHere = new Map((JSON.parse(this.game.supplyMap(side)) as [number, number, number][]).map(([x, y, v]) => [key(x, y), v]));
    if (!this.zocHere) this.zocHere = new Set((JSON.parse(this.game.zoc(side)) as [number, number][]).map(([x, y]) => key(x, y)));
    const owner = this.state.control.find(([[x, y]]) => x === c.col && y === c.row)?.[1];
    const u = this.unitAt(c.col, c.row);
    return {
      col: c.col,
      row: c.row,
      terrain: h.terrain,
      terrainName: this.t(`terrain.${h.terrain}`, h.terrain),
      texture: this.ctx.terrainTexture(h.terrain),
      closed: this.state.neutral.some(([x, y]) => x === c.col && y === c.row),
      info: this.ctx.terrain.get(h.terrain) ?? null,
      control: owner === undefined ? this.t("ui.nobody", "—") : this.sideName(owner),
      supply: this.supplyHere.get(k) ?? null,
      road: this.ctx.roadHexes.has(k) ? "road" : this.ctx.trackHexes.has(k) ? "track" : null,
      enemyZoc: this.zocHere.has(k),
      occupant: u ? `${this.unitName(u)}${u.hq_rating !== null ? ` ★${u.hq_rating}` : ""} (${this.sideName(u.side)})` : null,
    };
  }

  /** R-UI-04: the line about the hex under the pointer, bottom left. */
  private drawPointerLine() {
    const el = document.getElementById("hover-line");
    if (!el) return;
    const f = this.lineCell && this.hexFacts(this.lineCell);
    if (!f || this.state.outcome) {
      el.style.display = "none";
      return;
    }
    const target = this.selection?.targets.some((t) => {
      const u = this.state.units.find((x) => x.id === t.target);
      return u && u.col === f.col && u.row === f.row;
    });
    const supply = f.supply === null ? "" : ` · ${this.t("ui.supply_short")} ${f.supply}`;
    const when = this.t(this.state.night ? "ui.night" : "ui.day");
    el.innerHTML = `${this.t(target ? "ui.target" : "ui.cursor")} (${f.col}, ${f.row}) · ${f.terrainName.toLowerCase()} · ${f.control}<br>${when.charAt(0).toUpperCase()}${when.slice(1)}${supply}`;
    el.style.display = "block";
  }

  /** R-UI-04 card data of a unit for the attack window. */
  private card(u: LiveUnit): CardData {
    const tech = this.state.sides.find((x) => x.id === u.side)?.tech ?? {};
    const base = pickSprite(this.sprites, u.class, u.nation, tech);
    const ref = base ? styledSprite(base, "miniature", this.sprites, this.ctx.minis) : null;
    const nation = (u.nation ? this.t(`nation.${u.nation}`, "") : "") || this.sideName(u.side);
    // «Страна · модель (или ПТУР ур. N у пехоты) · штаб».
    const parts = [nation];
    const model = base ? this.ru[`model.${base.id}`] : undefined;
    if (model) parts.push(model);
    else if ((tech.antitank ?? 1) > 1 && u.class === "infantry") parts.push(fill(this.t("ui.atgm_level"), { n: tech.antitank ?? 1 }));
    const hq = this.state.units.find((h) => h.id === u.attached_hq);
    if (u.hq_rating !== null) parts.push(fill(this.t("ui.hq_rating"), { r: u.hq_rating }));
    else if (!hq) parts.push(this.t("ui.not_attached"));
    else parts.push(hq.name ? this.unitName(hq).replace(/^./, (c) => c.toLowerCase()) : fill(this.t("ui.under_hq"), { r: hq.hq_rating ?? "" }));
    const facing = this.facing.get(u.id) ?? this.ctx.sideFacing[u.side] ?? "left";
    const portrait = tokenHtml({
      w: 84,
      color: this.colors[u.side] ?? 0xb08d57,
      flag: this.ctx.flags[u.side] ?? null,
      label: u.hq_rating !== null ? `★${u.hq_rating}` : String(u.strength),
      model: ref?.style === "miniature" ? ref.url : (base?.url ?? null),
      anchor: ref?.style === "miniature" ? ref.anchor : [0.5, 0.7],
      mirrored: facing === "right",
    });
    return {
      unit: u,
      title: this.unitName(u),
      subtitle: parts.join(" · "),
      portrait,
      stars: expStars(u.exp),
      apMax: this.ctx.classAp.get(u.class) ?? null,
      maxEntrench: this.ctx.maxEntrench.get(u.terrain) ?? null,
      terrainName: this.t(`terrain.${u.terrain}`, u.terrain),
      fireName: this.t(`fire.${u.class}`, this.t(`unit.${u.class}`, u.class)),
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
        this.showWindow();
      };
    }
  }

  /** The pointer: the line about its hex, hex mode's right card, a target's forecast. */
  private hover(x: number, y: number) {
    if (!this.state || this.state.outcome) return;
    const c = this.cellAt(x, y);
    const moved = !this.pointer || this.pointer.col !== c.col || this.pointer.row !== c.row;
    this.pointer = this.ctx.hexes.has(key(c.col, c.row)) ? c : null;
    if (this.pointer) this.lineCell = this.pointer;
    if (moved) {
      this.drawPointerLine();
      if (!(this.selection && this.info === "unit")) this.drawHexCard("right");
    }
    const sel = this.selection;
    if (!sel || sel.aimed || this.info !== "unit" || this.showingResult) return;
    const there = this.unitAt(c.col, c.row);
    const target = there ? sel.targets.find((t) => t.target === there.id) : undefined;
    const id = target?.target ?? null;
    if (id === this.hovered) return;
    this.hovered = id;
    if (target) this.showForecast(target, false);
    else this.showWindow();
  }

  /** R-UI-04: the formation's name from the scenario, or the class when it has none. */
  private unitName(u: LiveUnit): string {
    const cls = this.t(`unit.${u.class}`, u.class);
    return u.name ? this.t(u.name, cls) : cls;
  }

  /** Unit name with its hex, for HQ and target lists. */
  private unitLabel(u: LiveUnit): string {
    return `${this.unitName(u)}${u.hq_rating !== null ? ` ★${u.hq_rating}` : ""} [${u.col}, ${u.row}]`;
  }

  /** The core's refusal of a command, in Russian; null if it would be accepted. */
  private refusal(cmd: object): string | null {
    const r = this.game.check(JSON.stringify(cmd));
    return r ? refusalText(r, this.ru) : null;
  }

  /** R-UI-04 action bar: each action's availability and reason from the core. */
  private actionStates(): Record<ActionId, ActionState> {
    const u = this.selection?.unit;
    const out = {} as Record<ActionId, ActionState>;
    const none = { enabled: false, tip: this.t("ui.act.no_unit") };
    for (const a of ACTIONS) out[a] = none;
    out.heli = { enabled: false, tip: this.t("ui.act.heli_off") };
    const canUndo = this.game.canUndo();
    out.undo = { enabled: canUndo, tip: this.t(canUndo ? "ui.act.undo_tip" : "ui.act.undo_off") };
    if (!u || this.state.outcome) return out;
    const cost = (elite: boolean) => {
      try {
        return this.game.reinforceCost(u.id, 1, elite);
      } catch {
        return "—";
      }
    };
    for (const elite of [false, true]) {
      const refused = this.refusal({ type: "reinforce", unit: u.id, steps: 1, elite });
      out[elite ? "elite" : "reinforce"] = {
        enabled: !refused,
        tip: refused ?? fill(this.t(elite ? "ui.act.elite_tip" : "ui.act.reinforce_tip"), { cost: cost(elite) }),
      };
    }
    const tt = JSON.parse(this.game.transferTargets(u.id)) as TransferTargets;
    if (tt.cost !== null && tt.hexes.length) {
      out.transfer = { enabled: true, on: !!this.transfer, tip: this.transfer ? this.t("ui.act.transfer_on") : fill(this.t("ui.act.transfer_tip"), { cost: tt.cost }) };
    } else {
      const own = hex([u.col, u.row]);
      out.transfer = { enabled: false, tip: tt.cost === null ? (this.refusal({ type: "transfer", unit: u.id, to: own }) ?? this.t("ui.act.no_targets")) : this.t("refusal.transfer.NoRoute") };
    }
    const pinRefused = this.refusal({ type: "pin_to_hq", unit: u.id, hq: null });
    const hq = this.state.units.find((h) => h.id === u.attached_hq);
    out.pin = { enabled: !pinRefused, tip: pinRefused ?? fill(this.t("ui.act.pin_tip"), { hq: hq ? this.unitLabel(hq) : this.t("ui.not_attached") }) };
    const bridges = JSON.parse(this.game.bridgeTargets(u.id)) as BridgeTarget[];
    const bombards = (this.selection?.targets ?? []).filter((t) => t.kind === "bombard");
    if (bombards.length || bridges.some((b) => b.kind === "bombard")) {
      out.bombard = { enabled: true, tip: this.t("ui.act.bombard_tip") };
    } else {
      const enemy = this.nearestEnemy(u);
      out.bombard = { enabled: false, tip: enemy ? (this.refusal({ type: "bombard", unit: u.id, target: enemy.id }) ?? this.t("ui.act.no_targets")) : this.t("ui.act.no_targets") };
    }
    if (bridges.some((b) => b.kind === "build")) {
      out.bridge = { enabled: true, tip: this.t("ui.act.bridge_tip") };
    } else {
      const a = offsetToAxial({ col: u.col, row: u.row });
      out.bridge = { enabled: false, tip: this.refusal({ type: "build_bridge", unit: u.id, toward: { q: a.q + 1, r: a.r } }) ?? this.t("ui.act.no_targets") };
    }
    return out;
  }

  private nearestEnemy(u: LiveUnit): LiveUnit | undefined {
    const a = offsetToAxial({ col: u.col, row: u.row });
    const dist = (v: LiveUnit) => {
      const b = offsetToAxial({ col: v.col, row: v.row });
      return (Math.abs(a.q - b.q) + Math.abs(a.r - b.r) + Math.abs(a.q + a.r - b.q - b.r)) / 2;
    };
    return this.state.units.filter((v) => v.side !== u.side).sort((x, y) => dist(x) - dist(y))[0];
  }

  /** R-UI-04 action bar (component «Панель действий»). */
  private drawActionBar() {
    const el = document.getElementById("actionbar")!;
    if (this.state.outcome) {
      el.style.display = "none";
      return;
    }
    el.style.display = "flex";
    const states = this.actionStates();
    el.innerHTML = actionBarHtml(states, this.selection && this.info === "unit" ? "unit" : "hex", this.ru);
    document.getElementById("prev-unit")!.onclick = () => this.nextUnit(-1);
    document.getElementById("next-unit")!.onclick = () => this.nextUnit(1);
    document.getElementById("info-mode")!.onclick = () => this.toggleInfo();
    for (const b of el.querySelectorAll<HTMLButtonElement>("[data-action]")) {
      const a = b.dataset.action as ActionId;
      b.onclick = () => {
        if (states[a].enabled) this.act(a);
      };
    }
  }

  private act(a: ActionId) {
    const u = this.selection?.unit;
    if (a === "undo") {
      void this.undo();
      return;
    }
    if (!u) return;
    if (a === "reinforce" || a === "elite") {
      void this.run(() => this.game.apply(JSON.stringify({ type: "reinforce", unit: u.id, steps: 1, elite: a === "elite" })));
    } else if (a === "transfer") {
      this.transfer = this.transfer ? null : (JSON.parse(this.game.transferTargets(u.id)) as TransferTargets);
      this.redrawOverlay();
      this.drawActionBar();
    } else if (a === "pin") {
      const hqs = this.state.units.filter((h) => h.side === u.side && h.hq_rating !== null);
      const choice = (hq: number | null, label: string, on: boolean) =>
        `<button data-cmd="${attr({ type: "pin_to_hq", unit: u.id, hq })}" class="${on ? "on" : ""}">${label}</button>`;
      this.popover(this.t("ui.act.choose_hq"), [
        choice(null, this.t("ui.hq_auto"), u.pinned_hq === null),
        ...hqs.map((h) => choice(h.id, this.unitLabel(h), u.pinned_hq === h.id)),
      ]);
    } else if (a === "bombard") {
      const sel = this.selection!;
      const targets = sel.targets.filter((t) => t.kind === "bombard");
      const bridges = (JSON.parse(this.game.bridgeTargets(u.id)) as BridgeTarget[]).filter((b) => b.kind === "bombard");
      if (targets.length === 1 && !bridges.length) {
        this.aim(targets[0]!);
        return;
      }
      const items = targets.map((t) => {
        const v = this.state.units.find((x) => x.id === t.target);
        return `<button data-aim="${t.target}">${v ? this.unitLabel(v) : t.target}</button>`;
      });
      for (const b of bridges) {
        items.push(`<button data-cmd="${attr({ type: "bombard_bridge", unit: u.id, a: hex(b.a), b: hex(b.b) })}">${this.t("ui.bombard_bridge")} [${b.a.join(", ")}]–[${b.b.join(", ")}]</button>`);
      }
      this.popover(this.t("ui.act.choose_target"), items);
    } else if (a === "bridge") {
      const builds = (JSON.parse(this.game.bridgeTargets(u.id)) as BridgeTarget[]).filter((b) => b.kind === "build");
      const cmd = (b: BridgeTarget) => ({ type: "build_bridge", unit: u.id, toward: hex(b.b) });
      if (builds.length === 1) {
        void this.run(() => this.game.apply(JSON.stringify(cmd(builds[0]!))));
        return;
      }
      this.popover(
        this.t("ui.act.choose_bridge"),
        builds.map((b) => `<button data-cmd="${attr(cmd(b))}">${this.t("ui.build_bridge")} [${b.a.join(", ")}]–[${b.b.join(", ")}]</button>`),
      );
    }
  }

  /** A short list of choices above the action bar (HQ, bombardment target, bridge). */
  private popover(title: string, items: string[]) {
    const box = document.getElementById("combat")!;
    box.innerHTML = `<div class="pop-title">${title}</div><div class="pop-list">${items.join("")}</div>
      <div class="actions"><button id="pop-close">${this.t("ui.cancel")}</button></div>`;
    box.style.display = "block";
    this.bindCommands(box);
    for (const b of box.querySelectorAll<HTMLButtonElement>("[data-aim]")) {
      b.onclick = () => {
        const t = this.selection?.targets.find((x) => x.target === Number(b.dataset.aim));
        this.showAdvanceOrHide();
        if (t) this.aim(t);
      };
    }
    document.getElementById("pop-close")!.onclick = () => this.showAdvanceOrHide();
  }

  /** R-UI-04 «i»: unit information (attack mode) or hexes. */
  private toggleInfo() {
    if (!this.selection) return;
    this.info = this.info === "unit" ? "hex" : "unit";
    this.hovered = null;
    this.drawActionBar();
    if (this.info === "unit" && this.selection.aimed) this.aim(this.selection.aimed);
    else this.showWindow();
  }

  private aim(target: Target) {
    if (!this.selection) return;
    this.selection.aimed = target;
    if (this.info !== "unit") {
      this.info = "unit";
      this.drawActionBar();
    }
    this.showForecast(target, true);
  }

  private strike(unit: LiveUnit, target: Target) {
    const before = this.card(unit);
    const defBefore = this.state.units.find((u) => u.id === target.target);
    const defCard = defBefore ? this.card(defBefore) : null;
    this.showingResult = true;
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
    if (!combat && !bombard) {
      this.showingResult = false;
      this.showWindow();
      return;
    }
    // Cards after the combat (a destroyed unit keeps its last card).
    const now = (c: CardData | null) => {
      const u = c && this.state.units.find((x) => x.id === c.unit.id);
      return u ? this.card(u) : c;
    };
    this.aw.showResult(now(att), now(def), combat?.attacker_losses ?? 0, combat?.defender_losses ?? bombard!.losses, RESULT_MS, () => {
      this.showingResult = false;
      this.showWindow();
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

  /** R-UI-05 keys: N next unit, Enter end of phase, Ctrl/Cmd+Z undo, U unit pictures, I unit / hex, Esc deselect. */
  private key(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || this.state?.outcome) return;
    if (document.getElementById("modal")!.style.display === "flex") return;
    const k = e.key.toLowerCase();
    if (k === "n" || k === "т") this.nextUnit(e.shiftKey ? -1 : 1);
    else if (e.key === "Enter") this.endPhase();
    else if (k === "z" && (e.ctrlKey || e.metaKey)) void this.undo();
    else if (k === "u" || k === "г") this.setStyle(this.ctx.style === "miniature" ? "medallion" : "miniature");
    else if (k === "i" || k === "ш") this.toggleInfo();
    else if (e.key === "Escape") {
      if (!this.selection) return;
      this.deselect();
    } else return;
    e.preventDefault();
  }

  /** R-UI-03: miniatures ↔ medallions; remembered in the settings. */
  private setStyle(style: UnitStyle) {
    this.ctx.style = style;
    try {
      localStorage.setItem(this.ctx.settingsKey, JSON.stringify({ units: style }));
    } catch {
      // no storage: the choice lasts for this session
    }
    void this.drawUnits();
  }

  /** Own units that can still move or strike (the same test as the dimming). */
  private ready(): LiveUnit[] {
    return this.state.units.filter(
      (u) => u.side === this.state.active_side && ((u.can_move && u.ap_halves > 0) || (u.strikes_left > 0 && u.ap_halves > 0)),
    );
  }

  /** R-UI-05: N — the next unit that has not acted, in id order, cycling. */
  private nextUnit(dir: 1 | -1 = 1) {
    const ready = this.ready();
    if (!ready.length) return;
    const cur = this.selection?.unit.id ?? (dir > 0 ? -1 : Infinity);
    const next =
      dir > 0
        ? (ready.find((u) => u.id > cur) ?? ready[0]!)
        : ([...ready].reverse().find((u) => u.id < cur) ?? ready[ready.length - 1]!);
    this.select(next);
    const c = hexCenter(offsetToAxial({ col: next.col, row: next.row }), this.size);
    this.camera.centerOn(c.x, c.y);
  }

  /** R-UI-05: warn before ending the phase with strikes left on units that have a target. */
  private endPhase() {
    if (this.state.outcome) return;
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

  private modal(html: string) {
    const modal = document.getElementById("modal")!;
    modal.querySelector(".panel")!.innerHTML = `${html}<div class="menu"><button id="m-close">${this.t("ui.close")}</button></div>`;
    modal.style.display = "flex";
    document.getElementById("m-close")!.onclick = () => (modal.style.display = "none");
  }

  /** Settings: how units look on the map (R-UI-03). */
  private settings() {
    const opt = (s: UnitStyle) =>
      `<label class="opt"><input type="radio" name="units" value="${s}"${this.ctx.style === s ? " checked" : ""}> ${this.t(`ui.settings.${s}`)}</label>`;
    this.modal(`<h2>${this.t("ui.settings")}</h2><fieldset class="settings"><legend>${this.t("ui.settings.units")} (U)</legend>${opt("miniature")}${opt("medallion")}</fieldset>`);
    for (const r of document.querySelectorAll<HTMLInputElement>('#modal input[name="units"]')) {
      r.onchange = () => this.setStyle(r.value as UnitStyle);
    }
  }

  private help() {
    const keys = ["n", "enter", "undo", "i", "u", "esc"].map((k) => `<li>${this.t(`ui.help.${k}`)}</li>`).join("");
    this.modal(`<h2>${this.t("ui.help")}</h2><b>${this.t("ui.help.keys")}</b><ul class="help">${keys}</ul><p class="help">${this.t("ui.help.mouse")}</p>`);
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
    this.facing = this.facingUndo.pop() ?? this.facing;
    this.log.push(this.t("ui.undone"));
    this.save();
    await this.refresh();
    if (!this.selection) this.showAdvanceOrHide();
  }

  /** Autosave: the command log replays the game (deterministic core). */
  private save() {
    try {
      if (this.state?.outcome) localStorage.removeItem(this.ctx.saveKey);
      else localStorage.setItem(this.ctx.saveKey, encodeSave(this.game.commands(), this.facing));
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
      this.facingUndo.push(new Map(this.facing));
      if (this.facingUndo.length > FACING_UNDO_LIMIT) this.facingUndo.shift();
      applyFacing(
        this.facing,
        events as ({ type: string } & Record<string, unknown>)[],
        (id) => colBefore.get(id),
        (id) => this.ctx.sideFacing[sideOf.get(id) ?? 0] ?? "left",
      );
      this.record(events);
      if (endPhase) {
        this.picked = null;
        this.deselect();
      }
      await this.refresh();
      this.save();
      if (!this.selection) this.showAdvanceOrHide();
      return events;
    } catch (e) {
      this.showingResult = false;
      const box = document.getElementById("combat")!;
      box.innerHTML = `<div class="bad">${refusalText(String(e).replace(/^Error: /, ""), this.ru)}</div>`;
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
    if (messages.length) this.toast(messages.join("<br>"), MESSAGE_MS);
  }

  private toast(html: string, ms = NOTICE_MS) {
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
    }, ms);
  }

  private showOutcome() {
    const o = this.state.outcome!;
    this.aw.hide();
    document.getElementById("actionbar")!.style.display = "none";
    document.getElementById("hover-line")!.style.display = "none";
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
