//! mapgen: DEM (GeoTIFF) → Ceasefire Line `map.json` + hillshade (stage 2).
//!
//! `mapgen --dem <tif|dir>... --bbox W,S,E,N --hex-km 5 [--config data/mapgen.json] --out <dir>`
//!
//! Deterministic: inputs are sorted, outputs depend only on them.

mod config;
mod dem;
mod features;
mod geo;
mod hillshade;
mod osm;
mod output;
mod relief;

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
            // OSM areas first (in config order), then relief.
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
    // Places make their hex a city (never water) and are listed for the editor.
    let mut places = Vec::new();
    for p in &osm.places {
        let Some([col, row]) = features::hex_at(&grid, proj.to_km(p.at.0, p.at.1)) else {
            continue;
        };
        let cell = &mut terrain[row as usize][col as usize];
        if cell != "water" {
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
    let bridges: Vec<[[i32; 2]; 2]> = roads.intersection(&canals).copied().collect();
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
    if sources.iter().any(|s| s.contains("OpenStreetMap")) {
        fs::write(out.join("LICENSE-DATA.md"), license_text(&sources))
            .map_err(|e| e.to_string())?;
    }
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
         `map.json`, `hillshade.png` и `hillshade.json` в этой папке построены утилитой `mapgen` из открытых данных.\n\n\
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
             Дороги, реки, водоёмы, застройка и названия — из OpenStreetMap: © OpenStreetMap contributors, \
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
            places: vec![Place {
                name: "Testtown".into(),
                kind: "town".into(),
                population: Some(5000),
                at: (35.40, 33.00),
            }],
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
        let (map, _) = ceasefire_core::scenario::load_map(&g.map.to_string()).unwrap();
        assert!(map.bridges().count() > 0);
    }
}
