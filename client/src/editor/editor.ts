// Stage 2 map editor: terrain, hex sides, control, units, reference underlay,
// undo/redo, validation from the core shown on the map, save (dev server).

import { Container, Graphics, Sprite, Texture } from "pixi.js";

import type { Camera } from "../camera";
import { SQRT3, axialToOffset, hexCenter, hexCorners, offsetToAxial, pixelToAxial, type Point } from "../hex";
import type { ScenarioView } from "../view";
import { cycleEdge, normalizeRoads, paintControl, paintTerrain, placeUnit, type MapDoc, type Off, type ScenarioDoc } from "./model";

type Tool = "terrain" | "edge" | "control" | "unit" | "erase";
type Locale = Record<string, string>;

export interface EditorHost {
  /** Rebuilds the map and units from a core view; returns when drawn. */
  render(view: ScenarioView): Promise<void>;
  world: Container;
  camera: Camera;
  hexSize: number;
  /** core.scenarioView(scenario, map, events) */
  view(scenario: string, map: string): ScenarioView;
}

const ISSUE_COLOR = 0xe0453a;
const UNDO_LIMIT = 200;

export class Editor {
  private tool: Tool = "terrain";
  private terrain = "plain";
  private pass = false;
  private side: number | null = 0;
  private unitClass = "infantry";
  private hqRating = 5;
  private undo: string[] = [];
  private redo: string[] = [];
  private issuesLayer = new Graphics();
  private underlay: Sprite | null = null;

  constructor(
    private host: EditorHost,
    private map: MapDoc,
    private scn: ScenarioDoc,
    private scenarioId: string,
    private ru: Locale,
    private terrains: string[],
    private unitClasses: string[],
  ) {
    normalizeRoads(this.map);
    host.camera.onTap = (x, y) => void this.tap(x, y);
    this.toolbar();
  }

  private t(k: string, fallback = k) {
    return this.ru[k] ?? fallback;
  }

  private snapshot() {
    return JSON.stringify({ map: this.map, scn: this.scn });
  }

  private restore(s: string) {
    const d = JSON.parse(s) as { map: MapDoc; scn: ScenarioDoc };
    this.map = d.map;
    this.scn = d.scn;
  }

  async refresh() {
    const view = this.host.view(JSON.stringify(this.scn), JSON.stringify(this.map));
    // The host rebuilds (and destroys) the scene; keep the editor's own layers.
    this.host.world.removeChild(this.issuesLayer);
    if (this.underlay) this.host.world.removeChild(this.underlay);
    await this.host.render(view);
    this.host.world.addChild(this.issuesLayer);
    if (this.underlay) this.host.world.addChildAt(this.underlay, 1);
    this.drawIssues(view);
  }

  private async edit(change: () => void) {
    this.undo.push(this.snapshot());
    if (this.undo.length > UNDO_LIMIT) this.undo.shift();
    this.redo = [];
    change();
    await this.refresh();
  }

  private async step(from: string[], to: string[]) {
    const s = from.pop();
    if (!s) return;
    to.push(this.snapshot());
    this.restore(s);
    await this.refresh();
  }

  /** Hex under a world point, and the neighbour across the nearest side. */
  private hexAndSide(w: Point): { hex: Off; across: Off | null } {
    const size = this.host.hexSize;
    const ax = pixelToAxial(w, size);
    const c = hexCenter(ax, size);
    const dx = w.x - c.x;
    const dy = w.y - c.y;
    const off = axialToOffset(ax);
    if (Math.hypot(dx, dy) < size * 0.45) return { hex: [off.col, off.row], across: null };
    // Flat-top: neighbour centres lie at 30° + 60°·k.
    const k = Math.round((Math.atan2(dy, dx) - Math.PI / 6) / (Math.PI / 3));
    const a = Math.PI / 6 + (k * Math.PI) / 3;
    const n = pixelToAxial({ x: c.x + SQRT3 * size * Math.cos(a), y: c.y + SQRT3 * size * Math.sin(a) }, size);
    const no = axialToOffset(n);
    return { hex: [off.col, off.row], across: [no.col, no.row] };
  }

  private inside([c, r]: Off) {
    return c >= 0 && r >= 0 && c < this.map.width && r < this.map.height;
  }

  private async tap(x: number, y: number) {
    const { hex, across } = this.hexAndSide(this.host.camera.toWorld(x, y));
    if (!this.inside(hex)) return;
    switch (this.tool) {
      case "terrain":
        return this.edit(() => paintTerrain(this.map, hex, this.terrain, this.pass && this.terrain === "mountains"));
      case "control":
        return this.edit(() => paintControl(this.map, hex, this.side));
      case "edge":
        if (across && this.inside(across)) await this.edit(() => cycleEdge(this.map, hex, across));
        return;
      case "erase":
        return this.edit(() => placeUnit(this.scn, hex, null));
      case "unit": {
        const side = this.side ?? 0;
        const key = this.scn.sides.find((s) => s.id === side)?.key;
        const nation = (this.scn.units.find((u) => u.side === side)?.nation as string | undefined) ?? key ?? null;
        const hq = this.unitClass === "hq";
        return this.edit(() =>
          placeUnit(this.scn, hex, {
            class: this.unitClass,
            side,
            hex,
            morale_pct: 80,
            nation,
            ...(hq ? { hq_rating: this.hqRating } : {}),
          }),
        );
      }
    }
  }

  private drawIssues(view: ScenarioView) {
    const g = this.issuesLayer;
    g.clear();
    const size = this.host.hexSize;
    const list = document.getElementById("panel")!;
    const hexes: Off[] = [];
    for (const i of view.issues) {
      if (Array.isArray(i.hex)) hexes.push(i.hex as Off);
      else if (typeof i.unit === "number" && this.scn.units[i.unit]) hexes.push(this.scn.units[i.unit]!.hex);
    }
    for (const [col, row] of hexes) {
      const c = hexCenter(offsetToAxial({ col, row }), size);
      g.poly(hexCorners(c, size * 0.9).flatMap((p) => [p.x, p.y])).stroke({ width: 4, color: ISSUE_COLOR });
    }
    list.style.display = "block";
    list.innerHTML = view.issues.length
      ? `<p class="bad">${this.t("ui.validation.problems")}: ${view.issues.length}</p><ul>${(view.issues)
          .map((i) => {
            const where = Array.isArray(i.hex) ? ` [${(i.hex as number[]).join(", ")}]` : "";
            const { kind, ...rest } = i;
            const detail = Object.keys(rest).length ? ` <span class="muted">${JSON.stringify(rest)}</span>` : "";
            return `<li>${this.t(`issue.${String(kind)}`, String(kind))}${where}${detail}</li>`;
          })
          .join("")}</ul>`
      : `<p class="ok">${this.t("ui.validation.ok")}</p>`;
  }

  private async save() {
    const status = document.getElementById("ed-status")!;
    try {
      for (const [file, doc] of [
        ["map.json", this.map],
        ["scenario.json", this.scn],
      ] as const) {
        const r = await fetch(`/__editor/save?scenario=${this.scenarioId}&file=${file}`, { method: "POST", body: JSON.stringify(doc) });
        if (!r.ok) throw new Error(`${file}: ${await r.text()}`);
      }
      status.textContent = this.t("editor.saved");
    } catch (e) {
      status.textContent = String(e);
    }
  }

  /** R-UI-01 reference underlay: a local image stretched over the map's bbox. */
  private async loadUnderlay(file: File, alpha: number) {
    const geo = this.map.geo;
    if (!geo?.size_km) return;
    const img = new Image();
    img.src = URL.createObjectURL(file);
    await img.decode();
    const s = new Sprite(Texture.from(img));
    const pxPerKm = this.host.hexSize / (geo.hex_km / SQRT3);
    s.position.set(-this.host.hexSize, (-this.host.hexSize * SQRT3) / 2);
    s.width = geo.size_km[0] * pxPerKm;
    s.height = geo.size_km[1] * pxPerKm;
    s.alpha = alpha;
    this.underlay?.destroy();
    this.underlay = s;
    await this.refresh();
  }

  private toolbar() {
    const top = document.getElementById("top")!;
    const opt = (v: string, label: string, sel = false) => `<option value="${v}"${sel ? " selected" : ""}>${label}</option>`;
    const sides = this.scn.sides.map((s) => opt(String(s.id), this.t(`side.${s.key}`, s.key), s.id === this.side)).join("");
    const geo = !!this.map.geo?.size_km;
    top.innerHTML = `
      <b>${this.t("editor.title")}</b>
      <select id="ed-tool">${(["terrain", "edge", "control", "unit", "erase"] as Tool[]).map((t) => opt(t, this.t(`editor.tool.${t}`), t === this.tool)).join("")}</select>
      <select id="ed-terrain">${this.terrains.map((t) => opt(t, this.t(`terrain.${t}`, t), t === this.terrain)).join("")}</select>
      <label><input type="checkbox" id="ed-pass"> ${this.t("editor.pass")}</label>
      <select id="ed-side">${sides}${opt("none", this.t("ui.nobody"))}</select>
      <select id="ed-class">${this.unitClasses.map((c) => opt(c, this.t(`unit.${c}`, c), c === this.unitClass)).join("")}</select>
      <label>★ <input type="number" id="ed-hq" min="5" max="9" value="${this.hqRating}" style="width:3em"></label>
      ${geo ? `<label>${this.t("editor.underlay")} <input type="file" id="ed-under" accept="image/*"></label><input type="range" id="ed-alpha" min="0" max="1" step="0.05" value="0.5">` : ""}
      <button id="ed-undo">↶</button><button id="ed-redo">↷</button>
      <button id="ed-save">${this.t("editor.save")}</button><span id="ed-status" class="muted"></span>`;
    const val = (id: string) => (document.getElementById(id) as HTMLInputElement | HTMLSelectElement).value;
    document.getElementById("ed-tool")!.onchange = () => (this.tool = val("ed-tool") as Tool);
    document.getElementById("ed-terrain")!.onchange = () => (this.terrain = val("ed-terrain"));
    document.getElementById("ed-pass")!.onchange = (e) => (this.pass = (e.target as HTMLInputElement).checked);
    document.getElementById("ed-side")!.onchange = () => (this.side = val("ed-side") === "none" ? null : Number(val("ed-side")));
    document.getElementById("ed-class")!.onchange = () => (this.unitClass = val("ed-class"));
    document.getElementById("ed-hq")!.onchange = () => (this.hqRating = Number(val("ed-hq")));
    document.getElementById("ed-undo")!.onclick = () => void this.step(this.undo, this.redo);
    document.getElementById("ed-redo")!.onclick = () => void this.step(this.redo, this.undo);
    document.getElementById("ed-save")!.onclick = () => void this.save();
    if (geo) {
      const alpha = () => Number(val("ed-alpha"));
      document.getElementById("ed-under")!.onchange = (e) => {
        const f = (e.target as HTMLInputElement).files?.[0];
        if (f) void this.loadUnderlay(f, alpha());
      };
      document.getElementById("ed-alpha")!.oninput = () => {
        if (this.underlay) this.underlay.alpha = alpha();
      };
    }
  }
}
