//! `mapgen bake`: the painted map under the hexes (R-UI-01, R-UI-02), in the
//! spirit of a continuous map rather than tiles per hex. Layers, bottom-up:
//!
//! 1. terrain textures in world coordinates, blended by soft masks of the hex
//!    types (box blur ×3 ≈ Gaussian, about a quarter of a hex wide);
//! 2. relief shading (multiply, normalised so flat ground is unchanged) and a
//!    light hypsometric tint;
//! 3. forests and orchards from OSM landuse, trees scattered by a seed, none
//!    within `river_clear_hex` of a river line;
//! 4. lakes and rivers from the original OSM geometry, outlined (the rules
//!    keep rivers on hex sides), over the forest so channels stay visible;
//! 5. roads from the original OSM lines, wider for bigger classes;
//! 6. city icons on the map's city hexes.
//!
//! Deterministic: single-threaded, fixed iteration order, own PRNG; the same
//! inputs give the same pixels.

use std::collections::BTreeMap;

use ceasefire_core::rng::Pcg32;
use ceasefire_core::scenario::axial_to_offset;
use image::{imageops, RgbImage, RgbaImage};
use tiny_skia::{
    Color, FillRule, FilterQuality, LineCap, LineJoin, Paint, Path, PathBuilder, Pattern, Pixmap,
    PixmapPaint, SpreadMode, Stroke, Transform,
};

use crate::config::{BakeConfig, HillshadeConfig, TintStop};
use crate::dem::Elevation;
use crate::features::{axial_at, AreaKm};
use crate::geo::{Grid, Km, Projection};
use crate::osm::{Area, LonLat, OsmData};

const SQRT3: f64 = 1.732_050_807_568_877_2;
const FALLBACK: [f32; 3] = [0.42, 0.365, 0.27];
/// Prefix of vegetation area kinds in `OsmData::areas` read for the bake.
pub const VEG: &str = "veg:";

/// Hex terrain ids per [row][col] (pass hexes are plain `mountains`).
pub struct HexMap {
    pub terrain: Vec<Vec<String>>,
}

/// Pictures; missing ones are drawn procedurally.
pub struct Art<'a> {
    /// Terrain id → texture.
    pub terrain: &'a BTreeMap<String, RgbImage>,
    pub tree: Option<&'a RgbaImage>,
    pub city: Option<&'a RgbaImage>,
}

pub struct BakeInput<'a> {
    pub dem: &'a dyn Elevation,
    pub osm: &'a OsmData,
    pub map: &'a HexMap,
    pub grid: &'a Grid,
    pub proj: &'a Projection,
    /// Where city icons go (km from the bbox's north-west corner).
    pub cities: &'a [Km],
    /// Roads drawn (OSM lines, or the historical layer) and their styles.
    pub roads: &'a [crate::osm::Line],
    pub road_styles: &'a [crate::config::RoadStyle],
    pub art: Art<'a>,
    pub cfg: &'a BakeConfig,
    pub shade: &'a HillshadeConfig,
}

/// "#rrggbb" → linear-free 0..1 RGB.
pub fn rgb(s: &str) -> Result<[f32; 3], String> {
    let h = s
        .strip_prefix('#')
        .filter(|h| h.len() == 6)
        .ok_or_else(|| format!("bad colour `{s}`"))?;
    let v = u32::from_str_radix(h, 16).map_err(|e| format!("bad colour `{s}`: {e}"))?;
    Ok([
        ((v >> 16) & 0xff) as f32 / 255.0,
        ((v >> 8) & 0xff) as f32 / 255.0,
        (v & 0xff) as f32 / 255.0,
    ])
}

fn color(c: [f32; 3], alpha: f32) -> Color {
    Color::from_rgba(c[0], c[1], c[2], alpha.clamp(0.0, 1.0)).unwrap_or(Color::BLACK)
}

pub fn size_px(proj: &Projection, cfg: &BakeConfig) -> (u32, u32) {
    let s = proj.size_km();
    (
        (s.x * cfg.px_per_km).ceil().max(1.0) as u32,
        (s.y * cfg.px_per_km).ceil().max(1.0) as u32,
    )
}

pub fn bake(i: &BakeInput<'_>) -> Result<RgbImage, String> {
    let (w, h) = size_px(i.proj, i.cfg);
    let mut rgbf = terrain_layer(i, w, h)?;
    relief_layer(i, w, h, &mut rgbf)?;
    let mut pm = Pixmap::new(w, h).ok_or("bake: empty map")?;
    for (k, px) in pm.pixels_mut().iter_mut().enumerate() {
        let c = &rgbf[3 * k..3 * k + 3];
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        *px = tiny_skia::PremultipliedColorU8::from_rgba(b(c[0]), b(c[1]), b(c[2]), 255)
            .ok_or("bake: colour")?;
    }
    drop(rgbf);
    // R-UI-01: rivers over the forest (and the forest-typed terrain blend).
    vegetation_layer(i, &mut pm)?;
    water_layer(i, &mut pm)?;
    road_layer(i, &mut pm)?;
    city_layer(i, &mut pm)?;
    let mut out = RgbImage::new(w, h);
    for (o, p) in out.pixels_mut().zip(pm.pixels()) {
        *o = image::Rgb([p.red(), p.green(), p.blue()]);
    }
    Ok(out)
}

// ---- 1. terrain -----------------------------------------------------------

/// Offset neighbours of a hex on the grid.
fn neighbours(grid: &Grid, col: i32, row: i32) -> Vec<[i32; 2]> {
    let (q, r) = (col, row - (col - (col & 1)) / 2);
    [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)]
        .iter()
        .map(|&(dq, dr)| {
            let (nq, nr) = (q + dq, r + dr);
            [nq, nr + (nq - (nq & 1)) / 2]
        })
        .filter(|&[c, r]| c >= 0 && r >= 0 && (c as u32) < grid.cols && (r as u32) < grid.rows)
        .collect()
}

/// Water hexes take the commonest land neighbour: lakes are painted from the
/// OSM shoreline instead (layer 3).
fn land_terrain(i: &BakeInput<'_>) -> Vec<Vec<String>> {
    let t = &i.map.terrain;
    let mut out = t.clone();
    for (row, line) in t.iter().enumerate() {
        for (col, id) in line.iter().enumerate() {
            if id != "water" {
                continue;
            }
            let mut count: BTreeMap<&str, u32> = BTreeMap::new();
            for [c, r] in neighbours(i.grid, col as i32, row as i32) {
                let n = &t[r as usize][c as usize];
                if n != "water" {
                    *count.entry(n).or_default() += 1;
                }
            }
            // Most frequent; ties → smallest id (BTreeMap order, max_by keeps the last max).
            if let Some((n, _)) = count.iter().rev().max_by_key(|(_, k)| **k) {
                out[row][col] = (*n).to_owned();
            }
        }
    }
    out
}

fn hex_of_pixel(i: &BakeInput<'_>, px: u32, py: u32) -> (usize, usize) {
    let p = i.cfg.px_per_km;
    let k = Km {
        x: (f64::from(px) + 0.5) / p,
        y: (f64::from(py) + 0.5) / p,
    };
    let [c, r] = axial_to_offset(axial_at(i.grid, k));
    (
        c.clamp(0, i.grid.cols as i32 - 1) as usize,
        r.clamp(0, i.grid.rows as i32 - 1) as usize,
    )
}

/// Texture resampled so one texel is one bake pixel.
fn scaled_texture(tex: &RgbImage, cfg: &BakeConfig) -> RgbImage {
    let k = cfg.texture_km_per_px * cfg.px_per_km;
    let tw = ((f64::from(tex.width()) * k).round() as u32).max(1);
    let th = ((f64::from(tex.height()) * k).round() as u32).max(1);
    imageops::resize(tex, tw, th, imageops::FilterType::Triangle)
}

/// Three passes of a box blur, edges clamped (≈ Gaussian with σ ≈ radius).
fn blur(buf: &mut [f32], w: usize, h: usize, radius: usize) {
    if radius == 0 {
        return;
    }
    let mut tmp = vec![0.0f32; w.max(h)];
    let norm = 1.0 / (2 * radius + 1) as f32;
    for _ in 0..3 {
        for y in 0..h {
            let row = &mut buf[y * w..(y + 1) * w];
            box_line(row, &mut tmp[..w], radius, norm);
        }
        let mut col = vec![0.0f32; h];
        for x in 0..w {
            for y in 0..h {
                col[y] = buf[y * w + x];
            }
            box_line(&mut col, &mut tmp[..h], radius, norm);
            for y in 0..h {
                buf[y * w + x] = col[y];
            }
        }
    }
}

fn box_line(line: &mut [f32], tmp: &mut [f32], r: usize, norm: f32) {
    let n = line.len();
    let at = |i: isize| line[i.clamp(0, n as isize - 1) as usize];
    let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
    for (i, t) in tmp.iter_mut().enumerate().take(n) {
        *t = sum * norm;
        let i = i as isize;
        sum += at(i + r as isize + 1) - at(i - r as isize);
    }
    line.copy_from_slice(&tmp[..n]);
}

fn terrain_layer(i: &BakeInput<'_>, w: u32, h: u32) -> Result<Vec<f32>, String> {
    let (wu, hu) = (w as usize, h as usize);
    let land = land_terrain(i);
    let mut ids: Vec<&str> = land.iter().flatten().map(String::as_str).collect();
    ids.sort_unstable();
    ids.dedup();
    let index: BTreeMap<&str, u8> = ids
        .iter()
        .enumerate()
        .map(|(k, id)| (*id, k as u8))
        .collect();
    let mut kind = vec![0u8; wu * hu];
    for py in 0..h {
        for px in 0..w {
            let (c, r) = hex_of_pixel(i, px, py);
            kind[py as usize * wu + px as usize] = index[land[r][c].as_str()];
        }
    }
    let blur_px = i.cfg.blend_blur_hex * i.grid.radius_km * SQRT3 * i.cfg.px_per_km;
    let radius = (blur_px / 2.0).round() as usize;
    let mut out = vec![0.0f32; wu * hu * 3];
    let mut mask = vec![0.0f32; wu * hu];
    for (k, id) in ids.iter().enumerate() {
        for (m, t) in mask.iter_mut().zip(&kind) {
            *m = if usize::from(*t) == k { 1.0 } else { 0.0 };
        }
        blur(&mut mask, wu, hu, radius);
        let tex = i.art.terrain.get(*id).map(|t| scaled_texture(t, i.cfg));
        for y in 0..hu {
            for x in 0..wu {
                let m = mask[y * wu + x];
                if m <= 0.0 {
                    continue;
                }
                let c = match &tex {
                    Some(t) => {
                        let p = t.get_pixel(x as u32 % t.width(), y as u32 % t.height()).0;
                        [
                            p[0] as f32 / 255.0,
                            p[1] as f32 / 255.0,
                            p[2] as f32 / 255.0,
                        ]
                    }
                    None => FALLBACK,
                };
                let o = &mut out[(y * wu + x) * 3..(y * wu + x) * 3 + 3];
                for ch in 0..3 {
                    o[ch] += m * c[ch];
                }
            }
        }
    }
    Ok(out)
}

// ---- 2. relief ------------------------------------------------------------

fn tint_at(stops: &[(f64, [f32; 3], f64)], e: f64) -> ([f32; 3], f64) {
    let first = stops[0];
    if e <= first.0 {
        return (first.1, first.2);
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if e <= b.0 {
            let t = ((e - a.0) / (b.0 - a.0)) as f32;
            let c = [0, 1, 2].map(|k| a.1[k] + (b.1[k] - a.1[k]) * t);
            return (c, a.2 + (b.2 - a.2) * f64::from(t));
        }
    }
    let last = stops[stops.len() - 1];
    (last.1, last.2)
}

fn tint_stops(tint: &[TintStop]) -> Result<Vec<(f64, [f32; 3], f64)>, String> {
    let mut s = tint
        .iter()
        .map(|t| Ok((t.elevation_m, rgb(&t.color)?, t.alpha)))
        .collect::<Result<Vec<_>, String>>()?;
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(s)
}

fn relief_layer(i: &BakeInput<'_>, w: u32, h: u32, rgbf: &mut [f32]) -> Result<(), String> {
    let (wu, hu) = (w as usize, h as usize);
    let p = i.cfg.px_per_km;
    let mut elev = vec![0.0f64; wu * hu];
    for y in 0..hu {
        for x in 0..wu {
            let (lon, lat) = i.proj.to_lonlat(Km {
                x: (x as f64 + 0.5) / p,
                y: (y as f64 + 0.5) / p,
            });
            elev[y * wu + x] = i.dem.elevation(lon, lat).unwrap_or(0.0);
        }
    }
    let stops = tint_stops(&i.cfg.tint)?;
    let cell_m = 1000.0 / p;
    let zenith = (90.0 - i.shade.altitude_deg).to_radians();
    let azimuth = ((360.0 - i.shade.azimuth_deg + 90.0) % 360.0).to_radians();
    let flat = zenith.cos().max(1e-6);
    let a = i.cfg.shade_strength as f32;
    let at = |x: isize, y: isize| {
        elev[y.clamp(0, hu as isize - 1) as usize * wu + x.clamp(0, wu as isize - 1) as usize]
    };
    for y in 0..hu {
        for x in 0..wu {
            let (xi, yi) = (x as isize, y as isize);
            let dzdx = (at(xi + 1, yi) - at(xi - 1, yi)) / (2.0 * cell_m);
            let dzdy = (at(xi, yi + 1) - at(xi, yi - 1)) / (2.0 * cell_m);
            let slope = (i.shade.z_factor * dzdx.hypot(dzdy)).atan();
            let aspect = dzdy.atan2(-dzdx);
            let s =
                zenith.cos() * slope.cos() + zenith.sin() * slope.sin() * (azimuth - aspect).cos();
            let factor = (s.max(0.0) / flat).min(1.35) as f32;
            let mul = 1.0 - a + a * factor;
            let (tc, ta) = tint_at(&stops, elev[y * wu + x]);
            let ta = ta as f32;
            let o = &mut rgbf[(y * wu + x) * 3..(y * wu + x) * 3 + 3];
            for ch in 0..3 {
                let v = o[ch] * mul;
                o[ch] = v + (tc[ch] - v) * ta;
            }
        }
    }
    Ok(())
}

// ---- vector helpers ---------------------------------------------------------

fn px(i: &BakeInput<'_>, (lon, lat): LonLat) -> (f32, f32) {
    let k = i.proj.to_km(lon, lat);
    (
        (k.x * i.cfg.px_per_km) as f32,
        (k.y * i.cfg.px_per_km) as f32,
    )
}

fn key(p: LonLat) -> (u64, u64) {
    (p.0.to_bits(), p.1.to_bits())
}

/// Joins an area's segments into rings (shared OSM nodes have identical
/// coordinates); open leftovers are closed as they are.
pub fn rings(segments: &[[LonLat; 2]]) -> Vec<Vec<LonLat>> {
    let mut from: BTreeMap<(u64, u64), Vec<usize>> = BTreeMap::new();
    for (k, s) in segments.iter().enumerate() {
        from.entry(key(s[0])).or_default().push(k);
        from.entry(key(s[1])).or_default().push(k);
    }
    let mut used = vec![false; segments.len()];
    let mut out = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut ring = vec![segments[start][0], segments[start][1]];
        loop {
            let end = *ring.last().unwrap_or(&ring[0]);
            if key(end) == key(ring[0]) {
                break;
            }
            let next = from
                .get(&key(end))
                .and_then(|l| l.iter().copied().find(|&k| !used[k]));
            let Some(k) = next else { break };
            used[k] = true;
            let s = segments[k];
            ring.push(if key(s[0]) == key(end) { s[1] } else { s[0] });
        }
        out.push(ring);
    }
    out
}

/// Closed rings only: a ring left open (nodes beyond the read margin) would
/// be closed by a straight chord across the map.
fn area_path(i: &BakeInput<'_>, a: &Area) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for ring in rings(&a.segments)
        .into_iter()
        .filter(|r| r.len() > 3 && key(r[0]) == key(r[r.len() - 1]))
    {
        let mut pts = ring.iter().map(|&p| px(i, p));
        let Some((x, y)) = pts.next() else { continue };
        pb.move_to(x, y);
        for (x, y) in pts {
            pb.line_to(x, y);
        }
        pb.close();
    }
    pb.finish()
}

fn line_path(pts: impl Iterator<Item = (f32, f32)>) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut first = true;
    for (x, y) in pts {
        if first {
            pb.move_to(x, y);
            first = false;
        } else {
            pb.line_to(x, y);
        }
    }
    pb.finish()
}

fn solid(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

fn stroke(width: f32) -> Stroke {
    Stroke {
        width: width.max(0.5),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    }
}

fn to_pixmap(img: &RgbImage) -> Option<Pixmap> {
    let mut pm = Pixmap::new(img.width(), img.height())?;
    for (o, p) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
        *o = tiny_skia::PremultipliedColorU8::from_rgba(p[0], p[1], p[2], 255)?;
    }
    Some(pm)
}

fn rgba_pixmap(img: &RgbaImage) -> Option<Pixmap> {
    let mut pm = Pixmap::new(img.width(), img.height())?;
    for (o, p) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
        *o = tiny_skia::ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply();
    }
    Some(pm)
}

// ---- 4. water -------------------------------------------------------------

fn water_layer(i: &BakeInput<'_>, pm: &mut Pixmap) -> Result<(), String> {
    let style = &i.cfg.water;
    let fill_c = rgb(&style.color)?;
    let edge = color(rgb(&style.edge_color)?, 1.0);
    let edge_w = (style.edge_km * i.cfg.px_per_km) as f32;
    let tex = i
        .art
        .terrain
        .get("water")
        .map(|t| scaled_texture(t, i.cfg))
        .and_then(|t| to_pixmap(&t));
    let mut fill = solid(color(fill_c, 1.0));
    if let Some(t) = &tex {
        fill.shader = Pattern::new(
            t.as_ref(),
            SpreadMode::Repeat,
            FilterQuality::Nearest,
            1.0,
            Transform::identity(),
        );
    }
    let tone = solid(color(fill_c, 0.35));
    let lakes: Vec<&Area> = i
        .osm
        .areas
        .iter()
        .filter(|a| a.terrain == "water" && area_km2(i, a) >= style.min_lake_km2)
        .collect();
    let mut shapes: Vec<Path> = lakes.iter().filter_map(|a| area_path(i, a)).collect();
    // Water hexes with no OSM lake (painted in the editor) keep their hexagon.
    let km_areas: Vec<AreaKm> = lakes.iter().map(|a| AreaKm::new(a, i.proj)).collect();
    for (row, line) in i.map.terrain.iter().enumerate() {
        for (col, id) in line.iter().enumerate() {
            if id != "water" {
                continue;
            }
            let c = i.grid.center(col as u32, row as u32);
            if km_areas.iter().any(|a| a.contains(c)) {
                continue;
            }
            let r = i.grid.radius_km;
            let corners = (0..6).map(|k| {
                let ang = std::f64::consts::FRAC_PI_3 * f64::from(k);
                let p = i.cfg.px_per_km;
                (
                    ((c.x + r * ang.cos()) * p) as f32,
                    ((c.y + r * ang.sin()) * p) as f32,
                )
            });
            let mut pb = PathBuilder::new();
            for (k, (x, y)) in corners.enumerate() {
                if k == 0 {
                    pb.move_to(x, y);
                } else {
                    pb.line_to(x, y);
                }
            }
            pb.close();
            shapes.extend(pb.finish());
        }
    }
    for path in &shapes {
        pm.fill_path(path, &fill, FillRule::EvenOdd, Transform::identity(), None);
        pm.fill_path(path, &tone, FillRule::EvenOdd, Transform::identity(), None);
        pm.stroke_path(
            path,
            &solid(edge),
            &stroke(edge_w),
            Transform::identity(),
            None,
        );
    }
    // Rivers: dark edge, then water; the parts inside lakes are left out.
    let mut rivers: Vec<(Path, f32)> = Vec::new();
    for l in &i.osm.rivers {
        let Some(&wk) = style.river_km.get(&l.kind) else {
            continue;
        };
        let w = wk as f32 * i.cfg.px_per_km as f32;
        let mut run: Vec<LonLat> = Vec::new();
        for pair in l.pts.windows(2) {
            let mid = i
                .proj
                .to_km((pair[0].0 + pair[1].0) / 2.0, (pair[0].1 + pair[1].1) / 2.0);
            if km_areas.iter().any(|a| a.contains(mid)) {
                rivers.extend(line_path(run.drain(..).map(|p| px(i, p))).map(|p| (p, w)));
                continue;
            }
            if run.is_empty() {
                run.push(pair[0]);
            }
            run.push(pair[1]);
        }
        rivers.extend(line_path(run.into_iter().map(|p| px(i, p))).map(|p| (p, w)));
    }
    for (path, w) in &rivers {
        pm.stroke_path(
            path,
            &solid(edge),
            &stroke(w + 2.0 * edge_w),
            Transform::identity(),
            None,
        );
    }
    let water = solid(color(fill_c, 1.0));
    for (path, w) in &rivers {
        pm.stroke_path(path, &water, &stroke(*w), Transform::identity(), None);
    }
    Ok(())
}

/// Area of an OSM area in km² (shoelace over its rings, even-odd holes subtract).
fn area_km2(i: &BakeInput<'_>, a: &Area) -> f64 {
    let rings_km: Vec<f64> = rings(&a.segments)
        .iter()
        .filter(|r| r.len() > 3 && key(r[0]) == key(r[r.len() - 1]))
        .map(|r| {
            let k: Vec<Km> = r.iter().map(|&(lon, lat)| i.proj.to_km(lon, lat)).collect();
            let s: f64 = (0..k.len())
                .map(|j| {
                    let (p, q) = (k[j], k[(j + 1) % k.len()]);
                    p.x * q.y - q.x * p.y
                })
                .sum();
            (s / 2.0).abs()
        })
        .collect();
    let outer = rings_km.iter().copied().fold(0.0, f64::max);
    2.0 * outer - rings_km.iter().sum::<f64>()
}

// ---- 3. vegetation --------------------------------------------------------

/// River lines in km, bucketed on a grid of `buffer`-sized cells, to keep
/// trees off the channel (R-UI-01).
pub struct RiverClear {
    buffer: f64,
    cells: BTreeMap<(i64, i64), Vec<[Km; 2]>>,
}

impl RiverClear {
    pub fn new(rivers: &[Vec<Km>], buffer: f64) -> Self {
        let mut cells: BTreeMap<(i64, i64), Vec<[Km; 2]>> = BTreeMap::new();
        if buffer > 0.0 {
            let cell = |v: f64| (v / buffer).floor() as i64;
            for l in rivers {
                for s in l.windows(2) {
                    let (a, b) = (s[0], s[1]);
                    for cx in cell(a.x.min(b.x))..=cell(a.x.max(b.x)) {
                        for cy in cell(a.y.min(b.y))..=cell(a.y.max(b.y)) {
                            cells.entry((cx, cy)).or_default().push([a, b]);
                        }
                    }
                }
            }
        }
        Self { buffer, cells }
    }

    /// True when `c` is at least `buffer` away from every river line.
    pub fn clear(&self, c: Km) -> bool {
        if self.cells.is_empty() {
            return true;
        }
        let (cx, cy) = (
            (c.x / self.buffer).floor() as i64,
            (c.y / self.buffer).floor() as i64,
        );
        let b2 = self.buffer * self.buffer;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(segs) = self.cells.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                if segs.iter().any(|&[a, b]| dist2(c, a, b) < b2) {
                    return false;
                }
            }
        }
        true
    }
}

/// Squared distance from `p` to the segment `a`–`b`.
fn dist2(p: Km, a: Km, b: Km) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (ex, ey) = (a.x + t * dx - p.x, a.y + t * dy - p.y);
    ex * ex + ey * ey
}

/// R-UI-01: tree centres (km) and size factors over the bounds `min`–`max`
/// (km): a staggered grid with seeded jitter, inside the area and off the
/// rivers. The PRNG is drawn for every grid point, kept or not.
pub fn scatter_trees(
    rule: &crate::config::VegetationRule,
    min: Km,
    max: Km,
    inside: impl Fn(Km) -> bool,
    rivers: &RiverClear,
    rng: &mut Pcg32,
) -> Vec<(Km, f64)> {
    let step = rule.spacing_km;
    let mut out = Vec::new();
    let mut y = min.y;
    while y <= max.y {
        let mut x = min.x;
        while x <= max.x {
            let jx = (rng.next_f64() - 0.5) * rule.jitter * step;
            let jy = (rng.next_f64() - 0.5) * rule.jitter * step;
            let size = 0.8 + 0.4 * rng.next_f64();
            let c = Km {
                x: x + jx,
                y: y + jy,
            };
            if inside(c) && rivers.clear(c) {
                out.push((c, size));
            }
            x += step;
        }
        y += step * SQRT3 / 2.0;
    }
    out
}

/// Kept river lines (those `water.river_km` draws) in km.
fn river_lines_km(i: &BakeInput<'_>) -> Vec<Vec<Km>> {
    i.osm
        .rivers
        .iter()
        .filter(|l| i.cfg.water.river_km.contains_key(&l.kind))
        .map(|l| {
            l.pts
                .iter()
                .map(|&(lon, lat)| i.proj.to_km(lon, lat))
                .collect()
        })
        .collect()
}

fn vegetation_layer(i: &BakeInput<'_>, pm: &mut Pixmap) -> Result<(), String> {
    let p = i.cfg.px_per_km;
    let tree = i.art.tree.and_then(rgba_pixmap);
    let mut rng = Pcg32::new(i.cfg.seed, 7);
    let hex_km = i.grid.radius_km * SQRT3;
    let clear = RiverClear::new(&river_lines_km(i), i.cfg.river_clear_hex * hex_km);
    for rule in &i.cfg.vegetation {
        let kind = format!("{VEG}{}", rule.kind);
        let ground = solid(color(rgb(&rule.ground_color)?, rule.ground_alpha as f32));
        let crown = solid(color(rgb(&rule.crown_color)?, 1.0));
        let light = solid(color(rgb(&rule.highlight_color)?, 0.7));
        let shadow = solid(color([0.0, 0.0, 0.0], 0.3));
        for a in i.osm.areas.iter().filter(|a| a.terrain == kind) {
            let Some(path) = area_path(i, a) else {
                continue;
            };
            pm.fill_path(
                &path,
                &ground,
                FillRule::EvenOdd,
                Transform::identity(),
                None,
            );
            let ak = AreaKm::new(a, i.proj);
            let b = path.bounds();
            let min = Km {
                x: f64::from(b.left()) / p,
                y: f64::from(b.top()) / p,
            };
            let max = Km {
                x: f64::from(b.right()) / p,
                y: f64::from(b.bottom()) / p,
            };
            for (c, size) in scatter_trees(rule, min, max, |c| ak.contains(c), &clear, &mut rng) {
                let (cx, cy) = ((c.x * p) as f32, (c.y * p) as f32);
                let r = (rule.crown_km * p * size) as f32;
                match &tree {
                    Some(t) => {
                        let s = 2.0 * r / t.width().max(1) as f32;
                        let tr = Transform::from_row(s, 0.0, 0.0, s, cx - r, cy - r);
                        pm.draw_pixmap(0, 0, t.as_ref(), &PixmapPaint::default(), tr, None);
                    }
                    None => {
                        for (dx, dy, rr, paint) in [
                            (0.35, 0.35, 1.0, &shadow),
                            (0.0, 0.0, 1.0, &crown),
                            (-0.3, -0.3, 0.5, &light),
                        ] {
                            if let Some(c) =
                                PathBuilder::from_circle(cx + dx * r, cy + dy * r, rr * r)
                            {
                                pm.fill_path(
                                    &c,
                                    paint,
                                    FillRule::Winding,
                                    Transform::identity(),
                                    None,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// ---- 5. roads -------------------------------------------------------------

fn road_layer(i: &BakeInput<'_>, pm: &mut Pixmap) -> Result<(), String> {
    let p = i.cfg.px_per_km as f32;
    let edge = solid(color(rgb(&i.cfg.road_edge_color)?, 1.0));
    let fill = solid(color(rgb(&i.cfg.road_color)?, 1.0));
    let edge_w = i.cfg.road_edge_km as f32 * p;
    let mut lines: Vec<(usize, Path, f32, &crate::config::RoadStyle)> = Vec::new();
    for l in i.roads {
        let Some((rank, style)) = i
            .road_styles
            .iter()
            .enumerate()
            .find(|(_, s)| s.kind == l.kind)
        else {
            continue;
        };
        if let Some(path) = line_path(l.pts.iter().map(|&q| px(i, q))) {
            lines.push((rank, path, style.width_km as f32 * p, style));
        }
    }
    // Stable: thin classes first so big roads stay on top at junctions.
    lines.sort_by_key(|(rank, ..)| *rank);
    for (_, path, w, style) in &lines {
        // Dirt roads: no dark edge, a dashed line of their own colour.
        if style.dash_km.is_none() {
            pm.stroke_path(
                path,
                &edge,
                &stroke(w + 2.0 * edge_w),
                Transform::identity(),
                None,
            );
        }
    }
    for (_, path, w, style) in &lines {
        let paint = match &style.color {
            Some(c) => solid(color(rgb(c)?, 1.0)),
            None => fill.clone(),
        };
        let mut st = stroke(*w);
        if let Some(d) = style.dash_km {
            let d = (d * f64::from(p)) as f32;
            st.dash = tiny_skia::StrokeDash::new(vec![d, d * 0.6], 0.0);
            st.line_cap = LineCap::Butt;
        }
        pm.stroke_path(path, &paint, &st, Transform::identity(), None);
    }
    Ok(())
}

// ---- 6. cities ------------------------------------------------------------

fn city_layer(i: &BakeInput<'_>, pm: &mut Pixmap) -> Result<(), String> {
    let p = i.cfg.px_per_km;
    let size = (i.cfg.city.size_km * p) as f32;
    let icon = i.art.city.and_then(rgba_pixmap);
    let roof = solid(color(rgb(&i.cfg.city.roof_color)?, 1.0));
    let edge = solid(color(rgb(&i.cfg.city.edge_color)?, 1.0));
    let shadow = solid(color([0.0, 0.0, 0.0], 0.35));
    // A cluster of four blocks around the centre.
    let blocks = [
        (-0.42, -0.18, 0.36),
        (0.06, -0.44, 0.34),
        (0.1, 0.02, 0.4),
        (-0.36, 0.24, 0.3),
    ];
    for c in i.cities {
        let (cx, cy) = ((c.x * p) as f32, (c.y * p) as f32);
        if let Some(t) = &icon {
            let s = size / t.width().max(1) as f32;
            let tr = Transform::from_row(s, 0.0, 0.0, s, cx - size / 2.0, cy - size / 2.0);
            pm.draw_pixmap(0, 0, t.as_ref(), &PixmapPaint::default(), tr, None);
            continue;
        }
        for (dx, dy, k) in blocks {
            let (x, y, side) = (cx + dx * size, cy + dy * size, k * size);
            let off = size * 0.06;
            if let Some(r) = tiny_skia::Rect::from_xywh(x + off, y + off, side, side) {
                pm.fill_rect(r, &shadow, Transform::identity(), None);
            }
            if let Some(r) = tiny_skia::Rect::from_xywh(x, y, side, side) {
                let path = PathBuilder::from_rect(r);
                pm.fill_path(&path, &roof, FillRule::Winding, Transform::identity(), None);
                pm.stroke_path(
                    &path,
                    &edge,
                    &stroke((size * 0.05).max(1.0)),
                    Transform::identity(),
                    None,
                );
            }
        }
    }
    Ok(())
}

// ---- tiles ----------------------------------------------------------------

/// Halves an image, 2 × 2 averages rounded half up; odd edges repeat.
pub fn half(img: &RgbImage) -> RgbImage {
    let (w, h) = (img.width().div_ceil(2), img.height().div_ceil(2));
    RgbImage::from_fn(w, h, |x, y| {
        let (x0, y0) = (2 * x, 2 * y);
        let (x1, y1) = (
            (x0 + 1).min(img.width() - 1),
            (y0 + 1).min(img.height() - 1),
        );
        let ps = [
            img.get_pixel(x0, y0),
            img.get_pixel(x1, y0),
            img.get_pixel(x0, y1),
            img.get_pixel(x1, y1),
        ];
        image::Rgb(
            [0, 1, 2].map(|c| ((ps.iter().map(|p| u32::from(p[c])).sum::<u32>() + 2) / 4) as u8),
        )
    })
}

/// Zoom levels: 0 is full size, each next one half, down to one tile.
pub fn pyramid(img: RgbImage, tile: u32) -> Vec<RgbImage> {
    let mut levels = vec![img];
    while let Some(last) = levels.last() {
        if last.width() <= tile && last.height() <= tile {
            break;
        }
        let next = half(last);
        levels.push(next);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_join_shuffled_segments() {
        let (a, b, c, d) = ((0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0));
        let r = rings(&[[c, d], [a, b], [d, a], [b, c]]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].len(), 5);
        assert_eq!(r[0].first(), r[0].last());
    }

    /// R-UI-01: no tree within the buffer of a river line, trees elsewhere;
    /// the seed's draws do not depend on the river.
    #[test]
    fn r_ui_01_bake_no_trees_within_the_river_buffer() {
        let cfg =
            crate::config::Config::from_json(include_str!("../../../data/mapgen.json")).unwrap();
        let rule = &cfg.bake.vegetation[0];
        let river = vec![
            Km { x: 5.0, y: 0.0 },
            Km { x: 5.0, y: 4.0 },
            Km { x: 7.0, y: 10.0 },
        ];
        let buffer = 1.25;
        let clear = RiverClear::new(std::slice::from_ref(&river), buffer);
        let (min, max) = (Km { x: 0.0, y: 0.0 }, Km { x: 10.0, y: 10.0 });
        let trees = scatter_trees(rule, min, max, |_| true, &clear, &mut Pcg32::new(1, 7));
        let all = scatter_trees(
            rule,
            min,
            max,
            |_| true,
            &RiverClear::new(&[], buffer),
            &mut Pcg32::new(1, 7),
        );
        assert!(trees.len() < all.len() && trees.len() > all.len() / 2);
        for (c, _) in &trees {
            let d = river
                .windows(2)
                .map(|s| dist2(*c, s[0], s[1]))
                .fold(f64::INFINITY, f64::min)
                .sqrt();
            assert!(d >= buffer, "tree at {c:?} is {d} km from the river");
        }
        // Same grid points, same jitter: the kept trees are a subset.
        assert!(trees.iter().all(|t| all.contains(t)));
    }

    #[test]
    fn blur_keeps_a_partition_of_unity() {
        let (w, h) = (9, 5);
        let mut a: Vec<f32> = (0..w * h)
            .map(|k| if k % w < 4 { 1.0 } else { 0.0 })
            .collect();
        let mut b: Vec<f32> = a.iter().map(|v| 1.0 - v).collect();
        blur(&mut a, w, h, 2);
        blur(&mut b, w, h, 2);
        for (x, y) in a.iter().zip(&b) {
            assert!((x + y - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn pyramid_halves_down_to_one_tile() {
        let img = RgbImage::from_pixel(1000, 300, image::Rgb([10, 20, 30]));
        let levels = pyramid(img, 256);
        let sizes: Vec<(u32, u32)> = levels.iter().map(|l| (l.width(), l.height())).collect();
        assert_eq!(sizes, vec![(1000, 300), (500, 150), (250, 75)]);
        assert_eq!(levels[2].get_pixel(0, 0).0, [10, 20, 30]);
    }
}
