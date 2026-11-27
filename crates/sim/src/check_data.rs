//! `sim check-data`: loads `data/*.json` and builds a game from every
//! scenario in `data/scenarios/` (the pre-commit check of AGENTS.md).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use ceasefire_core::game::GameState;
use ceasefire_core::scenario::load_scenario;

use crate::combat_matrix::load;

pub struct Options {
    data: PathBuf,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        data: PathBuf::from("data"),
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--data" => o.data = PathBuf::from(value),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

pub fn run(o: &Options) -> Result<(), String> {
    let data = Arc::new(load(&o.data)?);
    let dir = o.data.join("scenarios");
    let mut ids: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("scenario.json").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();
    if ids.is_empty() {
        return Err(format!("no scenarios in {}", dir.display()));
    }
    // The start menu's list (client) names exactly the scenario folders.
    let index = dir.join("index.json");
    let text = fs::read_to_string(&index).map_err(|e| format!("{}: {e}", index.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", index.display()))?;
    let mut listed: Vec<String> = v["scenarios"]
        .as_array()
        .ok_or_else(|| format!("{}: `scenarios` list missing", index.display()))?
        .iter()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect();
    listed.sort();
    if listed != ids {
        return Err(format!(
            "{}: lists {listed:?}, folders are {ids:?}",
            index.display()
        ));
    }
    for id in &ids {
        let d = dir.join(id);
        let read = |n: &str| fs::read_to_string(d.join(n));
        let scenario = read("scenario.json").map_err(|e| format!("{id}/scenario.json: {e}"))?;
        let map = read("map.json").map_err(|e| format!("{id}/map.json: {e}"))?;
        let events = read("events.json").ok();
        let setup =
            load_scenario(&scenario, &map, events.as_deref()).map_err(|e| format!("{id}: {e}"))?;
        let units = setup.units.len();
        GameState::new(Arc::clone(&data), setup).map_err(|e| format!("{id}: {e}"))?;
        println!("{id}: ok ({units} units)");
    }
    println!("data: ok ({} scenarios)", ids.len());
    Ok(())
}
