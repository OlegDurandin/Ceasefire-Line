// data/sprites.json: which picture stands for a unit class, nation and tech level.

export interface SpriteTier {
  tech_min: number;
  sprite: string;
  variants?: string[];
}

export interface SpriteClass {
  tech_branch: string | null;
  isr: SpriteTier[];
  egy_syr: SpriteTier[];
}

export interface SpritesFile {
  sprite_sides: Record<string, { nations: string[] }>;
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
): SpriteRef | null {
  const side = spriteSide(sprites, nation);
  const cls = sprites.units[unitClass];
  if (!side || !cls) return null;
  const tiers = (cls as unknown as Record<string, SpriteTier[] | undefined>)[side] ?? [];
  const level = cls.tech_branch ? (tech[cls.tech_branch] ?? 1) : 1;
  let best: SpriteTier | null = null;
  for (const t of tiers) if (t.tech_min <= level) best = t;
  return best ? { side, id: best.sprite, url: `/assets/units/${side}/${best.sprite}.png` } : null;
}

export function terrainTextureUrl(sprites: SpritesFile, terrain: string): string | null {
  const id = sprites.terrain[terrain];
  return id ? `/assets/terrain/${id}.png` : null;
}
