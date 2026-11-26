//! `sim supply-reach`: how far a side's steady supply (≥ 4) reaches over a
//! scenario map, without an HQ, with the R-SUP-02 HQ bonus and with an HQ as
//! a supply node (R-SUP-06), the HQ pushed to the edge of steady supply.
//! The whole map is taken as the side's (a breakthrough with no enemy left),
//! so only terrain, roads and canals limit supply.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use ceasefire_core::hex::Hex;
use ceasefire_core::movement::Occupancy;
use ceasefire_core::scenario::{axial_to_offset, load_scenario};
use ceasefire_core::side::SideId;
use ceasefire_core::supply::{compute_supply, HqPosition, SupplyInput, SupplyParams, SupplySource};

use crate::combat_matrix::load;

pub struct Options {
    data: PathBuf,
    scenario: String,
    side: u8,
    /// Steady supply: at least this.
    min: u8,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        data: PathBuf::from("data"),
        scenario: "sinai67".into(),
        side: 0,
        min: 4,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--data" => o.data = PathBuf::from(value),
            "--scenario" => o.scenario = value.clone(),
            "--side" => o.side = value.parse().map_err(|e| format!("--side: {e}"))?,
            "--min" => o.min = value.parse().map_err(|e| format!("--min: {e}"))?,
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

pub fn run(o: &Options) -> Result<(), String> {
    let data = load(&o.data)?;
    let dir = o.data.join("scenarios").join(&o.scenario);
    let read = |n: &str| {
        fs::read_to_string(dir.join(n)).map_err(|e| format!("{}: {e}", dir.join(n).display()))
    };
    let setup = load_scenario(&read("scenario.json")?, &read("map.json")?, None)
        .map_err(|e| e.to_string())?;
    let side = SideId(o.side);
    let sources: Vec<SupplySource> = setup
        .sources
        .iter()
        .filter(|s| setup.control.get(&s.hex) == Some(&side))
        .copied()
        .collect();
    if sources.is_empty() {
        return Err(format!("side {} has no supply sources", o.side));
    }
    let control: BTreeMap<Hex, SideId> = setup.map.hexes().map(|h| (h, side)).collect();
    let hex_km = setup.hex_km.unwrap_or(data.rules.scale.default_hex_km);
    let base_params = SupplyParams::for_scale(&data.rules.supply, hex_km, setup.supply_decay_ap)
        .map_err(|e| e.to_string())?;
    let occupancy = Occupancy::new();
    let supply = |p: &SupplyParams, hqs: &[HqPosition]| {
        compute_supply(&SupplyInput {
            map: &setup.map,
            terrain: &data.terrain,
            params: p,
            side,
            sources: &sources,
            occupancy: &occupancy,
            control: &control,
            hqs,
        })
        .map_err(|e| e.to_string())
    };
    let dist = |h: Hex| sources.iter().map(|s| s.hex.distance(h)).min().unwrap_or(0);
    // The farthest hex with steady supply (ties: lower hex).
    let edge = |m: &ceasefire_core::supply::SupplyMap| {
        m.iter()
            .filter(|&(_, v)| v >= o.min)
            .map(|(h, v)| (dist(h), std::cmp::Reverse(h), v))
            .max()
            .map(|(d, std::cmp::Reverse(h), v)| (d, h, v))
            .unwrap_or((0, Hex::new(0, 0), 0))
    };
    let steady =
        |m: &ceasefire_core::supply::SupplyMap| m.iter().filter(|&(_, v)| v >= o.min).count();
    let mut node_params = base_params;
    node_params.hq_node = true;
    let none = supply(&base_params, &[])?;
    let (d0, front, s_front) = edge(&none);
    let hq_at = |h: Hex| {
        [HqPosition {
            hex: h,
            rating: data.rules.hq.rating_base,
            strength: data.rules.unit.max_strength,
        }]
    };
    // Breadth: the HQ in the hex of steady supply that adds the most steady hexes.
    let mut best: Option<(usize, Hex)> = None;
    for (h, v) in none.iter() {
        if v < o.min {
            continue;
        }
        let n = steady(&supply(&node_params, &hq_at(h))?);
        if best.is_none_or(|(b, bh)| n > b || (n == b && h < bh)) {
            best = Some((n, h));
        }
    }
    let (n_node, h_best) = best.unwrap_or((0, front));
    let n_bonus = steady(&supply(&base_params, &hq_at(h_best))?);
    let (d_bonus, _, _) = edge(&supply(&base_params, &hq_at(front))?);
    let (d_node, h_node, _) = edge(&supply(&node_params, &hq_at(front))?);
    let mut out = String::new();
    let off = |h: Hex| axial_to_offset(h);
    let _ = writeln!(
        out,
        "«{}», сторона {}: снабжение ≥ {} (вся карта своя, противника нет)\n",
        o.scenario, o.side, o.min
    );
    let _ = writeln!(
        out,
        "| вариант | дальность, гексов от источника | гексов со снабжением ≥ {} |\n| --- | --- | --- |",
        o.min
    );
    let _ = writeln!(
        out,
        "| без штаба | {d0} (до {:?}, снабжение {s_front}) | {} |",
        off(front),
        steady(&none)
    );
    let _ = writeln!(
        out,
        "| штаб, бонус R-SUP-02 | {d_bonus} (штаб в {:?}) | {n_bonus} (штаб в {:?}) |",
        off(front),
        off(h_best)
    );
    let _ = writeln!(
        out,
        "| штаб, узел R-SUP-06 | {d_node} (до {:?}) | {n_node} (штаб в {:?}) |",
        off(h_node),
        off(h_best)
    );
    print!("{out}");
    Ok(())
}
