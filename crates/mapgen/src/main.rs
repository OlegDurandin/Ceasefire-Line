//! mapgen: DEM (GeoTIFF) → Ceasefire Line `map.json` + hillshade (stage 2),
//! and `mapgen bake`: the painted map of a (possibly edited) map.json in tiles.
//!
//! `mapgen --dem <tif|dir>... --bbox W,S,E,N --hex-km 5 [--config data/mapgen.json] --out <dir>`
//! `mapgen bake --map <dir> --dem <tif|dir>... [--osm <pbf>...] [--config ...] [--assets assets] [--sprites data/sprites.json]`
//!
//! `mapgen procgen --params <procgen.json> [--config data/mapgen.json] [--terrain data/terrain.json] --out <dir>`:
//! procedural relief for the fictional scenarios (R-UI-01, GDD section 14
//! «Срединные земли»): heights by seed, lakes and rivers by drainage, forest
//! by moisture, cities from the parameters, roads between them. Writes a
//! plain `map.json` (edited in the map editor like any other), `elevation.png`
//! (16 bit) and `procedural.json` (rivers, lakes, forest in lon/lat), plus the
//! hillshade. Parameters: `seed`, `cols`, `rows`, `hex_km`, `biome`
//! (`temperate` | `steppe` | `foothills`), `origin` [lon, lat] of the
//! north-west corner, `cities` [{`name` (locale key), `hint` [col, row]?,
//! `size` `large` | `small`}]; the tunable numbers are in `mapgen.json`
//! `procgen` (example: `data/procgen/example-temperate.json`).
//! `mapgen bake --map <dir>` on such a directory needs no `--dem`/`--osm`:
//! it reads elevation.png and procedural.json and the biome's textures.
//!
//! Deterministic: inputs are sorted, outputs depend only on them.

mod bake;
mod config;
mod dem;
mod features;
mod geo;
mod hillshade;
mod noise;
mod osm;
mod output;
mod procgen;
mod relief;
mod roads_layer;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use config::Config;
use dem::{Elevation, Mosaic, Tile};
use features::AreaKm;
use geo::{BBox, Grid, Projection};
use osm::OsmData;

struct Args {
    dem: Vec<PathBuf>,
    osm: Vec<PathBuf>,
    secondary_roads: bool,
    bbox: BBox,
    hex_km: f64,
    config: PathBuf,
    out: PathBuf,
}

fn usage() -> String {
    "usage: mapgen --dem <tif|dir>... [--osm <file.osm.pbf>...] [--secondary-roads] --bbox W,S,E,N --hex-km KM [--config data/mapgen.json] --out DIR".into()
}

fn parse(args: &[String]) -> Result<Args, String> {
    let (mut dem, mut bbox, mut hex_km, mut config, mut out) = (
        Vec::new(),
        None,
        None,
        PathBuf::from("data/mapgen.json"),
        None,
    );
    let (mut osm, mut secondary_roads) = (Vec::new(), false);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if flag == "--secondary-roads" {
            secondary_roads = true;
            continue;
        }
        let v = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))?;
        match flag.as_str() {
            "--dem" => dem.push(PathBuf::from(v)),
            "--osm" => osm.push(PathBuf::from(v)),
            "--bbox" => bbox = Some(BBox::parse(v)?),
            "--hex-km" => hex_km = Some(v.parse::<f64>().map_err(|e| format!("--hex-km: {e}"))?),
            "--config" => config = PathBuf::from(v),
            "--out" => out = Some(PathBuf::from(v)),
            _ => return Err(format!("unknown option {flag}\n{}", usage())),
        }
    }
    osm.sort();
    Ok(Args {
        dem,
        osm,
        secondary_roads,
        bbox: bbox.ok_or_else(usage)?,
        hex_km: hex_km.ok_or_else(usage)?,
        config,
        out: out.ok_or_else(usage)?,
    })
}

/// .tif/.tiff files given directly or found (non-recursively) in directories, sorted.
fn tiff_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            for e in fs::read_dir(p)
                .map_err(|e| format!("{}: {e}", p.display()))?
                .flatten()
            {
                let f = e.path();
                if f.extension().is_some_and(|x| {
                    x.eq_ignore_ascii_case("tif") || x.eq_ignore_ascii_case("tiff")
                }) {
                    out.push(f);
                }
            }
        } else {
            out.push(p.clone());
        }
    }
    out.sort();
    Ok(out)
}

pub struct Generated {
    pub map: serde_json::Value,
    pub hillshade: image::GrayImage,
    pub hillshade_meta: serde_json::Value,
}

/// The whole pipeline on any elevation source.
/// The whole pipeline on any elevation source and optional OSM data.
pub fn generate(
    dem: &dyn Elevation,
    osm: Option<&OsmData>,
    bbox: BBox,
    hex_km: f64,
    cfg: &Config,
    sources: &[String],
) -> Result<Generated, String> {
    let proj = Projection::new(bbox, cfg.earth_radius_km);
    let grid = Grid::fit(proj.size_km(), hex_km)?;
    let empty = OsmData::default();
    let osm = osm.unwrap_or(&empty);
    let areas: Vec<AreaKm> = osm.areas.iter().map(|a| AreaKm::new(a, &proj)).collect();
    let (cols, rows) = (grid.cols as usize, grid.rows as usize);
    let mut terrain = vec![vec![String::new(); cols]; rows];
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            // Sea by elevation first, then OSM areas (in config order), then relief.
            if let Some(sea) = &cfg.sea {
                if relief::low_share(
                    dem,
                    &proj,
                    &grid,
                    col,
                    row,
                    cfg.samples_per_hex_side,
                    sea.max_elevation_m,
                ) >= sea.min_share
                {
                    terrain[row as usize][col as usize] = sea.terrain.clone();
                    continue;
                }
            }
            let shares = features::area_shares(&grid, &areas, col, row, cfg.samples_per_hex_side);
            let by_area = cfg
                .osm
                .areas
                .iter()
                .find(|rule| {
                    shares
                        .get(&rule.terrain)
                        .is_some_and(|s| *s >= rule.min_share)
                })
                .map(|rule| rule.terrain.clone());
            terrain[row as usize][col as usize] = by_area.unwrap_or_else(|| {
                let stats = relief::hex_stats(dem, &proj, &grid, col, row, cfg);
                relief::classify(stats, cfg)
            });
        }
    }
    // Places are listed for the editor; big ones (config) make their hex a
    // city (never water).
    let mut places = Vec::new();
    for p in &osm.places {
        let Some([col, row]) = features::hex_at(&grid, proj.to_km(p.at.0, p.at.1)) else {
            continue;
        };
        let cell = &mut terrain[row as usize][col as usize];
        let big = cfg.osm.city_kinds.contains(&p.kind)
            || p.population
                .is_some_and(|n| n >= cfg.osm.city_min_population);
        if big && cell != "water" {
            *cell = "city".to_owned();
        }
        places.push(serde_json::json!({ "name": p.name, "kind": p.kind, "hex": [col, row], "population": p.population }));
    }
    let roads = features::road_edges(&grid, &proj, &osm.roads);
    let canals = features::river_edges(
        &grid,
        &proj,
        &osm.rivers,
        cfg.osm.river_min_length_km,
        cfg.osm.river_line_weight,
    );
    // A road over a river has a bridge (Bnot Ya'akov and the like).
    let bridges: Vec<[[i32; 2]; 2]> = if cfg.auto_bridges {
        roads.intersection(&canals).copied().collect()
    } else {
        Vec::new()
    };
    // A mountain hex with a road is a pass.
    let mut pass = vec![vec![false; cols]; rows];
    for [a, b] in &roads {
        for [c, r] in [a, b] {
            if terrain[*r as usize][*c as usize] == "mountains" {
                pass[*r as usize][*c as usize] = true;
            }
        }
    }
    let legend: Vec<(String, char)> = cfg.legend.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let roads: Vec<[[i32; 2]; 2]> = roads.into_iter().collect();
    let canals: Vec<[[i32; 2]; 2]> = canals.into_iter().collect();
    let map = output::map_json(&output::MapInput {
        grid: &grid,
        terrain: &terrain,
        pass: &pass,
        legend: &legend,
        pass_char: cfg.pass_char,
        bbox,
        hex_km,
        sources,
        roads: &roads,
        canals: &canals,
        bridges: &bridges,
        places: &places,
        size_km: proj.size_km(),
    })?;
    let hillshade = hillshade::hillshade(dem, &proj, &cfg.hillshade);
    let hillshade_meta = output::hillshade_json(
        &grid,
        hillshade.width(),
        hillshade.height(),
        cfg.hillshade.px_per_km,
    );
    Ok(Generated {
        map,
        hillshade,
        hillshade_meta,
    })
}

fn write(out: &Path, g: &Generated) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let pretty = |v: &serde_json::Value| {
        serde_json::to_string_pretty(v)
            .map(|s| s + "\n")
            .map_err(|e| e.to_string())
    };
    fs::write(out.join("map.json"), pretty(&g.map)?).map_err(|e| e.to_string())?;
    fs::write(out.join("hillshade.json"), pretty(&g.hillshade_meta)?).map_err(|e| e.to_string())?;
    let sources: Vec<&str> = g.map["geo"]["sources"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    // Copernicus DEM notices are always required; the ODbL part only with OSM.
    fs::write(out.join("LICENSE-DATA.md"), license_text(&sources)).map_err(|e| e.to_string())?;
    g.hillshade
        .save(out.join("hillshade.png"))
        .map_err(|e| e.to_string())
}

/// Copernicus WorldDEM-30 licence, Article 6(b): notice for adapted data.
pub const COPERNICUS_NOTICE: &str = "produced using Copernicus WorldDEM-30 © DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018 provided under COPERNICUS by the European Union and ESA; all rights reserved";
/// Article 6(c): the liability sentence for the licence covering distribution.
pub const COPERNICUS_LIABILITY: &str = "The organisations in charge of the Copernicus programme by law or by delegation do not incur any liability for any use of the Copernicus WorldDEM-30";
pub const OSM_NOTICE: &str = "© OpenStreetMap contributors (ODbL)";

/// Licence notes for a generated map: Copernicus DEM (always) and ODbL for
/// OpenStreetMap-derived data (when used). Scenario files, code and art are
/// not covered.
fn license_text(sources: &[&str]) -> String {
    let osm = sources.iter().any(|s| s.contains("OpenStreetMap"));
    let mut t = String::from(
        "# Лицензия данных карты\n\n\
         `map.json`, `hillshade.png`, `hillshade.json`, `tiles.json` и `tiles/` в этой папке построены утилитой `mapgen` из открытых данных.\n\n\
         ## Рельеф: Copernicus DEM GLO-30\n\n\
         Типы местности по рельефу и отмывка — изменённые данные Copernicus WorldDEM-30 (лицензия Copernicus WorldDEM-30, статья 6):\n\n",
    );
    t.push_str(&format!(
        "> {COPERNICUS_NOTICE}\n\n> {COPERNICUS_LIABILITY}\n\n"
    ));
    t.push_str(
        "Проект не связан с программой Copernicus, Европейским союзом, ESA, DLR или Airbus и не одобрен ими. \
         Эти условия действуют и для всех, кто получает карту от нас дальше (статья 6(e)).\n",
    );
    if osm {
        t.push_str(
            "\n## OpenStreetMap\n\n\
             Дороги, реки, водоёмы, леса и сады, застройка и названия — из OpenStreetMap: © OpenStreetMap contributors, \
             Open Database License (ODbL) 1.0, https://opendatacommons.org/licenses/odbl/. \
             `map.json` — производная база данных и распространяется на тех же условиях (ODbL 1.0) с указанием источника.\n",
        );
    }
    t.push_str(
        "\nЭто не распространяется на `scenario.json`, `events.json`, код и графику игры.\n",
    );
    t
}

fn run(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("bake") {
        return run_bake(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("roads") {
        return run_roads(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("airfields") {
        return run_airfields(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("procgen") {
        return run_procgen(&args[1..]);
    }
    let a = parse(args)?;
    let cfg = Config::from_json(
        &fs::read_to_string(&a.config).map_err(|e| format!("{}: {e}", a.config.display()))?,
    )?;
    let files = tiff_files(&a.dem)?;
    if files.is_empty() {
        return Err("no DEM tiles (--dem)".into());
    }
    let tiles = files
        .iter()
        .map(|f| Tile::open(f))
        .collect::<Result<Vec<_>, _>>()?;
    let mut sources = vec![COPERNICUS_NOTICE.to_owned()];
    let osm = if a.osm.is_empty() {
        None
    } else {
        sources.push(OSM_NOTICE.to_owned());
        Some(osm::read(&a.osm, a.bbox, &cfg.osm, a.secondary_roads)?)
    };
    let g = generate(
        &Mosaic(tiles),
        osm.as_ref(),
        a.bbox,
        a.hex_km,
        &cfg,
        &sources,
    )?;
    write(&a.out, &g)?;
    eprintln!(
        "{} × {} hexes → {}",
        g.map["width"],
        g.map["height"],
        a.out.join("map.json").display()
    );
    Ok(())
}

struct BakeArgs {
    map: PathBuf,
    dem: Vec<PathBuf>,
    osm: Vec<PathBuf>,
    config: PathBuf,
    assets: PathBuf,
    sprites: PathBuf,
}

fn parse_bake(args: &[String]) -> Result<BakeArgs, String> {
    let usage = "usage: mapgen bake --map DIR --dem <tif|dir>... [--osm <file.osm.pbf>...] [--config data/mapgen.json] [--assets assets] [--sprites data/sprites.json]";
    let mut a = BakeArgs {
        map: PathBuf::new(),
        dem: Vec::new(),
        osm: Vec::new(),
        config: PathBuf::from("data/mapgen.json"),
        assets: PathBuf::from("assets"),
        sprites: PathBuf::from("data/sprites.json"),
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let v = PathBuf::from(
            it.next()
                .ok_or_else(|| format!("missing value for {flag}\n{usage}"))?,
        );
        match flag.as_str() {
            "--map" => a.map = v,
            "--dem" => a.dem.push(v),
            "--osm" => a.osm.push(v),
            "--config" => a.config = v,
            "--assets" => a.assets = v,
            "--sprites" => a.sprites = v,
            _ => return Err(format!("unknown option {flag}\n{usage}")),
        }
    }
    if a.map.as_os_str().is_empty() {
        return Err(usage.into());
    }
    a.osm.sort();
    Ok(a)
}

fn read_json(p: &Path) -> Result<serde_json::Value, String> {
    let t = fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&t).map_err(|e| format!("{}: {e}", p.display()))
}

/// Terrain ids per [row][col] from map.json (legend characters).
fn hex_terrain(map: &serde_json::Value) -> Result<Vec<Vec<String>>, String> {
    let legend = map["legend"].as_object().ok_or("map.json: no legend")?;
    map["terrain"]
        .as_array()
        .ok_or("map.json: no terrain")?
        .iter()
        .map(|row| {
            row.as_str()
                .ok_or("map.json: terrain row")?
                .chars()
                .map(|ch| {
                    legend
                        .get(&ch.to_string())
                        .and_then(|l| l["terrain"].as_str())
                        .map(str::to_owned)
                        .ok_or_else(|| format!("map.json: unknown character '{ch}'"))
                })
                .collect()
        })
        .collect()
}

/// bbox, projection and grid of a generated map; checks its size.
fn map_geometry(map: &serde_json::Value, cfg: &Config) -> Result<(BBox, Projection, Grid), String> {
    let geo = &map["geo"];
    let b: Vec<f64> = geo["bbox"]
        .as_array()
        .map(|v| v.iter().filter_map(serde_json::Value::as_f64).collect())
        .unwrap_or_default();
    let [west, south, east, north] = b[..] else {
        return Err("map.json: geo.bbox is required".into());
    };
    let bbox = BBox {
        west,
        south,
        east,
        north,
    };
    let hex_km = geo["hex_km"].as_f64().ok_or("map.json: geo.hex_km")?;
    let proj = Projection::new(bbox, cfg.earth_radius_km);
    let grid = Grid::fit(proj.size_km(), hex_km)?;
    if Some(u64::from(grid.cols)) != map["width"].as_u64()
        || Some(u64::from(grid.rows)) != map["height"].as_u64()
    {
        return Err("map.json: width/height do not match geo.bbox and hex_km".into());
    }
    Ok((bbox, proj, grid))
}

/// The year of `scenario.json`'s `start_date` next to the map, if any.
fn scenario_year(dir: &Path) -> Option<i64> {
    let s = read_json(&dir.join("scenario.json")).ok()?;
    s["start_date"].as_str()?.get(..4)?.parse().ok()
}

/// `mapgen roads --map DIR (--use historical|osm | --remove historical) [--config …]`
fn run_roads(args: &[String]) -> Result<(), String> {
    let usage = "usage: mapgen roads --map DIR (--use historical|osm | --remove historical) [--year YYYY] [--config data/mapgen.json]";
    let (mut dir, mut action, mut config) = (None, None, PathBuf::from("data/mapgen.json"));
    let mut year: Option<i64> = None;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let v = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}\n{usage}"))?;
        match (flag.as_str(), v.as_str()) {
            ("--map", _) => dir = Some(PathBuf::from(v)),
            ("--use", "historical" | "osm") => action = Some(v.clone()),
            ("--remove", "historical") => action = Some("remove".to_owned()),
            ("--config", _) => config = PathBuf::from(v),
            ("--year", _) => year = Some(v.parse().map_err(|e| format!("--year: {e}"))?),
            _ => return Err(format!("bad option {flag} {v}\n{usage}")),
        }
    }
    let (dir, action) = (dir.ok_or(usage)?, action.ok_or(usage)?);
    let cfg = Config::from_json(
        &fs::read_to_string(&config).map_err(|e| format!("{}: {e}", config.display()))?,
    )?;
    let path = dir.join("map.json");
    let mut map = read_json(&path)?;
    let (_, proj, grid) = map_geometry(&map, &cfg)?;
    let year = year.or_else(|| scenario_year(&dir));
    let msg = roads_layer::apply(
        &mut map,
        &grid,
        &proj,
        &action,
        cfg.pass_char,
        year,
        cfg.auto_bridges,
    )?;
    let text = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())? + "\n";
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    eprintln!("{msg} → {}", path.display());
    Ok(())
}

/// `mapgen airfields --map DIR --from FILE [--config …]`: airfields given as
/// lon/lat with sources (R-AIR-01) → `airfields` hexes of map.json; the list
/// itself is kept as `airfield_sources`. Points off the map are skipped.
fn run_airfields(args: &[String]) -> Result<(), String> {
    let usage = "usage: mapgen airfields --map DIR --from FILE.json [--config data/mapgen.json]";
    let (mut dir, mut from, mut config) = (None, None, PathBuf::from("data/mapgen.json"));
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let v = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}\n{usage}"))?;
        match flag.as_str() {
            "--map" => dir = Some(PathBuf::from(v)),
            "--from" => from = Some(PathBuf::from(v)),
            "--config" => config = PathBuf::from(v),
            _ => return Err(format!("bad option {flag}\n{usage}")),
        }
    }
    let (dir, from) = (dir.ok_or(usage)?, from.ok_or(usage)?);
    let cfg = Config::from_json(
        &fs::read_to_string(&config).map_err(|e| format!("{}: {e}", config.display()))?,
    )?;
    let list = read_json(&from)?;
    let path = dir.join("map.json");
    let mut map = read_json(&path)?;
    let (bbox, proj, grid) = map_geometry(&map, &cfg)?;
    let mut hexes: BTreeSet<[i32; 2]> = BTreeSet::new();
    let mut kept = Vec::new();
    for a in list.as_array().ok_or("airfields: a JSON array")? {
        let (Some(lon), Some(lat)) = (a["lon"].as_f64(), a["lat"].as_f64()) else {
            return Err(format!("airfield without lon/lat: {a}"));
        };
        if lon < bbox.west || lon > bbox.east || lat < bbox.south || lat > bbox.north {
            continue;
        }
        let p = proj.to_km(lon, lat);
        let mut best: Option<(f64, [i32; 2])> = None;
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let c = grid.center(col, row);
                let d = (c.x - p.x).hypot(c.y - p.y);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, [col as i32, row as i32]));
                }
            }
        }
        if let Some((_, h)) = best {
            hexes.insert(h);
            let mut a = a.clone();
            a["hex"] = serde_json::json!(h);
            kept.push(a);
        }
    }
    let obj = map.as_object_mut().ok_or("map.json is not an object")?;
    obj.insert("airfields".into(), serde_json::json!(hexes));
    obj.insert("airfield_sources".into(), serde_json::Value::Array(kept));
    let text = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())? + "\n";
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    eprintln!("airfields: {} hexes → {}", hexes.len(), path.display());
    Ok(())
}

/// `mapgen procgen --params FILE [--config …] [--terrain …] --out DIR`.
fn run_procgen(args: &[String]) -> Result<(), String> {
    let usage = "usage: mapgen procgen --params FILE.json [--config data/mapgen.json] [--terrain data/terrain.json] --out DIR";
    let (mut params, mut out) = (None, None);
    let (mut config, mut terrain) = (
        PathBuf::from("data/mapgen.json"),
        PathBuf::from("data/terrain.json"),
    );
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let v = PathBuf::from(
            it.next()
                .ok_or_else(|| format!("missing value for {flag}\n{usage}"))?,
        );
        match flag.as_str() {
            "--params" => params = Some(v),
            "--config" => config = v,
            "--terrain" => terrain = v,
            "--out" => out = Some(v),
            _ => return Err(format!("unknown option {flag}\n{usage}")),
        }
    }
    let (params, out) = (params.ok_or(usage)?, out.ok_or(usage)?);
    let read = |p: &Path| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let cfg = Config::from_json(&read(&config)?)?;
    let p = procgen::Params::from_json(&read(&params)?)?;
    let costs = procgen::TerrainCosts::from_json(&read(&terrain)?)?;
    let g = procgen::generate(&p, &cfg, &costs)?;
    procgen::write(&out, &g)?;
    eprintln!(
        "{} × {} hexes, biome {}, seed {} → {}",
        g.map["width"],
        g.map["height"],
        p.biome,
        p.seed,
        out.join("map.json").display()
    );
    Ok(())
}

fn run_bake(args: &[String]) -> Result<(), String> {
    let a = parse_bake(args)?;
    let cfg = Config::from_json(
        &fs::read_to_string(&a.config).map_err(|e| format!("{}: {e}", a.config.display()))?,
    )?;
    let map = read_json(&a.map.join("map.json"))?;
    let (bbox, proj, grid) = map_geometry(&map, &cfg)?;
    let terrain = hex_terrain(&map)?;
    if a.map.join(procgen::PROCEDURAL_JSON).exists() {
        let pc = cfg
            .procgen
            .as_ref()
            .ok_or("procedural map, but the config has no `procgen` section")?;
        let pd = procgen::load_for_bake(&a.map, &proj, pc, bake::VEG)?
            .ok_or("procedural.json disappeared")?;
        return bake_procedural(&a, &cfg, pc, &map, &proj, &grid, terrain, pd);
    }
    if a.dem.is_empty() {
        return Err("mapgen bake: --dem is required (not a procedural map)".into());
    }
    let tiles = tiff_files(&a.dem)?
        .iter()
        .map(|f| Tile::open(f))
        .collect::<Result<Vec<_>, _>>()?;
    let dem = Mosaic(tiles);
    let mut sources = vec![COPERNICUS_NOTICE.to_owned()];
    let osm = if a.osm.is_empty() {
        OsmData::default()
    } else {
        sources.push(OSM_NOTICE.to_owned());
        let mut oc = cfg.osm.clone();
        oc.roads = cfg.bake.roads.iter().map(|r| r.kind.clone()).collect();
        oc.secondary_roads.clear();
        oc.areas.retain(|r| r.terrain == "water");
        oc.areas
            .extend(cfg.bake.vegetation.iter().map(|v| config::AreaRule {
                terrain: format!("{}{}", bake::VEG, v.kind),
                tags: v.tags.clone(),
                min_share: 1.0,
            }));
        osm::read(&a.osm, bbox, &oc, false)?
    };
    // City icons: the main place in each city hex, else its centre.
    let mut cities = Vec::new();
    for (row, line) in terrain.iter().enumerate() {
        for (col, t) in line.iter().enumerate() {
            if t != "city" {
                continue;
            }
            let best = osm
                .places
                .iter()
                .filter(|p| {
                    features::hex_at(&grid, proj.to_km(p.at.0, p.at.1))
                        == Some([col as i32, row as i32])
                })
                .max_by(|x, y| {
                    let rank = |p: &osm::Place| {
                        (
                            cfg.osm.city_kinds.contains(&p.kind),
                            p.population.unwrap_or(0),
                        )
                    };
                    rank(x).cmp(&rank(y)).then_with(|| y.name.cmp(&x.name))
                });
            cities.push(match best {
                Some(p) => proj.to_km(p.at.0, p.at.1),
                None => grid.center(col as u32, row as u32),
            });
        }
    }
    // Art from sprites.json; missing pictures are drawn procedurally.
    let sprites = read_json(&a.sprites)?;
    let textures = load_textures(&sprites, &a.assets, None)?;
    let (tree, city) = (
        map_object(&sprites, &a.assets, "tree"),
        map_object(&sprites, &a.assets, "city"),
    );
    // Roads: the historical layer when the map uses it, otherwise OSM lines.
    let historical = map["roads_from"].as_str() == Some(roads_layer::FROM_HISTORICAL);
    let (road_lines, road_styles) = if historical {
        (
            roads_layer::layer_lines(&map, scenario_year(&a.map))?,
            &cfg.bake.historical_roads,
        )
    } else {
        (osm.roads.clone(), &cfg.bake.roads)
    };
    let img = bake::bake(&bake::BakeInput {
        dem: &dem,
        osm: &osm,
        map: &bake::HexMap { terrain },
        grid: &grid,
        proj: &proj,
        cities: &cities,
        roads: &road_lines,
        road_styles,
        art: bake::Art {
            terrain: &textures,
            tree: tree.as_ref(),
            city: city.as_ref(),
        },
        cfg: &cfg.bake,
        shade: &cfg.hillshade,
    })?;
    write_tiles(&a.map, img, &cfg.bake, &grid)?;
    let srcs: Vec<&str> = sources.iter().map(String::as_str).collect();
    fs::write(a.map.join("LICENSE-DATA.md"), license_text(&srcs)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Terrain textures named by sprites.json. `overrides` (a procedural biome's
/// set) come first and fall back to sprites.json, and a missing file is then
/// skipped (drawn procedurally); without overrides a missing file is an error.
fn load_textures(
    sprites: &serde_json::Value,
    assets: &Path,
    overrides: Option<&BTreeMap<String, String>>,
) -> Result<BTreeMap<String, image::RgbImage>, String> {
    let mut files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(o) = overrides {
        for (id, f) in o {
            files.entry(id.clone()).or_default().push(f.clone());
        }
    }
    if let Some(t) = sprites["terrain"].as_object() {
        for (id, f) in t {
            if let Some(f) = f.as_str() {
                files.entry(id.clone()).or_default().push(f.to_owned());
            }
        }
    }
    let mut textures = BTreeMap::new();
    for (id, list) in files {
        let mut found = false;
        for f in &list {
            let p = assets.join("terrain").join(format!("{f}.png"));
            if overrides.is_some() && !p.exists() {
                eprintln!("texture {} missing for `{id}`", p.display());
                continue;
            }
            let img = image::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            textures.insert(id.clone(), img.to_rgb8());
            found = true;
            break;
        }
        if !found && overrides.is_some() {
            eprintln!("no texture for `{id}`: drawn procedurally");
        }
    }
    Ok(textures)
}

fn map_object(sprites: &serde_json::Value, assets: &Path, role: &str) -> Option<image::RgbaImage> {
    let id = sprites["map_objects"][role].as_str()?;
    image::open(assets.join("objects").join(format!("{id}.png")))
        .ok()
        .map(|i| i.to_rgba8())
}

/// `mapgen bake` of a `mapgen procgen` directory: elevation.png and the
/// rivers, lakes and forest of procedural.json instead of DEM and OSM; roads
/// from map.json (as edited); the biome's texture set; no licence file (no
/// third-party data).
#[allow(clippy::too_many_arguments)]
fn bake_procedural(
    a: &BakeArgs,
    cfg: &Config,
    pc: &config::ProcgenConfig,
    map: &serde_json::Value,
    proj: &Projection,
    grid: &Grid,
    terrain: Vec<Vec<String>>,
    pd: procgen::BakeData,
) -> Result<(), String> {
    let mut bc = cfg.bake.clone();
    bc.px_per_km = pc.bake_px_per_km;
    for w in bc.water.river_km.values_mut() {
        *w *= pc.bake_line_scale;
    }
    let mut road_styles = cfg.bake.historical_roads.clone();
    for st in &mut road_styles {
        st.width_km *= pc.bake_line_scale;
    }
    let mut cities = Vec::new();
    for (row, line) in terrain.iter().enumerate() {
        for (col, t) in line.iter().enumerate() {
            if *t == pc.city_terrain {
                cities.push(grid.center(col as u32, row as u32));
            }
        }
    }
    let sprites = read_json(&a.sprites)?;
    // R-UI-01: the biome's texture set (sprites.json `biome_terrain`) over `terrain`.
    let overrides: BTreeMap<String, String> = sprites["biome_terrain"][&pd.biome]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let textures = load_textures(&sprites, &a.assets, Some(&overrides))?;
    let (tree, city) = (
        map_object(&sprites, &a.assets, "tree"),
        map_object(&sprites, &a.assets, "city"),
    );
    let roads = procgen::road_lines(map, grid, proj, pc);
    let img = bake::bake(&bake::BakeInput {
        dem: &pd.dem,
        osm: &pd.osm,
        map: &bake::HexMap { terrain },
        grid,
        proj,
        cities: &cities,
        roads: &roads,
        road_styles: &road_styles,
        art: bake::Art {
            terrain: &textures,
            tree: tree.as_ref(),
            city: city.as_ref(),
        },
        cfg: &bc,
        shade: &cfg.hillshade,
    })?;
    write_tiles(&a.map, img, &bc, grid)
}

/// Tile pyramid and tiles.json of a baked map.
fn write_tiles(
    map_dir: &Path,
    img: image::RgbImage,
    bc: &config::BakeConfig,
    grid: &Grid,
) -> Result<(), String> {
    let digest = fnv1a(img.as_raw());
    let (width, height) = img.dimensions();
    let tile = bc.tile_px;
    let dir = map_dir.join("tiles");
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut levels = Vec::new();
    for (z, level) in bake::pyramid(img, tile).iter().enumerate() {
        let (cols, rows) = (level.width().div_ceil(tile), level.height().div_ceil(tile));
        let zdir = dir.join(z.to_string());
        fs::create_dir_all(&zdir).map_err(|e| format!("{}: {e}", zdir.display()))?;
        for ty in 0..rows {
            for tx in 0..cols {
                let (x0, y0) = (tx * tile, ty * tile);
                let (w, h) = (tile.min(level.width() - x0), tile.min(level.height() - y0));
                let part = image::imageops::crop_imm(level, x0, y0, w, h).to_image();
                let path = zdir.join(format!("{tx}_{ty}.webp"));
                let file =
                    fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                image::codecs::webp::WebPEncoder::new_lossless(std::io::BufWriter::new(file))
                    .encode(part.as_raw(), w, h, image::ExtendedColorType::Rgb8)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
            }
        }
        levels.push(serde_json::json!({
            "z": z, "width": level.width(), "height": level.height(), "cols": cols, "rows": rows,
        }));
    }
    let r = grid.radius_km;
    let meta = serde_json::json!({
        "_comment": "mapgen bake: painted map tiles (R-UI-01). tiles/<z>/<x>_<y>.webp; level z is 2^z times smaller.",
        "format": "webp",
        "tile_px": tile,
        "px_per_km": bc.px_per_km,
        "hex_radius_km": r,
        "top_left_from_hex00_km": [-r, -r * 3f64.sqrt() / 2.0],
        "width_px": width,
        "height_px": height,
        "levels": levels,
        "fnv1a64": format!("{digest:016x}"),
    });
    let pretty = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())? + "\n";
    fs::write(map_dir.join("tiles.json"), pretty).map_err(|e| e.to_string())?;
    eprintln!(
        "{width} × {height} px, {} levels, fnv1a64 {digest:016x} → {}",
        meta["levels"].as_array().map_or(0, Vec::len),
        dir.display()
    );
    Ok(())
}

/// FNV-1a 64 over the full-size pixels: a fingerprint to check determinism.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// West: a low plain; a 400 m escarpment along lon 35.5; east of it, a
    /// 2500 m cone around (35.9, 33.3).
    struct Synthetic;
    impl Elevation for Synthetic {
        fn elevation(&self, lon: f64, lat: f64) -> Option<f64> {
            let step = 400.0 / (1.0 + (-(lon - 35.5) * 80.0).exp());
            let d = (lon - 35.9).hypot(lat - 33.3);
            let cone = (2500.0 * (1.0 - d / 0.12)).max(0.0);
            Some(-200.0 + step + cone)
        }
    }

    fn cfg() -> Config {
        Config::from_json(include_str!("../../../data/mapgen.json")).unwrap()
    }

    #[test]
    fn classifies_plain_escarpment_and_mountain() {
        let bbox = BBox::parse("35.30,33.00,36.10,33.45").unwrap();
        let g = generate(&Synthetic, None, bbox, 5.0, &cfg(), &[]).unwrap();
        let rows: Vec<String> = g.map["terrain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().to_owned())
            .collect();
        let at = |col: usize, row: usize| rows[row].as_bytes()[col] as char;
        assert_eq!(at(0, rows.len() - 1), 'p', "low plain in the south-west");
        assert!(
            rows.iter().any(|r| r.contains('m')),
            "the cone is mountains:\n{}",
            rows.join("\n")
        );
        assert!(
            rows.iter().any(|r| r.contains('h')),
            "the escarpment is hills:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn output_is_deterministic_and_loads_in_the_core() {
        let bbox = BBox::parse("35.45,32.60,36.50,33.45").unwrap();
        let a = generate(&Synthetic, None, bbox, 5.0, &cfg(), &["test".into()]).unwrap();
        let b = generate(&Synthetic, None, bbox, 5.0, &cfg(), &["test".into()]).unwrap();
        assert_eq!(a.map.to_string(), b.map.to_string());
        assert_eq!(a.hillshade.as_raw(), b.hillshade.as_raw());
        assert_eq!(
            (a.map["width"].as_u64(), a.map["height"].as_u64()),
            (Some(22), Some(18))
        );
        let (map, control) = ceasefire_core::scenario::load_map(&a.map.to_string()).unwrap();
        assert_eq!(map.hexes().count(), 22 * 18);
        assert!(control.is_empty(), "control is drawn in the editor");
    }

    fn square(w: f64, s: f64, e: f64, n: f64) -> Vec<[(f64, f64); 2]> {
        vec![
            [(w, s), (e, s)],
            [(e, s), (e, n)],
            [(e, n), (w, n)],
            [(w, n), (w, s)],
        ]
    }

    #[test]
    fn osm_features_end_to_end() {
        use osm::{Area, Line, Place};
        let cfg = cfg();
        let bbox = BBox::parse("35.30,32.70,36.10,33.45").unwrap();
        let mut data = OsmData {
            // A lake in the south and a "Hula" lake inside the exclusion box.
            areas: vec![
                Area {
                    terrain: "water".into(),
                    segments: square(35.52, 32.72, 35.65, 32.88),
                },
                Area {
                    terrain: "water".into(),
                    segments: square(35.57, 33.04, 35.64, 33.14),
                },
            ],
            // A river north–south and a road east–west across it and over the cone.
            rivers: vec![Line {
                kind: "river".into(),
                pts: vec![(35.62, 33.44), (35.62, 32.95)],
            }],
            roads: vec![Line {
                kind: "primary".into(),
                pts: vec![(35.31, 33.30), (36.09, 33.30)],
            }],
            places: vec![
                Place {
                    name: "Testtown".into(),
                    kind: "city".into(),
                    population: None,
                    at: (35.40, 33.00),
                },
                // A small town is listed but stays relief.
                Place {
                    name: "Smalltown".into(),
                    kind: "town".into(),
                    population: Some(900),
                    at: (35.40, 32.80),
                },
            ],
        };
        osm::drop_excluded_water(&mut data, &cfg.osm);
        assert_eq!(data.areas.len(), 1, "Hula is dropped");
        let g = generate(&Synthetic, Some(&data), bbox, 5.0, &cfg, &[]).unwrap();
        let rows: Vec<String> = g.map["terrain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().to_owned())
            .collect();
        let all = rows.join("\n");
        assert!(all.contains('w'), "the lake is water:\n{all}");
        assert!(all.contains('c'), "the town is a city:\n{all}");
        assert!(
            all.contains('P'),
            "the road over the mountain makes a pass:\n{all}"
        );
        assert!(!g.map["roads"].as_array().unwrap().is_empty());
        assert!(!g.map["canals"].as_array().unwrap().is_empty());
        assert!(
            !g.map["bridges"].as_array().unwrap().is_empty(),
            "road over the river has a bridge"
        );
        assert_eq!(g.map["geo"]["places"][0]["name"], "Testtown");
        assert_eq!(g.map["geo"]["places"][1]["name"], "Smalltown");
        assert_eq!(all.matches('c').count(), 1, "only the city:\n{all}");
        let (map, _) = ceasefire_core::scenario::load_map(&g.map.to_string()).unwrap();
        assert!(map.bridges().count() > 0);
    }

    #[test]
    fn bake_is_deterministic_and_paints_every_layer() {
        use osm::{Area, Line, Place};
        let mut cfg = cfg();
        cfg.bake.px_per_km = 12.0;
        let bbox = BBox::parse("35.30,32.70,36.10,33.45").unwrap();
        let proj = Projection::new(bbox, cfg.earth_radius_km);
        let grid = Grid::fit(proj.size_km(), 5.0).unwrap();
        let rows = grid.rows as usize;
        let cols = grid.cols as usize;
        let mut terrain = vec![vec!["plain".to_owned(); cols]; rows];
        terrain[2][2] = "city".into();
        let data = OsmData {
            areas: vec![
                Area {
                    terrain: "water".into(),
                    segments: square(35.52, 32.72, 35.65, 32.88),
                },
                Area {
                    terrain: format!("{}forest", bake::VEG),
                    segments: square(35.80, 33.20, 35.95, 33.35),
                },
            ],
            rivers: vec![Line {
                kind: "river".into(),
                pts: vec![(35.70, 33.44), (35.70, 32.95)],
            }],
            roads: vec![Line {
                kind: "primary".into(),
                pts: vec![(35.31, 33.10), (36.09, 33.10)],
            }],
            places: vec![Place {
                name: "Testtown".into(),
                kind: "city".into(),
                population: None,
                at: (35.40, 33.30),
            }],
        };
        let mut textures = BTreeMap::new();
        textures.insert(
            "plain".to_owned(),
            image::RgbImage::from_pixel(64, 64, image::Rgb([200, 160, 100])),
        );
        let cities = [grid.center(2, 2)];
        let run = || {
            bake::bake(&bake::BakeInput {
                dem: &Synthetic,
                osm: &data,
                map: &bake::HexMap {
                    terrain: terrain.clone(),
                },
                grid: &grid,
                proj: &proj,
                cities: &cities,
                roads: &data.roads,
                road_styles: &cfg.bake.roads,
                art: bake::Art {
                    terrain: &textures,
                    tree: None,
                    city: None,
                },
                cfg: &cfg.bake,
                shade: &cfg.hillshade,
            })
            .unwrap()
        };
        let (a, b) = (run(), run());
        assert_eq!(
            fnv1a(a.as_raw()),
            fnv1a(b.as_raw()),
            "same inputs, same pixels"
        );
        let at = |lon: f64, lat: f64| {
            let k = proj.to_km(lon, lat);
            let p = cfg.bake.px_per_km;
            a.get_pixel((k.x * p) as u32, (k.y * p) as u32).0
        };
        let lake = at(35.585, 32.80);
        assert!(lake[2] > lake[0], "lake is blue: {lake:?}");
        let road = at(35.40, 33.10);
        assert!(road[0] > 200 && road[1] > 190, "road is light: {road:?}");
        let forest = at(35.875, 33.275);
        assert!(
            forest[1] > forest[2] && forest[0] < 150,
            "forest is green: {forest:?}"
        );
        // The pyramid ends in one tile.
        let levels = bake::pyramid(a, 64);
        let last = levels.last().unwrap();
        assert!(last.width() <= 64 && last.height() <= 64);
    }

    /// R-UI-01: a river through a forest is painted over it, and the forest
    /// keeps a tree-free band of `river_clear_hex` along the channel.
    #[test]
    fn r_ui_01_bake_river_over_forest_with_a_clear_buffer() {
        use osm::{Area, Line};
        let mut cfg = cfg();
        cfg.bake.px_per_km = 12.0;
        let bbox = BBox::parse("35.30,32.70,36.10,33.45").unwrap();
        let proj = Projection::new(bbox, cfg.earth_radius_km);
        let grid = Grid::fit(proj.size_km(), 5.0).unwrap();
        let terrain = vec![vec!["forest".to_owned(); grid.cols as usize]; grid.rows as usize];
        let data = OsmData {
            areas: vec![Area {
                terrain: format!("{}forest", bake::VEG),
                segments: square(35.40, 32.75, 35.75, 33.05),
            }],
            rivers: vec![Line {
                kind: "river".into(),
                pts: vec![(35.575, 33.10), (35.575, 32.72)],
            }],
            ..OsmData::default()
        };
        let textures = BTreeMap::new();
        let a = bake::bake(&bake::BakeInput {
            dem: &Synthetic,
            osm: &data,
            map: &bake::HexMap { terrain },
            grid: &grid,
            proj: &proj,
            cities: &[],
            roads: &[],
            road_styles: &cfg.bake.roads,
            art: bake::Art {
                terrain: &textures,
                tree: None,
                city: None,
            },
            cfg: &cfg.bake,
            shade: &cfg.hillshade,
        })
        .unwrap();
        let p = cfg.bake.px_per_km;
        let river = proj.to_km(35.575, 32.90);
        let at = |dx_km: f64| {
            a.get_pixel(((river.x + dx_km) * p) as u32, (river.y * p) as u32)
                .0
        };
        let water = bake::rgb(&cfg.bake.water.color).unwrap();
        let on = at(0.0);
        for ch in 0..3 {
            let want = (water[ch] * 255.0).round() as i32;
            assert!(
                (i32::from(on[ch]) - want).abs() <= 2,
                "river pixel {on:?} is the water colour"
            );
        }
        // Crown pixels in a row across the river: none inside the buffer.
        let crown = bake::rgb(&cfg.bake.vegetation[0].crown_color).unwrap();
        let is_crown =
            |c: [u8; 3]| (0..3).all(|ch| (f32::from(c[ch]) / 255.0 - crown[ch]).abs() < 0.03);
        let buffer = cfg.bake.river_clear_hex * grid.radius_km * 3f64.sqrt();
        let step = 1.0 / p;
        let count = |from: f64, to: f64| {
            let mut n = 0;
            let mut d = from;
            while d < to {
                for y in -20..=20 {
                    let c = a
                        .get_pixel(
                            ((river.x + d) * p) as u32,
                            ((river.y + f64::from(y) * step) * p) as u32,
                        )
                        .0;
                    n += usize::from(is_crown(c));
                }
                d += step;
            }
            n
        };
        let rule = &cfg.bake.vegetation[0];
        let near = buffer - rule.crown_km * 1.2 - step;
        assert_eq!(count(-near, near), 0, "no trees within the buffer");
        assert!(count(buffer + 0.3, buffer + 2.0) > 0, "trees beyond it");
    }

    /// `mapgen procgen` twice gives the same files; `mapgen bake` works on
    /// the directory without DEM or OSM, also with no texture files at all.
    #[test]
    fn r_ui_01_procgen_cli_and_bake_end_to_end() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let tmp = std::env::temp_dir().join(format!("mapgen-procgen-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let params = tmp.join("params.json");
        fs::write(
            &params,
            r#"{ "seed": 7, "cols": 8, "rows": 6, "hex_km": 10, "biome": "temperate",
                 "origin": [20.0, 50.0],
                 "cities": [ { "name": "place.test.a", "hint": [1, 2], "size": "large" },
                             { "name": "place.test.b", "size": "small" } ] }"#,
        )
        .unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let cfg = s(&root.join("data/mapgen.json"));
        let procgen = |out: &Path| {
            let args = [
                "procgen",
                "--params",
                &s(&params),
                "--config",
                &cfg,
                "--terrain",
                &s(&root.join("data/terrain.json")),
                "--out",
                &s(out),
            ];
            run(&args.map(String::from)).unwrap();
        };
        let (a, b) = (tmp.join("a"), tmp.join("b"));
        procgen(&a);
        procgen(&b);
        for f in [
            "map.json",
            "procedural.json",
            "elevation.png",
            "hillshade.png",
        ] {
            assert_eq!(
                fs::read(a.join(f)).unwrap(),
                fs::read(b.join(f)).unwrap(),
                "{f} is byte-identical"
            );
        }
        let bake = |assets: &Path| {
            let args = [
                "bake",
                "--map",
                &s(&a),
                "--config",
                &cfg,
                "--assets",
                &s(assets),
                "--sprites",
                &s(&root.join("data/sprites.json")),
            ];
            run(&args.map(String::from)).unwrap();
            read_json(&a.join("tiles.json")).unwrap()["fnv1a64"].clone()
        };
        let with_art = bake(&root.join("assets"));
        assert_eq!(with_art, bake(&root.join("assets")), "same pixels");
        assert!(a.join("tiles/0/0_0.webp").exists());
        assert!(!a.join("LICENSE-DATA.md").exists(), "no third-party data");
        let empty = tmp.join("no-assets");
        fs::create_dir_all(&empty).unwrap();
        assert_ne!(
            with_art,
            bake(&empty),
            "missing textures: drawn procedurally"
        );
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn licence_text_carries_the_mandatory_notices() {
        let dem_only = license_text(&[COPERNICUS_NOTICE]);
        assert!(dem_only.contains(COPERNICUS_NOTICE), "Copernicus 6(b)");
        assert!(dem_only.contains(COPERNICUS_LIABILITY), "Copernicus 6(c)");
        assert!(!dem_only.contains("ODbL"), "no OSM, no ODbL section");
        let with_osm = license_text(&[COPERNICUS_NOTICE, OSM_NOTICE]);
        assert!(with_osm.contains("ODbL") && with_osm.contains("© OpenStreetMap contributors"));
    }
}
