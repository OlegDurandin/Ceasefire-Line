//! Local equirectangular projection around the bbox centre and the flat-top,
//! odd-q hex grid laid over the bbox (same conventions as the core).

/// Degrees: west, south, east, north.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl BBox {
    pub fn parse(s: &str) -> Result<Self, String> {
        let v: Vec<f64> = s
            .split(',')
            .map(|x| x.trim().parse::<f64>().map_err(|e| format!("bbox: {e}")))
            .collect::<Result<_, _>>()?;
        let [west, south, east, north] = v[..] else {
            return Err("bbox: need west,south,east,north".into());
        };
        if !(west < east && south < north) {
            return Err("bbox: west < east and south < north required".into());
        }
        Ok(Self {
            west,
            south,
            east,
            north,
        })
    }
}

/// Kilometres east (x) and south (y) of the bbox's north-west corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Km {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Projection {
    bbox: BBox,
    km_per_deg_lat: f64,
    km_per_deg_lon: f64,
}

impl Projection {
    pub fn new(bbox: BBox, earth_radius_km: f64) -> Self {
        let km_per_deg_lat = earth_radius_km * std::f64::consts::PI / 180.0;
        let lat0 = ((bbox.south + bbox.north) / 2.0).to_radians();
        Self {
            bbox,
            km_per_deg_lat,
            km_per_deg_lon: km_per_deg_lat * lat0.cos(),
        }
    }

    pub fn size_km(self) -> Km {
        self.to_km(self.bbox.east, self.bbox.south)
    }

    pub fn to_km(self, lon: f64, lat: f64) -> Km {
        Km {
            x: (lon - self.bbox.west) * self.km_per_deg_lon,
            y: (self.bbox.north - lat) * self.km_per_deg_lat,
        }
    }

    pub fn to_lonlat(self, p: Km) -> (f64, f64) {
        (
            self.bbox.west + p.x / self.km_per_deg_lon,
            self.bbox.north - p.y / self.km_per_deg_lat,
        )
    }
}

const SQRT3: f64 = 1.732_050_807_568_877_2;

/// Hex grid fitted into the bbox: `hex_km` between neighbouring centres.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    /// Corner radius R (km); neighbour centres are √3·R apart.
    pub radius_km: f64,
    pub cols: u32,
    pub rows: u32,
}

impl Grid {
    pub fn fit(size: Km, hex_km: f64) -> Result<Self, String> {
        let r = hex_km / SQRT3;
        let h = SQRT3 * r;
        // Column c centre x = R + 1.5·R·c; odd columns are shifted down by h/2.
        let cols = ((size.x - 2.0 * r) / (1.5 * r)).floor() as i64 + 1;
        let rows = ((size.y - h - h / 2.0) / h).floor() as i64 + 1;
        if cols < 1 || rows < 1 {
            return Err("bbox is smaller than one hex".into());
        }
        Ok(Self {
            radius_km: r,
            cols: cols as u32,
            rows: rows as u32,
        })
    }

    /// Centre of the hex at offset [col, row], in km from the NW corner.
    pub fn center(&self, col: u32, row: u32) -> Km {
        let r = self.radius_km;
        let h = SQRT3 * r;
        let shift = if col % 2 == 1 { h / 2.0 } else { 0.0 };
        Km {
            x: r + 1.5 * r * f64::from(col),
            y: h / 2.0 + h * f64::from(row) + shift,
        }
    }

    /// Is `p` inside the flat-top hexagon centred at `c`?
    pub fn contains(&self, c: Km, p: Km) -> bool {
        let (dx, dy) = ((p.x - c.x).abs(), (p.y - c.y).abs());
        let r = self.radius_km;
        let h = SQRT3 * r / 2.0;
        dy <= h && SQRT3 * dx + dy <= SQRT3 * r
    }

    /// n × n lattice over the hex's bounding box, inside the hexagon only.
    pub fn samples(&self, col: u32, row: u32, n: u32) -> Vec<Km> {
        let c = self.center(col, row);
        let r = self.radius_km;
        let h = SQRT3 * r / 2.0;
        let mut out = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let fx = f64::from(i) / f64::from(n - 1) * 2.0 - 1.0;
                let fy = f64::from(j) / f64::from(n - 1) * 2.0 - 1.0;
                let p = Km {
                    x: c.x + fx * r,
                    y: c.y + fy * h,
                };
                if self.contains(c, p) {
                    out.push(p);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golan_bbox_fits_about_22_by_18() {
        let bbox = BBox::parse("35.45,32.60,36.50,33.45").unwrap();
        let p = Projection::new(bbox, 6371.0);
        let g = Grid::fit(p.size_km(), 5.0).unwrap();
        assert_eq!((g.cols, g.rows), (22, 18));
    }

    #[test]
    fn neighbour_centres_are_hex_km_apart() {
        let g = Grid {
            radius_km: 5.0 / SQRT3,
            cols: 4,
            rows: 4,
        };
        for (a, b) in [
            ((0, 0), (1, 0)),
            ((1, 0), (2, 0)),
            ((0, 0), (0, 1)),
            ((1, 1), (2, 1)),
        ] {
            let (p, q) = (g.center(a.0, a.1), g.center(b.0, b.1));
            assert!(
                ((p.x - q.x).hypot(p.y - q.y) - 5.0).abs() < 1e-9,
                "{a:?}-{b:?}"
            );
            let ax =
                |c: u32, r: u32| ceasefire_core::scenario::offset_to_axial([c as i32, r as i32]);
            assert!(
                ax(a.0, a.1).is_adjacent(ax(b.0, b.1)),
                "core agrees they are neighbours"
            );
        }
    }

    #[test]
    fn projection_round_trip() {
        let p = Projection::new(BBox::parse("35,32,36,33").unwrap(), 6371.0);
        let km = p.to_km(35.4, 32.3);
        let (lon, lat) = p.to_lonlat(km);
        assert!((lon - 35.4).abs() < 1e-12 && (lat - 32.3).abs() < 1e-12);
    }
}
