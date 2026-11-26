//! `mapgen roads`: the historical road layer of a map (R-UI-01).
//!
//! `map.json` keeps `historical_roads` — roads as lon/lat polylines, each with
//! its `source` (map, sheet, edition) or `"assumption": true`, a class
//! (`primary`, `secondary`, `track` = dirt road) and optional years of use
//! `from`/`to` — apart from the OSM-derived data. A scenario takes the roads
//! that exist in its year; tracks go to `tracks` (R-MOVE-01). `--use historical` derives the rules' road sides,
//! bridges (road over river/canal) and passes (mountain hex with a road) from
//! the layer and keeps the OSM ones in `osm_backup`; `--use osm` brings them
//! back; `--remove historical` also deletes the layer. One action each.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::features::{self, Off};
use crate::geo::{Grid, Projection};
use crate::osm::Line;

pub const FROM_HISTORICAL: &str = "historical_roads";

/// Whether a road of the layer exists in `year` (no year: always).
fn in_use(r: &Value, year: Option<i64>) -> bool {
    let Some(y) = year else { return true };
    r["from"].as_i64().is_none_or(|f| f <= y) && r["to"].as_i64().is_none_or(|t| y <= t)
}

/// The layer's polylines in use in `year` as `Line`s (`kind` = road class).
pub fn layer_lines(map: &Value, year: Option<i64>) -> Result<Vec<Line>, String> {
    let mut out = Vec::new();
    for (k, r) in map["historical_roads"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let id = r["id"].as_str().unwrap_or("?");
        if r["source"].is_null() && r["assumption"] != json!(true) {
            return Err(format!(
                "historical_roads[{k}] `{id}`: needs a source or \"assumption\": true"
            ));
        }
        let pts = r["pts"]
            .as_array()
            .ok_or_else(|| format!("historical_roads[{k}] `{id}`: no pts"))?
            .iter()
            .map(|p| match (p[0].as_f64(), p[1].as_f64()) {
                (Some(lon), Some(lat)) => Ok((lon, lat)),
                _ => Err(format!("historical_roads[{k}] `{id}`: bad point {p}")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if pts.len() < 2 {
            return Err(format!("historical_roads[{k}] `{id}`: fewer than 2 points"));
        }
        if !in_use(r, year) {
            continue;
        }
        out.push(Line {
            kind: r["class"].as_str().unwrap_or("primary").to_owned(),
            pts,
        });
    }
    Ok(out)
}

/// Hex sides of a list of hex polylines (`[[c, r], …]`, pairs included).
fn edge_list(v: &Value) -> BTreeSet<[Off; 2]> {
    let mut out = BTreeSet::new();
    for line in v.as_array().into_iter().flatten() {
        let hexes: Vec<Off> = line
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| Some([h[0].as_i64()? as i32, h[1].as_i64()? as i32]))
            .collect();
        for w in hexes.windows(2) {
            out.insert(if w[0] <= w[1] {
                [w[0], w[1]]
            } else {
                [w[1], w[0]]
            });
        }
    }
    out
}

/// Applies `use` (historical | osm) or removes the layer; returns a summary.
pub fn apply(
    map: &mut Value,
    grid: &Grid,
    proj: &Projection,
    action: &str,
    pass_char: char,
    year: Option<i64>,
    auto_bridges: bool,
) -> Result<String, String> {
    let using_hist = map["roads_from"].as_str() == Some(FROM_HISTORICAL);
    let obj = map.as_object_mut().ok_or("map.json is not an object")?;
    let restore = |obj: &mut serde_json::Map<String, Value>| {
        if let Some(b) = obj.remove("osm_backup") {
            for k in ["roads", "tracks", "bridges", "terrain"] {
                match b.get(k) {
                    Some(v) => {
                        obj.insert(k.to_owned(), v.clone());
                    }
                    None => {
                        obj.remove(k);
                    }
                }
            }
        }
        obj.remove("roads_from");
    };
    match action {
        "osm" => {
            restore(obj);
            Ok("roads: OSM".into())
        }
        "remove" => {
            restore(obj);
            let n = obj
                .remove("historical_roads")
                .and_then(|v| v.as_array().map(Vec::len))
                .unwrap_or(0);
            Ok(format!("historical layer removed ({n} roads); roads: OSM"))
        }
        "historical" => {
            let lines = layer_lines(&Value::Object(obj.clone()), year)?;
            if lines.is_empty() {
                return Err("map.json has no historical_roads".into());
            }
            if using_hist {
                restore(obj);
            }
            // Only the keys the map has, so switching back restores it exactly.
            let backup: serde_json::Map<String, Value> = ["roads", "tracks", "bridges", "terrain"]
                .into_iter()
                .filter_map(|k| Some((k.to_owned(), obj.get(k)?.clone())))
                .collect();
            let (track_lines, road_lines): (Vec<Line>, Vec<Line>) =
                lines.iter().cloned().partition(|l| l.kind == "track");
            let roads = features::road_edges(grid, proj, &road_lines);
            // A side with a road is not also a track.
            let tracks: BTreeSet<[Off; 2]> = features::road_edges(grid, proj, &track_lines)
                .difference(&roads)
                .copied()
                .collect();
            // Canal sides are pairs of hexes: the same parser as polylines.
            let canal_pairs = edge_list(obj.get("canals").unwrap_or(&json!([])));
            let crossings: BTreeSet<[Off; 2]> = roads.union(&tracks).copied().collect();
            // Without auto bridges (Suez 1967: ferries) the crossings stay open water.
            let bridges: Vec<[Off; 2]> = if auto_bridges {
                crossings.intersection(&canal_pairs).copied().collect()
            } else {
                Vec::new()
            };
            // Passes: a mountain hex with a historical road (existing passes stay).
            let legend = obj.get("legend").cloned().unwrap_or(json!({}));
            let mountain: BTreeSet<char> = legend
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(_, v)| v["terrain"] == "mountains" && v["pass"] != json!(true))
                .filter_map(|(k, _)| k.chars().next())
                .collect();
            let mut rows: Vec<Vec<char>> = obj
                .get("terrain")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|r| r.as_str().unwrap_or("").chars().collect())
                .collect();
            let mut passes = 0;
            for [c, r] in crossings.iter().flat_map(|e| e.iter().copied()) {
                if let Some(ch) = rows
                    .get_mut(r as usize)
                    .and_then(|row| row.get_mut(c as usize))
                {
                    if mountain.contains(ch) {
                        *ch = pass_char;
                        passes += 1;
                    }
                }
            }
            obj.insert(
                "terrain".into(),
                json!(rows
                    .iter()
                    .map(|r| r.iter().collect::<String>())
                    .collect::<Vec<_>>()),
            );
            obj.insert(
                "roads".into(),
                json!(roads.iter().map(|[a, b]| vec![*a, *b]).collect::<Vec<_>>()),
            );
            obj.insert(
                "tracks".into(),
                json!(tracks.iter().map(|[a, b]| vec![*a, *b]).collect::<Vec<_>>()),
            );
            obj.insert("bridges".into(), json!(bridges));
            obj.insert("roads_from".into(), json!(FROM_HISTORICAL));
            obj.insert("osm_backup".into(), Value::Object(backup));
            Ok(format!(
                "roads: historical layer{}, {} roads → {} road sides + {} track sides, {} bridges, {passes} new passes",
                year.map_or(String::new(), |y| format!(" for {y}")),
                lines.len(),
                roads.len(),
                tracks.len(),
                bridges.len()
            ))
        }
        other => Err(format!("unknown roads action `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::BBox;

    fn grid() -> (Grid, Projection) {
        let bbox = BBox::parse("35.45,32.60,36.50,33.45").unwrap();
        let proj = Projection::new(bbox, 6371.0);
        (Grid::fit(proj.size_km(), 5.0).unwrap(), proj)
    }

    fn map() -> Value {
        json!({
            "legend": { "p": { "terrain": "plain" }, "m": { "terrain": "mountains" }, "P": { "terrain": "mountains", "pass": true } },
            "terrain": ["pppp", "pmpp"],
            "roads": [[[0, 0], [1, 0]]],
            "canals": [],
            "bridges": [],
            "historical_roads": [
                { "id": "a", "name": "A", "class": "primary", "source": { "map": "M", "sheet": "S", "edition": "1960" }, "pts": [[35.47, 33.43, "x"], [35.62, 33.43]] },
                { "id": "b", "name": "B", "class": "secondary", "assumption": true, "note": "n", "pts": [[35.47, 33.38], [35.52, 33.36]] }
            ]
        })
    }

    #[test]
    fn use_and_switch_back_in_one_action() {
        let (g, p) = grid();
        let mut m = map();
        let before = m.clone();
        let msg = apply(&mut m, &g, &p, "historical", 'P', None, true).unwrap();
        assert!(msg.contains("2 roads"), "{msg}");
        assert_eq!(m["roads_from"], FROM_HISTORICAL);
        assert_ne!(m["roads"], before["roads"]);
        apply(&mut m, &g, &p, "osm", 'P', None, true).unwrap();
        assert_eq!(m["roads"], before["roads"]);
        assert_eq!(m["terrain"], before["terrain"]);
        assert!(m.get("osm_backup").is_none() && m.get("roads_from").is_none());
        assert!(m["historical_roads"].is_array(), "the layer itself stays");
        apply(&mut m, &g, &p, "historical", 'P', None, true).unwrap();
        apply(&mut m, &g, &p, "remove", 'P', None, true).unwrap();
        assert_eq!(
            m,
            before
                .as_object()
                .map(|o| {
                    let mut o = o.clone();
                    o.remove("historical_roads");
                    Value::Object(o)
                })
                .unwrap()
        );
    }

    #[test]
    fn years_pick_the_roads_of_the_scenario_and_tracks_go_apart() {
        let (g, p) = grid();
        let mut m = map();
        m["historical_roads"][1]["class"] = json!("track");
        m["historical_roads"][1]["to"] = json!(1969);
        m["historical_roads"].as_array_mut().unwrap().push(json!({
            "id": "c", "name": "C", "class": "secondary", "from": 1970, "assumption": true,
            "pts": [[35.47, 33.38], [35.52, 33.36]]
        }));
        let kinds = |y| -> Vec<String> {
            layer_lines(&m, Some(y))
                .unwrap()
                .into_iter()
                .map(|l| l.kind)
                .collect()
        };
        assert_eq!(kinds(1967), ["primary", "track"]);
        assert_eq!(kinds(1973), ["primary", "secondary"]);
        let mut y67 = m.clone();
        apply(&mut y67, &g, &p, "historical", 'P', Some(1967), true).unwrap();
        assert!(
            !y67["tracks"].as_array().unwrap().is_empty(),
            "1967: the dirt road is a track"
        );
        let mut y73 = m.clone();
        apply(&mut y73, &g, &p, "historical", 'P', Some(1973), true).unwrap();
        assert!(y73["tracks"].as_array().unwrap().is_empty(), "1973: paved");
        apply(&mut y73, &g, &p, "osm", 'P', None, true).unwrap();
        assert!(y73["tracks"].as_array().is_none_or(Vec::is_empty));
    }

    #[test]
    fn every_road_needs_a_source_or_an_assumption_mark() {
        let mut m = map();
        m["historical_roads"][1]["assumption"] = json!(false);
        assert!(layer_lines(&m, None).unwrap_err().contains("`b`"));
    }
}
