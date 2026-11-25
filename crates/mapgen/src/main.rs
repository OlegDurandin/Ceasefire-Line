//! mapgen: DEM (GeoTIFF) → Ceasefire Line `map.json` + hillshade (stage 2).
//!
//! `mapgen --dem <tif|dir>... --bbox W,S,E,N --hex-km 5 [--config data/mapgen.json] --out <dir>`
//!
//! Deterministic: inputs are sorted, outputs depend only on them.

mod config;
mod dem;
mod geo;
mod hillshade;
mod output;
mod relief;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use config::Config;
use dem::{Elevation, Mosaic, Tile};
use geo::{BBox, Grid, Projection};

struct Args {
    dem: Vec<PathBuf>,
    bbox: BBox,
    hex_km: f64,
    config: PathBuf,
    out: PathBuf,
}

fn usage() -> String {
    "usage: mapgen --dem <tif|dir>... --bbox W,S,E,N --hex-km KM [--config data/mapgen.json] --out DIR".into()
}

fn parse(args: &[String]) -> Result<Args, String> {
    let (mut dem, mut bbox, mut hex_km, mut config, mut out) = (
        Vec::new(),
        None,
        None,
        PathBuf::from("data/mapgen.json"),
        None,
    );
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let v = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))?;
        match flag.as_str() {
            "--dem" => dem.push(PathBuf::from(v)),
            "--bbox" => bbox = Some(BBox::parse(v)?),
            "--hex-km" => hex_km = Some(v.parse::<f64>().map_err(|e| format!("--hex-km: {e}"))?),
            "--config" => config = PathBuf::from(v),
            "--out" => out = Some(PathBuf::from(v)),
            _ => return Err(format!("unknown option {flag}\n{}", usage())),
        }
    }
    Ok(Args {
        dem,
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
pub fn generate(
    dem: &dyn Elevation,
    bbox: BBox,
    hex_km: f64,
    cfg: &Config,
    sources: &[String],
) -> Result<Generated, String> {
    let proj = Projection::new(bbox, cfg.earth_radius_km);
    let grid = Grid::fit(proj.size_km(), hex_km)?;
    let mut terrain = vec![vec![String::new(); grid.cols as usize]; grid.rows as usize];
    let pass = vec![vec![false; grid.cols as usize]; grid.rows as usize];
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let stats = relief::hex_stats(dem, &proj, &grid, col, row, cfg);
            terrain[row as usize][col as usize] = relief::classify(stats, cfg);
        }
    }
    let legend: Vec<(String, char)> = cfg.legend.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let map = output::map_json(&output::MapInput {
        grid: &grid,
        terrain: &terrain,
        pass: &pass,
        legend: &legend,
        pass_char: cfg.pass_char,
        bbox,
        hex_km,
        sources,
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
    g.hillshade
        .save(out.join("hillshade.png"))
        .map_err(|e| e.to_string())
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
    let sources = vec!["NASA / USGS SRTM".to_owned()];
    let g = generate(&Mosaic(tiles), a.bbox, a.hex_km, &cfg, &sources)?;
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
        let g = generate(&Synthetic, bbox, 5.0, &cfg(), &[]).unwrap();
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
        let a = generate(&Synthetic, bbox, 5.0, &cfg(), &["test".into()]).unwrap();
        let b = generate(&Synthetic, bbox, 5.0, &cfg(), &["test".into()]).unwrap();
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
}
