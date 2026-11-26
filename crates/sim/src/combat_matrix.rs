//! `sim combat-matrix [--data DIR] [--runs N] [--seed S] [--strength N] [--readiness PCT]`
//!
//! Every ground attacker class vs every ground defender class, on every
//! terrain, at entrenchment 0/2/4 (capped by terrain) and tech levels 1/2.
//! Prints CSV: forecast and Monte-Carlo means, plus FREE / SUICIDE flags
//! (GDD section 17: no attack should be always free or always suicidal).

use std::fs;
use std::path::{Path, PathBuf};

use ceasefire_core::combat::{
    forecast_ground, resolve_ground, Combatant, ExchangeForecast, GroundAttack,
};
use ceasefire_core::data::{GameData, GameDataJson, UnitTag};
use ceasefire_core::rng::Pcg32;
use ceasefire_core::tech::{TechBranch, TechLevels};

/// Share of runs that makes an attack "always" free or suicidal.
pub const FLAG_SHARE: f64 = 0.9;
const ENTRENCH_LEVELS: [u8; 3] = [0, 2, 4];
const TECH_LEVELS: [u8; 2] = [1, 2];

pub struct Options {
    data: PathBuf,
    runs: u32,
    seed: u64,
    strength: u8,
    readiness: u8,
}

fn usage() -> String {
    "usage: sim combat-matrix [--data DIR] [--runs N] [--seed S] [--strength N] [--readiness PCT]"
        .to_owned()
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut opts = Options {
        data: PathBuf::from("data"),
        runs: 10_000,
        seed: 1,
        strength: 10,
        readiness: 80,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))?;
        let bad = |e: std::num::ParseIntError| format!("{flag}: {e}");
        match flag.as_str() {
            "--data" => opts.data = PathBuf::from(value),
            "--runs" => opts.runs = value.parse().map_err(bad)?,
            "--seed" => opts.seed = value.parse().map_err(bad)?,
            "--strength" => opts.strength = value.parse().map_err(bad)?,
            "--readiness" => opts.readiness = value.parse().map_err(bad)?,
            _ => return Err(format!("unknown option {flag}\n{}", usage())),
        }
    }
    Ok(opts)
}

pub fn load(dir: &Path) -> Result<GameData, String> {
    let read = |name: &str| {
        let path = dir.join(name);
        fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let (rules, terrain, units, tech) = (
        read("rules.json")?,
        read("terrain.json")?,
        read("units.json")?,
        read("tech.json")?,
    );
    GameData::from_json(GameDataJson {
        rules: &rules,
        terrain: &terrain,
        units: &units,
        tech: &tech,
    })
    .map_err(|e| e.to_string())
}

pub fn tech_at(level: u8) -> TechLevels {
    [
        TechBranch::Armor,
        TechBranch::Antitank,
        TechBranch::AirDefense,
        TechBranch::Aviation,
        TechBranch::Ew,
    ]
    .into_iter()
    .fold(TechLevels::default(), |t, b| t.with(b, level))
}

/// One attacker × defender × terrain × entrenchment × tech cell.
pub struct MatrixRow {
    pub tech: u8,
    pub attacker: String,
    pub defender: String,
    pub terrain: String,
    pub entrench: u8,
    pub forecast: ExchangeForecast,
    pub mc_def_loss: f64,
    pub mc_att_loss: f64,
    /// Share of runs where only the defender lost steps.
    pub free_share: f64,
    /// Share of runs where only the attacker lost steps.
    pub suicide_share: f64,
}

impl MatrixRow {
    pub fn flag(&self) -> &'static str {
        if self.free_share >= FLAG_SHARE {
            "FREE"
        } else if self.suicide_share >= FLAG_SHARE {
            "SUICIDE"
        } else {
            ""
        }
    }
}

/// Ground attackers (not artillery) × ground defenders on every terrain, at
/// entrenchment 0/2/4 (capped) and tech levels 1/2, forecast + Monte-Carlo.
pub fn matrix(
    data: &GameData,
    runs: u32,
    seed: u64,
    strength: u8,
    readiness: u8,
) -> Result<Vec<MatrixRow>, String> {
    let rules = &data.rules.combat;
    let attackers: Vec<_> = data
        .units
        .iter()
        .filter(|u| u.target_class.is_ground() && u.strikes > 0 && !u.has_tag(UnitTag::Bombard))
        .collect();
    let defenders: Vec<_> = data
        .units
        .iter()
        .filter(|u| u.target_class.is_ground())
        .collect();
    let mut out = Vec::new();
    for level in TECH_LEVELS {
        let tech = tech_at(level);
        for att_class in &attackers {
            for def_class in &defenders {
                for terrain in data.terrain.iter().filter(|t| !t.def.impassable) {
                    let max_e = terrain.def.max_entrench;
                    let mut levels: Vec<u8> =
                        ENTRENCH_LEVELS.iter().map(|&e| e.min(max_e)).collect();
                    levels.dedup();
                    for entrench in levels {
                        let combatant = |class, entrenchment| -> Result<Combatant<'_>, String> {
                            Ok(Combatant {
                                class,
                                stats: data
                                    .tech
                                    .unit_stats(class, &tech)
                                    .map_err(|e| e.to_string())?,
                                strength,
                                readiness_pct: readiness,
                                exp_stars: 0,
                                entrenchment,
                            })
                        };
                        let attacker = combatant(att_class, 0)?;
                        let defender = combatant(def_class, entrench)?;
                        let ctx = GroundAttack {
                            terrain_mult: terrain.def.defense_mult,
                            night: false,
                            across_canal: false,
                            attack_mult: 1.0,
                        };
                        let forecast = forecast_ground(rules, &attacker, &defender, &ctx)
                            .map_err(|e| e.to_string())?;
                        let mut rng = Pcg32::new(seed, out.len() as u64);
                        let (mut def_sum, mut att_sum, mut free, mut suicide) =
                            (0u64, 0u64, 0u32, 0u32);
                        for _ in 0..runs {
                            let o = resolve_ground(rules, &attacker, &defender, &ctx, &mut rng)
                                .map_err(|e| e.to_string())?;
                            def_sum += u64::from(o.defender_losses);
                            att_sum += u64::from(o.attacker_losses);
                            free += u32::from(o.attacker_losses == 0 && o.defender_losses > 0);
                            suicide += u32::from(o.defender_losses == 0 && o.attacker_losses > 0);
                        }
                        let n = f64::from(runs.max(1));
                        out.push(MatrixRow {
                            tech: level,
                            attacker: att_class.id.clone(),
                            defender: def_class.id.clone(),
                            terrain: terrain.def.id.clone(),
                            entrench,
                            forecast,
                            mc_def_loss: def_sum as f64 / n,
                            mc_att_loss: att_sum as f64 / n,
                            free_share: f64::from(free) / n,
                            suicide_share: f64::from(suicide) / n,
                        });
                    }
                }
            }
        }
    }
    Ok(out)
}

pub fn combat_matrix(opts: &Options) -> Result<(), String> {
    let data = load(&opts.data)?;
    let rows = matrix(&data, opts.runs, opts.seed, opts.strength, opts.readiness)?;
    println!(
        "tech,attacker,defender,terrain,entrench,n_att,p_att,e_def_loss,n_def,p_def,e_att_loss,\
         mc_def_loss,mc_att_loss,free_share,suicide_share,flag"
    );
    for r in &rows {
        let x = r.forecast.exchange;
        println!(
            "{},{},{},{},{},{},{:.3},{:.2},{},{:.3},{:.2},{:.2},{:.2},{:.3},{:.3},{}",
            r.tech,
            r.attacker,
            r.defender,
            r.terrain,
            r.entrench,
            x.vs_defender.n,
            x.vs_defender.p,
            r.forecast.defender_losses.expected,
            x.vs_attacker.n,
            x.vs_attacker.p,
            r.forecast.attacker_losses.expected,
            r.mc_def_loss,
            r.mc_att_loss,
            r.free_share,
            r.suicide_share,
            r.flag(),
        );
    }
    let flagged = rows.iter().filter(|r| !r.flag().is_empty()).count();
    eprintln!(
        "{} rows, {flagged} flagged (FREE/SUICIDE at {FLAG_SHARE} share of runs)",
        rows.len()
    );
    Ok(())
}
