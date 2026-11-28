//! `mapgen procgen`: procedural relief for the fictional scenarios (R-UI-01,
//! GDD section 14 «Вымышленные сценарии: «Срединные земли»»).
//!
//! Pipeline, all on a raster of `procgen.raster_km` over a synthetic bbox:
//!
//! 1. heights: base fBm + ridged fBm under a range mask + a regional tilt
//!    (own hash noise, `noise.rs`), quantised to 16 bits — the same numbers
//!    are written to `elevation.png` and read back by `mapgen bake`;
//! 2. drainage: depressions filled by priority-flood, lakes where the fill is
//!    deep enough, D8 flow (steepest descent on the filled surface, the flood
//!    order on flats), accumulation; streams and major rivers by drained area;
//! 3. moisture → forest (noise, water nearby, height, slope);
//! 4. hexes: water from lakes, relief rules of the biome, forest; major rivers
//!    run along hex sides as `canals` (the same path search as OSM rivers);
//! 5. cities at their hints or the best lowland hexes near rivers;
//! 6. roads: least-cost hex paths (terrain AP) over a spanning tree of the
//!    cities plus a few extra links; large–large links are roads, the rest
//!    tracks; a road over a major river has a bridge, a mountain hex with a
//!    road is a pass.
//!
//! Deterministic: fixed iteration orders, integer heap keys, own PRNG; the
//! same parameters give byte-identical files.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fs;
use std::path::Path;

use ceasefire_core::rng::Pcg32;
use ceasefire_core::scenario::{axial_to_offset, offset_to_axial};
use image::{GrayImage, ImageBuffer, Luma};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::{BiomeConfig, Config, ProcgenConfig};
use crate::dem::Tile;
use crate::geo::{BBox, Grid, Km, Projection};
use crate::noise;
use crate::osm::{Area, Line, LonLat, OsmData};

pub type Off = [i32; 2];
pub type Side = [Off; 2];
type Elevation16 = ImageBuffer<Luma<u16>, Vec<u16>>;

/// Name of the file that marks a procedural map directory.
pub const PROCEDURAL_JSON: &str = "procedural.json";
const ELEVATION_PNG: &str = "elevation.png";
const Q_MAX: f64 = 65535.0;
const SQRT3: f64 = 1.732_050_807_568_877_2;

/// `procgen.json`: what a fictional scenario asks for.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    pub seed: u64,
    pub cols: u32,
    pub rows: u32,
    pub hex_km: f64,
    /// A key of `procgen.biomes` (temperate | steppe | foothills).
    pub biome: String,
    /// Lon/lat of the map's north-west corner (geometry only, the land is fictional).
    pub origin: [f64; 2],
    pub cities: Vec<CityParam>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityParam {
    /// Locale key, e.g. `place.velen67.velen`.
    pub name: String,
    /// Preferred hex [col, row]; the city goes to the best hex nearby.
    #[serde(default)]
    pub hint: Option<Off>,
    pub size: CitySize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CitySize {
    Large,
    Small,
}

impl Params {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let p: Params = serde_json::from_str(json).map_err(|e| format!("procgen params: {e}"))?;
        if p.cols < 2 || p.rows < 2 || p.hex_km <= 0.0 {
            return Err("procgen params: need cols, rows ≥ 2 and hex_km > 0".into());
        }
        Ok(p)
    }
}

/// Movement costs from `terrain.json` (R-MOVE-01), for laying roads.
#[derive(Debug, Clone)]
pub struct TerrainCosts {
    ap: BTreeMap<String, f64>,
    impassable: BTreeSet<String>,
}

impl TerrainCosts {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| format!("terrain.json: {e}"))?;
        let (mut ap, mut impassable) = (BTreeMap::new(), BTreeSet::new());
        for t in v["terrain"].as_array().ok_or("terrain.json: no terrain")? {
            let id = t["id"].as_str().ok_or("terrain.json: terrain without id")?;
            ap.insert(id.to_owned(), t["ap"].as_f64().unwrap_or(1.0));
            if t["impassable"].as_bool() == Some(true) {
                impassable.insert(id.to_owned());
            }
        }
        Ok(Self { ap, impassable })
    }
}

/// The synthetic bbox: `origin` is its north-west corner, the size fits
/// exactly `cols × rows` hexes of `hex_km` (`Grid::fit`).
pub fn bbox_for(p: &Params, earth_radius_km: f64) -> Result<BBox, String> {
    let r = p.hex_km / SQRT3;
    let h = SQRT3 * r;
    // Grid::fit: cols = ⌊(x − 2R)/1.5R⌋ + 1, rows = ⌊(y − 1.5h)/h⌋ + 1; a
    // quarter step of slack keeps the floor away from rounding.
    let x = 2.0 * r + 1.5 * r * f64::from(p.cols - 1) + 0.25 * r;
    let y = 1.5 * h + h * f64::from(p.rows - 1) + 0.25 * h;
    let [west, north] = p.origin;
    let km_per_deg_lat = earth_radius_km * std::f64::consts::PI / 180.0;
    let south = north - y / km_per_deg_lat;
    let lat0 = ((south + north) / 2.0).to_radians();
    let east = west + x / (km_per_deg_lat * lat0.cos());
    let bbox = BBox {
        west,
        south,
        east,
        north,
    };
    let g = Grid::fit(Projection::new(bbox, earth_radius_km).size_km(), p.hex_km)?;
    if (g.cols, g.rows) != (p.cols, p.rows) {
        return Err(format!(
            "procgen: bbox fits {} × {} hexes, not {} × {}",
            g.cols, g.rows, p.cols, p.rows
        ));
    }
    Ok(bbox)
}

/// Square raster over the bbox; cell (0, 0) is at the north-west corner.
#[derive(Debug, Clone, Copy)]
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub cell_km: f64,
}

impl Raster {
    fn new(size: Km, cell_km: f64) -> Self {
        Self {
            w: ((size.x / cell_km).ceil() as usize).max(2),
            h: ((size.y / cell_km).ceil() as usize).max(2),
            cell_km,
        }
    }

    fn len(self) -> usize {
        self.w * self.h
    }

    fn centre(self, i: usize) -> Km {
        Km {
            x: ((i % self.w) as f64 + 0.5) * self.cell_km,
            y: ((i / self.w) as f64 + 0.5) * self.cell_km,
        }
    }

    fn cell_at(self, p: Km) -> Option<usize> {
        let (x, y) = (p.x / self.cell_km, p.y / self.cell_km);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        (x < self.w && y < self.h).then_some(y * self.w + x)
    }

    fn border(self, i: usize) -> bool {
        let (x, y) = (i % self.w, i / self.w);
        x == 0 || y == 0 || x + 1 == self.w || y + 1 == self.h
    }

    /// 8 neighbours with their distance in cells, fixed order.
    fn neighbours8(self, i: usize) -> impl Iterator<Item = (usize, f64)> {
        let (x, y) = ((i % self.w) as isize, (i / self.w) as isize);
        let (w, h) = (self.w as isize, self.h as isize);
        const D8: [(isize, isize, f64); 8] = [
            (1, 0, 1.0),
            (1, 1, std::f64::consts::SQRT_2),
            (0, 1, 1.0),
            (-1, 1, std::f64::consts::SQRT_2),
            (-1, 0, 1.0),
            (-1, -1, std::f64::consts::SQRT_2),
            (0, -1, 1.0),
            (1, -1, std::f64::consts::SQRT_2),
        ];
        D8.into_iter().filter_map(move |(dx, dy, d)| {
            let (nx, ny) = (x + dx, y + dy);
            (nx >= 0 && ny >= 0 && nx < w && ny < h).then(|| ((ny * w + nx) as usize, d))
        })
    }
}

/// Elevation in metres of a 16-bit level.
fn metres(q: u16, min_m: f64, max_m: f64) -> f64 {
    min_m + f64::from(q) * (max_m - min_m) / Q_MAX
}

/// A bilinear elevation source over the raster (lon/lat is linear in km in
/// the local projection, so the raster is a regular lon/lat grid).
fn tile(proj: &Projection, r: Raster, z: &[u16], min_m: f64, max_m: f64) -> Result<Tile, String> {
    let (lon0, lat0) = proj.to_lonlat(Km {
        x: r.cell_km / 2.0,
        y: r.cell_km / 2.0,
    });
    let (wlon, nlat) = proj.to_lonlat(Km { x: 0.0, y: 0.0 });
    let (elon, slat) = proj.to_lonlat(Km {
        x: r.cell_km * r.w as f64,
        y: r.cell_km * r.h as f64,
    });
    let data = z.iter().map(|&q| metres(q, min_m, max_m)).collect();
    Tile::from_grid(
        r.w,
        r.h,
        data,
        lon0,
        lat0,
        (elon - wlon) / r.w as f64,
        (nlat - slat) / r.h as f64,
    )
}

const TILT_DIRS: [(f64, f64); 8] = [
    (1.0, 0.0),
    (
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ),
    (0.0, 1.0),
    (
        -std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ),
    (-1.0, 0.0),
    (
        -std::f64::consts::FRAC_1_SQRT_2,
        -std::f64::consts::FRAC_1_SQRT_2,
    ),
    (0.0, -1.0),
    (
        std::f64::consts::FRAC_1_SQRT_2,
        -std::f64::consts::FRAC_1_SQRT_2,
    ),
];

/// Step 1: heights as 16-bit levels over the biome's elevation range.
fn heights(seed: u64, b: &BiomeConfig, r: Raster) -> Vec<u16> {
    let (sb, sr, sm) = (
        noise::channel(seed, 1),
        noise::channel(seed, 2),
        noise::channel(seed, 3),
    );
    let (dx, dy) = TILT_DIRS[(noise::channel(seed, 9) >> 61) as usize];
    let (cx, cy) = (r.w as f64 * r.cell_km / 2.0, r.h as f64 * r.cell_km / 2.0);
    let half_diag = cx.hypot(cy).max(1e-9);
    let raw: Vec<f64> = (0..r.len())
        .map(|i| {
            let p = r.centre(i);
            let base = noise::fbm(sb, &b.base, p.x, p.y);
            let ridge = noise::ridged(sr, &b.ridge, p.x, p.y);
            let m = noise::fbm(sm, &b.ridge_mask, p.x, p.y);
            let t = ((m - b.ridge_mask_low) / (b.ridge_mask_high - b.ridge_mask_low).max(1e-9))
                .clamp(0.0, 1.0);
            let mask = t * t * (3.0 - 2.0 * t);
            let tilt = b.tilt * 0.5 * ((p.x - cx) * dx + (p.y - cy) * dy) / half_diag;
            b.base_weight * base + b.ridge_weight * ridge * mask + tilt
        })
        .collect();
    let lo = raw.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = raw.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (hi - lo).max(1e-12);
    raw.iter()
        .map(|v| {
            let n = (v - lo) / span;
            let n = (1.0 - b.lowland_curve) * n + b.lowland_curve * n * n;
            (n.clamp(0.0, 1.0) * Q_MAX).round() as u16
        })
        .collect()
}

/// Step 2: filled surface, receivers and drained cells.
pub struct Drainage {
    pub filled: Vec<u16>,
    /// Downstream cell; `None` at the map edge (outlets).
    pub recv: Vec<Option<usize>>,
    /// Cells drained through each cell, itself included.
    pub acc: Vec<u32>,
}

fn drainage(r: Raster, z: &[u16]) -> Drainage {
    let n = r.len();
    let mut filled = z.to_vec();
    let mut flood_recv: Vec<Option<usize>> = vec![None; n];
    let mut pop = vec![0u32; n];
    let mut closed = vec![false; n];
    let mut heap = BinaryHeap::new();
    for i in 0..n {
        if r.border(i) {
            closed[i] = true;
            heap.push(Reverse((z[i], i)));
        }
    }
    let mut k = 0u32;
    while let Some(Reverse((f, c))) = heap.pop() {
        pop[c] = k;
        k += 1;
        for (nb, _) in r.neighbours8(c) {
            if !closed[nb] {
                closed[nb] = true;
                filled[nb] = z[nb].max(f);
                flood_recv[nb] = Some(c);
                heap.push(Reverse((filled[nb], nb)));
            }
        }
    }
    // D8 on the filled surface; on flats the flood order (towards the outlet).
    let recv: Vec<Option<usize>> = (0..n)
        .map(|c| {
            if r.border(c) {
                return None;
            }
            let mut best: Option<(f64, usize)> = None;
            for (nb, d) in r.neighbours8(c) {
                if filled[nb] < filled[c] {
                    let drop = f64::from(filled[c] - filled[nb]) / d;
                    if best.is_none_or(|(b, _)| drop > b) {
                        best = Some((drop, nb));
                    }
                }
            }
            best.map(|(_, nb)| nb).or(flood_recv[c])
        })
        .collect();
    // Every receiver has a smaller (filled, flood order) key: upstream first.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&c| Reverse((filled[c], pop[c])));
    let mut acc = vec![1u32; n];
    for &c in &order {
        if let Some(d) = recv[c] {
            acc[d] += acc[c];
        }
    }
    Drainage { filled, recv, acc }
}

/// 8-connected components of `mask` with at least `min` cells.
fn components(r: Raster, mask: &[bool], min: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; r.len()];
    let mut out = Vec::new();
    for s in 0..r.len() {
        if !mask[s] || seen[s] {
            continue;
        }
        let (mut comp, mut stack) = (Vec::new(), vec![s]);
        seen[s] = true;
        while let Some(c) = stack.pop() {
            comp.push(c);
            for (nb, _) in r.neighbours8(c) {
                if mask[nb] && !seen[nb] {
                    seen[nb] = true;
                    stack.push(nb);
                }
            }
        }
        if comp.len() >= min {
            comp.sort_unstable();
            out.push(comp);
        }
    }
    out
}

/// A river run of one class, as raster cells from upstream to downstream.
#[derive(Debug, Clone)]
pub struct River {
    pub cells: Vec<usize>,
    pub major: bool,
}

fn rivers(
    r: Raster,
    d: &Drainage,
    lake: &[bool],
    min_cells: f64,
    major_cells: f64,
) -> (Vec<River>, Vec<bool>) {
    let n = r.len();
    let river: Vec<bool> = (0..n)
        .map(|c| !lake[c] && f64::from(d.acc[c]) >= min_cells)
        .collect();
    let mut up_river = vec![false; n];
    let mut lake_in: Vec<Option<usize>> = vec![None; n];
    for c in 0..n {
        if let Some(t) = d.recv[c] {
            if river[c] {
                up_river[t] = true;
            } else if lake[c] && lake_in[t].is_none() {
                lake_in[t] = Some(c);
            }
        }
    }
    let mut visited = vec![false; n];
    let mut out = Vec::new();
    for s in (0..n).filter(|&c| river[c] && !up_river[c]) {
        let mut cells: Vec<usize> = lake_in[s].into_iter().collect();
        let mut c = s;
        loop {
            cells.push(c);
            if visited[c] {
                break;
            }
            visited[c] = true;
            match d.recv[c] {
                None => break,
                Some(t) if lake[t] => {
                    cells.push(t);
                    break;
                }
                Some(t) => c = t,
            }
        }
        // Split into runs by class (a segment takes its downstream land cell's).
        let major = |a: usize, b: usize| {
            let c = if lake[b] { a } else { b };
            f64::from(d.acc[c]) >= major_cells
        };
        let mut run = vec![cells[0]];
        let mut class: Option<bool> = None;
        for w in cells.windows(2) {
            let m = major(w[0], w[1]);
            if class.is_some_and(|k| k != m) {
                out.push(River {
                    cells: std::mem::replace(&mut run, vec![w[0]]),
                    major: class.unwrap_or(false),
                });
            }
            class = Some(m);
            run.push(w[1]);
        }
        if run.len() >= 2 {
            out.push(River {
                cells: run,
                major: class.unwrap_or(false),
            });
        }
    }
    (out, river)
}

/// Distance (cells) to the nearest `source` cell, two-pass chamfer 1 / √2.
fn distance(r: Raster, source: &[bool]) -> Vec<f64> {
    let mut d: Vec<f64> = source
        .iter()
        .map(|&s| if s { 0.0 } else { f64::INFINITY })
        .collect();
    let s2 = std::f64::consts::SQRT_2;
    let (w, h) = (r.w as isize, r.h as isize);
    let at = |x: isize, y: isize| (y * w + x) as usize;
    for y in 0..h {
        for x in 0..w {
            let mut v = d[at(x, y)];
            for (dx, dy, c) in [(-1, 0, 1.0), (-1, -1, s2), (0, -1, 1.0), (1, -1, s2)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && nx < w {
                    v = v.min(d[at(nx, ny)] + c);
                }
            }
            d[at(x, y)] = v;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let mut v = d[at(x, y)];
            for (dx, dy, c) in [(1, 0, 1.0), (1, 1, s2), (0, 1, 1.0), (-1, 1, s2)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && nx < w && ny < h {
                    v = v.min(d[at(nx, ny)] + c);
                }
            }
            d[at(x, y)] = v;
        }
    }
    d
}

/// Step 3: forest cells.
fn forest(
    seed: u64,
    b: &BiomeConfig,
    r: Raster,
    z: &[u16],
    lake: &[bool],
    river: &[bool],
) -> Vec<bool> {
    let water: Vec<bool> = lake.iter().zip(river).map(|(l, v)| *l || *v).collect();
    let dist = distance(r, &water);
    let sm = noise::channel(seed, 4);
    let cell_m = r.cell_km * 1000.0;
    let span_m = b.elevation_max_m - b.elevation_min_m;
    let zm = |x: usize, y: usize| metres(z[y * r.w + x], b.elevation_min_m, b.elevation_max_m);
    (0..r.len())
        .map(|i| {
            if lake[i] {
                return false;
            }
            let (x, y) = (i % r.w, i / r.w);
            let (x0, x1) = (x.saturating_sub(1), (x + 1).min(r.w - 1));
            let (y0, y1) = (y.saturating_sub(1), (y + 1).min(r.h - 1));
            let dzdx = (zm(x1, y) - zm(x0, y)) / ((x1 - x0) as f64 * cell_m);
            let dzdy = (zm(x, y1) - zm(x, y0)) / ((y1 - y0) as f64 * cell_m);
            let slope = dzdx.hypot(dzdy).atan().to_degrees();
            let p = r.centre(i);
            let alt = (zm(x, y) - b.elevation_min_m) / span_m.max(1e-9);
            let near = (1.0 - dist[i] * r.cell_km / b.moisture_water_reach_km.max(1e-9)).max(0.0);
            let m = b.moisture_noise_weight * noise::fbm(sm, &b.moisture, p.x, p.y)
                + b.moisture_water_weight * near
                + b.moisture_altitude_weight * alt
                + b.moisture_slope_weight * (slope / b.moisture_slope_ref_deg.max(1e-9)).min(1.0);
            m >= b.forest_threshold
        })
        .collect()
}

/// Offset neighbours of a hex on the grid, fixed order.
pub fn hex_neighbours(grid: &Grid, h: Off) -> Vec<Off> {
    offset_to_axial(h)
        .neighbors()
        .into_iter()
        .map(axial_to_offset)
        .filter(|&[c, r]| c >= 0 && r >= 0 && (c as u32) < grid.cols && (r as u32) < grid.rows)
        .collect()
}

fn side(a: Off, b: Off) -> Side {
    if a <= b {
        [a, b]
    } else {
        [b, a]
    }
}

fn hex_dist(a: Off, b: Off) -> u32 {
    offset_to_axial(a).distance(offset_to_axial(b))
}

/// Per-hex shares of raster masks.
struct HexShares {
    lake: f64,
    forest: f64,
    river: bool,
}

/// Everything `procgen` makes; the extra fields are for tests.
pub struct Generated {
    pub map: Value,
    pub procedural: Value,
    pub elevation: Elevation16,
    pub hillshade: GrayImage,
    pub hillshade_meta: Value,
    #[cfg_attr(not(test), allow(dead_code))]
    pub raster: Raster,
    #[cfg_attr(not(test), allow(dead_code))]
    pub filled: Vec<u16>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub rivers: Vec<River>,
    /// Share of each hex [row][col] in lakes.
    #[cfg_attr(not(test), allow(dead_code))]
    pub lake_share: Vec<Vec<f64>>,
}

/// The whole pipeline.
pub fn generate(
    p: &Params,
    cfg: &Config,
    terrain_costs: &TerrainCosts,
) -> Result<Generated, String> {
    let pc = cfg
        .procgen
        .as_ref()
        .ok_or("mapgen.json: no `procgen` section")?;
    let b = pc.biomes.get(&p.biome).ok_or_else(|| {
        format!(
            "procgen: unknown biome `{}` (have: {})",
            p.biome,
            pc.biomes.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;
    let bbox = bbox_for(p, cfg.earth_radius_km)?;
    let proj = Projection::new(bbox, cfg.earth_radius_km);
    let grid = Grid::fit(proj.size_km(), p.hex_km)?;
    let r = Raster::new(proj.size_km(), pc.raster_km);
    let z = heights(p.seed, b, r);
    let (min_m, max_m) = (b.elevation_min_m, b.elevation_max_m);
    let dem = tile(&proj, r, &z, min_m, max_m)?;
    let d = drainage(r, &z);
    let m_per_q = (max_m - min_m) / Q_MAX;
    let deep: Vec<bool> = (0..r.len())
        .map(|c| f64::from(d.filled[c] - z[c]) * m_per_q > b.lake_min_depth_m)
        .collect();
    let cell_km2 = pc.raster_km * pc.raster_km;
    let min_lake = (b.lake_min_area_km2 / cell_km2).ceil() as usize;
    let mut lakes = components(r, &deep, min_lake);
    let depth = |comp: &Vec<usize>| comp.iter().map(|&c| d.filled[c] - z[c]).max().unwrap_or(0);
    lakes.sort_by_key(|comp| (Reverse(depth(comp)), comp[0]));
    let budget = (b.lake_max_share * r.len() as f64) as usize;
    let mut used = 0;
    lakes.retain(|comp| {
        let keep = used + comp.len() <= budget;
        used += if keep { comp.len() } else { 0 };
        keep
    });
    lakes.sort_by_key(|comp| comp[0]);
    let mut lake = vec![false; r.len()];
    for c in lakes.iter().flatten() {
        lake[*c] = true;
    }
    let (river_runs, river) = rivers(
        r,
        &d,
        &lake,
        b.river_min_area_km2 / cell_km2,
        b.major_river_min_area_km2 / cell_km2,
    );
    let wood = forest(p.seed, b, r, &z, &lake, &river);

    // Step 4: hexes.
    let (cols, rows) = (grid.cols as usize, grid.rows as usize);
    let mut shares: Vec<Vec<HexShares>> = Vec::with_capacity(rows);
    let mut mean_m = vec![vec![0.0; cols]; rows];
    let mut terrain = vec![vec![String::new(); cols]; rows];
    for row in 0..grid.rows {
        let mut line = Vec::with_capacity(cols);
        for col in 0..grid.cols {
            let (mut n, mut l, mut f, mut rv) = (0usize, 0usize, 0usize, false);
            for s in grid.samples(col, row, cfg.samples_per_hex_side) {
                let Some(c) = r.cell_at(s) else { continue };
                n += 1;
                l += usize::from(lake[c]);
                f += usize::from(wood[c]);
                rv |= river[c];
            }
            let n = n.max(1) as f64;
            let sh = HexShares {
                lake: l as f64 / n,
                forest: f as f64 / n,
                river: rv,
            };
            let stats = crate::relief::hex_stats(&dem, &proj, &grid, col, row, cfg);
            let (cu, ru) = (col as usize, row as usize);
            mean_m[ru][cu] = stats.map_or(min_m, |s| s.mean_m);
            terrain[ru][cu] = if sh.lake >= pc.lake_min_share {
                pc.water_terrain.clone()
            } else {
                let by_relief = stats
                    .and_then(|s| {
                        b.relief_rules
                            .iter()
                            .find(|rule| {
                                s.relief_m >= rule.min_relief_m || s.slope_deg >= rule.min_slope_deg
                            })
                            .map(|rule| rule.terrain.clone())
                    })
                    .unwrap_or_else(|| b.default_terrain.clone());
                if b.forest_on.contains(&by_relief) && sh.forest >= pc.forest_min_share {
                    pc.forest_terrain.clone()
                } else {
                    by_relief
                }
            };
            line.push(sh);
        }
        shares.push(line);
    }
    let lake_share: Vec<Vec<f64>> = shares
        .iter()
        .map(|l| l.iter().map(|s| s.lake).collect())
        .collect();
    let to_lonlat = |c: usize| {
        let k = r.centre(c);
        proj.to_lonlat(k)
    };
    let major_lines: Vec<Line> = river_runs
        .iter()
        .filter(|v| v.major)
        .map(|v| Line {
            kind: pc.river_kind.clone(),
            pts: v.cells.iter().map(|&c| to_lonlat(c)).collect(),
        })
        .collect();
    let is_water =
        |t: &Vec<Vec<String>>, [c, r]: Off| t[r as usize][c as usize] == pc.water_terrain;
    let canals: BTreeSet<Side> = crate::features::river_edges(
        &grid,
        &proj,
        &major_lines,
        pc.major_river_min_length_km,
        pc.major_river_line_weight,
    )
    .into_iter()
    .filter(|[a, b]| !is_water(&terrain, *a) && !is_water(&terrain, *b))
    .collect();

    // Step 5: cities.
    let main_land = main_land(&grid, &terrain, &pc.water_terrain);
    let mut placed: Vec<(Off, CitySize)> = Vec::new();
    for (k, c) in p.cities.iter().enumerate() {
        let at = place_city(
            p,
            pc,
            &grid,
            &terrain,
            &main_land,
            &shares,
            &mean_m,
            (min_m, max_m),
            &placed,
            k,
            c,
        )?;
        terrain[at[1] as usize][at[0] as usize] = pc.city_terrain.clone();
        placed.push((at, c.size));
    }

    // Step 6: roads.
    let (roads, tracks) = lay_roads(pc, terrain_costs, &grid, &terrain, &canals, &placed)?;
    let (pass, bridges) = passes_and_bridges(pc, &terrain, &roads, &tracks, &canals);
    let places: Vec<Value> = p
        .cities
        .iter()
        .zip(&placed)
        .map(|(c, (at, size))| {
            let kind = match size {
                CitySize::Large => &pc.cities.kind_large,
                CitySize::Small => &pc.cities.kind_small,
            };
            json!({ "name": c.name, "kind": kind, "hex": at })
        })
        .collect();
    let legend: Vec<(String, char)> = cfg.legend.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let roads_v: Vec<Side> = roads.iter().copied().collect();
    let tracks_v: Vec<Side> = tracks.iter().copied().collect();
    let canals_v: Vec<Side> = canals.iter().copied().collect();
    let source = format!(
        "mapgen procgen: procedural relief, seed {}, biome {}",
        p.seed, p.biome
    );
    let mut map = crate::output::map_json(&crate::output::MapInput {
        grid: &grid,
        terrain: &terrain,
        pass: &pass,
        legend: &legend,
        pass_char: cfg.pass_char,
        bbox,
        hex_km: p.hex_km,
        sources: std::slice::from_ref(&source),
        roads: &roads_v,
        canals: &canals_v,
        bridges: &bridges,
        places: &places,
        size_km: proj.size_km(),
    })?;
    map["_comment"] = json!(format!(
        "Generated by mapgen procgen (procedural relief, seed {}, biome {}); edit in the map editor. Fictional land: no geodata.",
        p.seed, p.biome
    ));
    map["tracks"] = json!(tracks_v);
    map["geo"]["procedural"] = json!(true);

    // Bake data: rivers, lakes and forest in lon/lat.
    let ll = |k: Km| {
        let (lon, lat) = proj.to_lonlat(k);
        json!([round6(lon), round6(lat)])
    };
    let river_json: Vec<Value> = river_runs
        .iter()
        .map(|v| {
            let pts: Vec<Km> = v.cells.iter().map(|&c| r.centre(c)).collect();
            let pts = chaikin(pts, pc.river_smooth_iterations);
            json!({
                "kind": if v.major { &pc.river_kind } else { &pc.stream_kind },
                "pts": pts.into_iter().map(ll).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut areas = Vec::new();
    for comp in &lakes {
        let rings = outline(r, comp, None);
        areas.push(json!({ "kind": "water", "rings": rings_json(&rings, r, &proj) }));
    }
    let t = pc.forest_area_tile_cells.max(1) as usize;
    for ty in (0..r.h).step_by(t) {
        for tx in (0..r.w).step_by(t) {
            let cells: Vec<usize> = (ty..(ty + t).min(r.h))
                .flat_map(|y| (tx..(tx + t).min(r.w)).map(move |x| y * r.w + x))
                .filter(|&c| wood[c])
                .collect();
            if cells.is_empty() {
                continue;
            }
            let rings = outline(r, &cells, Some((tx, ty, t)));
            areas.push(json!({ "kind": "forest", "rings": rings_json(&rings, r, &proj) }));
        }
    }
    let procedural = json!({
        "_comment": "mapgen procgen: data for `mapgen bake` (R-UI-01). elevation.png is 16-bit grayscale, level q → min_m + q·(max_m − min_m)/65535, cell (0, 0) at the bbox's north-west corner; rivers (kind = bake.water.river_km key) and areas (water | forest rings) in lon/lat.",
        "seed": p.seed,
        "biome": p.biome,
        "elevation": {
            "png": ELEVATION_PNG,
            "width": r.w,
            "height": r.h,
            "cell_km": r.cell_km,
            "min_m": min_m,
            "max_m": max_m,
        },
        "rivers": river_json,
        "areas": areas,
    });
    let elevation = Elevation16::from_raw(r.w as u32, r.h as u32, z.clone())
        .ok_or("procgen: elevation raster")?;
    let hillshade = crate::hillshade::hillshade(&dem, &proj, &cfg.hillshade);
    let hillshade_meta = crate::output::hillshade_json(
        &grid,
        hillshade.width(),
        hillshade.height(),
        cfg.hillshade.px_per_km,
    );
    Ok(Generated {
        map,
        procedural,
        elevation,
        hillshade,
        hillshade_meta,
        raster: r,
        filled: d.filled,
        rivers: river_runs,
        lake_share,
    })
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Chaikin corner cutting; the ends stay.
fn chaikin(mut pts: Vec<Km>, iterations: u32) -> Vec<Km> {
    for _ in 0..iterations {
        if pts.len() < 3 {
            break;
        }
        let mut out = vec![pts[0]];
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            out.push(Km {
                x: 0.75 * a.x + 0.25 * b.x,
                y: 0.75 * a.y + 0.25 * b.y,
            });
            out.push(Km {
                x: 0.25 * a.x + 0.75 * b.x,
                y: 0.25 * a.y + 0.75 * b.y,
            });
        }
        out.push(pts[pts.len() - 1]);
        pts = out;
    }
    pts
}

/// Outline of a set of cells as closed rings of cell corners (x, y in
/// cells); with `tile`, cells outside the tile count as outside.
fn outline(
    r: Raster,
    cells: &[usize],
    tile: Option<(usize, usize, usize)>,
) -> Vec<Vec<(i64, i64)>> {
    let set: BTreeSet<usize> = cells.iter().copied().collect();
    let inside = |x: i64, y: i64| {
        if x < 0 || y < 0 || x >= r.w as i64 || y >= r.h as i64 {
            return false;
        }
        if let Some((tx, ty, t)) = tile {
            let (tx, ty, t) = (tx as i64, ty as i64, t as i64);
            if x < tx || y < ty || x >= tx + t || y >= ty + t {
                return false;
            }
        }
        set.contains(&(y as usize * r.w + x as usize))
    };
    let mut segments: Vec<[(i64, i64); 2]> = Vec::new();
    for &c in &set {
        let (x, y) = ((c % r.w) as i64, (c / r.w) as i64);
        // Clockwise on screen, cell interior on the right.
        if !inside(x, y - 1) {
            segments.push([(x, y), (x + 1, y)]);
        }
        if !inside(x + 1, y) {
            segments.push([(x + 1, y), (x + 1, y + 1)]);
        }
        if !inside(x, y + 1) {
            segments.push([(x + 1, y + 1), (x, y + 1)]);
        }
        if !inside(x - 1, y) {
            segments.push([(x, y + 1), (x, y)]);
        }
    }
    // Directed chaining: each corner's outgoing edges, smallest first.
    let mut from: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (k, s) in segments.iter().enumerate() {
        from.entry(s[0]).or_default().push(k);
    }
    let mut used = vec![false; segments.len()];
    let mut out = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut ring = vec![segments[start][0], segments[start][1]];
        while ring[ring.len() - 1] != ring[0] {
            let end = ring[ring.len() - 1];
            let Some(k) = from
                .get(&end)
                .and_then(|l| l.iter().copied().find(|&k| !used[k]))
            else {
                break;
            };
            used[k] = true;
            ring.push(segments[k][1]);
        }
        out.push(ring);
    }
    out
}

fn rings_json(rings: &[Vec<(i64, i64)>], r: Raster, proj: &Projection) -> Vec<Value> {
    rings
        .iter()
        .map(|ring| {
            Value::Array(
                ring.iter()
                    .map(|&(x, y)| {
                        let (lon, lat) = proj.to_lonlat(Km {
                            x: x as f64 * r.cell_km,
                            y: y as f64 * r.cell_km,
                        });
                        json!([round6(lon), round6(lat)])
                    })
                    .collect(),
            )
        })
        .collect()
}

/// Hexes of the largest land region (not water, by hex neighbours).
fn main_land(grid: &Grid, terrain: &[Vec<String>], water: &str) -> BTreeSet<Off> {
    let mut seen: BTreeSet<Off> = BTreeSet::new();
    let mut best: BTreeSet<Off> = BTreeSet::new();
    for row in 0..grid.rows as i32 {
        for col in 0..grid.cols as i32 {
            let s = [col, row];
            if seen.contains(&s) || terrain[row as usize][col as usize] == water {
                continue;
            }
            let mut comp = BTreeSet::from([s]);
            seen.insert(s);
            let mut stack = vec![s];
            while let Some(h) = stack.pop() {
                for n in hex_neighbours(grid, h) {
                    if terrain[n[1] as usize][n[0] as usize] != water && seen.insert(n) {
                        comp.insert(n);
                        stack.push(n);
                    }
                }
            }
            if comp.len() > best.len() {
                best = comp;
            }
        }
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn place_city(
    p: &Params,
    pc: &ProcgenConfig,
    grid: &Grid,
    terrain: &[Vec<String>],
    main_land: &BTreeSet<Off>,
    shares: &[Vec<HexShares>],
    mean_m: &[Vec<f64>],
    (min_m, max_m): (f64, f64),
    placed: &[(Off, CitySize)],
    k: usize,
    c: &CityParam,
) -> Result<Off, String> {
    let cp = &pc.cities;
    let on_grid = |[col, row]: Off| {
        col >= 0 && row >= 0 && (col as u32) < grid.cols && (row as u32) < grid.rows
    };
    if let Some(h) = c.hint {
        if !on_grid(h) {
            return Err(format!(
                "procgen: city `{}` hint {h:?} is off the map",
                c.name
            ));
        }
    }
    let mut rng = Pcg32::new(noise::channel(p.seed, 100 + k as u64), 11);
    let jitter: Vec<f64> = (0..grid.cols * grid.rows).map(|_| rng.next_f64()).collect();
    let margin = cp.border_margin_hex as i32;
    let max_radius = grid.cols.max(grid.rows) * 2;
    let mut radius = cp.hint_radius_hex.max(1);
    loop {
        let mut best: Option<(f64, Off)> = None;
        for row in 0..grid.rows as i32 {
            for col in 0..grid.cols as i32 {
                let h = [col, row];
                let t = &terrain[row as usize][col as usize];
                if !main_land.contains(&h)
                    || *t == pc.water_terrain
                    || *t == pc.city_terrain
                    || cp.forbidden_terrain.contains(t)
                    || placed
                        .iter()
                        .any(|(o, _)| hex_dist(*o, h) < cp.min_spacing_hex)
                {
                    continue;
                }
                let sh = &shares[row as usize][col as usize];
                let low =
                    1.0 - (mean_m[row as usize][col as usize] - min_m) / (max_m - min_m).max(1e-9);
                let mut score = cp.weight_river * f64::from(u8::from(sh.river))
                    + cp.weight_lowland * low
                    + cp.weight_jitter * jitter[(row as u32 * grid.cols + col as u32) as usize];
                match c.hint {
                    Some(hint) => {
                        let dist = hex_dist(hint, h);
                        if dist > radius {
                            continue;
                        }
                        score += cp.weight_hint_distance * f64::from(dist);
                    }
                    None => {
                        if col < margin
                            || row < margin
                            || col >= grid.cols as i32 - margin
                            || row >= grid.rows as i32 - margin
                        {
                            continue;
                        }
                        let spread = placed
                            .iter()
                            .map(|(o, _)| f64::from(hex_dist(*o, h)))
                            .fold(cp.spread_cap_hex, f64::min);
                        score += cp.weight_spread * spread / cp.spread_cap_hex.max(1e-9);
                    }
                }
                if best.is_none_or(|(b, _)| score > b) {
                    best = Some((score, h));
                }
            }
        }
        if let Some((_, h)) = best {
            return Ok(h);
        }
        if c.hint.is_none() || radius >= max_radius {
            return Err(format!("procgen: no hex for city `{}`", c.name));
        }
        radius *= 2;
    }
}

/// Least-cost hex paths from `src` (costs in thousandths of AP).
fn dijkstra(
    grid: &Grid,
    cost: &dyn Fn(Off, Off) -> Option<u64>,
    src: Off,
) -> BTreeMap<Off, (u64, Option<Off>)> {
    let mut best: BTreeMap<Off, (u64, Option<Off>)> = BTreeMap::from([(src, (0, None))]);
    let mut heap = BinaryHeap::from([Reverse((0u64, src))]);
    while let Some(Reverse((c, h))) = heap.pop() {
        if best.get(&h).is_some_and(|&(b, _)| c > b) {
            continue;
        }
        for n in hex_neighbours(grid, h) {
            let Some(step) = cost(h, n) else { continue };
            let nc = c + step;
            if best.get(&n).is_none_or(|&(b, _)| nc < b) {
                best.insert(n, (nc, Some(h)));
                heap.push(Reverse((nc, n)));
            }
        }
    }
    best
}

fn path_to(best: &BTreeMap<Off, (u64, Option<Off>)>, goal: Off) -> Vec<Off> {
    let mut out = vec![goal];
    let mut h = goal;
    while let Some(&(_, Some(prev))) = best.get(&h) {
        out.push(prev);
        h = prev;
    }
    out.reverse();
    out
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut i = i;
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Step 6: roads (highways) and tracks as hex sides.
fn lay_roads(
    pc: &ProcgenConfig,
    tc: &TerrainCosts,
    grid: &Grid,
    terrain: &[Vec<String>],
    canals: &BTreeSet<Side>,
    cities: &[(Off, CitySize)],
) -> Result<(BTreeSet<Side>, BTreeSet<Side>), String> {
    let rc = &pc.roads;
    let base = |a: Off, b: Off| -> Option<f64> {
        let t = &terrain[b[1] as usize][b[0] as usize];
        if tc.impassable.contains(t) {
            return None;
        }
        let mut c = tc.ap.get(t).copied().unwrap_or(1.0).max(0.1);
        if *t == pc.mountain_terrain {
            c += rc.mountain_extra_cost;
        }
        if canals.contains(&side(a, b)) {
            c += rc.river_crossing_cost;
        }
        Some(c)
    };
    let milli = |c: f64| (c * 1000.0).round() as u64;
    let n = cities.len();
    // Pairwise costs on the empty map.
    let mut cost = vec![vec![u64::MAX; n]; n];
    for i in 0..n {
        let best = dijkstra(grid, &|a, b| base(a, b).map(milli), cities[i].0);
        for j in 0..n {
            if let Some(&(c, _)) = best.get(&cities[j].0) {
                cost[i][j] = c;
            }
        }
    }
    let mut pairs: Vec<(u64, usize, usize)> = Vec::new();
    for (i, row) in cost.iter().enumerate() {
        for (j, &c) in row.iter().enumerate().skip(i + 1) {
            if c == u64::MAX {
                return Err(format!(
                    "procgen: cities {i} and {j} cannot be joined by road"
                ));
            }
            pairs.push((c, i, j));
        }
    }
    pairs.sort_unstable();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut links: Vec<(usize, usize)> = Vec::new();
    let inf = u64::MAX / 4;
    let mut net = vec![vec![inf; n]; n];
    for (i, row) in net.iter_mut().enumerate() {
        row[i] = 0;
    }
    // Highways first: a tree over the large cities, then the rest joins it.
    let large = |i: usize| cities[i].1 == CitySize::Large;
    let ordered: Vec<(u64, usize, usize)> = pairs
        .iter()
        .filter(|&&(_, i, j)| large(i) && large(j))
        .chain(pairs.iter().filter(|&&(_, i, j)| !(large(i) && large(j))))
        .copied()
        .collect();
    for &(c, i, j) in &ordered {
        let (a, b) = (find(&mut parent, i), find(&mut parent, j));
        if a != b {
            parent[a] = b;
            links.push((i, j));
            net[i][j] = c;
            net[j][i] = c;
        }
    }
    let floyd = |net: &mut Vec<Vec<u64>>| {
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    let v = net[i][k].saturating_add(net[k][j]);
                    if v < net[i][j] {
                        net[i][j] = v;
                    }
                }
            }
        }
    };
    floyd(&mut net);
    let mut extra = 0;
    for &(c, i, j) in &pairs {
        if extra >= rc.extra_links || links.contains(&(i, j)) {
            continue;
        }
        if net[i][j] as f64 > rc.extra_detour_ratio * c as f64 {
            links.push((i, j));
            net[i][j] = c;
            net[j][i] = c;
            floyd(&mut net);
            extra += 1;
        }
    }
    let (mut roads, mut tracks) = (BTreeSet::new(), BTreeSet::new());
    for (i, j) in links {
        let used: BTreeSet<Side> = roads.union(&tracks).copied().collect();
        let step = |a: Off, b: Off| {
            base(a, b).map(|c| {
                milli(if used.contains(&side(a, b)) {
                    c * rc.reuse_factor
                } else {
                    c
                })
            })
        };
        let best = dijkstra(grid, &step, cities[i].0);
        if !best.contains_key(&cities[j].0) {
            return Err(format!(
                "procgen: cities {i} and {j} cannot be joined by road"
            ));
        }
        let path = path_to(&best, cities[j].0);
        let highway = large(i) && large(j);
        for w in path.windows(2) {
            let s = side(w[0], w[1]);
            if highway {
                roads.insert(s);
            } else {
                tracks.insert(s);
            }
        }
    }
    let tracks: BTreeSet<Side> = tracks.difference(&roads).copied().collect();
    Ok((roads, tracks))
}

/// A mountain hex with a road or track is a pass; a road or track over a
/// major river side has a bridge.
fn passes_and_bridges(
    pc: &ProcgenConfig,
    terrain: &[Vec<String>],
    roads: &BTreeSet<Side>,
    tracks: &BTreeSet<Side>,
    canals: &BTreeSet<Side>,
) -> (Vec<Vec<bool>>, Vec<Side>) {
    let mut pass: Vec<Vec<bool>> = terrain.iter().map(|l| vec![false; l.len()]).collect();
    for [a, b] in roads.iter().chain(tracks) {
        for [c, r] in [a, b] {
            if terrain[*r as usize][*c as usize] == pc.mountain_terrain {
                pass[*r as usize][*c as usize] = true;
            }
        }
    }
    let bridges = roads
        .union(tracks)
        .filter(|s| canals.contains(*s))
        .copied()
        .collect();
    (pass, bridges)
}

/// Writes map.json, procedural.json, elevation.png, hillshade.png/json.
pub fn write(out: &Path, g: &Generated) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let pretty = |v: &Value| {
        serde_json::to_string_pretty(v)
            .map(|s| s + "\n")
            .map_err(|e| e.to_string())
    };
    let put = |name: &str, text: String| {
        fs::write(out.join(name), text).map_err(|e| format!("{}: {e}", out.join(name).display()))
    };
    put("map.json", pretty(&g.map)?)?;
    // Machine data (long coordinate lists): compact, one line.
    let compact = serde_json::to_string(&g.procedural).map_err(|e| e.to_string())? + "\n";
    put(PROCEDURAL_JSON, compact)?;
    put("hillshade.json", pretty(&g.hillshade_meta)?)?;
    g.elevation
        .save(out.join(ELEVATION_PNG))
        .map_err(|e| format!("{ELEVATION_PNG}: {e}"))?;
    g.hillshade
        .save(out.join("hillshade.png"))
        .map_err(|e| format!("hillshade.png: {e}"))
}

/// What `mapgen bake` needs from a procedural map directory.
pub struct BakeData {
    pub dem: Tile,
    pub osm: OsmData,
    pub biome: String,
}

fn lonlat(v: &Value) -> Result<LonLat, String> {
    match (v[0].as_f64(), v[1].as_f64()) {
        (Some(lon), Some(lat)) => Ok((lon, lat)),
        _ => Err(format!("{PROCEDURAL_JSON}: bad point {v}")),
    }
}

/// Reads procedural.json and elevation.png of `dir`, if it is a procedural map.
pub fn load_for_bake(
    dir: &Path,
    proj: &Projection,
    pc: &ProcgenConfig,
    veg_prefix: &str,
) -> Result<Option<BakeData>, String> {
    let path = dir.join(PROCEDURAL_JSON);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let e = &v["elevation"];
    let png = dir.join(e["png"].as_str().unwrap_or(ELEVATION_PNG));
    let img = image::open(&png)
        .map_err(|err| format!("{}: {err}", png.display()))?
        .to_luma16();
    let r = Raster {
        w: img.width() as usize,
        h: img.height() as usize,
        cell_km: e["cell_km"]
            .as_f64()
            .ok_or("procedural.json: elevation.cell_km")?,
    };
    let (min_m, max_m) = (
        e["min_m"]
            .as_f64()
            .ok_or("procedural.json: elevation.min_m")?,
        e["max_m"]
            .as_f64()
            .ok_or("procedural.json: elevation.max_m")?,
    );
    let dem = tile(proj, r, img.as_raw(), min_m, max_m)?;
    let mut osm = OsmData::default();
    for river in v["rivers"].as_array().into_iter().flatten() {
        osm.rivers.push(Line {
            kind: river["kind"].as_str().unwrap_or("stream").to_owned(),
            pts: river["pts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(lonlat)
                .collect::<Result<_, _>>()?,
        });
    }
    for a in v["areas"].as_array().into_iter().flatten() {
        let terrain = match a["kind"].as_str() {
            Some("water") => "water".to_owned(),
            Some("forest") => format!("{veg_prefix}{}", pc.forest_vegetation),
            other => return Err(format!("{PROCEDURAL_JSON}: unknown area kind {other:?}")),
        };
        let mut segments = Vec::new();
        for ring in a["rings"].as_array().into_iter().flatten() {
            let pts: Vec<LonLat> = ring
                .as_array()
                .into_iter()
                .flatten()
                .map(lonlat)
                .collect::<Result<_, _>>()?;
            segments.extend(pts.windows(2).map(|w| [w[0], w[1]]));
        }
        osm.areas.push(Area { terrain, segments });
    }
    Ok(Some(BakeData {
        dem,
        osm,
        biome: v["biome"].as_str().unwrap_or_default().to_owned(),
    }))
}

/// Roads and tracks of map.json as centre-to-centre lines for the bake.
pub fn road_lines(map: &Value, grid: &Grid, proj: &Projection, pc: &ProcgenConfig) -> Vec<Line> {
    let mut out = Vec::new();
    for (key, kind) in [
        ("tracks", &pc.roads.bake_track_kind),
        ("roads", &pc.roads.bake_road_kind),
    ] {
        for line in map[key].as_array().into_iter().flatten() {
            let pts: Vec<LonLat> = line
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|h| {
                    let (c, r) = (h[0].as_u64()?, h[1].as_u64()?);
                    Some(proj.to_lonlat(grid.center(c as u32, r as u32)))
                })
                .collect();
            if pts.len() >= 2 {
                out.push(Line {
                    kind: kind.clone(),
                    pts,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real config with a coarse hillshade (fast in debug builds).
    fn cfg() -> Config {
        let mut c = Config::from_json(include_str!("../../../data/mapgen.json")).unwrap();
        c.hillshade.px_per_km = 1.0;
        c
    }

    fn costs() -> TerrainCosts {
        TerrainCosts::from_json(include_str!("../../../data/terrain.json")).unwrap()
    }

    fn params(seed: u64, biome: &str) -> Params {
        Params::from_json(&format!(
            r#"{{
                "seed": {seed}, "cols": 16, "rows": 12, "hex_km": 10, "biome": "{biome}",
                "origin": [20.0, 50.0],
                "cities": [
                    {{ "name": "place.test.a", "hint": [3, 3], "size": "large" }},
                    {{ "name": "place.test.b", "hint": [12, 8], "size": "large" }},
                    {{ "name": "place.test.c", "size": "small" }},
                    {{ "name": "place.test.d", "size": "small" }}
                ]
            }}"#
        ))
        .unwrap()
    }

    fn rows(m: &Value) -> Vec<String> {
        m["terrain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().to_owned())
            .collect()
    }

    fn count(m: &Value, ch: char) -> usize {
        rows(m).iter().map(|r| r.matches(ch).count()).sum()
    }

    #[test]
    fn r_ui_01_procgen_bbox_fits_the_requested_grid() {
        for (cols, rows, km) in [(30, 24, 10.0), (7, 5, 5.0), (2, 2, 10.0), (61, 40, 10.0)] {
            let mut p = params(1, "temperate");
            (p.cols, p.rows, p.hex_km) = (cols, rows, km);
            assert!(bbox_for(&p, 6371.0).is_ok(), "{cols} × {rows}");
        }
    }

    #[test]
    fn r_ui_01_procgen_same_seed_same_map() {
        let (c, t) = (cfg(), costs());
        let a = generate(&params(5, "temperate"), &c, &t).unwrap();
        let b = generate(&params(5, "temperate"), &c, &t).unwrap();
        assert_eq!(a.map.to_string(), b.map.to_string());
        assert_eq!(a.procedural.to_string(), b.procedural.to_string());
        assert_eq!(a.elevation.as_raw(), b.elevation.as_raw());
        let other = generate(&params(6, "temperate"), &c, &t).unwrap();
        assert_ne!(rows(&a.map), rows(&other.map), "another seed, another land");
    }

    #[test]
    fn r_ui_01_procgen_rivers_run_downhill() {
        let g = generate(&params(5, "temperate"), &cfg(), &costs()).unwrap();
        assert!(!g.rivers.is_empty(), "some rivers");
        assert!(g.rivers.iter().any(|r| r.major), "a major river");
        for r in &g.rivers {
            for w in r.cells.windows(2) {
                assert!(
                    g.filled[w[1]] <= g.filled[w[0]],
                    "river goes uphill at {w:?}"
                );
                let (a, b) = (w[0] as isize, w[1] as isize);
                let rw = g.raster.w as isize;
                assert!(
                    (a % rw - b % rw).abs() <= 1 && (a / rw - b / rw).abs() <= 1,
                    "steps between neighbour cells"
                );
            }
        }
    }

    #[test]
    fn r_ui_01_procgen_lakes_are_water_and_cities_are_not() {
        let (c, t) = (cfg(), costs());
        for seed in [1, 2, 3, 5] {
            let g = generate(&params(seed, "temperate"), &c, &t).unwrap();
            let rs = rows(&g.map);
            let pc = c.procgen.as_ref().unwrap();
            for (row, line) in rs.iter().enumerate() {
                for (col, ch) in line.chars().enumerate() {
                    let lake = g.lake_share[row][col] >= pc.lake_min_share;
                    assert_eq!(ch == 'w', lake, "seed {seed} [{col},{row}]");
                }
            }
            let places = g.map["geo"]["places"].as_array().unwrap();
            assert_eq!(places.len(), 4);
            for p in places {
                let (col, row) = (p["hex"][0].as_u64().unwrap(), p["hex"][1].as_u64().unwrap());
                assert_eq!(rs[row as usize].as_bytes()[col as usize], b'c', "city hex");
            }
        }
    }

    /// Every city reaches every other over roads and tracks.
    #[test]
    fn r_ui_01_procgen_cities_connected_by_roads() {
        let (c, t) = (cfg(), costs());
        for seed in [1, 2, 3, 4, 5] {
            for biome in ["temperate", "steppe", "foothills"] {
                let g = generate(&params(seed, biome), &c, &t).unwrap();
                let mut adj: BTreeMap<Off, Vec<Off>> = BTreeMap::new();
                for key in ["roads", "tracks"] {
                    for s in g.map[key].as_array().unwrap() {
                        let h = |k: usize| {
                            [
                                s[k][0].as_i64().unwrap() as i32,
                                s[k][1].as_i64().unwrap() as i32,
                            ]
                        };
                        adj.entry(h(0)).or_default().push(h(1));
                        adj.entry(h(1)).or_default().push(h(0));
                    }
                }
                let cities: Vec<Off> = g.map["geo"]["places"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        [
                            p["hex"][0].as_i64().unwrap() as i32,
                            p["hex"][1].as_i64().unwrap() as i32,
                        ]
                    })
                    .collect();
                let mut seen = BTreeSet::from([cities[0]]);
                let mut stack = vec![cities[0]];
                while let Some(h) = stack.pop() {
                    for n in adj.get(&h).into_iter().flatten() {
                        if seen.insert(*n) {
                            stack.push(*n);
                        }
                    }
                }
                for city in &cities {
                    assert!(seen.contains(city), "{biome} seed {seed}: {city:?} cut off");
                }
                assert!(
                    !g.map["roads"].as_array().unwrap().is_empty(),
                    "two large cities make a highway"
                );
            }
        }
    }

    #[test]
    fn r_ui_01_procgen_temperate_has_more_forest_than_steppe() {
        let (c, t) = (cfg(), costs());
        let (mut temperate, mut steppe) = (0, 0);
        for seed in [1, 2, 3] {
            temperate += count(
                &generate(&params(seed, "temperate"), &c, &t).unwrap().map,
                't',
            );
            steppe += count(&generate(&params(seed, "steppe"), &c, &t).unwrap().map, 't');
        }
        assert!(
            temperate >= 3 * steppe.max(1),
            "forest hexes: temperate {temperate}, steppe {steppe}"
        );
    }

    /// The map loads in the core; mountain hexes with a road are passes and
    /// every road over a major river side has a bridge.
    #[test]
    fn r_ui_01_procgen_map_loads_and_roads_cross_mountains_by_passes() {
        let (c, t) = (cfg(), costs());
        for seed in [1, 2, 3, 4] {
            for biome in ["temperate", "foothills"] {
                let g = generate(&params(seed, biome), &c, &t).unwrap();
                let (map, control) =
                    ceasefire_core::scenario::load_map(&g.map.to_string()).unwrap();
                assert_eq!(map.hexes().count(), 16 * 12);
                assert!(control.is_empty(), "control is drawn in the editor");
                let canals: BTreeSet<String> = g.map["canals"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::to_string)
                    .collect();
                let bridges: BTreeSet<String> = g.map["bridges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::to_string)
                    .collect();
                for key in ["roads", "tracks"] {
                    for s in g.map[key].as_array().unwrap() {
                        for k in 0..2 {
                            let off = [
                                s[k][0].as_i64().unwrap() as i32,
                                s[k][1].as_i64().unwrap() as i32,
                            ];
                            let cell = map.cell(offset_to_axial(off)).unwrap();
                            assert!(
                                cell.terrain != "mountains" || cell.pass,
                                "{biome} seed {seed}: road through {off:?} without a pass"
                            );
                            assert_ne!(cell.terrain, "water");
                        }
                        if canals.contains(&s.to_string()) {
                            assert!(bridges.contains(&s.to_string()), "bridge on {s}");
                        }
                    }
                }
            }
        }
    }

    /// A wall of mountains with one low gap and a river along column 4: the
    /// road takes the gap or pays for a pass, and bridges the river.
    #[test]
    fn r_ui_01_procgen_roads_make_passes_and_bridges() {
        let c = cfg();
        let pc = c.procgen.as_ref().unwrap();
        let proj = Projection::new(BBox::parse("20,49.5,21.2,50").unwrap(), 6371.0);
        let grid = Grid::fit(proj.size_km(), 10.0).unwrap();
        let (cols, rows) = (grid.cols as usize, grid.rows as usize);
        assert!(cols >= 8 && rows >= 4, "{cols} × {rows}");
        let mut terrain = vec![vec!["plain".to_owned(); cols]; rows];
        for line in terrain.iter_mut() {
            line[2] = "mountains".to_owned();
        }
        let mut canals = BTreeSet::new();
        for r in 0..rows as i32 {
            for n in hex_neighbours(&grid, [4, r]) {
                if n[0] == 5 {
                    canals.insert(side([4, r], n));
                }
            }
        }
        let cities = [([0, 1], CitySize::Large), ([7, 1], CitySize::Large)];
        let (roads, tracks) = lay_roads(pc, &costs(), &grid, &terrain, &canals, &cities).unwrap();
        assert!(tracks.is_empty(), "large–large is a highway");
        let (pass, bridges) = passes_and_bridges(pc, &terrain, &roads, &tracks, &canals);
        assert_eq!(
            pass.iter().flatten().filter(|p| **p).count(),
            1,
            "one pass through the wall"
        );
        assert_eq!(bridges.len(), 1, "one bridge over the river");
        assert!(canals.contains(&bridges[0]));
    }

    #[test]
    fn r_ui_01_procgen_outline_rings_are_closed() {
        let r = Raster {
            w: 5,
            h: 4,
            cell_km: 1.0,
        };
        // An L shape and a lone cell apart from it.
        let rings = outline(r, &[0, 1, 5, 12], None);
        assert_eq!(rings.len(), 2);
        for ring in &rings {
            assert_eq!(ring.first(), ring.last());
        }
        assert_eq!(rings.iter().map(Vec::len).sum::<usize>(), 9 + 5);
    }
}
