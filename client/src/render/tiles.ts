// The painted map baked by `mapgen bake` (R-UI-01, R-UI-02): a pyramid of
// tiles under the live layers. The level follows the camera's zoom so a
// tile pixel stays close to a screen pixel.

import { Assets, Container, Sprite, type Texture } from "pixi.js";

export interface TilesMeta {
  format: string;
  tile_px: number;
  px_per_km: number;
  hex_radius_km: number;
  /** Top-left of level 0 relative to the centre of hex (0, 0), in km. */
  top_left_from_hex00_km: [number, number];
  width_px: number;
  height_px: number;
  levels: { z: number; width: number; height: number; cols: number; rows: number }[];
}

/** World pixels per level-0 pixel for hexes of corner radius `hexSize`. */
export function worldPerBakePx(meta: TilesMeta, hexSize: number): number {
  return hexSize / meta.hex_radius_km / meta.px_per_km;
}

/**
 * Coarsest level whose pixels are still at most one screen pixel (level z
 * has pixels 2^z times bigger than level 0), so the map stays sharp.
 */
export function pickLevel(meta: TilesMeta, hexSize: number, cameraScale: number): number {
  const screenPerBakePx = cameraScale * worldPerBakePx(meta, hexSize);
  const z = Math.floor(Math.log2(1 / screenPerBakePx));
  return Math.max(0, Math.min(meta.levels.length - 1, z));
}

export class BakedMap {
  readonly container = new Container();
  private levels = new Map<number, Container>();
  private current = -1;

  constructor(
    private meta: TilesMeta,
    private baseUrl: string,
    hexSize: number,
  ) {
    const k = hexSize / meta.hex_radius_km;
    this.container.position.set(meta.top_left_from_hex00_km[0] * k, meta.top_left_from_hex00_km[1] * k);
    this.container.scale.set(worldPerBakePx(meta, hexSize));
  }

  /** Shows the level for this zoom, loading its tiles the first time. */
  async show(z: number) {
    if (z === this.current) return;
    this.current = z;
    let layer = this.levels.get(z);
    if (!layer) {
      layer = await this.load(z);
      this.levels.set(z, layer);
    }
    // A later call may have switched again while this level was loading.
    if (this.current !== z) return;
    for (const [lz, l] of this.levels) {
      l.visible = lz === z;
      if (l.visible && !l.parent) this.container.addChild(l);
    }
  }

  private async load(z: number): Promise<Container> {
    const level = this.meta.levels[z]!;
    const t = this.meta.tile_px;
    const step = 2 ** z;
    const layer = new Container();
    const jobs: Promise<void>[] = [];
    for (let ty = 0; ty < level.rows; ty++) {
      for (let tx = 0; tx < level.cols; tx++) {
        const url = `${this.baseUrl}/tiles/${z}/${tx}_${ty}.${this.meta.format}`;
        jobs.push(
          Assets.load<Texture>(url).then((tex) => {
            const s = new Sprite(tex);
            s.position.set(tx * t * step, ty * t * step);
            s.scale.set(step);
            layer.addChild(s);
          }),
        );
      }
    }
    await Promise.all(jobs);
    return layer;
  }
}
