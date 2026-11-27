//! `sim tournament` (stage 4 step 7): automatic tuning of the AI weights in
//! `data/ai.json` against a set of opponents on several scenarios at once —
//! the goal is robustness, not the best score on one scenario.
//!
//! - Candidates: the incumbent weights and `--candidates` perturbations of the
//!   tuned weights (each × a factor in [1 − spread, 1 + spread], seeded).
//! - Opponents: the greedy baseline, the incumbent v1 and two v1 variants with
//!   perturbed weights (`--spread`).
//! - Every candidate plays every opponent on every scenario, `--games` games in
//!   each role (fixed seeds); a game scores ½ × outcome (1 win, ½ draw, 0)
//!   plus ½ × VP share (0.5 + margin / 2 Σ objective VP, clamped): the margin
//!   tells apart games the scenario's balance decides alone.
//! - Fitness = the minimum score over (scenario, opponent); ties by the mean.
//! - `--rounds` > 1: the next round perturbs the best with half the spread.
//!
//! The best weights go to `--out` (an `ai.json`); `data/ai.json` is not touched.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ceasefire_ai::{greedy_next, play_phase, AiHints, AiParams, AiV1};
use ceasefire_core::data::GameData;
use ceasefire_core::game::GameState;
use ceasefire_core::rng::Pcg32;
use ceasefire_core::scenario::load_scenario;
use ceasefire_core::side::SideId;

use crate::combat_matrix::load_patched;

/// Weights the tournament tunes (`section.key` in `ai.json`).
const TUNED: [&str; 16] = [
    "attack.min_score_defensive",
    "attack.min_score_aggressive",
    "attack.retreat_bonus",
    "attack.objective_bonus",
    "attack.focus_bonus",
    "move.goal_weight",
    "move.terrain_weight",
    "move.entrench_weight",
    "move.threat_weight",
    "move.contact_bonus",
    "move.objective_bonus",
    "pockets.min_vp",
    "position.w_cut",
    "position.w_pocket",
    "position.w_risk",
    "position.w_choke",
];

const MAX_PHASES: u32 = 400;

pub struct Options {
    data: PathBuf,
    scenarios: Vec<String>,
    candidates: u32,
    games: u32,
    rounds: u32,
    spread: f64,
    seed: u64,
    out: Option<PathBuf>,
    report: Option<PathBuf>,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        data: PathBuf::from("data"),
        scenarios: vec!["golan67".into(), "sinai67".into()],
        candidates: 12,
        games: 10,
        rounds: 2,
        spread: 0.5,
        seed: 1,
        out: None,
        report: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag}: value missing"))?;
        let num = |v: &str| v.parse::<u32>().map_err(|e| format!("{flag}: {e}"));
        match flag.as_str() {
            "--data" => o.data = PathBuf::from(value),
            "--scenarios" => o.scenarios = value.split(',').map(str::to_owned).collect(),
            "--candidates" => o.candidates = num(value)?,
            "--games" => o.games = num(value)?.max(1),
            "--rounds" => o.rounds = num(value)?.max(1),
            "--spread" => o.spread = value.parse().map_err(|e| format!("--spread: {e}"))?,
            "--seed" => o.seed = value.parse().map_err(|e| format!("--seed: {e}"))?,
            "--out" => o.out = Some(PathBuf::from(value)),
            "--report" => o.report = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

#[derive(Clone)]
enum Player {
    Greedy,
    V1(Box<AiParams>),
}

struct Scenario {
    id: String,
    texts: (String, String, Option<String>),
    hints: AiHints,
}

fn get(v: &serde_json::Value, path: &str) -> Option<f64> {
    let (a, b) = path.split_once('.')?;
    v.get(a)?.get(b)?.as_f64()
}

fn set(v: &mut serde_json::Value, path: &str, x: f64) {
    if let Some((a, b)) = path.split_once('.') {
        if let Some(slot) = v.get_mut(a).and_then(|s| s.get_mut(b)) {
            *slot = serde_json::json!(x);
        }
    }
}

/// `base` with every tuned weight × a seeded factor in [1 − spread, 1 + spread]
/// (the sign of a negative weight is kept).
fn perturb(base: &serde_json::Value, rng: &mut Pcg32, spread: f64) -> serde_json::Value {
    let mut v = base.clone();
    for path in TUNED {
        if let Some(x) = get(base, path) {
            let f = 1.0 + spread * (rng.next_f64() * 2.0 - 1.0);
            set(&mut v, path, (x * f * 1000.0).round() / 1000.0);
        }
    }
    v
}

fn params(v: &serde_json::Value) -> Result<AiParams, String> {
    AiParams::from_json(&v.to_string())
}

/// One game: `a` plays side `a_side`, `b` the other; the score of `a`.
fn play(
    data: &Arc<GameData>,
    sc: &Scenario,
    a: &Player,
    b: &Player,
    a_side: u8,
    seed: u64,
) -> Result<f64, String> {
    let mut setup = load_scenario(&sc.texts.0, &sc.texts.1, sc.texts.2.as_deref())
        .map_err(|e| e.to_string())?;
    setup.seed = seed;
    let (mut s, _) = GameState::new(Arc::clone(data), setup).map_err(|e| e.to_string())?;
    let make = |p: &Player| match p {
        Player::Greedy => None,
        Player::V1(w) => Some(AiV1::new((**w).clone(), sc.hints.clone(), seed)),
    };
    let mut ais = [make(a), make(b)];
    let mut phases = 0;
    while s.outcome().is_none() && phases < MAX_PHASES {
        let side = s.active_side().0;
        let slot = usize::from(side != a_side);
        let r = match ais[slot].as_mut() {
            None => play_phase(&mut s, greedy_next, 400),
            Some(ai) => {
                let max = ai.max_commands();
                play_phase(&mut s, |st| ai.next(st), max)
            }
        };
        r.map_err(|e| e.to_string())?;
        phases += 1;
    }
    let o = s.outcome().ok_or("the game did not end")?;
    let result = match o.winner {
        Some(w) if w == SideId(a_side) => 1.0,
        None => 0.5,
        Some(_) => 0.0,
    };
    // The VP margin tells apart games a scenario's balance decides alone
    // («Синай-67»): ½ outcome + ½ margin, normalised by the objectives' VP.
    let vp = |side: u8| o.vp.iter().find(|x| x.0 == SideId(side)).map_or(0, |x| x.1);
    let scale = f64::from(
        s.objectives()
            .iter()
            .map(|x| x.vp.max(1))
            .sum::<i32>()
            .max(1),
    );
    let margin = f64::from(vp(a_side) - vp(1 - a_side));
    let share = (0.5 + margin / (2.0 * scale)).clamp(0.0, 1.0);
    Ok(0.5 * result + 0.5 * share)
}

/// Score of `cand` against each (scenario, opponent): mean over both roles.
fn evaluate(
    data: &Arc<GameData>,
    scenarios: &[Scenario],
    cands: &[Player],
    opponents: &[(String, Player)],
    games: u32,
    seed: u64,
) -> Result<Vec<Vec<f64>>, String> {
    // Jobs: (candidate, scenario, opponent, role, game).
    let mut jobs = Vec::new();
    for c in 0..cands.len() {
        for s in 0..scenarios.len() {
            for o in 0..opponents.len() {
                for role in 0..2u8 {
                    for g in 0..games {
                        jobs.push((c, s, o, role, g));
                    }
                }
            }
        }
    }
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(jobs.len().max(1));
    let results: Vec<Result<f64, String>> = std::thread::scope(|scope| {
        let chunk = jobs.len().div_ceil(threads).max(1);
        let handles: Vec<_> = jobs
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|&(c, s, o, role, g)| {
                            play(
                                data,
                                &scenarios[s],
                                &cands[c],
                                &opponents[o].1,
                                role,
                                seed + u64::from(g),
                            )
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
    let mut table = vec![vec![0.0; scenarios.len() * opponents.len()]; cands.len()];
    for (&(c, s, o, _, _), r) in jobs.iter().zip(results) {
        table[c][s * opponents.len() + o] += r? / f64::from(2 * games);
    }
    Ok(table)
}

fn fitness(row: &[f64]) -> (f64, f64) {
    let min = row.iter().copied().fold(f64::INFINITY, f64::min);
    let mean = row.iter().sum::<f64>() / row.len().max(1) as f64;
    (min, mean)
}

pub fn run(o: &Options) -> Result<(), String> {
    let started = Instant::now();
    let data = Arc::new(load_patched(&o.data, &[])?);
    let base: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(o.data.join("ai.json")).map_err(|e| format!("ai.json: {e}"))?,
    )
    .map_err(|e| format!("ai.json: {e}"))?;
    let mut scenarios = Vec::new();
    for id in &o.scenarios {
        let dir = o.data.join("scenarios").join(id);
        let read = |n: &str| {
            fs::read_to_string(dir.join(n)).map_err(|e| format!("{}: {e}", dir.join(n).display()))
        };
        let texts = (
            read("scenario.json")?,
            read("map.json")?,
            read("events.json").ok(),
        );
        let hints = AiHints::from_scenario_json(&texts.0)?;
        scenarios.push(Scenario {
            id: id.clone(),
            texts,
            hints,
        });
    }
    let mut rng = Pcg32::new(o.seed, 7);
    let incumbent = Player::V1(Box::new(params(&base)?));
    let opponents: Vec<(String, Player)> = vec![
        ("greedy".into(), Player::Greedy),
        ("v1".into(), incumbent.clone()),
        (
            "v1~a".into(),
            Player::V1(Box::new(params(&perturb(&base, &mut rng, o.spread))?)),
        ),
        (
            "v1~b".into(),
            Player::V1(Box::new(params(&perturb(&base, &mut rng, o.spread))?)),
        ),
    ];
    let columns: Vec<String> = scenarios
        .iter()
        .flat_map(|s| opponents.iter().map(move |(n, _)| format!("{}/{n}", s.id)))
        .collect();
    let mut best = base.clone();
    let mut report = format!(
        "# Турнир весов ИИ v1\n\nСценарии: {}; соперники: «жадина», v1, v1 с разбросом ×2; {} партий в каждой роли; оценка партии — ½ исхода + ½ доли победных очков; оценка кандидата — минимум по (сценарий, соперник), затем среднее.\n",
        o.scenarios.join(", "),
        o.games
    );
    let mut spread = o.spread;
    for round in 1..=o.rounds {
        let mut values = vec![best.clone()];
        for _ in 0..o.candidates {
            values.push(perturb(&best, &mut rng, spread));
        }
        let cands: Vec<Player> = values
            .iter()
            .map(|v| params(v).map(|p| Player::V1(Box::new(p))))
            .collect::<Result<_, _>>()?;
        let table = evaluate(
            &data,
            &scenarios,
            &cands,
            &opponents,
            o.games,
            o.seed * 1000,
        )?;
        let mut order: Vec<usize> = (0..cands.len()).collect();
        order.sort_by(|&a, &b| {
            let (fa, fb) = (fitness(&table[a]), fitness(&table[b]));
            fb.0.total_cmp(&fa.0)
                .then(fb.1.total_cmp(&fa.1))
                .then(a.cmp(&b))
        });
        report.push_str(&format!(
            "\n## Раунд {round} (разброс ±{:.0}%)\n\n| # | кандидат | мин | среднее | {} |\n| --- | --- | --- | --- |{}\n",
            spread * 100.0,
            columns.join(" | "),
            " --- |".repeat(columns.len())
        ));
        for (rank, &c) in order.iter().enumerate() {
            let (min, mean) = fitness(&table[c]);
            let name = if c == 0 {
                "текущие".to_owned()
            } else {
                format!("c{c}")
            };
            let cells: Vec<String> = table[c].iter().map(|x| format!("{x:.2}")).collect();
            report.push_str(&format!(
                "| {} | {name} | {min:.2} | {mean:.2} | {} |\n",
                rank + 1,
                cells.join(" | ")
            ));
        }
        best = values[order[0]].clone();
        eprintln!(
            "round {round}: best {} min {:.2} mean {:.2} ({:.0} s)",
            if order[0] == 0 {
                "incumbent".to_owned()
            } else {
                format!("c{}", order[0])
            },
            fitness(&table[order[0]]).0,
            fitness(&table[order[0]]).1,
            started.elapsed().as_secs_f64()
        );
        spread /= 2.0;
    }
    report.push_str("\n## Лучшие веса\n\n| вес | было | стало |\n| --- | --- | --- |\n");
    for path in TUNED {
        let (a, b) = (
            get(&base, path).unwrap_or(0.0),
            get(&best, path).unwrap_or(0.0),
        );
        report.push_str(&format!("| `{path}` | {a} | {b} |\n"));
    }
    if let Some(p) = &o.out {
        let text = serde_json::to_string_pretty(&best).map_err(|e| e.to_string())? + "\n";
        fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    match &o.report {
        Some(p) => fs::write(p, &report).map_err(|e| format!("{}: {e}", p.display()))?,
        None => println!("{report}"),
    }
    Ok(())
}
