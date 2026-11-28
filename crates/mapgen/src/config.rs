//! `data/mapgen.json`: classification parameters (R-UI-01).

use std::collections::BTreeMap;

use serde::Deserialize;

/// First matching rule wins: relief ≥ `min_relief_m` OR slope ≥ `min_slope_deg`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReliefRule {
    pub terrain: String,
    pub min_relief_m: f64,
    pub min_slope_deg: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HillshadeConfig {
    pub px_per_km: f64,
    /// Light from (degrees clockwise from north); 315 = north-west (R-UI-06).
    pub azimuth_deg: f64,
    pub altitude_deg: f64,
    pub z_factor: f64,
}

/// An OSM area kind: any of `tags` (key, value) makes a way or multipolygon count.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaRule {
    pub terrain: String,
    pub tags: Vec<(String, String)>,
    /// Share of the hex's samples inside such areas for the hex to take this terrain.
    pub min_share: f64,
}

/// Water inside this box is ignored (Lake Hula, drained before 1967).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterExclude {
    pub name: String,
    /// West, south, east, north.
    pub bbox: [f64; 4],
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OsmConfig {
    /// Extra degrees around the bbox when reading OSM.
    pub margin_deg: f64,
    /// `highway` values drawn as roads.
    pub roads: Vec<String>,
    /// Added with `--secondary-roads`.
    pub secondary_roads: Vec<String>,
    /// `waterway` values drawn as canals/major rivers on hex sides.
    pub rivers: Vec<String>,
    pub river_min_length_km: f64,
    /// How strongly a river's hex-side path sticks to the OSM line.
    pub river_line_weight: f64,
    /// Checked in order before the relief rules; the first reaching its share wins.
    pub areas: Vec<AreaRule>,
    /// `place` values read and listed in `geo.places` for the editor.
    pub places: Vec<String>,
    /// Places of these kinds always make their hex a city.
    pub city_kinds: Vec<String>,
    /// Other listed places make a city only with at least this population
    /// (OSM populations are present-day: historical cities are fixed in the editor).
    pub city_min_population: u64,
    pub water_exclude: Vec<WaterExclude>,
    /// If not empty, a `waterway=canal` way is kept only when its name (any
    /// language) contains one of these (the Suez Canal, not irrigation canals).
    #[serde(default)]
    pub canal_names: Vec<String>,
}

/// Sea from the DEM: extracts carry coastlines, not sea polygons.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeaConfig {
    pub terrain: String,
    /// Samples at or below this elevation (or without data) count as sea.
    pub max_elevation_m: f64,
    /// Share of such samples that makes the hex sea.
    pub min_share: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    pub earth_radius_km: f64,
    /// Samples per hex: a square lattice of n × n points, those inside the hexagon kept.
    pub samples_per_hex_side: u32,
    /// Finite-difference step for slopes.
    pub slope_step_m: f64,
    pub relief_rules: Vec<ReliefRule>,
    pub default_terrain: String,
    /// Terrain id → one character in map.json rows.
    pub legend: BTreeMap<String, char>,
    /// Character for a mountain hex with a pass.
    pub pass_char: char,
    pub osm: OsmConfig,
    /// Optional: sea by elevation, checked before the OSM areas.
    #[serde(default)]
    pub sea: Option<SeaConfig>,
    /// A road over a canal or river side makes a bridge (false: crossings
    /// stay open, e.g. the Suez Canal in 1967 had ferries only).
    #[serde(default = "yes")]
    pub auto_bridges: bool,
    pub hillshade: HillshadeConfig,
    pub bake: BakeConfig,
    /// `mapgen procgen`: procedural relief for fictional scenarios (R-UI-01).
    #[serde(default)]
    pub procgen: Option<ProcgenConfig>,
}

/// `mapgen procgen` (R-UI-01, section 14 «Срединные земли»): shared numbers
/// and one block per biome.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcgenConfig {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    /// Elevation raster cell, km.
    pub raster_km: f64,
    /// `mapgen bake` resolution for procedural maps (they are large, 10 km hexes).
    pub bake_px_per_km: f64,
    /// Rivers and roads are drawn this many times wider (the coarser bake).
    pub bake_line_scale: f64,
    /// Share of a hex's samples in lake cells that makes it `water`.
    pub lake_min_share: f64,
    /// Share of a hex's samples in forest cells that makes it `forest`.
    pub forest_min_share: f64,
    /// Terrain ids written for water, forest, city, mountains.
    pub water_terrain: String,
    pub forest_terrain: String,
    pub city_terrain: String,
    pub mountain_terrain: String,
    /// `waterway` kinds of the procedural rivers (widths in `bake.water.river_km`).
    pub stream_kind: String,
    pub river_kind: String,
    /// Major rivers become impassable hex sides (`canals`) along their course
    /// like OSM rivers (`features::river_edges`).
    pub major_river_min_length_km: f64,
    pub major_river_line_weight: f64,
    /// Chaikin passes over the river polylines drawn by bake.
    pub river_smooth_iterations: u32,
    /// `bake.vegetation` kind of the procedural forest.
    pub forest_vegetation: String,
    /// Forest areas are cut into squares of this many raster cells (fast tree scatter).
    pub forest_area_tile_cells: u32,
    pub cities: CityPlacement,
    pub roads: ProcRoads,
    pub biomes: BTreeMap<String, BiomeConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityPlacement {
    /// Search radius around a hint, hexes.
    pub hint_radius_hex: u32,
    pub min_spacing_hex: u32,
    /// Cities without a hint stay this far from the map edge.
    pub border_margin_hex: u32,
    /// Score terms (higher is better).
    pub weight_hint_distance: f64,
    pub weight_river: f64,
    pub weight_lowland: f64,
    pub weight_spread: f64,
    pub spread_cap_hex: f64,
    pub weight_jitter: f64,
    /// Terrains a city may not stand on (besides water).
    pub forbidden_terrain: Vec<String>,
    /// `geo.places` kind per size.
    pub kind_large: String,
    pub kind_small: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcRoads {
    /// Added to the AP of entering a mountain hex (roads keep out of mountains).
    pub mountain_extra_cost: f64,
    /// Added for crossing a major river side.
    pub river_crossing_cost: f64,
    /// Cost multiplier on sides that already carry a road (roads merge).
    pub reuse_factor: f64,
    /// Links added beyond the spanning tree.
    pub extra_links: u32,
    /// An extra link is added when the network route is this many times longer.
    pub extra_detour_ratio: f64,
    /// Kinds of the roads drawn by bake (`bake.historical_roads`).
    pub bake_road_kind: String,
    pub bake_track_kind: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeConfig {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    pub elevation_min_m: f64,
    pub elevation_max_m: f64,
    pub base: crate::noise::NoiseConfig,
    pub base_weight: f64,
    pub ridge: crate::noise::NoiseConfig,
    pub ridge_weight: f64,
    /// Ridges only where this mask is high (mountain ranges, not everywhere).
    pub ridge_mask: crate::noise::NoiseConfig,
    pub ridge_mask_low: f64,
    pub ridge_mask_high: f64,
    /// Regional slope across the map (share of the height range).
    pub tilt: f64,
    /// 0 = linear heights, 1 = squared (wider lowlands).
    pub lowland_curve: f64,
    /// Filled depressions deeper than this are lakes.
    pub lake_min_depth_m: f64,
    /// Smaller lakes are left dry.
    pub lake_min_area_km2: f64,
    /// Deepest lakes first, until they cover this share of the map; the
    /// other depressions stay dry (flat ground).
    pub lake_max_share: f64,
    /// Drained area for a drawn stream and for a major river (impassable side).
    pub river_min_area_km2: f64,
    pub major_river_min_area_km2: f64,
    pub relief_rules: Vec<ReliefRule>,
    pub default_terrain: String,
    pub moisture: crate::noise::NoiseConfig,
    pub moisture_noise_weight: f64,
    pub moisture_water_weight: f64,
    pub moisture_water_reach_km: f64,
    /// Negative: drier with height (share of the height range).
    pub moisture_altitude_weight: f64,
    /// Negative: less forest on steep ground; positive: more (foothills).
    pub moisture_slope_weight: f64,
    pub moisture_slope_ref_deg: f64,
    pub forest_threshold: f64,
    /// Terrains that turn into forest where it grows.
    pub forest_on: Vec<String>,
}

/// `mapgen bake`: the painted map under the hexes (R-UI-01, R-UI-02).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BakeConfig {
    /// Resolution of the finest zoom level.
    pub px_per_km: f64,
    pub tile_px: u32,
    /// Size of one terrain texture pixel on the ground.
    pub texture_km_per_px: f64,
    /// Width of the soft border between terrain types, in hexes (distance between centres).
    pub blend_blur_hex: f64,
    /// Multiply strength of the relief shading (0 = none).
    pub shade_strength: f64,
    /// Hypsometric tint, stops by elevation.
    pub tint: Vec<TintStop>,
    pub water: WaterStyle,
    pub vegetation: Vec<VegetationRule>,
    /// `highway` classes drawn, thinnest first.
    pub roads: Vec<RoadStyle>,
    /// Classes of the historical road layer (`mapgen roads`), thinnest first.
    pub historical_roads: Vec<RoadStyle>,
    pub road_color: String,
    pub road_edge_color: String,
    pub road_edge_km: f64,
    pub city: CityStyle,
    /// Seed for the scatter of trees.
    pub seed: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TintStop {
    pub elevation_m: f64,
    pub color: String,
    pub alpha: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterStyle {
    pub color: String,
    pub edge_color: String,
    pub edge_km: f64,
    /// Smaller lakes and ponds are left out (mostly later reservoirs).
    pub min_lake_km2: f64,
    /// `waterway` class → line width, km.
    pub river_km: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VegetationRule {
    pub kind: String,
    pub tags: Vec<(String, String)>,
    /// Ground tint under the trees.
    pub ground_color: String,
    pub ground_alpha: f64,
    /// Distance between trees and how far each is moved from its cell (0..1).
    pub spacing_km: f64,
    pub jitter: f64,
    pub crown_km: f64,
    pub crown_color: String,
    pub highlight_color: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoadStyle {
    pub kind: String,
    pub width_km: f64,
    /// Own colour (else `road_color`) and dashes (dirt roads).
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub dash_km: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityStyle {
    pub size_km: f64,
    pub roof_color: String,
    pub edge_color: String,
}

impl Config {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let c: Config = serde_json::from_str(json).map_err(|e| format!("mapgen.json: {e}"))?;
        let mut seen = std::collections::BTreeSet::new();
        for (t, ch) in &c.legend {
            if !seen.insert(*ch) || *ch == c.pass_char {
                return Err(format!(
                    "mapgen.json: legend character '{ch}' for `{t}` is not unique"
                ));
            }
        }
        for t in c
            .relief_rules
            .iter()
            .map(|r| &r.terrain)
            .chain([&c.default_terrain])
        {
            if !c.legend.contains_key(t) {
                return Err(format!(
                    "mapgen.json: terrain `{t}` has no legend character"
                ));
            }
        }
        if let Some(p) = &c.procgen {
            for (name, b) in &p.biomes {
                for t in b
                    .relief_rules
                    .iter()
                    .map(|r| &r.terrain)
                    .chain([&b.default_terrain])
                    .chain([
                        &p.water_terrain,
                        &p.forest_terrain,
                        &p.city_terrain,
                        &p.mountain_terrain,
                    ])
                {
                    if !c.legend.contains_key(t) {
                        return Err(format!(
                            "mapgen.json: procgen.biomes.{name}: terrain `{t}` has no legend character"
                        ));
                    }
                }
            }
        }
        if c.samples_per_hex_side < 2 {
            return Err("mapgen.json: samples_per_hex_side must be ≥ 2".into());
        }
        Ok(c)
    }
}

fn yes() -> bool {
    true
}
