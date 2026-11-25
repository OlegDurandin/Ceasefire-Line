// mapgen's hillshade.png under the terrain textures (R-UI-01).

export interface HillshadeMeta {
  png: string;
  width_px: number;
  height_px: number;
  px_per_km: number;
  hex_radius_km: number;
  /** Image top-left relative to the centre of hex (0, 0), in km. */
  top_left_from_hex00_km: [number, number];
}

/** World position and scale of the image for hexes of corner radius `hexSize` px. */
export function hillshadePlacement(meta: HillshadeMeta, hexSize: number): { x: number; y: number; scale: number } {
  const worldPxPerKm = hexSize / meta.hex_radius_km;
  return {
    x: meta.top_left_from_hex00_km[0] * worldPxPerKm,
    y: meta.top_left_from_hex00_km[1] * worldPxPerKm,
    scale: worldPxPerKm / meta.px_per_km,
  };
}
