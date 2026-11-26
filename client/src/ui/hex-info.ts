// R-UI-04 hex mode of the bottom window, as on the approved mock (board
// «Режим гекса»): the selected hex on the left, the hex under the pointer on
// the right — terrain, AP, defence, max entrenchment, own supply, road,
// enemy ZOC and who is there.

import { attrText, num } from "../explain";
import { fill } from "../log";

type Locale = Record<string, string>;

export interface TerrainInfo {
  ap: number;
  defense_mult: number;
  max_entrench: number;
}

export interface HexFacts {
  col: number;
  row: number;
  terrain: string;
  terrainName: string;
  /** Texture for the hexagonal portrait. */
  texture: string | null;
  /** R-MOVE-06: neutral closed territory. */
  closed: boolean;
  info: TerrainInfo | null;
  control: string;
  /** Own side's supply here, or null where it does not reach. */
  supply: number | null;
  road: "road" | "track" | null;
  enemyZoc: boolean;
  occupant: string | null;
}

export interface HexRules {
  roadAp: number;
  trackAp: number;
  entrenchMinSupply: number;
}

/** The text of one hex card (pure, for tests): title, subtitle, rows, occupant. */
export function hexRows(f: HexFacts, ru: Locale): { label: string; value: string; tip: string }[] {
  const t = (k: string) => ru[k] ?? k;
  // AP 0 in data/terrain.json is impassable terrain (water).
  const move = f.closed ? t("ui.hex.closed") : !f.info || f.info.ap <= 0 ? t("ui.hex.impassable") : `${num(f.info.ap, 1)} AP`;
  const road = f.road === "road" ? t("ui.hex.road_road") : f.road === "track" ? t("ui.hex.road_track") : t("ui.hex.road_none");
  return [
    { label: t("ui.hex.move"), value: move, tip: "move" },
    { label: t("ui.hex.defense"), value: f.info ? `×${num(f.info.defense_mult, 1)}` : "—", tip: "defense" },
    { label: t("ui.hex.max_entrench"), value: f.info ? String(f.info.max_entrench) : "—", tip: "entrench" },
    { label: t("ui.hex.supply"), value: f.supply === null ? "—" : String(f.supply), tip: "supply" },
    { label: t("ui.hex.road"), value: road, tip: "road" },
    { label: t("ui.hex.enemy_zoc"), value: t(f.enemyZoc ? "ui.yes" : "ui.no"), tip: "zoc" },
  ];
}

/** `touch`: no hover on this screen — the right card comes from a long press. */
export function hexCardHtml(f: HexFacts | null, where: "selected" | "cursor", side: "left" | "right", ru: Locale, r: HexRules, touch = false): string {
  const t = (k: string) => ru[k] ?? k;
  if (!f) {
    const hint = where === "selected" ? "ui.hex.pick" : touch ? "ui.hex.hold_hint" : "ui.hex.hover_hint";
    return `<div class="aw-card ${side} hx-empty"><div class="aw-info"><div class="aw-sub">${t(hint)}</div></div></div>`;
  }
  const tips: Record<string, string> = {
    move: fill(t("ui.hex.move_tip"), { road: num(r.roadAp, 1), track: num(r.trackAp, 1) }),
    defense: t("ui.hex.defense_tip"),
    entrench: fill(t("ui.entrench_tip"), { terrain: f.terrainName, max: f.info?.max_entrench ?? "—", min: r.entrenchMinSupply }),
    supply: t("ui.hex.supply_tip"),
    road: t("ui.hex.road"),
    zoc: t("ui.hex.zoc_tip"),
  };
  const grid = hexRows(f, ru)
    .map((x) => `<span class="aw-l">${x.label}</span><span class="aw-v tip" title="${attrText(tips[x.tip] ?? "")}">${x.value}</span>`)
    .join("");
  const portrait = `<div class="hx-portrait" aria-hidden="true"><div style="${f.texture ? `background-image: url('${f.texture}')` : ""}"></div></div>`;
  return `<div class="aw-card ${side}">${portrait}
    <div class="aw-info">
      <div class="aw-title">${f.terrainName}</div>
      <div class="aw-sub">${t(where === "selected" ? "ui.hex.selected" : "ui.hex.cursor")} (${f.col}, ${f.row}) · ${t("ui.hex.control")}: ${f.control}</div>
      <div class="aw-grid hx-grid">${grid}</div>
      <div class="hx-occ">${t("ui.hex.in_hex")}: ${f.occupant ?? t("ui.hex.empty")}</div>
    </div></div>`;
}
