//! Hillshade raster over the bbox (light from the north-west, R-UI-06),
//! used as the map underlay by the client.

use image::GrayImage;

use crate::config::HillshadeConfig;
use crate::dem::Elevation;
use crate::geo::{Km, Projection};

/// Lambert shading (ESRI formula). Pixels without data get the flat value.
pub fn hillshade(dem: &dyn Elevation, proj: &Projection, cfg: &HillshadeConfig) -> GrayImage {
    let size = proj.size_km();
    let w = (size.x * cfg.px_per_km).ceil().max(1.0) as u32;
    let h = (size.y * cfg.px_per_km).ceil().max(1.0) as u32;
    let cell_km = 1.0 / cfg.px_per_km;
    let cell_m = cell_km * 1000.0;
    let zenith = (90.0 - cfg.altitude_deg).to_radians();
    let azimuth = ((360.0 - cfg.azimuth_deg + 90.0) % 360.0).to_radians();
    let flat = (255.0 * zenith.cos()).round() as u8;
    GrayImage::from_fn(w, h, |px, py| {
        let c = Km {
            x: (f64::from(px) + 0.5) * cell_km,
            y: (f64::from(py) + 0.5) * cell_km,
        };
        let z = |dx: f64, dy: f64| {
            let (lon, lat) = proj.to_lonlat(Km {
                x: c.x + dx * cell_km,
                y: c.y + dy * cell_km,
            });
            dem.elevation(lon, lat)
        };
        let (Some(e), Some(wv), Some(n), Some(s)) =
            (z(1.0, 0.0), z(-1.0, 0.0), z(0.0, -1.0), z(0.0, 1.0))
        else {
            return image::Luma([flat]);
        };
        let dzdx = (e - wv) / (2.0 * cell_m);
        let dzdy = (s - n) / (2.0 * cell_m);
        let slope = (cfg.z_factor * dzdx.hypot(dzdy)).atan();
        let aspect = dzdy.atan2(-dzdx);
        let v = 255.0
            * (zenith.cos() * slope.cos() + zenith.sin() * slope.sin() * (azimuth - aspect).cos());
        image::Luma([v.clamp(0.0, 255.0).round() as u8])
    })
}
