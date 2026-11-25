// Stage 2 step 1b: hotseat play on a scenario — select, move, attack,
// forecast, end phase, outcome. All rules run in the core (WASM).

import { Assets, Container, Graphics, type Texture } from "pixi.js";

import type { Camera } from "./camera";
import type { Forecast, GameStateView, LiveUnit, Reach, Target } from "./game-view";
import { axialToOffset, pixelToAxial } from "./hex";
import { drawControl } from "./render/control";
import { setGridActive } from "./render/map";
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

const NIGHT_TINT = 0x8796c4;

export class Play {
  private state!: GameStateView;
  private selection: Selection | null = null;
  private unitsLayer = new Container();
  private overlay = new Graphics();
  private control = new Graphics();
  private textures = new Map<string, Texture>();

  constructor(
    private game: Game,
    private world: Container,
    private camera: Camera,
    private sprites: SpritesFile,
    private ru: Locale,
    private sideKeys: string[],
    private colors: number[],
    private size: number,
  ) {
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
      <button id="end-phase">${this.t("ui.end_phase")}</button>`;
    document.getElementById("end-phase")!.onclick = () => this.run(() => this.game.endPhase(), true);
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
    this.selection = { unit, reach, targets, aimed };
    this.redrawOverlay();
    this.showInfo(unit);
    if (aimed) this.aim(aimed);
    else this.showAdvanceOrHide();
  }

  private deselect() {
    this.selection = null;
    this.redrawOverlay();
    document.getElementById("info")!.style.display = "none";
    this.showAdvanceOrHide();
  }

  private redrawOverlay() {
    const sel = this.selection;
    const targetHexes = (sel?.targets ?? [])
      .map((t) => this.state.units.find((u) => u.id === t.target))
      .filter((u): u is LiveUnit => !!u);
    drawOverlays(this.overlay, this.size, sel?.reach ?? [], targetHexes, sel ? sel.unit : null);
    setGridActive(this.world, !!sel);
  }

  private showInfo(u: LiveUnit) {
    const info = document.getElementById("info")!;
    info.style.display = "block";
    info.innerHTML = `<b>${this.t(`unit.${u.class}`, u.class)}</b> · ${this.sideName(u.side)}<br>
      ${this.t("ui.strength")} ${u.strength} · ${this.t("ui.readiness")} ${u.readiness_pct}% · ${this.t("ui.morale")} ${u.morale_pct}%<br>
      AP ${u.ap_halves / 2} · ${this.t("ui.strikes")} ${u.strikes_left} · ${this.t("ui.supply")} ${u.supply} · ${this.t("ui.entrench")} ${u.entrenchment}`;
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
