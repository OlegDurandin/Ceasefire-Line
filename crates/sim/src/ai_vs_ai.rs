//! `sim ai-vs-ai`: many games of the AI against itself on one scenario
//! (stage 4, GDD section 17 «Исход сценария» and the scenario metrics).
//! Games run in parallel threads here, in the CLI; the core and the AI stay
//! single-threaded and deterministic: game i always uses seed `seed + i`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ceasefire_ai::{greedy_next, play_phase, AiHints, AiParams, AiV1};
use ceasefire_core::data::GameData;
use ceasefire_core::game::{Event, GameState, NmReason, VictoryLevel};
use ceasefire_core::scenario::load_scenario;
use ceasefire_core::side::SideId;

use crate::combat_matrix::load;

/// Section 17 (designer decision 2026-11-26): each side 45–55% with a draw
/// counted as half a win, draws at most 35%.
const WIN_SHARE: (f64, f64) = (0.45, 0.55);
const MAX_DRAWS: f64 = 0.35;
/// Guards: phases per game, commands per phase (R-AI-02: budgets in operations).
const MAX_PHASES: u32 = 200;
const MAX_COMMANDS: u32 = 400;

pub struct Options {
    data: PathBuf,
    scenario: String,
    runs: u32,
    seed: u64,
    report: Option<PathBuf>,
    /// Player of each side, by side id: `greedy` or `v1`.
    players: Vec<Player>,
    /// Print a phase-by-phase trace of the game with this seed (stderr).
    trace: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Player {
    Greedy,
    V1,
}

impl Player {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "greedy" => Ok(Self::Greedy),
            "v1" => Ok(Self::V1),
            _ => Err(format!("unknown player `{s}` (greedy | v1)")),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Greedy => "«жадина»",
            Self::V1 => "ИИ v1",
        }
    }
}

/// What every game shares: data, scenario texts, the AI's weights and hints.
struct Shared {
    data: Arc<GameData>,
    texts: (String, String, Option<String>),
    params: AiParams,
    hints: AiHints,
    players: Vec<Player>,
    trace: Option<u64>,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        data: PathBuf::from("data"),
        scenario: "golan67".into(),
        runs: 500,
        seed: 1,
        report: None,
        players: vec![Player::Greedy, Player::Greedy],
        trace: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--data" => o.data = PathBuf::from(value),
            "--scenario" => o.scenario = value.clone(),
            "--runs" => o.runs = value.parse().map_err(|e| format!("--runs: {e}"))?,
            "--seed" => o.seed = value.parse().map_err(|e| format!("--seed: {e}"))?,
            "--report" => o.report = Some(PathBuf::from(value)),
            "--trace" => o.trace = Some(value.parse().map_err(|e| format!("--trace: {e}"))?),
            "--players" => {
                o.players = value
                    .split(',')
                    .map(Player::parse)
                    .collect::<Result<_, _>>()?
            }
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

/// What one game left behind.
#[derive(Debug, Clone, Default)]
struct Game {
    winner: Option<SideId>,
    level: Option<VictoryLevel>,
    turn: u32,
    vp: Vec<(SideId, i32)>,
    steps_lost: Vec<(SideId, u32)>,
    phases: u32,
    commands: u32,
    refused: u32,
    attacks: u32,
    bombardments: u32,
    retreats: u32,
    advances: u32,
    destroyed: u32,
    destroyed_encircled: u32,
    fired: Vec<String>,
}

fn play_one(sh: &Shared, seed: u64) -> Result<Game, String> {
    let texts = &sh.texts;
    let mut setup =
        load_scenario(&texts.0, &texts.1, texts.2.as_deref()).map_err(|e| e.to_string())?;
    setup.seed = seed;
    let (mut s, _) = GameState::new(Arc::clone(&sh.data), setup).map_err(|e| e.to_string())?;
    let mut g = Game::default();
    let mut v1 = AiV1::new(sh.params.clone(), sh.hints.clone(), seed);
    while s.outcome().is_none() && g.phases < MAX_PHASES {
        let player = sh
            .players
            .get(usize::from(s.active_side().0))
            .copied()
            .unwrap_or(Player::Greedy);
        let r = match player {
            Player::Greedy => play_phase(&mut s, greedy_next, MAX_COMMANDS),
            Player::V1 => {
                let max = v1.max_commands();
                play_phase(&mut s, |st| v1.next(st), max)
            }
        }
        .map_err(|e| e.to_string())?;
        g.phases += 1;
        if sh.trace == Some(seed) {
            let holders: Vec<String> = s
                .objectives()
                .iter()
                .map(|o| {
                    format!(
                        "{:?}:{:?}",
                        ceasefire_core::scenario::axial_to_offset(o.hex),
                        s.controller(o.hex).map(|c| c.0)
                    )
                })
                .collect();
            let near: Vec<String> = s
                .objectives()
                .iter()
                .map(|o| {
                    let mut d: Vec<(u32, String, u8, u8)> = s
                        .units()
                        .map(|u| (u.hex.distance(o.hex), u.class.clone(), u.side.0, u.strength))
                        .filter(|x| x.0 <= 3)
                        .collect();
                    d.sort();
                    format!(
                        "{:?}: {}",
                        ceasefire_core::scenario::axial_to_offset(o.hex),
                        d.iter()
                            .map(|x| format!(
                                "{}{}@{}({})",
                                if x.2 == 0 { "I" } else { "S" },
                                x.1.get(..3).unwrap_or(&x.1),
                                x.0,
                                x.3
                            ))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                })
                .collect();
            let cmds: Vec<String> = r
                .commands
                .iter()
                .map(|c| {
                    format!("{c:?}")
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_owned()
                })
                .collect();
            eprintln!(
                "turn {} side {} | {} | cmds {:?}\n    {}",
                s.turn(),
                s.active_side().0,
                holders.join(" "),
                cmds,
                near.join("\n    ")
            );
        }
        g.commands += r.commands.len() as u32;
        g.refused += r.refused;
        for e in &r.events {
            match e {
                Event::CombatResolved { .. } => g.attacks += 1,
                Event::Bombarded { .. } => g.bombardments += 1,
                Event::Retreated { .. } => g.retreats += 1,
                Event::Advanced { .. } => g.advances += 1,
                Event::UnitDestroyed { .. } => g.destroyed += 1,
                Event::NationalMorale {
                    reason:
                        NmReason::UnitDestroyed {
                            encircled: true, ..
                        },
                    ..
                } => g.destroyed_encircled += 1,
                Event::ScenarioEventFired { id } => g.fired.push(id.clone()),
                _ => {}
            }
        }
    }
    let o = s.outcome().ok_or("the game did not end")?;
    g.winner = o.winner;
    g.level = Some(o.level);
    g.vp = o.vp.clone();
    g.turn = s.turn();
    let mut sides: Vec<SideId> = o.vp.iter().map(|(s, _)| *s).collect();
    sides.sort();
    g.steps_lost = sides
        .iter()
        .map(|id| (*id, s.side(*id).map_or(0, |x| x.steps_lost)))
        .collect();
    Ok(g)
}

pub fn run(o: &Options) -> Result<(), String> {
    let data = Arc::new(load(&o.data)?);
    let dir = o.data.join("scenarios").join(&o.scenario);
    let read = |n: &str| {
        fs::read_to_string(dir.join(n)).map_err(|e| format!("{}: {e}", dir.join(n).display()))
    };
    let texts = Arc::new((
        read("scenario.json")?,
        read("map.json")?,
        read("events.json").ok(),
    ));
    let params = AiParams::from_json(
        &fs::read_to_string(o.data.join("ai.json")).map_err(|e| format!("ai.json: {e}"))?,
    )?;
    let shared = Arc::new(Shared {
        data: Arc::clone(&data),
        texts: (*texts).clone(),
        params,
        hints: AiHints::from_scenario_json(&texts.0)?,
        players: o.players.clone(),
        trace: o.trace,
    });
    let started = Instant::now();
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(o.runs.max(1) as usize);
    let mut games: Vec<Option<Result<Game, String>>> = vec![None; o.runs as usize];
    std::thread::scope(|scope| {
        let chunks: Vec<_> = games
            .chunks_mut(o.runs.div_ceil(threads as u32).max(1) as usize)
            .collect();
        let mut start = 0u64;
        for chunk in chunks {
            let shared = Arc::clone(&shared);
            let first = start;
            start += chunk.len() as u64;
            let base = o.seed;
            scope.spawn(move || {
                for (k, slot) in chunk.iter_mut().enumerate() {
                    *slot = Some(play_one(&shared, base + first + k as u64));
                }
            });
        }
    });
    let elapsed = started.elapsed().as_secs_f64();
    let games: Vec<Game> = games
        .into_iter()
        .map(|g| g.unwrap_or_else(|| Err("not played".into())))
        .collect::<Result<_, _>>()?;
    let keys: BTreeMap<u8, String> = serde_json::from_str::<serde_json::Value>(&texts.0)
        .ok()
        .and_then(|v| v.get("sides").and_then(|s| s.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|s| {
            Some((
                u8::try_from(s.get("id")?.as_u64()?).ok()?,
                s.get("key")?.as_str()?.to_owned(),
            ))
        })
        .collect();
    let report = render(o, &games, elapsed, &keys);
    match &o.report {
        Some(path) => {
            fs::write(path, &report).map_err(|e| format!("{}: {e}", path.display()))?;
            eprintln!("report: {}", path.display());
        }
        None => print!("{report}"),
    }
    Ok(())
}

fn render(o: &Options, games: &[Game], elapsed: f64, keys: &BTreeMap<u8, String>) -> String {
    let name = |s: &SideId| keys.get(&s.0).cloned().unwrap_or_else(|| s.0.to_string());
    let n = games.len().max(1) as f64;
    let sides: Vec<SideId> = games
        .first()
        .map(|g| g.vp.iter().map(|(s, _)| *s).collect())
        .unwrap_or_default();
    let draws = games.iter().filter(|g| g.winner.is_none()).count() as f64;
    let mut out = String::new();
    let _ = writeln!(out, "# ИИ против ИИ: «{}»\n", o.scenario);
    let _ = writeln!(
        out,
        "Сгенерирован `sim ai-vs-ai`: {} партий, сиды {}–{}; игроки по сторонам: {}. Время {:.1} с ({:.2} мс на фазу в среднем).\n",
        games.len(),
        o.seed,
        o.seed + games.len() as u64 - 1,
        o.players
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{} — {}", name(&SideId(i as u8)), p.label()))
            .collect::<Vec<_>>()
            .join(", "),
        elapsed,
        elapsed * 1000.0 / games.iter().map(|g| g.phases).sum::<u32>().max(1) as f64
    );
    let _ = writeln!(out, "## Исход (раздел 17)\n\nНичья — половина победы каждой стороне (решение дизайнера 2026-11-26).\n\n| сторона | побед | из них решительных | доля с ничьими пополам | цель 45–55% |\n| --- | --- | --- | --- | --- |");
    for side in &sides {
        let wins = games.iter().filter(|g| g.winner == Some(*side)).count() as f64;
        let decisive = games
            .iter()
            .filter(|g| g.winner == Some(*side) && g.level == Some(VictoryLevel::Decisive))
            .count();
        let share = (wins + draws / 2.0) / n;
        let ok = (WIN_SHARE.0..=WIN_SHARE.1).contains(&share);
        let _ = writeln!(
            out,
            "| {} | {wins:.0} | {decisive} | {:.1}% | {} |",
            name(side),
            share * 100.0,
            if ok { "да" } else { "**нет**" }
        );
    }
    let _ = writeln!(
        out,
        "\nНичьих: {draws:.0} ({:.1}%), цель ≤ {:.0}%: {}.\n",
        draws / n * 100.0,
        MAX_DRAWS * 100.0,
        if draws / n <= MAX_DRAWS {
            "да"
        } else {
            "**нет**"
        }
    );
    let avg = |f: &dyn Fn(&Game) -> f64| games.iter().map(f).sum::<f64>() / n;
    let _ = writeln!(
        out,
        "## Метрики (в среднем за партию)\n\n| метрика | значение |\n| --- | --- |"
    );
    for side in &sides {
        let vp = avg(&|g| {
            g.vp.iter()
                .find(|(s, _)| s == side)
                .map_or(0.0, |v| f64::from(v.1))
        });
        let lost = avg(&|g| {
            g.steps_lost
                .iter()
                .find(|(s, _)| s == side)
                .map_or(0.0, |v| f64::from(v.1))
        });
        let _ = writeln!(
            out,
            "| {}: победные очки | {vp:.1} |\n| {}: потеряно шагов | {lost:.1} |",
            name(side),
            name(side)
        );
    }
    let rows: [(&str, f64); 9] = [
        ("последний ход", avg(&|g| f64::from(g.turn))),
        ("команд", avg(&|g| f64::from(g.commands))),
        ("атак", avg(&|g| f64::from(g.attacks))),
        ("обстрелов", avg(&|g| f64::from(g.bombardments))),
        ("отступлений", avg(&|g| f64::from(g.retreats))),
        ("занятий гекса", avg(&|g| f64::from(g.advances))),
        ("уничтожено юнитов", avg(&|g| f64::from(g.destroyed))),
        (
            "из них в окружении",
            avg(&|g| f64::from(g.destroyed_encircled)),
        ),
        ("отказов ядра", avg(&|g| f64::from(g.refused))),
    ];
    for (k, v) in rows {
        let _ = writeln!(out, "| {k} | {v:.2} |");
    }
    // Victory point margin of the first side over the second (for the thresholds).
    if sides.len() == 2 {
        let (s0, s1) = (sides[0], sides[1]);
        let mut margins: Vec<i32> = games
            .iter()
            .map(|g| {
                let v = |s: SideId| g.vp.iter().find(|(x, _)| *x == s).map_or(0, |v| v.1);
                v(s0) - v(s1)
            })
            .collect();
        margins.sort();
        let q = |p: f64| margins[((margins.len() - 1) as f64 * p).round() as usize];
        let _ = writeln!(
            out,
            "\n## Перевес победных очков ({} − {})\n\n10% {} · 25% {} · медиана {} · 75% {} · 90% {}\n",
            name(&s0),
            name(&s1),
            q(0.1),
            q(0.25),
            q(0.5),
            q(0.75),
            q(0.9)
        );
        let mut counts: BTreeMap<i32, u32> = BTreeMap::new();
        for m in &margins {
            *counts.entry(*m).or_insert(0) += 1;
        }
        let _ = writeln!(out, "| перевес | партий | накоплено |\n| --- | --- | --- |");
        let mut acc = 0;
        for (m, k) in counts {
            acc += k;
            let _ = writeln!(out, "| {m} | {k} | {:.1}% |", f64::from(acc) / n * 100.0);
        }
    }
    let mut fired: BTreeMap<&str, u32> = BTreeMap::new();
    for g in games {
        for id in &g.fired {
            *fired.entry(id.as_str()).or_insert(0) += 1;
        }
    }
    let _ = writeln!(out, "\n## События сценария (в скольких партиях сработали)\n\n| событие | партий |\n| --- | --- |");
    for (id, k) in fired {
        let _ = writeln!(out, "| {id} | {k} ({:.0}%) |", f64::from(k) / n * 100.0);
    }
    out
}
