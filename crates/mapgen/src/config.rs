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
    pub hillshade: HillshadeConfig,
    pub bake: BakeConfig,
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
        if c.samples_per_hex_side < 2 {
            return Err("mapgen.json: samples_per_hex_side must be ≥ 2".into());
        }
        Ok(c)
    }
}
