//! Elevation from GeoTIFF tiles (Copernicus DEM GLO-30 COG: tiled, DEFLATE,
//! float32; SRTM-style int16 works too). Georeferencing from the
//! ModelTiepoint / ModelPixelScale tags and the GeoKey raster type.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;

/// Anything that can answer "elevation in metres at lon/lat".
pub trait Elevation {
    fn elevation(&self, lon: f64, lat: f64) -> Option<f64>;
}

/// Void marker used by SRTM (Copernicus has no voids over land).
const VOID: f64 = -32768.0;
/// GeoKey GTRasterTypeGeoKey and its PixelIsPoint value.
const RASTER_TYPE_KEY: u16 = 1025;
const PIXEL_IS_POINT: u16 = 2;

pub struct Tile {
    width: usize,
    height: usize,
    data: Vec<f64>,
    /// Lon/lat of the centre of pixel (0, 0) and the pixel size in degrees.
    lon0: f64,
    lat0: f64,
    dlon: f64,
    dlat: f64,
}

impl Tile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let err = |e: tiff::TiffError| format!("{}: {e}", path.display());
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut dec = Decoder::new(BufReader::new(file)).map_err(err)?;
        let (w, h) = dec.dimensions().map_err(err)?;
        let tie = dec.get_tag_f64_vec(Tag::ModelTiepointTag).map_err(err)?;
        let scale = dec.get_tag_f64_vec(Tag::ModelPixelScaleTag).map_err(err)?;
        let point = dec
            .get_tag_u16_vec(Tag::GeoKeyDirectoryTag)
            .ok()
            .and_then(|keys| raster_type(&keys))
            == Some(PIXEL_IS_POINT);
        let data: Vec<f64> = match dec.read_image().map_err(err)? {
            DecodingResult::I16(v) => v.into_iter().map(f64::from).collect(),
            DecodingResult::U16(v) => v.into_iter().map(f64::from).collect(),
            DecodingResult::I32(v) => v.into_iter().map(f64::from).collect(),
            DecodingResult::F32(v) => v.into_iter().map(f64::from).collect(),
            DecodingResult::F64(v) => v,
            _ => return Err(format!("{}: unsupported sample format", path.display())),
        };
        let (&[i, j, _, x, y, _, ..], &[sx, sy, ..]) = (tie.as_slice(), scale.as_slice()) else {
            return Err(format!("{}: bad georeferencing tags", path.display()));
        };
        // Tiepoint maps raster (i, j) to (x, y); for PixelIsArea that raster point
        // is a pixel's corner, so pixel centres are half a pixel further in.
        let half = if point { 0.0 } else { 0.5 };
        Self::from_grid(
            w as usize,
            h as usize,
            data,
            x + (half - i) * sx,
            y - (half - j) * sy,
            sx,
            sy,
        )
    }

    /// A tile from raw samples; `lon0/lat0` is the centre of pixel (0, 0).
    pub fn from_grid(
        width: usize,
        height: usize,
        data: Vec<f64>,
        lon0: f64,
        lat0: f64,
        dlon: f64,
        dlat: f64,
    ) -> Result<Self, String> {
        if data.len() != width * height || dlon <= 0.0 || dlat <= 0.0 {
            return Err("bad tile grid".into());
        }
        Ok(Self {
            width,
            height,
            data,
            lon0,
            lat0,
            dlon,
            dlat,
        })
    }

    fn at(&self, x: usize, y: usize) -> Option<f64> {
        let v = *self.data.get(y * self.width + x)?;
        (v != VOID).then_some(v)
    }
}

impl Elevation for Tile {
    /// Bilinear between the four nearest pixel centres; `None` outside or on voids.
    fn elevation(&self, lon: f64, lat: f64) -> Option<f64> {
        let fx = (lon - self.lon0) / self.dlon;
        let fy = (self.lat0 - lat) / self.dlat;
        if fx < 0.0 || fy < 0.0 || fx > (self.width - 1) as f64 || fy > (self.height - 1) as f64 {
            return None;
        }
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let top = self.at(x0, y0)? * (1.0 - tx) + self.at(x1, y0)? * tx;
        let bottom = self.at(x0, y1)? * (1.0 - tx) + self.at(x1, y1)? * tx;
        Some(top * (1.0 - ty) + bottom * ty)
    }
}

fn raster_type(keys: &[u16]) -> Option<u16> {
    // Header: version, revision, minor, number of keys; then 4 u16 per key.
    let n = usize::from(*keys.get(3)?);
    (0..n).find_map(|k| {
        let e = keys.get(4 + 4 * k..8 + 4 * k)?;
        (e[0] == RASTER_TYPE_KEY && e[1] == 0).then_some(e[3])
    })
}

/// Several tiles; the first that has a value answers. Tiles are kept in the
/// order given (sorted by path by the caller) for determinism.
pub struct Mosaic(pub Vec<Tile>);

impl Elevation for Mosaic {
    fn elevation(&self, lon: f64, lat: f64) -> Option<f64> {
        self.0.iter().find_map(|t| t.elevation(lon, lat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_inside_and_none_outside() {
        let t = Tile::from_grid(2, 2, vec![0.0, 10.0, 20.0, 30.0], 35.0, 33.0, 1.0, 1.0).unwrap();
        assert_eq!(t.elevation(35.0, 33.0), Some(0.0));
        assert_eq!(t.elevation(35.5, 32.5), Some(15.0));
        assert_eq!(t.elevation(34.9, 33.0), None);
    }

    #[test]
    fn geotiff_round_trip() {
        use tiff::encoder::{colortype::GrayI16, TiffEncoder};
        let dir = std::env::temp_dir().join(format!("mapgen-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n33e035.tif");
        let (w, h) = (4u32, 3u32);
        let data: Vec<i16> = (0..(w * h) as i16).map(|v| v * 10).collect();
        {
            let mut enc = TiffEncoder::new(File::create(&path).unwrap()).unwrap();
            let mut img = enc.new_image::<GrayI16>(w, h).unwrap();
            // PixelIsArea: tiepoint at the top-left corner of pixel (0, 0).
            img.encoder()
                .write_tag(Tag::ModelTiepointTag, &[0.0, 0.0, 0.0, 35.0, 33.0, 0.0][..])
                .unwrap();
            img.encoder()
                .write_tag(Tag::ModelPixelScaleTag, &[0.25, 0.5, 0.0][..])
                .unwrap();
            img.encoder()
                .write_tag(
                    Tag::GeoKeyDirectoryTag,
                    &[1u16, 1, 0, 1, RASTER_TYPE_KEY, 0, 1, 1][..],
                )
                .unwrap();
            img.write_data(&data).unwrap();
        }
        let t = Tile::open(&path).unwrap();
        // Centre of pixel (1, 1): lon 35 + 1.5·0.25, lat 33 − 1.5·0.5 → value 50.
        assert_eq!(t.elevation(35.375, 32.25), Some(50.0));
        std::fs::remove_dir_all(dir).ok();
    }
}
