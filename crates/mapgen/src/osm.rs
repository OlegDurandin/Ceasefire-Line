//! Reading `.osm.pbf` extracts: roads, rivers, areas (closed ways and
//! multipolygons) and places inside the bbox. Three passes — relations,
//! ways, nodes — so only what is needed is kept in memory. Elements are
//! keyed by id, so overlapping extracts (Israel + Syria) do not duplicate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use osmpbf::{Element, ElementReader, RelMemberType};

use crate::config::OsmConfig;
use crate::geo::BBox;

pub type LonLat = (f64, f64);

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// `highway` or `waterway` value.
    pub kind: String,
    pub pts: Vec<LonLat>,
}

/// Area as a soup of segments: a point is inside if a ray crosses an odd
/// number of them (works for multipolygons without assembling rings).
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub terrain: String,
    pub segments: Vec<[LonLat; 2]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub kind: String,
    pub population: Option<u64>,
    pub at: LonLat,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OsmData {
    pub roads: Vec<Line>,
    pub rivers: Vec<Line>,
    pub areas: Vec<Area>,
    pub places: Vec<Place>,
}

fn area_terrain(cfg: &OsmConfig, tags: &[(String, String)]) -> Option<String> {
    cfg.areas
        .iter()
        .find(|rule| rule.tags.iter().any(|t| tags.contains(t)))
        .map(|rule| rule.terrain.clone())
}

fn owned<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<(String, String)> {
    tags.map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

fn tag<'a>(tags: &'a [(String, String)], key: &str) -> Option<&'a str> {
    tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

struct WayRec {
    tags: Vec<(String, String)>,
    refs: Vec<i64>,
}

/// Reads the extracts (sorted by path by the caller) for the bbox.
pub fn read(
    paths: &[std::path::PathBuf],
    bbox: BBox,
    cfg: &OsmConfig,
    secondary_roads: bool,
) -> Result<OsmData, String> {
    let err = |p: &Path, e: osmpbf::Error| format!("{}: {e}", p.display());
    let road_kinds: BTreeSet<&str> = cfg
        .roads
        .iter()
        .chain(if secondary_roads {
            cfg.secondary_roads.iter()
        } else {
            [].iter()
        })
        .map(String::as_str)
        .collect();
    let river_kinds: BTreeSet<&str> = cfg.rivers.iter().map(String::as_str).collect();

    // Pass 1: multipolygon relations of interest → their member ways.
    let mut relations: BTreeMap<i64, (String, Vec<i64>)> = BTreeMap::new();
    for p in paths {
        ElementReader::from_path(p)
            .map_err(|e| err(p, e))?
            .for_each(|el| {
                if let Element::Relation(r) = el {
                    let tags = owned(r.tags());
                    if tag(&tags, "type") != Some("multipolygon") {
                        return;
                    }
                    if let Some(terrain) = area_terrain(cfg, &tags) {
                        let ways = r
                            .members()
                            .filter(|m| m.member_type == RelMemberType::Way)
                            .map(|m| m.member_id)
                            .collect();
                        relations.insert(r.id(), (terrain, ways));
                    }
                }
            })
            .map_err(|e| err(p, e))?;
    }
    let member_ways: BTreeSet<i64> = relations
        .values()
        .flat_map(|(_, w)| w.iter().copied())
        .collect();

    // Pass 2: ways — roads, rivers, closed areas, relation members.
    let mut ways: BTreeMap<i64, WayRec> = BTreeMap::new();
    for p in paths {
        ElementReader::from_path(p)
            .map_err(|e| err(p, e))?
            .for_each(|el| {
                if let Element::Way(w) = el {
                    let tags = owned(w.tags());
                    let wanted = tag(&tags, "highway").is_some_and(|v| road_kinds.contains(v))
                        || tag(&tags, "waterway").is_some_and(|v| river_kinds.contains(v))
                        || area_terrain(cfg, &tags).is_some()
                        || member_ways.contains(&w.id());
                    if wanted {
                        ways.insert(
                            w.id(),
                            WayRec {
                                tags,
                                refs: w.refs().collect(),
                            },
                        );
                    }
                }
            })
            .map_err(|e| err(p, e))?;
    }
    let needed: BTreeSet<i64> = ways.values().flat_map(|w| w.refs.iter().copied()).collect();

    // Pass 3: node coordinates and places.
    let (w, s, e, n) = (
        bbox.west - cfg.margin_deg,
        bbox.south - cfg.margin_deg,
        bbox.east + cfg.margin_deg,
        bbox.north + cfg.margin_deg,
    );
    let inside = |(lon, lat): LonLat| lon >= w && lon <= e && lat >= s && lat <= n;
    let mut nodes: BTreeMap<i64, LonLat> = BTreeMap::new();
    let mut places: BTreeMap<i64, Place> = BTreeMap::new();
    for p in paths {
        ElementReader::from_path(p)
            .map_err(|e| err(p, e))?
            .for_each(|el| {
                let (id, at, tags) = match el {
                    Element::Node(nd) => (nd.id(), (nd.lon(), nd.lat()), owned(nd.tags())),
                    Element::DenseNode(nd) => (nd.id(), (nd.lon(), nd.lat()), owned(nd.tags())),
                    _ => return,
                };
                if needed.contains(&id) {
                    nodes.insert(id, at);
                }
                let kind = tag(&tags, "place");
                if let (Some(kind), Some(name)) = (kind, tag(&tags, "name")) {
                    if cfg.places.iter().any(|k| k == kind) && inside(at) {
                        let population =
                            tag(&tags, "population").and_then(|v| v.replace(',', "").parse().ok());
                        places.insert(
                            id,
                            Place {
                                name: name.to_owned(),
                                kind: kind.to_owned(),
                                population,
                                at,
                            },
                        );
                    }
                }
            })
            .map_err(|e| err(p, e))?;
    }

    let coords = |refs: &[i64]| -> Vec<LonLat> {
        refs.iter().filter_map(|r| nodes.get(r).copied()).collect()
    };
    let near = |pts: &[LonLat]| pts.iter().any(|&p| inside(p));
    // Only between consecutive nodes that are both known: nodes outside the
    // read margin are missing, and joining across them would draw chords.
    let segments = |refs: &[i64]| {
        refs.windows(2)
            .filter_map(|w| Some([*nodes.get(&w[0])?, *nodes.get(&w[1])?]))
            .collect::<Vec<_>>()
    };
    let mut data = OsmData::default();
    for wr in ways.values() {
        let pts = coords(&wr.refs);
        if pts.len() < 2 || !near(&pts) {
            continue;
        }
        if let Some(kind) = tag(&wr.tags, "highway").filter(|v| road_kinds.contains(v)) {
            data.roads.push(Line {
                kind: kind.to_owned(),
                pts: pts.clone(),
            });
        }
        if let Some(kind) = tag(&wr.tags, "waterway").filter(|v| river_kinds.contains(v)) {
            // Only named canals when a list is given (Suez, not irrigation).
            let named = |names: &[String]| {
                wr.tags
                    .iter()
                    .filter(|(k, _)| k == "name" || k.starts_with("name:"))
                    .any(|(_, v)| names.iter().any(|n| v.contains(n.as_str())))
            };
            if kind == "canal" && !cfg.canal_names.is_empty() && !named(&cfg.canal_names) {
                continue;
            }
            data.rivers.push(Line {
                kind: kind.to_owned(),
                pts: pts.clone(),
            });
        }
        let closed = wr.refs.first() == wr.refs.last();
        if let (true, Some(terrain)) = (closed, area_terrain(cfg, &wr.tags)) {
            data.areas.push(Area {
                terrain,
                segments: segments(&wr.refs),
            });
        }
    }
    for (terrain, members) in relations.values() {
        let mut segs = Vec::new();
        let mut all = Vec::new();
        for id in members {
            if let Some(wr) = ways.get(id) {
                segs.extend(segments(&wr.refs));
                all.extend(coords(&wr.refs));
            }
        }
        if segs.is_empty() || !near(&all) {
            continue;
        }
        data.areas.push(Area {
            terrain: terrain.clone(),
            segments: segs,
        });
    }
    data.places = places.into_values().collect();
    drop_excluded_water(&mut data, cfg);
    Ok(data)
}

/// Water areas touching an exclusion box (Lake Hula, drained before 1967)
/// are dropped.
pub fn drop_excluded_water(data: &mut OsmData, cfg: &OsmConfig) {
    let hit = |&(lon, lat): &LonLat| {
        cfg.water_exclude
            .iter()
            .any(|x| lon >= x.bbox[0] && lat >= x.bbox[1] && lon <= x.bbox[2] && lat <= x.bbox[3])
    };
    data.areas
        .retain(|a| a.terrain != "water" || !a.segments.iter().any(|s| hit(&s[0]) || hit(&s[1])));
}
