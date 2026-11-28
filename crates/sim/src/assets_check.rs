//! `sim assets-check`: completeness and format of generated art (GDD R-UI-06)
//! and the `data/sprites.json` binding. Reports; fixes only with `--fix`, and
//! only by downscaling a larger image of the right proportions.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use image::{imageops::FilterType, ColorType, DynamicImage, GenericImageView};
use serde::Deserialize;

/// Terrain textures (R-UI-06).
const TERRAIN_SIZE: u32 = 1024;
/// Unit medallions (R-UI-06).
const UNIT_SIZE: u32 = 512;
/// Seamlessness: (mean RGB difference between opposite edges) / (mean RGB
/// difference between neighbouring pixels inside the texture). A tile shows a
/// seam above this ratio.
const SEAM_MAX_RATIO: f64 = 1.5;
/// Outer border (px) of a unit image that must be fully transparent.
const UNIT_EDGE_MARGIN_PX: u32 = 1;
/// Map objects (bridges): same format as unit medallions, top-down.
const OBJECT_SIZE: u32 = 512;
/// Packs v1 + v2.
const EXPECTED_TERRAIN: usize = 12;
const EXPECTED_UNITS: usize = 37;
const EXPECTED_OBJECTS: usize = 2;
/// R-UI-07 illustrations (16:9): briefings and event windows.
const BRIEFING_SIZE: (u32, u32) = (1600, 900);
const EVENT_SIZE: (u32, u32) = (1200, 675);
const SPRITE_SIDES: [&str; 3] = ["isr", "egy_syr", "jor"];
/// Miniatures pack (style `miniature`): `assets/units_minis/<side>/<id>.png`.
const MINIS_DIR: &str = "units_minis";
const EXPECTED_MINIS: usize = 50;
/// The model's lowest opaque row must sit at the manifest anchor ± this share.
const ANCHOR_TOLERANCE: f64 = 0.06;
/// Alpha from which a pixel counts as part of the model (soft shadows are lighter).
const MODEL_ALPHA: u8 = 128;

pub struct Options {
    assets: PathBuf,
    data: PathBuf,
    report: PathBuf,
    fix: bool,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        assets: PathBuf::from("assets"),
        data: PathBuf::from("data"),
        report: PathBuf::from("docs/assets-report.md"),
        fix: false,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if flag == "--fix" {
            o.fix = true;
            continue;
        }
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--assets" => o.assets = PathBuf::from(value),
            "--data" => o.data = PathBuf::from(value),
            "--report" => o.report = PathBuf::from(value),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    id: String,
    group: String,
    side: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SpriteTier {
    pub tech_min: u8,
    pub sprite: String,
    #[serde(default)]
    pub variants: Vec<String>,
    pub year_min: Option<u16>,
    pub year_max: Option<u16>,
    pub dir: Option<String>,
    #[serde(default)]
    pub model_only: bool,
    #[serde(default)]
    pub nations: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SpriteClass {
    pub tech_branch: Option<String>,
    pub isr: Vec<SpriteTier>,
    pub egy_syr: Vec<SpriteTier>,
    #[serde(default)]
    pub jor: Vec<SpriteTier>,
}

/// A unit picture style: folder under `assets/` and the anchor (contact point).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleDef {
    pub dir: String,
    pub anchor: [f64; 2],
    /// Per-picture anchors; pictures missing here fall back to the medallion.
    #[serde(default)]
    pub manifest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MiniEntry {
    id: String,
    side: String,
    style: String,
    anchor: [f64; 2],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpritesFile {
    pub sprite_sides: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub styles: BTreeMap<String, StyleDef>,
    #[serde(default)]
    pub default_style: Option<String>,
    /// Terrain id from data/terrain.json → texture id in assets/terrain.
    pub terrain: BTreeMap<String, String>,
    /// Textures kept for later terrain types or variants.
    #[serde(default)]
    pub unused_terrain_textures: Vec<String>,
    pub units: BTreeMap<String, SpriteClass>,
    pub helicopter_transfer: BTreeMap<String, String>,
    /// Map object role → id in assets/objects.
    pub map_objects: BTreeMap<String, String>,
    /// R-UI-07: unit picture id → localisation key of the model's reference note.
    #[serde(default)]
    pub model_info: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct UnitsFile {
    units: Vec<UnitId>,
}

#[derive(Debug, Deserialize)]
struct TerrainFile {
    terrain: Vec<UnitId>,
}

#[derive(Debug, Deserialize)]
struct UnitId {
    id: String,
}

/// Every sprite id referenced in `sprites.json`, with the side folder it lives in.
pub fn referenced_sprites(s: &SpritesFile) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for class in s.units.values() {
        for (side, tiers) in [
            ("isr", &class.isr),
            ("egy_syr", &class.egy_syr),
            ("jor", &class.jor),
        ] {
            for t in tiers {
                out.insert((side.to_owned(), t.sprite.clone()));
                for v in &t.variants {
                    out.insert((side.to_owned(), v.clone()));
                }
            }
        }
    }
    for (side, id) in &s.helicopter_transfer {
        out.insert((side.clone(), id.clone()));
    }
    out
}

/// Problems with `sprites.json` against `units.json` and the asset folder.
pub fn check_sprites(data: &Path, assets: &Path) -> Result<Vec<String>, String> {
    let read = |p: PathBuf| fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()));
    let sprites: SpritesFile = serde_json::from_str(&read(data.join("sprites.json"))?)
        .map_err(|e| format!("sprites.json: {e}"))?;
    let units: UnitsFile = serde_json::from_str(&read(data.join("units.json"))?)
        .map_err(|e| format!("units.json: {e}"))?;
    let mut problems = Vec::new();
    for u in &units.units {
        if !sprites.units.contains_key(&u.id) {
            problems.push(format!("unit class `{}` missing in sprites.json", u.id));
        }
    }
    for id in sprites.units.keys() {
        if !units.units.iter().any(|u| &u.id == id) {
            problems.push(format!("sprites.json class `{id}` is not in units.json"));
        }
    }
    for side in SPRITE_SIDES {
        if !sprites.sprite_sides.contains_key(side) {
            problems.push(format!("sprite side `{side}` not declared"));
        }
    }
    for (class, c) in &sprites.units {
        for (side, tiers) in [("isr", &c.isr), ("egy_syr", &c.egy_syr), ("jor", &c.jor)] {
            if c.tech_branch.is_none() && tiers.iter().any(|t| t.tech_min != 1) {
                problems.push(format!(
                    "`{class}`/{side}: technology tiers need a tech_branch"
                ));
            }
            for t in tiers {
                for nation in &t.nations {
                    let declared = sprites
                        .sprite_sides
                        .get(side)
                        .and_then(|v| v["nations"].as_array())
                        .is_some_and(|ns| ns.iter().any(|n| n.as_str() == Some(nation)));
                    if !declared {
                        problems.push(format!(
                            "`{class}`/{side}: unknown sprite nation `{nation}`"
                        ));
                    }
                }
                if t.tech_min == 0 || t.year_min.zip(t.year_max).is_some_and(|(a, b)| a > b) {
                    problems.push(format!(
                        "`{class}`/{side}: invalid technology or year range"
                    ));
                }
                if t.dir
                    .as_deref()
                    .is_some_and(|d| d != "units" && d != MINIS_DIR)
                {
                    problems.push(format!("`{class}`/{side}: unsupported sprite directory"));
                }
            }
            let defaults: Vec<_> = tiers.iter().filter(|t| !t.model_only).collect();
            for (i, a) in defaults.iter().enumerate() {
                for b in &defaults[i + 1..] {
                    let overlap = a.year_min.unwrap_or(0) <= b.year_max.unwrap_or(u16::MAX)
                        && b.year_min.unwrap_or(0) <= a.year_max.unwrap_or(u16::MAX);
                    if a.tech_min == b.tech_min && overlap {
                        problems.push(format!(
                            "`{class}`/{side}: overlapping year ranges at the same technology"
                        ));
                    }
                }
            }
        }
    }
    let terrain: TerrainFile = serde_json::from_str(&read(data.join("terrain.json"))?)
        .map_err(|e| format!("terrain.json: {e}"))?;
    for t in &terrain.terrain {
        match sprites.terrain.get(&t.id) {
            None => problems.push(format!("terrain `{}` has no texture in sprites.json", t.id)),
            Some(tex) if !assets.join("terrain").join(format!("{tex}.png")).is_file() => {
                problems.push(format!(
                    "terrain `{}`: texture file `{tex}.png` missing",
                    t.id
                ));
            }
            Some(_) => {}
        }
    }
    for id in sprites.terrain.keys() {
        if !terrain.terrain.iter().any(|t| &t.id == id) {
            problems.push(format!(
                "sprites.json terrain `{id}` is not in terrain.json"
            ));
        }
    }
    for tex in &sprites.unused_terrain_textures {
        if !assets.join("terrain").join(format!("{tex}.png")).is_file() {
            problems.push(format!("unused terrain texture `{tex}.png` missing"));
        }
    }
    for (role, id) in &sprites.map_objects {
        if !assets.join("objects").join(format!("{id}.png")).is_file() {
            problems.push(format!(
                "map object `{role}`: file `objects/{id}.png` missing"
            ));
        }
    }
    for c in sprites.units.values() {
        for (side, tiers) in [("isr", &c.isr), ("egy_syr", &c.egy_syr), ("jor", &c.jor)] {
            for t in tiers {
                for id in std::iter::once(&t.sprite).chain(&t.variants) {
                    let path = assets
                        .join(t.dir.as_deref().unwrap_or("units"))
                        .join(side)
                        .join(format!("{id}.png"));
                    check_dimensions(&path, (UNIT_SIZE, UNIT_SIZE), &mut problems);
                }
            }
        }
    }
    for (side, id) in &sprites.helicopter_transfer {
        check_dimensions(
            &assets.join("units").join(side).join(format!("{id}.png")),
            (UNIT_SIZE, UNIT_SIZE),
            &mut problems,
        );
    }
    // Check every manifest entry, including new illustrations and unselected miniatures.
    for manifest in [
        assets.join("manifest.json"),
        assets.join(MINIS_DIR).join("manifest.json"),
    ] {
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&read(manifest.clone())?).map_err(|e| e.to_string())?;
        let base = manifest.parent().ok_or("manifest has no parent")?;
        for e in entries {
            if let (Some(file), Some(w), Some(h)) = (
                e["file"].as_str(),
                e["width"].as_u64(),
                e["height"].as_u64(),
            ) {
                check_dimensions(&base.join(file), (w as u32, h as u32), &mut problems);
            } else {
                problems.push(format!(
                    "{}: entry missing file or dimensions",
                    manifest.display()
                ));
            }
        }
    }
    Ok(problems)
}

fn check_dimensions(path: &Path, expected: (u32, u32), problems: &mut Vec<String>) {
    match image::image_dimensions(path) {
        Ok(actual) if actual == expected => {}
        Ok(actual) => problems.push(format!(
            "{}: dimensions {actual:?}, expected {expected:?}",
            path.display()
        )),
        Err(e) => problems.push(format!("{}: {e}", path.display())),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Status {
    Ok,
    Missing,
    WrongSize(u32, u32),
    NotTransparent(String),
    TouchesEdge,
    Seam(f64, f64),
    Unreadable(String),
    /// Miniature: the model's bottom is not at the manifest anchor.
    AnchorOff(f64, f64),
}

impl Status {
    fn label(&self) -> String {
        match self {
            Status::Ok => "ок".into(),
            Status::Missing => "нет".into(),
            Status::WrongSize(w, h) => format!("не тот размер ({w}×{h})"),
            Status::NotTransparent(why) => format!("не прозрачный ({why})"),
            Status::TouchesEdge => "касается края".into(),
            Status::Seam(h, v) => format!("шов (отношение л–п {h:.2}, в–н {v:.2})"),
            Status::Unreadable(e) => format!("не читается ({e})"),
            Status::AnchorOff(bottom, anchor) => {
                format!("низ модели {bottom:.2}, а якорь {anchor:.2}")
            }
        }
    }
}

struct Row {
    id: String,
    path: String,
    statuses: Vec<Status>,
    notes: Vec<String>,
}

/// Mean absolute RGB difference between two pixel lines.
fn line_diff(
    img: &image::RgbImage,
    a: impl Iterator<Item = (u32, u32)>,
    b: impl Iterator<Item = (u32, u32)>,
) -> f64 {
    let (mut sum, mut n) = (0u64, 0u64);
    for ((ax, ay), (bx, by)) in a.zip(b) {
        let (p, q) = (img.get_pixel(ax, ay), img.get_pixel(bx, by));
        for c in 0..3 {
            sum += u64::from(p[c].abs_diff(q[c]));
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        sum as f64 / n as f64
    }
}

/// Seam ratios: opposite-edge difference over the mean difference of all
/// neighbouring columns (left–right) and rows (top–bottom) inside.
/// Returns (left–right edge, top–bottom edge, inner columns, inner rows).
fn seam_metrics(img: &DynamicImage) -> [f64; 4] {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let lr = line_diff(&rgb, (0..h).map(|y| (0, y)), (0..h).map(|y| (w - 1, y)));
    let tb = line_diff(&rgb, (0..w).map(|x| (x, 0)), (0..w).map(|x| (x, h - 1)));
    let cols = (0..w - 1)
        .map(|x| line_diff(&rgb, (0..h).map(|y| (x, y)), (0..h).map(|y| (x + 1, y))))
        .sum::<f64>()
        / f64::from(w - 1);
    let rows = (0..h - 1)
        .map(|y| line_diff(&rgb, (0..w).map(|x| (x, y)), (0..w).map(|x| (x, y + 1))))
        .sum::<f64>()
        / f64::from(h - 1);
    [lr, tb, cols, rows]
}

fn ratio(edge: f64, inner: f64) -> f64 {
    if inner > 0.0 {
        edge / inner
    } else if edge > 0.0 {
        f64::INFINITY
    } else {
        1.0
    }
}

fn unit_checks(img: &DynamicImage) -> Vec<Status> {
    let mut out = Vec::new();
    if img.color() != ColorType::Rgba8 {
        out.push(Status::NotTransparent(format!(
            "{:?}, нужен RGBA8",
            img.color()
        )));
        return out;
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
    if corners.iter().any(|&(x, y)| rgba.get_pixel(x, y)[3] != 0) {
        out.push(Status::NotTransparent("углы".into()));
    }
    let m = UNIT_EDGE_MARGIN_PX.min(w / 2).min(h / 2);
    let touches = (0..w).any(|x| {
        (0..m).any(|d| rgba.get_pixel(x, d)[3] != 0 || rgba.get_pixel(x, h - 1 - d)[3] != 0)
    }) || (0..h).any(|y| {
        (0..m).any(|d| rgba.get_pixel(d, y)[3] != 0 || rgba.get_pixel(w - 1 - d, y)[3] != 0)
    });
    if touches {
        out.push(Status::TouchesEdge);
    }
    out
}

/// R-UI-07: an illustration is a readable WebP of the exact size (PNG only for transparent tokens and textures).
fn check_illustration(path: &Path, (w, h): (u32, u32)) -> Vec<Status> {
    if !path.is_file() {
        return vec![Status::Missing];
    }
    match image::image_dimensions(path) {
        Ok(d) if d == (w, h) => vec![Status::Ok],
        Ok((a, b)) => vec![Status::WrongSize(a, b)],
        Err(e) => vec![Status::Unreadable(e.to_string())],
    }
}

fn check_file(
    path: &Path,
    size: u32,
    terrain: bool,
    fix: bool,
    notes: &mut Vec<String>,
) -> Vec<Status> {
    if !path.is_file() {
        return vec![Status::Missing];
    }
    let mut img = match image::open(path) {
        Ok(i) => i,
        Err(e) => return vec![Status::Unreadable(e.to_string())],
    };
    let (w, h) = img.dimensions();
    if (w, h) != (size, size) {
        let larger_same_ratio = w == h && w > size;
        if larger_same_ratio && fix {
            img = img.resize_exact(size, size, FilterType::Lanczos3);
            if let Err(e) = img.save(path) {
                return vec![Status::Unreadable(format!("save after resize: {e}"))];
            }
            notes.push(format!("ресайз {w}×{h} → {size}×{size}"));
        } else {
            if larger_same_ratio {
                notes.push("можно уменьшить: `--fix`".into());
            }
            return vec![Status::WrongSize(w, h)];
        }
    }
    let mut statuses = Vec::new();
    if terrain {
        let [lr, tb, cols, rows] = seam_metrics(&img);
        let (rh, rv) = (ratio(lr, cols), ratio(tb, rows));
        notes.push(format!(
            "края л–п {lr:.1} / внутри {cols:.1} = {rh:.2}; в–н {tb:.1} / внутри {rows:.1} = {rv:.2}"
        ));
        if rh > SEAM_MAX_RATIO || rv > SEAM_MAX_RATIO {
            statuses.push(Status::Seam(rh, rv));
        }
    } else {
        statuses.extend(unit_checks(&img));
    }
    if statuses.is_empty() {
        statuses.push(Status::Ok);
    }
    statuses
}

fn png_bytes(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                png_bytes(&p)
            } else if p.extension().is_some_and(|x| x == "png") {
                e.metadata().map_or(0, |m| m.len())
            } else {
                0
            }
        })
        .sum()
}

/// Lowest opaque row (share of the height, bottom edge) and the horizontal
/// centre of the opaque box (share of the width).
fn model_contact(img: &DynamicImage) -> Option<(f64, f64)> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let (mut x0, mut x1, mut y1) = (w, 0, None);
    for (x, y, p) in rgba.enumerate_pixels() {
        if p[3] >= MODEL_ALPHA {
            x0 = x0.min(x);
            x1 = x1.max(x);
            y1 = Some(y1.map_or(y, |v: u32| v.max(y)));
        }
    }
    let y1 = y1?;
    Some((
        f64::from(y1 + 1) / f64::from(h),
        f64::from(x0 + x1 + 1) / 2.0 / f64::from(w),
    ))
}

/// The miniatures pack: format, transparent border, anchor against the model;
/// plus the sprites that will fall back to the medallion.
/// Rows, problems, and sprites that fall back to the medallion.
type MinisCheck = (Vec<Row>, Vec<String>, Vec<String>);

fn check_minis(o: &Options, sprites: &SpritesFile) -> Result<MinisCheck, String> {
    let path = o.assets.join(MINIS_DIR).join("manifest.json");
    let manifest: Vec<MiniEntry> = serde_json::from_str(
        &fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| format!("{MINIS_DIR}/manifest.json: {e}"))?;
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    for e in &manifest {
        let rel = format!("{MINIS_DIR}/{}/{}.png", e.side, e.id);
        let mut notes = Vec::new();
        let mut statuses = check_file(&o.assets.join(&rel), UNIT_SIZE, false, o.fix, &mut notes);
        if !SPRITE_SIDES.contains(&e.side.as_str()) || e.style != "miniature" {
            statuses.push(Status::Unreadable(format!(
                "сторона `{}`, стиль `{}`",
                e.side, e.style
            )));
        }
        if let Ok(img) = image::open(o.assets.join(&rel)) {
            if let Some((bottom, cx)) = model_contact(&img) {
                notes.push(format!(
                    "низ {bottom:.2} (якорь {:.2}), центр {cx:.2}",
                    e.anchor[1]
                ));
                if (bottom - e.anchor[1]).abs() > ANCHOR_TOLERANCE {
                    statuses.retain(|s| *s != Status::Ok);
                    statuses.push(Status::AnchorOff(bottom, e.anchor[1]));
                }
            }
        }
        rows.push(Row {
            id: e.id.clone(),
            path: rel,
            statuses,
            notes,
        });
    }
    if manifest.len() != EXPECTED_MINIS {
        problems.push(format!(
            "{MINIS_DIR}/manifest.json: {} entries, expected {EXPECTED_MINIS}",
            manifest.len()
        ));
    }
    for (name, style) in &sprites.styles {
        if !o.assets.join(&style.dir).is_dir() {
            problems.push(format!(
                "style `{name}`: folder assets/{} missing",
                style.dir
            ));
        }
        if style.anchor.iter().any(|v| !(0.0..=1.0).contains(v)) {
            problems.push(format!("style `{name}`: anchor must be within 0..1"));
        }
        if let Some(m) = &style.manifest {
            if !o.assets.join(m).is_file() {
                problems.push(format!("style `{name}`: manifest assets/{m} missing"));
            }
        }
    }
    if let Some(d) = &sprites.default_style {
        if !sprites.styles.contains_key(d) {
            problems.push(format!("default_style `{d}` is not in styles"));
        }
    }
    let have: BTreeSet<(String, String)> = manifest
        .iter()
        .map(|e| (e.side.clone(), e.id.clone()))
        .collect();
    let fallback = referenced_sprites(sprites)
        .into_iter()
        .filter(|k| !have.contains(k))
        .map(|(side, id)| format!("{side}/{id}"))
        .collect();
    Ok((rows, problems, fallback))
}

pub fn run(o: &Options) -> Result<(), String> {
    let manifest_path = o.assets.join("manifest.json");
    let manifest: Vec<ManifestEntry> = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?,
    )
    .map_err(|e| format!("manifest.json: {e}"))?;

    let mut rows = Vec::new();
    let (mut terrain_n, mut unit_n, mut object_n) = (0, 0, 0);
    for e in &manifest {
        let (rel, size, terrain) = match (e.group.as_str(), e.side.as_deref()) {
            ("terrain", _) => {
                terrain_n += 1;
                (format!("terrain/{}.png", e.id), TERRAIN_SIZE, true)
            }
            ("unit", Some(side)) if SPRITE_SIDES.contains(&side) => {
                unit_n += 1;
                (format!("units/{side}/{}.png", e.id), UNIT_SIZE, false)
            }
            ("object", None) => {
                object_n += 1;
                (format!("objects/{}.png", e.id), OBJECT_SIZE, false)
            }
            (group @ ("briefing" | "event"), None) => {
                let (dir, size) = if group == "briefing" {
                    ("briefing", BRIEFING_SIZE)
                } else {
                    ("events", EVENT_SIZE)
                };
                let rel = format!("{dir}/{}.webp", e.id);
                let statuses = check_illustration(&o.assets.join(&rel), size);
                rows.push(Row {
                    id: e.id.clone(),
                    path: rel,
                    statuses,
                    notes: vec![],
                });
                continue;
            }
            _ => {
                rows.push(Row {
                    id: e.id.clone(),
                    path: "—".into(),
                    statuses: vec![Status::Unreadable("неизвестная группа или сторона".into())],
                    notes: vec![],
                });
                continue;
            }
        };
        let mut notes = Vec::new();
        let statuses = check_file(&o.assets.join(&rel), size, terrain, o.fix, &mut notes);
        rows.push(Row {
            id: e.id.clone(),
            path: rel,
            statuses,
            notes,
        });
    }

    let mut problems = Vec::new();
    if terrain_n != EXPECTED_TERRAIN {
        problems.push(format!(
            "manifest: {terrain_n} terrain entries, expected {EXPECTED_TERRAIN}"
        ));
    }
    if unit_n != EXPECTED_UNITS {
        problems.push(format!(
            "manifest: {unit_n} unit entries, expected {EXPECTED_UNITS}"
        ));
    }
    if object_n != EXPECTED_OBJECTS {
        problems.push(format!(
            "manifest: {object_n} object entries, expected {EXPECTED_OBJECTS}"
        ));
    }
    problems.extend(check_sprites(&o.data, &o.assets)?);
    let sprites_path = o.data.join("sprites.json");
    let sprites: SpritesFile = serde_json::from_str(
        &fs::read_to_string(&sprites_path)
            .map_err(|e| format!("{}: {e}", sprites_path.display()))?,
    )
    .map_err(|e| format!("sprites.json: {e}"))?;
    // R-UI-07: reference notes only for unit pictures the manifest has.
    for id in sprites.model_info.keys() {
        if !manifest.iter().any(|e| e.group == "unit" && &e.id == id) {
            problems.push(format!(
                "sprites.json model_info: {id} is not a unit picture"
            ));
        }
    }
    let (minis, mini_problems, fallback) = check_minis(o, &sprites)?;
    problems.extend(mini_problems);

    let total = png_bytes(&o.assets);
    let game = png_bytes(&o.assets.join("terrain"))
        + png_bytes(&o.assets.join("units"))
        + png_bytes(&o.assets.join(MINIS_DIR))
        + png_bytes(&o.assets.join("objects"));
    let bad = rows
        .iter()
        .chain(&minis)
        .filter(|r| r.statuses != [Status::Ok])
        .count();
    let mut report = render(&rows, &problems, total, game);
    report.push_str(&render_minis(&minis, &fallback));
    if let Some(dir) = o.report.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&o.report, report).map_err(|e| format!("{}: {e}", o.report.display()))?;
    eprintln!(
        "{} files, {bad} with problems, {} other problems; report: {}",
        rows.len() + minis.len(),
        problems.len(),
        o.report.display()
    );
    if bad > 0 || !problems.is_empty() {
        return Err("asset check failed".into());
    }
    Ok(())
}

fn mb(bytes: u64) -> String {
    format!("{:.1} МБ", bytes as f64 / (1024.0 * 1024.0))
}

fn render_minis(rows: &[Row], fallback: &[String]) -> String {
    let mut s = String::new();
    let ok = rows.iter().filter(|r| r.statuses == [Status::Ok]).count();
    let _ = writeln!(s, "\n## Миниатюры (стиль `miniature`)\n");
    let _ = writeln!(
        s,
        "Папка `assets/{MINIS_DIR}/`, файлов по манифесту: {}, в порядке: {ok}. Проверки как у медальонов (512 × 512, RGBA, прозрачная рамка) и якорь: самая нижняя строка модели (альфа ≥ {MODEL_ALPHA}) должна быть на `anchor[1]` из манифеста ± {ANCHOR_TOLERANCE}.\n",
        rows.len()
    );
    if fallback.is_empty() {
        let _ = writeln!(s, "У всех спрайтов из `sprites.json` есть миниатюра.\n");
    } else {
        let _ = writeln!(
            s,
            "Нет миниатюры — показывается медальон (запасной режим): {}.\n",
            fallback
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(
        s,
        "| id | файл | статус | детали |\n| --- | --- | --- | --- |"
    );
    for r in rows {
        let status: Vec<String> = r.statuses.iter().map(Status::label).collect();
        let _ = writeln!(
            s,
            "| {} | `{}` | {} | {} |",
            r.id,
            r.path,
            status.join("; "),
            r.notes.join("; ")
        );
    }
    s
}

fn render(rows: &[Row], problems: &[String], total: u64, game: u64) -> String {
    let mut s = String::new();
    let ok = rows.iter().filter(|r| r.statuses == [Status::Ok]).count();
    let _ = writeln!(s, "# Отчёт по ассетам\n");
    let _ = writeln!(
        s,
        "Сгенерирован `sim assets-check`. Файлов по манифесту: {}, в порядке: {ok}. PNG всего: {}, из них игровые (terrain + units + objects): {}.\n",
        rows.len(),
        mb(total),
        mb(game)
    );
    let _ = writeln!(
        s,
        "Пороги: шов — отношение средней разницы RGB противоположных краёв к средней разнице соседних пикселей внутри > {SEAM_MAX_RATIO}; у юнитов и объектов карты прозрачная рамка {UNIT_EDGE_MARGIN_PX} px. Модели техники визуально не оценивались.\n"
    );
    if problems.is_empty() {
        let _ = writeln!(s, "Привязка `data/sprites.json`: замечаний нет.\n");
    } else {
        let _ = writeln!(s, "## Замечания\n");
        for p in problems {
            let _ = writeln!(s, "- {p}");
        }
        let _ = writeln!(s);
    }
    let _ = writeln!(
        s,
        "| id | файл | статус | детали |\n| --- | --- | --- | --- |"
    );
    for r in rows {
        let status: Vec<String> = r.statuses.iter().map(Status::label).collect();
        let _ = writeln!(
            s,
            "| {} | `{}` | {} | {} |",
            r.id,
            r.path,
            status.join("; "),
            r.notes.join("; ")
        );
    }
    s
}
