//! Per-hex relief statistics from the DEM and terrain by relief (R-UI-01).

use crate::config::Config;
use crate::dem::Elevation;
use crate::geo::{Grid, Km, Projection};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReliefStats {
    pub mean_m: f64,
    pub relief_m: f64,
    pub slope_deg: f64,
    pub samples: usize,
}

/// Slope (degrees) at a point from central differences `step` metres apart.
fn slope_at(dem: &dyn Elevation, proj: &Projection, p: Km, step_m: f64) -> Option<f64> {
    let s = step_m / 1000.0;
    let z = |dx: f64, dy: f64| {
        let (lon, lat) = proj.to_lonlat(Km {
            x: p.x + dx,
            y: p.y + dy,
        });
        dem.elevation(lon, lat)
    };
    let dzdx = (z(s, 0.0)? - z(-s, 0.0)?) / (2.0 * step_m);
    let dzdy = (z(0.0, s)? - z(0.0, -s)?) / (2.0 * step_m);
    Some(dzdx.hypot(dzdy).atan().to_degrees())
}

/// `None` if no sample of the hex has elevation data.
pub fn hex_stats(
    dem: &dyn Elevation,
    proj: &Projection,
    grid: &Grid,
    col: u32,
    row: u32,
    cfg: &Config,
) -> Option<ReliefStats> {
    let (mut sum, mut min, mut max, mut n) = (0.0, f64::INFINITY, f64::NEG_INFINITY, 0usize);
    let (mut slope_sum, mut slope_n) = (0.0, 0usize);
    for p in grid.samples(col, row, cfg.samples_per_hex_side) {
        let (lon, lat) = proj.to_lonlat(p);
        if let Some(z) = dem.elevation(lon, lat) {
            sum += z;
            min = min.min(z);
            max = max.max(z);
            n += 1;
        }
        if let Some(s) = slope_at(dem, proj, p, cfg.slope_step_m) {
            slope_sum += s;
            slope_n += 1;
        }
    }
    (n > 0).then(|| ReliefStats {
        mean_m: sum / n as f64,
        relief_m: max - min,
        slope_deg: if slope_n > 0 {
            slope_sum / slope_n as f64
        } else {
            0.0
        },
        samples: n,
    })
}

/// First rule whose relief or slope threshold is reached; else the default.
pub fn classify(stats: Option<ReliefStats>, cfg: &Config) -> String {
    let Some(s) = stats else {
        return cfg.default_terrain.clone();
    };
    cfg.relief_rules
        .iter()
        .find(|r| s.relief_m >= r.min_relief_m || s.slope_deg >= r.min_slope_deg)
        .map_or_else(|| cfg.default_terrain.clone(), |r| r.terrain.clone())
}
