// data/sprites.json: which picture stands for a unit class, nation and tech level.

export interface SpriteTier {
  tech_min: number;
  sprite: string;
  variants?: string[];
  year_min?: number;
  year_max?: number;
  nations?: string[];
  /** Preferred source when a model has no medallion. */
  dir?: string;
  model_only?: boolean;
}

export interface SpriteClass {
  tech_branch: string | null;
  isr: SpriteTier[];
  egy_syr: SpriteTier[];
  jor?: SpriteTier[];
}

/** A unit picture style (data/sprites.json `styles`). */
export interface StyleDef {
  dir: string;
  anchor: [number, number];
  manifest?: string;
}

export type UnitStyle = "miniature" | "medallion";

/** `side/id` → anchor of the miniatures that exist (assets/units_minis/manifest.json). */
export type MiniIndex = Map<string, [number, number]>;

export function miniIndex(manifest: { id: string; side: string; anchor: [number, number] }[]): MiniIndex {
  return new Map(manifest.map((e) => [`${e.side}/${e.id}`, e.anchor] as const));
}

export interface StyledSprite extends SpriteRef {
  style: UnitStyle;
  /** Contact point of the picture, as a share of its size. */
  anchor: [number, number];
}

/**
 * R-UI-03, R-UI-06: the picture in the wanted style; a sprite without a
 * miniature falls back to its medallion.
 */
export function styledSprite(ref: SpriteRef, style: UnitStyle, sprites: SpritesFile, minis: MiniIndex): StyledSprite {
  const def = sprites.styles?.miniature;
  const anchor = minis.get(`${ref.side}/${ref.id}`);
  if (style === "miniature" && def && anchor) {
    return { ...ref, url: `/assets/${def.dir}/${ref.side}/${ref.id}.png`, style, anchor };
  }
  if (ref.url.includes("/units_minis/") && anchor) return { ...ref, style: "miniature", anchor };
  return { ...ref, style: "medallion", anchor: sprites.styles?.medallion?.anchor ?? [0.5, 0.5] };
}

export interface SpritesFile {
  sprite_sides: Record<string, { nations: string[] }>;
  styles?: Record<string, StyleDef>;
  default_style?: UnitStyle;
  terrain: Record<string, string>;
  units: Record<string, SpriteClass>;
  helicopter_transfer: Record<string, string>;
  map_objects: Record<string, string>;
}

export type TechLevels = Record<string, number>;

export interface SpriteRef {
  side: string;
  id: string;
  url: string;
}

/** Sprite side folder for a nation, or null if none lists it. */
export function spriteSide(sprites: SpritesFile, nation: string | null | undefined): string | null {
  if (!nation) return null;
  for (const [side, def] of Object.entries(sprites.sprite_sides)) {
    if (def.nations.includes(nation)) return side;
  }
  return null;
}

/** Highest tier whose tech_min the side's level reaches; null → draw APP-6 instead. */
export function pickSprite(
  sprites: SpritesFile,
  unitClass: string,
  nation: string | null | undefined,
  tech: TechLevels,
  year?: number,
  model?: string | null,
): SpriteRef | null {
  const side = spriteSide(sprites, nation);
  const cls = sprites.units[unitClass];
  if (!side || !cls) return null;
  const tiers = (cls as unknown as Record<string, SpriteTier[] | undefined>)[side] ?? [];
  const level = cls.tech_branch ? (tech[cls.tech_branch] ?? 1) : 1;
  let best: SpriteTier | null = null;
  const inPeriod = (t: SpriteTier) =>
    (year === undefined || ((t.year_min === undefined || year >= t.year_min) && (t.year_max === undefined || year <= t.year_max))) &&
    (!t.nations || (nation != null && t.nations.includes(nation)));
  // An explicit model overrides technology within its nation and historical period.
  if (model) {
    const tier = tiers.find((t) => inPeriod(t) && (t.sprite === model || t.variants?.includes(model)));
    if (tier) return { side, id: model, url: `/assets/${tier.dir ?? "units"}/${side}/${model}.png` };
  }
  for (const t of tiers) {
    if (!t.model_only && inPeriod(t) && t.tech_min <= level && (!best || t.tech_min >= best.tech_min)) best = t;
  }
  return best ? { side, id: best.sprite, url: `/assets/${best.dir ?? "units"}/${side}/${best.sprite}.png` } : null;
}

export function terrainTextureUrl(sprites: SpritesFile, terrain: string): string | null {
  const id = sprites.terrain[terrain];
  return id ? `/assets/terrain/${id}.png` : null;
}
