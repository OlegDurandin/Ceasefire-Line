//! `sim tactical`: the tactical AI against itself on dumped battle snapshots
//! (T-TECH-02 acceptance of T1): the mean outcome percentile should be near
//! 50 (the tactical battle is neither better nor worse than the dice).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ceasefire_ai::tactical::TacticalAi;
use ceasefire_core::tactical::TacticalOutcome;
use ceasefire_tactical::battle::TurnMode;
use ceasefire_tactical::{build_battle, BattleInput, TacCommand, TacRules};

use crate::combat_matrix::load;

const MAX_COMMANDS: usize = 3000;

pub struct Options {
    data: PathBuf,
    snapshots: PathBuf,
    games: u64,
    limit: usize,
    modes: Vec<TurnMode>,
    report: Option<PathBuf>,
    json: Option<PathBuf>,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        data: PathBuf::from("data"),
        snapshots: PathBuf::from("battles"),
        games: 10,
        limit: usize::MAX,
        modes: vec![TurnMode::Alternating, TurnMode::IgougoOverwatch],
        report: None,
        json: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let v = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--data" => o.data = PathBuf::from(v),
            "--snapshots" => o.snapshots = PathBuf::from(v),
            "--games" => o.games = v.parse().map_err(|e| format!("--games: {e}"))?,
            "--limit" => o.limit = v.parse().map_err(|e| format!("--limit: {e}"))?,
            "--report" => o.report = Some(PathBuf::from(v)),
            "--json" => o.json = Some(PathBuf::from(v)),
            "--mode" => {
                o.modes = match v.as_str() {
                    "alternating" => vec![TurnMode::Alternating],
                    "igougo" => vec![TurnMode::IgougoOverwatch],
                    "both" => vec![TurnMode::Alternating, TurnMode::IgougoOverwatch],
                    _ => return Err(format!("--mode {v}: alternating | igougo | both")),
                }
            }
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

#[derive(Debug, Clone)]
struct Played {
    outcome: TacticalOutcome,
    percentile: f64,
    turns: u32,
    margin: i32,
    commands: usize,
    secs: f64,
    stuck: bool,
}

fn play(
    rules: &Arc<TacRules>,
    data: &ceasefire_core::data::GameData,
    input: &BattleInput,
    seed: u64,
    mode: TurnMode,
) -> Result<Played, String> {
    let started = Instant::now();
    let mut b =
        build_battle(Arc::clone(rules), data, input, seed, mode).map_err(|e| e.to_string())?;
    let ai = TacticalAi::new(seed);
    let mut n = 0;
    let mut stuck = false;
    while b.result.is_none() && n < MAX_COMMANDS {
        let cmd = ai.next(&b);
        if b.apply(cmd).is_err() {
            stuck = true;
            b.apply(TacCommand::Resign).map_err(|e| e.to_string())?;
        }
        n += 1;
    }
    if b.result.is_none() {
        stuck = true;
        b.apply(TacCommand::Resign).map_err(|e| e.to_string())?;
    }
    let r = b.result.clone().ok_or("no result")?;
    Ok(Played {
        outcome: r.outcome,
        percentile: r.percentile,
        turns: r.turns,
        margin: r.margin,
        commands: n,
        secs: started.elapsed().as_secs_f64(),
        stuck,
    })
}

fn mode_name(m: TurnMode) -> &'static str {
    match m {
        TurnMode::Alternating => "поочерёдные активации",
        TurnMode::IgougoOverwatch => "IGOUGO + наблюдение (`igougo_overwatch`)",
    }
}

pub fn run(o: &Options) -> Result<(), String> {
    let data = Arc::new(load(&o.data)?);
    let rules = Arc::new(
        TacRules::from_json(
            &fs::read_to_string(o.data.join("tactical.json"))
                .map_err(|e| format!("tactical.json: {e}"))?,
        )
        .map_err(|e| e.to_string())?,
    );
    let mut files: Vec<PathBuf> = fs::read_dir(&o.snapshots)
        .map_err(|e| format!("{}: {e}", o.snapshots.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    // An even spread over the snapshots when limited.
    if files.len() > o.limit {
        let step = files.len() as f64 / o.limit as f64;
        files = (0..o.limit)
            .map(|i| files[(i as f64 * step) as usize].clone())
            .collect();
    }
    let inputs: Vec<(String, BattleInput)> = files
        .iter()
        .map(|p| {
            let text = fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let input: BattleInput =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok((
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                input,
            ))
        })
        .collect::<Result<_, String>>()?;
    let mut out = String::new();
    let mut json = serde_json::Map::new();
    let _ = writeln!(out, "# Тактические бои T1: ИИ против ИИ\n\nСгенерирован `sim tactical`: {} снимков, {} сидов на снимок.\n", inputs.len(), o.games);
    for &mode in &o.modes {
        let started = Instant::now();
        let jobs: Vec<(usize, u64)> = (0..inputs.len())
            .flat_map(|i| (0..o.games).map(move |s| (i, s)))
            .collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let chunk = jobs.len().div_ceil(threads).max(1);
        let results: Vec<Result<(String, Played), String>> = std::thread::scope(|sc| {
            let handles: Vec<_> = jobs
                .chunks(chunk)
                .map(|part| {
                    let (rules, data, inputs) = (&rules, &data, &inputs);
                    sc.spawn(move || {
                        part.iter()
                            .map(|&(i, seed)| {
                                play(rules, data, &inputs[i].1, seed + 1, mode)
                                    .map(|p| (inputs[i].0.clone(), p))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        let wall = started.elapsed().as_secs_f64();
        let games: Vec<(String, Played)> = results.into_iter().collect::<Result<_, _>>()?;
        let n = games.len().max(1) as f64;
        let mean_pct = games.iter().map(|g| g.1.percentile).sum::<f64>() / n;
        let mean_turns = games.iter().map(|g| f64::from(g.1.turns)).sum::<f64>() / n;
        let mean_margin = games.iter().map(|g| f64::from(g.1.margin)).sum::<f64>() / n;
        let mean_ms = games.iter().map(|g| g.1.secs).sum::<f64>() / n * 1000.0;
        let mean_cmds = games.iter().map(|g| g.1.commands as f64).sum::<f64>() / n;
        let stuck = games.iter().filter(|g| g.1.stuck).count();
        let mut dist: BTreeMap<TacticalOutcome, usize> = BTreeMap::new();
        for g in &games {
            *dist.entry(g.1.outcome).or_insert(0) += 1;
        }
        let by_scenario = |prefix: &str| {
            let xs: Vec<f64> = games
                .iter()
                .filter(|g| g.0.starts_with(prefix))
                .map(|g| g.1.percentile)
                .collect();
            (xs.len(), xs.iter().sum::<f64>() / xs.len().max(1) as f64)
        };
        let _ = writeln!(out, "## {}\n", mode_name(mode));
        let _ = writeln!(out, "| метрика | значение |\n| --- | --- |");
        let _ = writeln!(out, "| боёв | {} |", games.len());
        let _ = writeln!(
            out,
            "| средний перцентиль итога (цель ≈ 50) | {mean_pct:.1} |"
        );
        for p in ["golan67", "sinai67"] {
            let (k, m) = by_scenario(p);
            if k > 0 {
                let _ = writeln!(out, "| — «{p}» ({k} боёв) | {m:.1} |");
            }
        }
        let _ = writeln!(out, "| средний перевес M | {mean_margin:.2} |");
        let _ = writeln!(out, "| средняя длина боя, ходов | {mean_turns:.2} |");
        let _ = writeln!(out, "| приказов за бой | {mean_cmds:.0} |");
        let _ = writeln!(
            out,
            "| время боя ИИ против ИИ, мс (один поток) | {mean_ms:.1} |"
        );
        let _ = writeln!(out, "| время серии, с (все потоки) | {wall:.1} |");
        let _ = writeln!(
            out,
            "| боёв, завершённых сдачей из-за отказа команды | {stuck} |"
        );
        let mut by_turns: BTreeMap<u32, usize> = BTreeMap::new();
        for g in &games {
            *by_turns.entry(g.1.turns).or_insert(0) += 1;
        }
        let _ = writeln!(
            out,
            "| боёв по длине (ход: боёв) | {} |",
            by_turns
                .iter()
                .map(|(t, k)| format!("{t}: {k}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = writeln!(out, "\n| итог | боёв | доля |\n| --- | --- | --- |");
        for o in TacticalOutcome::ALL {
            let k = dist.get(&o).copied().unwrap_or(0);
            let _ = writeln!(out, "| {o:?} | {k} | {:.1}% |", k as f64 / n * 100.0);
        }
        let _ = writeln!(out);
        json.insert(
            format!("{mode:?}"),
            serde_json::json!({
                "battles": games.len(),
                "mean_percentile": mean_pct,
                "mean_turns": mean_turns,
                "mean_margin": mean_margin,
                "ms_per_battle": mean_ms,
                "wall_secs": wall,
                "stuck": stuck,
                "by_turns": games.iter().fold(BTreeMap::new(), |mut m: BTreeMap<u32, usize>, g| { *m.entry(g.1.turns).or_insert(0) += 1; m }),
                "outcomes": dist.iter().map(|(k, v)| (format!("{k:?}"), *v)).collect::<BTreeMap<_, _>>(),
                "golan67": by_scenario("golan67"),
                "sinai67": by_scenario("sinai67"),
            }),
        );
    }
    if let Some(p) = &o.json {
        fs::write(p, serde_json::Value::Object(json).to_string()).map_err(|e| e.to_string())?;
    }
    match &o.report {
        Some(p) => fs::write(p, &out).map_err(|e| format!("{}: {e}", p.display()))?,
        None => print!("{out}"),
    }
    Ok(())
}
