//! OSM features on the hex grid: area shares, roads across hex sides,
//! rivers along hex sides, bridges, passes, places (R-UI-01).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use ceasefire_core::hex::Hex;
use ceasefire_core::scenario::axial_to_offset;

use crate::geo::{Grid, Km, Projection};
use crate::osm::{Area, Line};

pub type Off = [i32; 2];
const SQRT3: f64 = 1.732_050_807_568_877_2;

/// Axial hex containing a km point (unbounded).
pub(crate) fn axial_at(grid: &Grid, p: Km) -> Hex {
    let r = grid.radius_km;
    let (x, y) = (p.x - r, p.y - SQRT3 * r / 2.0);
    let fq = (2.0 / 3.0 * x) / r;
    let fr = (-1.0 / 3.0 * x + SQRT3 / 3.0 * y) / r;
    let fs = -fq - fr;
    let (mut q, mut rr, s) = (fq.round(), fr.round(), fs.round());
    let (dq, dr, ds) = ((q - fq).abs(), (rr - fr).abs(), (s - fs).abs());
    if dq > dr && dq > ds {
        q = -rr - s;
    } else if dr > ds {
        rr = -q - s;
    }
    Hex::new(q as i32, rr as i32)
}

fn on_grid(grid: &Grid, [c, r]: Off) -> bool {
    c >= 0 && r >= 0 && (c as u32) < grid.cols && (r as u32) < grid.rows
}

/// Offset hex containing a km point, if it is on the grid.
pub fn hex_at(grid: &Grid, p: Km) -> Option<Off> {
    let off = axial_to_offset(axial_at(grid, p));
    on_grid(grid, off).then_some(off)
}

fn canonical(a: Off, b: Off) -> [Off; 2] {
    if a <= b {
        [a, b]
    } else {
        [b, a]
    }
}

/// Area segments projected to km, with a bounding box for quick rejects.
pub struct AreaKm {
    pub terrain: String,
    segments: Vec<[Km; 2]>,
    min: Km,
    max: Km,
}

impl AreaKm {
    pub fn new(a: &Area, proj: &Projection) -> Self {
        let segments: Vec<[Km; 2]> = a
            .segments
            .iter()
            .map(|[p, q]| [proj.to_km(p.0, p.1), proj.to_km(q.0, q.1)])
            .collect();
        let pts = segments.iter().flatten();
        let min = pts.clone().fold(
            Km {
                x: f64::INFINITY,
                y: f64::INFINITY,
            },
            |m, p| Km {
                x: m.x.min(p.x),
                y: m.y.min(p.y),
            },
        );
        let max = pts.fold(
            Km {
                x: f64::NEG_INFINITY,
                y: f64::NEG_INFINITY,
            },
            |m, p| Km {
                x: m.x.max(p.x),
                y: m.y.max(p.y),
            },
        );
        Self {
            terrain: a.terrain.clone(),
            segments,
            min,
            max,
        }
    }

    /// Even-odd rule over all segments (multipolygons need no ring assembly).
    pub fn contains(&self, p: Km) -> bool {
        if p.x < self.min.x || p.x > self.max.x || p.y < self.min.y || p.y > self.max.y {
            return false;
        }
        let mut inside = false;
        for [a, b] in &self.segments {
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if x > p.x {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

/// Share of the hex's samples inside areas of each terrain.
pub fn area_shares(
    grid: &Grid,
    areas: &[AreaKm],
    col: u32,
    row: u32,
    n: u32,
) -> BTreeMap<String, f64> {
    let samples = grid.samples(col, row, n);
    let mut hits: BTreeMap<String, usize> = BTreeMap::new();
    for p in &samples {
        let mut seen = BTreeSet::new();
        for a in areas {
            if !seen.contains(&a.terrain) && a.contains(*p) {
                seen.insert(a.terrain.clone());
                *hits.entry(a.terrain.clone()).or_insert(0) += 1;
            }
        }
    }
    let total = samples.len().max(1) as f64;
    hits.into_iter()
        .map(|(t, k)| (t, k as f64 / total))
        .collect()
}

/// Points along a line in km, at most `step` apart.
fn densify(pts: &[Km], step: f64) -> Vec<Km> {
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (b.x - a.x).hypot(b.y - a.y);
        let k = (d / step).ceil().max(1.0) as usize;
        for i in 0..k {
            let t = i as f64 / k as f64;
            out.push(Km {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            });
        }
    }
    if let Some(&last) = pts.last() {
        out.push(last);
    }
    out
}

/// Hexes on the straight line between two axial hexes (cube lerp).
fn hex_line(a: Hex, b: Hex) -> Vec<Hex> {
    let n = a.distance(b);
    (0..=n)
        .map(|i| {
            let t = if n == 0 {
                0.0
            } else {
                f64::from(i) / f64::from(n)
            };
            let (fq, fr) = (
                f64::from(a.q) + (f64::from(b.q - a.q)) * t + 1e-6,
                f64::from(a.r) + f64::from(b.r - a.r) * t + 1e-6,
            );
            let fs = -fq - fr;
            let (mut q, mut r, s) = (fq.round(), fr.round(), fs.round());
            let (dq, dr, ds) = ((q - fq).abs(), (r - fr).abs(), (s - fs).abs());
            if dq > dr && dq > ds {
                q = -r - s;
            } else if dr > ds {
                r = -q - s;
            }
            Hex::new(q as i32, r as i32)
        })
        .collect()
}

/// Roads cross hex sides: consecutive hexes along the line are joined.
pub fn road_edges(grid: &Grid, proj: &Projection, roads: &[Line]) -> BTreeSet<[Off; 2]> {
    let mut out = BTreeSet::new();
    for line in roads {
        let km: Vec<Km> = line
            .pts
            .iter()
            .map(|&(lon, lat)| proj.to_km(lon, lat))
            .collect();
        let mut hexes: Vec<Hex> = Vec::new();
        for p in densify(&km, grid.radius_km / 4.0) {
            let h = axial_at(grid, p);
            if let Some(&prev) = hexes.last() {
                if prev == h {
                    continue;
                }
                if !prev.is_adjacent(h) {
                    hexes.extend(hex_line(prev, h).into_iter().skip(1).filter(|x| *x != h));
                }
            }
            hexes.push(h);
        }
        for w in hexes.windows(2) {
            let (a, b) = (axial_to_offset(w[0]), axial_to_offset(w[1]));
            if w[0].is_adjacent(w[1]) && on_grid(grid, a) && on_grid(grid, b) {
                out.insert(canonical(a, b));
            }
        }
    }
    out
}

type VKey = (i64, i64);
/// A hex side as the pair of its corner keys.
type SideKey = (VKey, VKey);
/// Best cost to a corner and the corner + side it was reached by.
type Reached = (u64, Option<(VKey, SideKey)>);

fn vkey(p: Km) -> VKey {
    ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64)
}

struct SideInfo {
    a: Km,
    b: Km,
    hexes: (Off, Option<Off>),
}

fn dist_to_polyline(p: Km, line: &[Km]) -> f64 {
    line.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
        })
        .fold(f64::INFINITY, f64::min)
}

/// Rivers run along hex sides: the cheapest path over hex corners between
/// the corners nearest to the line's ends, side cost = length × (1 +
/// weight × distance to the line / R).
pub fn river_edges(
    grid: &Grid,
    proj: &Projection,
    rivers: &[Line],
    min_length_km: f64,
    weight: f64,
) -> BTreeSet<[Off; 2]> {
    let r = grid.radius_km;
    let mut sides: BTreeMap<SideKey, SideInfo> = BTreeMap::new();
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let c = grid.center(col, row);
            let corner = |i: u32| {
                let a = f64::from(i) * std::f64::consts::PI / 3.0;
                Km {
                    x: c.x + r * a.cos(),
                    y: c.y + r * a.sin(),
                }
            };
            for i in 0..6 {
                let (a, b) = (corner(i), corner((i + 1) % 6));
                let mid = Km {
                    x: (a.x + b.x) / 2.0,
                    y: (a.y + b.y) / 2.0,
                };
                let other = hex_at(
                    grid,
                    Km {
                        x: 2.0 * mid.x - c.x,
                        y: 2.0 * mid.y - c.y,
                    },
                );
                let (ka, kb) = (vkey(a), vkey(b));
                let key = if ka <= kb { (ka, kb) } else { (kb, ka) };
                sides.entry(key).or_insert(SideInfo {
                    a,
                    b,
                    hexes: ([col as i32, row as i32], other),
                });
            }
        }
    }
    let mut adj: BTreeMap<VKey, Vec<(VKey, SideKey)>> = BTreeMap::new();
    let mut points: BTreeMap<VKey, Km> = BTreeMap::new();
    for (key, s) in &sides {
        adj.entry(key.0).or_default().push((key.1, *key));
        adj.entry(key.1).or_default().push((key.0, *key));
        points.insert(vkey(s.a), s.a);
        points.insert(vkey(s.b), s.b);
    }
    let size = proj.size_km();
    let mut out = BTreeSet::new();
    for line in rivers {
        // Pieces of the line inside the map.
        let km: Vec<Km> = line
            .pts
            .iter()
            .map(|&(lon, lat)| proj.to_km(lon, lat))
            .collect();
        let mut pieces: Vec<Vec<Km>> = vec![Vec::new()];
        for p in km {
            if p.x >= 0.0 && p.y >= 0.0 && p.x <= size.x && p.y <= size.y {
                if let Some(last) = pieces.last_mut() {
                    last.push(p);
                }
            } else if pieces.last().is_some_and(|l| !l.is_empty()) {
                pieces.push(Vec::new());
            }
        }
        for piece in pieces.into_iter().filter(|p| p.len() >= 2) {
            let length: f64 = piece
                .windows(2)
                .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
                .sum();
            if length < min_length_km {
                continue;
            }
            let nearest = |p: Km| {
                points
                    .iter()
                    .min_by(|x, y| {
                        let dx = (x.1.x - p.x).hypot(x.1.y - p.y);
                        let dy = (y.1.x - p.x).hypot(y.1.y - p.y);
                        dx.total_cmp(&dy).then(x.0.cmp(y.0))
                    })
                    .map(|(k, _)| *k)
            };
            let (Some(start), Some(goal)) = (nearest(piece[0]), nearest(piece[piece.len() - 1]))
            else {
                continue;
            };
            // Dijkstra on integer micro-km costs for a deterministic order.
            let cost_of = |s: &SideInfo| {
                let mid = Km {
                    x: (s.a.x + s.b.x) / 2.0,
                    y: (s.a.y + s.b.y) / 2.0,
                };
                let len = (s.b.x - s.a.x).hypot(s.b.y - s.a.y);
                (len * (1.0 + weight * dist_to_polyline(mid, &piece) / r) * 1e6) as u64
            };
            let mut best: BTreeMap<VKey, Reached> = BTreeMap::from([(start, (0, None))]);
            let mut heap = BinaryHeap::from([Reverse((0u64, start))]);
            while let Some(Reverse((cost, v))) = heap.pop() {
                if v == goal {
                    break;
                }
                if best.get(&v).is_some_and(|&(b, _)| cost > b) {
                    continue;
                }
                for &(next, side) in adj.get(&v).into_iter().flatten() {
                    let c = cost + cost_of(&sides[&side]);
                    if best.get(&next).is_none_or(|&(b, _)| c < b) {
                        best.insert(next, (c, Some((v, side))));
                        heap.push(Reverse((c, next)));
                    }
                }
            }
            let mut v = goal;
            while let Some(&(_, Some((prev, side)))) = best.get(&v) {
                if let (a, Some(b)) = sides[&side].hexes {
                    out.insert(canonical(a, b));
                }
                v = prev;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::BBox;

    fn setup() -> (Grid, Projection) {
        let proj = Projection::new(BBox::parse("35.0,33.0,35.5,33.4").unwrap(), 6371.0);
        (Grid::fit(proj.size_km(), 5.0).unwrap(), proj)
    }

    #[test]
    fn hex_at_inverts_centres() {
        let (g, _) = setup();
        for row in 0..g.rows {
            for col in 0..g.cols {
                assert_eq!(
                    hex_at(&g, g.center(col, row)),
                    Some([col as i32, row as i32])
                );
            }
        }
    }

    #[test]
    fn a_straight_road_joins_neighbours_across_the_map() {
        let (g, p) = setup();
        let road = Line {
            kind: "primary".into(),
            pts: vec![(35.01, 33.2), (35.49, 33.2)],
        };
        let edges = road_edges(&g, &p, &[road]);
        assert!(edges.len() >= g.cols as usize - 2, "{edges:?}");
        for [a, b] in &edges {
            let ax = |o: &Off| ceasefire_core::scenario::offset_to_axial(*o);
            assert!(ax(a).is_adjacent(ax(b)));
        }
    }

    #[test]
    fn a_river_follows_hex_sides() {
        let (g, p) = setup();
        let river = Line {
            kind: "river".into(),
            pts: vec![(35.25, 33.39), (35.25, 33.01)],
        };
        let edges = river_edges(&g, &p, &[river], 5.0, 20.0);
        assert!(edges.len() >= g.rows as usize, "{edges:?}");
        // All sides lie near the river's longitude.
        let x_river = p.to_km(35.25, 33.2).x;
        for [a, b] in &edges {
            let (ca, cb) = (
                g.center(a[0] as u32, a[1] as u32),
                g.center(b[0] as u32, b[1] as u32),
            );
            assert!(
                ((ca.x + cb.x) / 2.0 - x_river).abs() < g.radius_km * 1.6,
                "side {a:?}-{b:?} far from the river"
            );
        }
        let short = Line {
            kind: "river".into(),
            pts: vec![(35.25, 33.2), (35.26, 33.2)],
        };
        assert!(
            river_edges(&g, &p, &[short], 5.0, 20.0).is_empty(),
            "shorter than the minimum"
        );
    }

    #[test]
    fn even_odd_area_with_a_hole() {
        let (_, p) = setup();
        let square = |w: f64, s: f64, e: f64, n: f64| {
            vec![
                [(w, s), (e, s)],
                [(e, s), (e, n)],
                [(e, n), (w, n)],
                [(w, n), (w, s)],
            ]
        };
        let mut segs = square(35.1, 33.1, 35.4, 33.3);
        segs.extend(square(35.2, 33.15, 35.3, 33.25));
        let a = AreaKm::new(
            &Area {
                terrain: "water".into(),
                segments: segs,
            },
            &p,
        );
        assert!(a.contains(p.to_km(35.15, 33.2)));
        assert!(!a.contains(p.to_km(35.25, 33.2)), "inner ring is a hole");
        assert!(!a.contains(p.to_km(35.45, 33.2)));
    }
}
