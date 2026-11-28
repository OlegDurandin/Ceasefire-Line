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
use ceasefire_core::combat::Variance;
use ceasefire_core::data::GameData;
use ceasefire_core::game::{Event, GameState, NmReason, UnitId, VictoryLevel};
use ceasefire_core::scenario::load_scenario;
use ceasefire_core::side::SideId;

use crate::combat_matrix::load_patched;

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
    /// Summary metrics as JSON (for A/B tables).
    json: Option<PathBuf>,
    /// Player of each side, by side id: `greedy` or `v1`.
    players: Vec<Player>,
    /// Print a phase-by-phase trace of the game with this seed (stderr).
    trace: Option<u64>,
    /// Rule variants switched on over the scenario (`--variant a,b`).
    variants: BTreeMap<String, bool>,
    /// Rules values replaced (`--set section.key=value`).
    sets: Vec<(String, String)>,
    /// П10: loss draws (`binomial` | `sc`), sim only.
    variance: Variance,
    /// Hexes (offset `c,r;c,r`) whose first control by side 0 is reported
    /// (e.g. the Sinai passes: «выход к перевалам», section 17).
    watch: Vec<ceasefire_core::hex::Hex>,
    /// T1: write every ground attack as a tactical battle input here.
    dump: Option<PathBuf>,
    /// Scripted orders for one side on some turns (`crate::script`).
    script: Option<PathBuf>,
    /// JSON lines: side 0's attacks and bombardments of the first turns.
    battle_log: Option<PathBuf>,
    /// `--unit-log`: scripted formations' units at the end of each scripted phase.
    unit_log: Option<PathBuf>,
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
    variants: BTreeMap<String, bool>,
    variance: Variance,
    watch: Vec<ceasefire_core::hex::Hex>,
    dump: Option<PathBuf>,
    scenario: String,
    script: Option<crate::script::Script>,
    battle_log: bool,
    unit_log: bool,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        data: PathBuf::from("data"),
        scenario: "golan67".into(),
        runs: 500,
        seed: 1,
        report: None,
        json: None,
        players: vec![Player::Greedy, Player::Greedy],
        trace: None,
        variants: BTreeMap::new(),
        sets: Vec::new(),
        variance: Variance::Binomial,
        watch: Vec::new(),
        dump: None,
        script: None,
        battle_log: None,
        unit_log: None,
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
            "--json" => o.json = Some(PathBuf::from(value)),
            "--trace" => o.trace = Some(value.parse().map_err(|e| format!("--trace: {e}"))?),
            "--variant" => {
                for v in value.split(',').filter(|v| !v.is_empty()) {
                    o.variants.insert(v.to_owned(), true);
                }
            }
            "--set" => {
                let (k, v) = value
                    .split_once('=')
                    .ok_or_else(|| format!("--set {value}: expected section.key=value"))?;
                o.sets.push((k.to_owned(), v.to_owned()));
            }
            "--dump-battles" => o.dump = Some(PathBuf::from(value)),
            "--script" => o.script = Some(PathBuf::from(value)),
            "--battle-log" => o.battle_log = Some(PathBuf::from(value)),
            "--unit-log" => o.unit_log = Some(PathBuf::from(value)),
            "--watch" => {
                for cr in value.split(';').filter(|x| !x.is_empty()) {
                    let (c, r) = cr
                        .split_once(',')
                        .ok_or_else(|| format!("--watch {cr}: expected c,r"))?;
                    let c: i32 = c.trim().parse().map_err(|e| format!("--watch: {e}"))?;
                    let r: i32 = r.trim().parse().map_err(|e| format!("--watch: {e}"))?;
                    o.watch
                        .push(ceasefire_core::scenario::offset_to_axial([c, r]));
                }
            }
            "--variance" => {
                o.variance = match value.as_str() {
                    "binomial" => Variance::Binomial,
                    "sc" => Variance::Sc,
                    _ => return Err(format!("--variance {value}: binomial | sc")),
                }
            }
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
    /// R-NM-02: ended by sudden death (NM), not by the date.
    sudden_death: bool,
    /// NM per side at the end.
    nm_end: Vec<(SideId, u8)>,
    /// R-NM-02: steps taken out through exits, per side.
    exited: BTreeMap<u8, u32>,
    /// NM change per side by reason (R-NM-01).
    nm_by: BTreeMap<(u8, &'static str), i32>,
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
    /// Steps lost by ground units out of supply (side's hex supply 0 at the
    /// start of the phase) and by units in supply.
    steps_encircled: u32,
    steps_frontal: u32,
    /// Attacks whose forecast favours the attacker (E[defender] > E[attacker]).
    favourable_attacks: u32,
    /// First turn from which the VP leader never changes (None: tie at the end).
    turning_point: Option<u32>,
    /// R-AI-01 operations: phases with carried operations, and of those kept.
    ops_carried: u32,
    ops_kept: u32,
    /// Step 7 item 5 per side: reserves to own chokepoints, withdrawals.
    defend: BTreeMap<u8, (u32, u32)>,
    /// R-AI-01 п. 3 per side: MPP on reinforcement, purchases, boosts, transfers.
    spent: BTreeMap<u8, [u32; 4]>,
    /// MPP left unspent at the end, per side.
    mpp_left: BTreeMap<u8, u32>,
    /// Breakthrough forecasts against outcomes.
    breaches: Vec<(f64, bool)>,
    /// First turn side 0 controls any watched hex.
    watch_turn: Option<u32>,
    /// Battles written by `--dump-battles`.
    dumped: u32,
    /// `--battle-log`: side 0's attacks and bombardments up to turn
    /// `BATTLE_LOG_TURNS`, with their inputs and results.
    battles: Vec<serde_json::Value>,
    /// Ground attack commands, those on HQs, and those whose HQ target was in
    /// contact (an enemy unit next to it) at the start of the phase.
    ground_attacks: u32,
    hq_attacks: u32,
    hq_attacks_in_contact: u32,
    /// R-MOVE-07 per side: moves made in a breakthrough and hexes entered by them.
    breakthrough: BTreeMap<u8, (u32, u32)>,
    /// `--unit-log` entries.
    unit_log: Vec<serde_json::Value>,
    /// R-NM-02 per side at the end: VP of held objectives and of exits.
    vp_objectives: BTreeMap<u8, i32>,
    vp_exits: BTreeMap<u8, i32>,
}

/// Strength and «encircled» of every ground unit at the last look.
type Snapshot = BTreeMap<UnitId, (u8, bool)>;

/// Steps lost since `prev`, split into encircled and in-supply.
fn charge_steps(s: &GameState, prev: &Snapshot, g: &mut Game) {
    for (id, (strength, encircled)) in prev {
        let now = s.unit(*id).map_or(0, |u| u.strength);
        let lost = u32::from(strength.saturating_sub(now));
        if *encircled {
            g.steps_encircled += lost;
        } else {
            g.steps_frontal += lost;
        }
    }
}

/// Every ground unit with «encircled» = its side's hex supply is 0 (R-NM-01).
fn snapshot(s: &GameState) -> Snapshot {
    let mut supply = BTreeMap::new();
    s.units()
        .map(|u| {
            let map = supply
                .entry(u.side)
                .or_insert_with(|| s.hex_supply(u.side).ok());
            let encircled = map.as_ref().is_none_or(|m| m.get(u.hex) == 0);
            (u.id, (u.strength, encircled))
        })
        .collect()
}

/// Turns covered by `--battle-log`.
const BATTLE_LOG_TURNS: u32 = 3;

/// `--battle-log`: the inputs of a ground attack or bombardment before it is
/// applied (the same explain functions as the forecast, R-UI-04).
fn battle_entry(s: &GameState, cmd: &ceasefire_core::game::Command) -> Option<serde_json::Value> {
    use ceasefire_core::game::Command;
    let (kind, unit, target) = match cmd {
        Command::Attack { unit, target } => ("attack", *unit, *target),
        Command::Bombard { unit, target } => ("bombard", *unit, *target),
        _ => return None,
    };
    let a = s.unit(unit)?;
    let d = s.unit(target)?;
    let inputs = match kind {
        "attack" => serde_json::to_value(s.explain_attack(unit, target).ok()?).ok()?,
        _ => serde_json::to_value([s.explain_bombard(unit, target).ok()?]).ok()?,
    };
    let rules = &s.data().rules.combat;
    let params: Vec<_> =
        serde_json::from_value::<Vec<ceasefire_core::combat::FireInput>>(inputs.clone())
            .ok()?
            .iter()
            .map(|i| ceasefire_core::combat::fire_params(rules, i))
            .collect();
    Some(serde_json::json!({
        "turn": s.turn(),
        "night": s.is_night(),
        "kind": kind,
        "attacker": { "id": unit.0, "class": a.class, "name": a.name, "formation": a.formation,
            "hex": ceasefire_core::scenario::axial_to_offset(a.hex), "strength": a.strength,
            "supply": a.supply, "readiness": a.readiness_pct(&s.data().rules.readiness) },
        "defender": { "id": target.0, "class": d.class, "name": d.name,
            "hex": ceasefire_core::scenario::axial_to_offset(d.hex), "strength": d.strength,
            "supply": d.supply, "entrenchment": d.entrenchment,
            "terrain": s.map().cell(d.hex).map(|c| c.terrain.clone()) },
        "inputs": inputs,
        "params": params,
    }))
}

/// Results of the logged fights, in command order (one result event each).
fn fill_results(seed: u64, logged: &mut [serde_json::Value], events: &[Event]) {
    // Air strikes and the other side's fights give no such event; results
    // are matched in order by attacker and defender ids.
    let mut results = events.iter().filter_map(|e| match e {
        Event::CombatResolved {
            attacker,
            defender,
            attacker_losses,
            defender_losses,
            ..
        } => Some((
            Some(*attacker_losses),
            *defender_losses,
            *attacker,
            *defender,
        )),
        Event::Bombarded {
            unit,
            target,
            losses,
            ..
        } => Some((None, *losses, *unit, *target)),
        _ => None,
    });
    for entry in logged.iter_mut() {
        let a = entry["attacker"]["id"].as_u64();
        let d = entry["defender"]["id"].as_u64();
        if let Some((al, dl, _, _)) = results
            .by_ref()
            .find(|x| Some(u64::from(x.2 .0)) == a && Some(u64::from(x.3 .0)) == d)
        {
            entry["seed"] = serde_json::json!(seed);
            entry["attacker_losses"] = serde_json::json!(al);
            entry["defender_losses"] = serde_json::json!(dl);
        }
    }
}

fn play_one(sh: &Shared, seed: u64) -> Result<Game, String> {
    let texts = &sh.texts;
    let mut setup =
        load_scenario(&texts.0, &texts.1, texts.2.as_deref()).map_err(|e| e.to_string())?;
    setup.seed = seed;
    for (k, v) in &sh.variants {
        setup.variants.insert(k.clone(), *v);
    }
    let (mut s, _) = GameState::new(Arc::clone(&sh.data), setup).map_err(|e| e.to_string())?;
    s.set_variance(sh.variance);
    let mut g = Game::default();
    let mut leaders: Vec<(u32, Option<SideId>)> = Vec::new();
    let mut v1 = AiV1::new(sh.params.clone(), sh.hints.clone(), seed);
    while s.outcome().is_none() && g.phases < MAX_PHASES {
        let player = sh
            .players
            .get(usize::from(s.active_side().0))
            .copied()
            .unwrap_or(Player::Greedy);
        // Encirclement is taken at the start of the phase; strengths after every command.
        let start = snapshot(&s);
        let encircled: BTreeMap<UnitId, bool> = start.iter().map(|(k, v)| (*k, v.1)).collect();
        let mut prev = start;
        let mut look = |st: &GameState, g: &mut Game| {
            charge_steps(st, &prev, g);
            prev = st
                .units()
                .map(|u| {
                    (
                        u.id,
                        (u.strength, encircled.get(&u.id).copied().unwrap_or(false)),
                    )
                })
                .collect();
        };
        let mut steps = Game::default();
        // HQs of the side not moving, in contact at the start of the phase.
        let active = s.active_side();
        let hq_contact: BTreeMap<UnitId, bool> = s
            .units()
            .filter(|u| u.side != active && u.hq_rating.is_some())
            .map(|h| {
                (
                    h.id,
                    s.units()
                        .any(|e| e.side == active && e.hex.is_adjacent(h.hex)),
                )
            })
            .collect();
        let mut hq_seen = (0u32, 0u32, 0u32);
        let mut logged: Vec<serde_json::Value> = Vec::new();
        let mut dump = |st: &GameState, cmd: &ceasefire_core::game::Command| {
            if sh.battle_log && st.active_side() == SideId(0) && st.turn() <= BATTLE_LOG_TURNS {
                if let Some(entry) = battle_entry(st, cmd) {
                    logged.push(entry);
                }
            }
            if let ceasefire_core::game::Command::Move { unit, path } = cmd {
                if let Some(u) = st.unit(*unit).filter(|u| u.breakthrough) {
                    let e = g.breakthrough.entry(u.side.0).or_default();
                    e.0 += 1;
                    e.1 += u32::try_from(path.len()).unwrap_or(u32::MAX);
                }
            }
            if let ceasefire_core::game::Command::Attack { target, .. } = cmd {
                hq_seen.0 += 1;
                if let Some(c) = hq_contact.get(target) {
                    hq_seen.1 += 1;
                    hq_seen.2 += u32::from(*c);
                }
            }
            if let Some(dir) = &sh.dump {
                if let Some(input) = crate::battles::battle_of(st, cmd) {
                    let name = format!("{}-{seed}-{}", sh.scenario, g.dumped);
                    if crate::battles::write(dir, &name, &input).is_ok() {
                        g.dumped += 1;
                    }
                }
            }
        };
        let mover = s.active_side().0;
        let orders: Option<Vec<crate::script::Order>> = sh
            .script
            .as_ref()
            .and_then(|x| x.orders(&s))
            .map(<[_]>::to_vec);
        let scripted = |st: &GameState| orders.as_deref().and_then(|o| crate::script::next(st, o));
        // `--unit-log`: where the scripted units start the phase.
        let starts: BTreeMap<UnitId, (ceasefire_core::hex::Hex, u32)> = match (&orders, sh.unit_log)
        {
            (Some(o), true) => s
                .units()
                .filter(|u| {
                    o.iter()
                        .any(|x| u.formation.as_deref() == Some(x.formation.as_str()))
                })
                .map(|u| (u.id, (u.hex, u.ap_halves)))
                .collect(),
            _ => BTreeMap::new(),
        };
        let r = match player {
            Player::Greedy => play_phase(
                &mut s,
                |st| {
                    look(st, &mut steps);
                    let c = scripted(st).unwrap_or_else(|| greedy_next(st));
                    dump(st, &c);
                    c
                },
                MAX_COMMANDS,
            ),
            Player::V1 => {
                let max = v1.max_commands();
                play_phase(
                    &mut s,
                    |st| {
                        look(st, &mut steps);
                        let c = scripted(st).unwrap_or_else(|| v1.next(st));
                        dump(st, &c);
                        c
                    },
                    max,
                )
            }
        }
        .map_err(|e| e.to_string())?;
        if !logged.is_empty() {
            fill_results(seed, &mut logged, &r.events);
            g.battles.append(&mut logged);
        }
        g.ground_attacks += hq_seen.0;
        g.hq_attacks += hq_seen.1;
        g.hq_attacks_in_contact += hq_seen.2;
        if let Some(o) = &orders {
            if sh.unit_log {
                g.unit_log.extend(unit_entries(&s, seed, o, &starts));
            }
        }
        // The closing EndPhase: losses up to the next side's phase start (attrition).
        look(&s, &mut steps);
        if g.watch_turn.is_none() && sh.watch.iter().any(|h| s.controller(*h) == Some(SideId(0))) {
            g.watch_turn = Some(s.turn());
        }
        g.steps_encircled += steps.steps_encircled;
        g.steps_frontal += steps.steps_frontal;
        g.phases += 1;
        if s.active_side() == SideId(0) || s.outcome().is_some() {
            let vp = s.victory_points();
            let best = vp.iter().map(|v| v.1).max().unwrap_or(0);
            let top: Vec<SideId> = vp.iter().filter(|v| v.1 == best).map(|v| v.0).collect();
            // The turn just finished (the next one has begun unless the game is over).
            let done = if s.outcome().is_some() {
                s.turn()
            } else {
                s.turn() - 1
            };
            leaders.push((done, (top.len() == 1).then(|| top[0])));
        }
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
                Event::CombatResolved { exchange, .. } => {
                    g.attacks += 1;
                    let e = |f: &ceasefire_core::combat::FireParams| f64::from(f.n) * f.p;
                    if e(&exchange.vs_defender) > e(&exchange.vs_attacker) {
                        g.favourable_attacks += 1;
                    }
                }
                Event::Bombarded { .. } => g.bombardments += 1,
                Event::Retreated { .. } => g.retreats += 1,
                Event::Advanced { .. } => g.advances += 1,
                Event::UnitDestroyed { .. } => g.destroyed += 1,
                Event::NationalMorale {
                    side,
                    change,
                    reason,
                    ..
                } => {
                    let why = match reason {
                        NmReason::ObjectiveLost { .. } => "objective_lost",
                        NmReason::ObjectiveTaken { .. } => "objective_taken",
                        NmReason::UnitDestroyed {
                            encircled: true, ..
                        } => "destroyed_encircled",
                        NmReason::UnitDestroyed { .. } => "destroyed",
                        NmReason::StepsLost { .. } => "steps_lost",
                        NmReason::KeyHexHeld { .. } => "key_hex",
                        NmReason::HqDestroyed { .. } => "hq_destroyed",
                        NmReason::ScenarioEvent { .. } => "event",
                    };
                    *g.nm_by.entry((side.0, why)).or_default() += change;
                    if let NmReason::UnitDestroyed {
                        encircled: true, ..
                    } = reason
                    {
                        g.destroyed_encircled += 1;
                    }
                }
                Event::UnitExited { steps, .. } => {
                    *g.exited.entry(mover).or_default() += u32::from(*steps)
                }
                Event::ScenarioEventFired { id, .. } => g.fired.push(id.clone()),
                // R-AI-01 п. 3: MPP by use, per side.
                Event::Reinforced { cost, .. } => g.spent.entry(mover).or_default()[0] += cost,
                Event::Purchased { cost, side, .. } => {
                    g.spent.entry(side.0).or_default()[1] += cost
                }
                Event::TechBoostBought { cost, side, .. } => {
                    g.spent.entry(side.0).or_default()[2] += cost
                }
                Event::Transferred { cost, .. } => g.spent.entry(mover).or_default()[3] += cost,
                _ => {}
            }
        }
    }
    let o = s.outcome().ok_or("the game did not end")?;
    g.ops_carried = v1.stats.carried_phases;
    g.ops_kept = v1.stats.kept_phases;
    g.breaches = v1.stats.breaches.clone();
    g.defend = v1.stats.defend.clone();
    g.mpp_left = s.sides().iter().map(|x| (x.id.0, x.mpp)).collect();
    // Turning point: the first turn-end look after the last change of leader.
    let final_leader = leaders.last().and_then(|l| l.1);
    g.turning_point = final_leader.and_then(|w| {
        let k = leaders
            .iter()
            .rposition(|l| l.1 != Some(w))
            .map_or(0, |k| k + 1);
        leaders.get(k).map(|l| l.0)
    });
    g.winner = o.winner;
    g.level = Some(o.level);
    g.sudden_death = o.reason == ceasefire_core::game::EndReason::SuddenDeath;
    g.nm_end = s
        .sides()
        .iter()
        .map(|x| (x.id, x.national_morale_pct))
        .collect();
    g.vp = o.vp.clone();
    for ob in s.objectives() {
        if let Some(h) = s.controller(ob.hex) {
            *g.vp_objectives.entry(h.0).or_default() += ob.vp;
        }
    }
    for (side, _) in &o.vp {
        g.vp_exits.insert(side.0, s.exited_vp(*side));
    }
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
    let data = Arc::new(load_patched(&o.data, &o.sets)?);
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
        variants: o.variants.clone(),
        variance: o.variance,
        watch: o.watch.clone(),
        dump: o.dump.clone(),
        scenario: o.scenario.clone(),
        battle_log: o.battle_log.is_some(),
        unit_log: o.unit_log.is_some(),
        script: o
            .script
            .as_deref()
            .map(crate::script::Script::load)
            .transpose()?,
    });
    if let Some(d) = &o.dump {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
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
    if let Some(path) = &o.unit_log {
        let text: String = games
            .iter()
            .flat_map(|g| &g.unit_log)
            .map(|v| format!("{v}\n"))
            .collect();
        fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if let Some(path) = &o.battle_log {
        let mut text = String::new();
        for b in games.iter().flat_map(|g| &g.battles) {
            text.push_str(&b.to_string());
            text.push('\n');
        }
        fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
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
    if let Some(path) = &o.json {
        let summary = summary(o, &games);
        fs::write(path, summary.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
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
    let m = metrics(games);
    let _ = writeln!(
        out,
        "\n## Метрики SC-выравнивания\n\n| метрика | значение |\n| --- | --- |\n\
         | шагов потеряно в окружении / вне окружения | {:.1} / {:.1} (доля в окружении {:.1}%) |\n\
         | атак при перевесе (E[потери защитника] > E[потери атакующего]) | {:.1}% |\n\
         | потерь шагов за ход (обе стороны) | {:.2} |\n\
         | ход перелома (лидер по победным очкам дальше не меняется) | {} |",
        m.steps_encircled,
        m.steps_frontal,
        m.encircled_share * 100.0,
        m.favourable_share * 100.0,
        m.losses_per_turn,
        m.turning_point.map_or("—".to_owned(), |t| format!(
            "{t:.2} (в {:.0}% партий)",
            m.turning_share * 100.0
        ))
    );
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

/// The SC-alignment metrics over a series of games.
struct Metrics {
    steps_encircled: f64,
    steps_frontal: f64,
    encircled_share: f64,
    favourable_share: f64,
    losses_per_turn: f64,
    turning_point: Option<f64>,
    turning_share: f64,
}

fn metrics(games: &[Game]) -> Metrics {
    let n = games.len().max(1) as f64;
    let sum = |f: &dyn Fn(&Game) -> f64| games.iter().map(f).sum::<f64>();
    let enc = sum(&|g| f64::from(g.steps_encircled));
    let front = sum(&|g| f64::from(g.steps_frontal));
    let attacks = sum(&|g| f64::from(g.attacks));
    let turning: Vec<f64> = games
        .iter()
        .filter_map(|g| g.turning_point.map(f64::from))
        .collect();
    Metrics {
        steps_encircled: enc / n,
        steps_frontal: front / n,
        encircled_share: enc / (enc + front).max(1.0),
        favourable_share: sum(&|g| f64::from(g.favourable_attacks)) / attacks.max(1.0),
        losses_per_turn: sum(&|g| {
            f64::from(g.steps_lost.iter().map(|x| x.1).sum::<u32>()) / f64::from(g.turn.max(1))
        }) / n,
        turning_point: (!turning.is_empty())
            .then(|| turning.iter().sum::<f64>() / turning.len() as f64),
        turning_share: turning.len() as f64 / n,
    }
}

/// Summary for A/B tables: outcome shares and the metrics.
fn summary(o: &Options, games: &[Game]) -> serde_json::Value {
    let n = games.len().max(1) as f64;
    let m = metrics(games);
    let sides: Vec<SideId> = games
        .first()
        .map(|g| g.vp.iter().map(|(s, _)| *s).collect())
        .unwrap_or_default();
    let draws = games.iter().filter(|g| g.winner.is_none()).count() as f64 / n;
    let share = |side: SideId| games.iter().filter(|g| g.winner == Some(side)).count() as f64 / n;
    let decisive = |side: SideId| {
        games
            .iter()
            .filter(|g| g.winner == Some(side) && g.level == Some(VictoryLevel::Decisive))
            .count() as f64
            / n
    };
    let attacks_all: u32 = games.iter().map(|g| g.ground_attacks).sum();
    let hq_all: u32 = games.iter().map(|g| g.hq_attacks).sum();
    let hq_share = f64::from(hq_all) / f64::from(attacks_all.max(1));
    let hq_contact_share = f64::from(games.iter().map(|g| g.hq_attacks_in_contact).sum::<u32>())
        / f64::from(hq_all.max(1));
    let carried: u32 = games.iter().map(|g| g.ops_carried).sum();
    let kept: u32 = games.iter().map(|g| g.ops_kept).sum();
    let ops_kept_share = if carried == 0 {
        serde_json::Value::Null
    } else {
        serde_json::json!(f64::from(kept) / f64::from(carried))
    };
    let mut turns: Vec<u32> = games.iter().filter_map(|g| g.watch_turn).collect();
    turns.sort();
    let watch_turns = serde_json::json!({
        "share": turns.len() as f64 / n,
        "median": turns.get(turns.len() / 2),
        "p25": turns.get(turns.len() / 4),
        "p75": turns.get(turns.len() * 3 / 4),
        // Games by the turn of the first control (cumulative shares follow).
        "by_turn": turns.iter().fold(BTreeMap::<u32, u32>::new(), |mut m, t| {
            *m.entry(*t).or_default() += 1;
            m
        }),
    });
    // VP of side 0 minus side 1 per game, sorted (victory thresholds).
    let mut vp_margins: Vec<i32> = games
        .iter()
        .map(|g| {
            let v = |s: u8| g.vp.iter().find(|x| x.0 .0 == s).map_or(0, |x| x.1);
            v(0) - v(1)
        })
        .collect();
    vp_margins.sort_unstable();
    // Mean VP parts per side: NM, held objectives, exits, enemy steps (the rest).
    let vp_parts: Vec<serde_json::Value> = (0u8..2)
        .map(|side| {
            let mean = |f: &dyn Fn(&Game) -> f64| games.iter().map(f).sum::<f64>() / n;
            let total = mean(&|g| f64::from(g.vp.iter().find(|x| x.0 .0 == side).map_or(0, |x| x.1)));
            let nm = mean(&|g| f64::from(g.nm_end.iter().find(|x| x.0 .0 == side).map_or(0, |x| x.1)));
            let obj = mean(&|g| f64::from(g.vp_objectives.get(&side).copied().unwrap_or(0)));
            let exits = mean(&|g| f64::from(g.vp_exits.get(&side).copied().unwrap_or(0)));
            serde_json::json!({"nm": nm, "objectives": obj, "exits": exits, "steps": total - nm - obj - exits})
        })
        .collect();
    serde_json::json!({
        "scenario": o.scenario,
        "runs": games.len(),
        "seed": o.seed,
        "variants": o.variants,
        "sets": o.sets,
        "variance": format!("{:?}", o.variance),
        "wins": sides.iter().map(|s| share(*s)).collect::<Vec<_>>(),
        "vp_margins": vp_margins,
        "vp_parts": vp_parts,
        "vp_mean": sides.iter().map(|side| games.iter().map(|g| f64::from(g.vp.iter().find(|x| x.0 == *side).map_or(0, |x| x.1))).sum::<f64>() / n).collect::<Vec<_>>(),
        "nm_end": sides.iter().map(|side| games.iter().map(|g| f64::from(g.nm_end.iter().find(|x| x.0 == *side).map_or(0, |x| x.1))).sum::<f64>() / n).collect::<Vec<_>>(),
        "nm_by_reason": (0u8..2).map(|side| {
            let mut m: BTreeMap<&str, f64> = BTreeMap::new();
            for g in games {
                for ((sd, why), v) in &g.nm_by {
                    if *sd == side {
                        *m.entry(why).or_default() += f64::from(*v) / n;
                    }
                }
            }
            m
        }).collect::<Vec<_>>(),
        "breakthrough_moves": (0u8..2).map(|side| games.iter().map(|g| f64::from(g.breakthrough.get(&side).map_or(0, |b| b.0))).sum::<f64>() / n).collect::<Vec<_>>(),
        "breakthrough_hexes": (0u8..2).map(|side| games.iter().map(|g| f64::from(g.breakthrough.get(&side).map_or(0, |b| b.1))).sum::<f64>() / n).collect::<Vec<_>>(),
        "exited_steps": (0u8..2).map(|side| games.iter().map(|g| f64::from(g.exited.get(&side).copied().unwrap_or(0))).sum::<f64>() / n).collect::<Vec<_>>(),
        "sudden_death_share": games.iter().filter(|g| g.sudden_death).count() as f64 / n,
        "decisive": sides.iter().map(|s| decisive(*s)).collect::<Vec<_>>(),
        "draws": draws,
        "last_turn": games.iter().map(|g| f64::from(g.turn)).sum::<f64>() / n,
        "attacks": games.iter().map(|g| f64::from(g.attacks)).sum::<f64>() / n,
        "destroyed": games.iter().map(|g| f64::from(g.destroyed)).sum::<f64>() / n,
        "destroyed_encircled": games.iter().map(|g| f64::from(g.destroyed_encircled)).sum::<f64>() / n,
        "steps_encircled": m.steps_encircled,
        "steps_frontal": m.steps_frontal,
        "encircled_share": m.encircled_share,
        "favourable_share": m.favourable_share,
        "losses_per_turn": m.losses_per_turn,
        "turning_point": m.turning_point,
        "turning_share": m.turning_share,
        "refused": games.iter().map(|g| f64::from(g.refused)).sum::<f64>() / n,
        "ops_kept_share": ops_kept_share,
        "mpp_use": (0u8..2).map(|side| {
            let sum = |i: usize| games.iter().map(|g| f64::from(g.spent.get(&side).map_or(0, |x| x[i]))).sum::<f64>();
            let all: f64 = (0..4).map(sum).sum();
            let share = |i: usize| if all > 0.0 { sum(i) / all } else { 0.0 };
            serde_json::json!({
                "spent_per_game": all / n,
                "reinforce": share(0), "purchase": share(1), "boost": share(2), "transfer": share(3),
                "left_per_game": games.iter().map(|g| f64::from(g.mpp_left.get(&side).copied().unwrap_or(0))).sum::<f64>() / n,
            })
        }).collect::<Vec<_>>(),
        "defend_guards": (0u8..2).map(|side| games.iter().map(|g| f64::from(g.defend.get(&side).map_or(0, |d| d.0))).sum::<f64>() / n).collect::<Vec<_>>(),
        "defend_withdrawals": (0u8..2).map(|side| games.iter().map(|g| f64::from(g.defend.get(&side).map_or(0, |d| d.1))).sum::<f64>() / n).collect::<Vec<_>>(),
        "breach_calibration": calibration(games),
        "watch_turns": watch_turns,
        "ground_attacks": games.iter().map(|g| f64::from(g.ground_attacks)).sum::<f64>() / n,
        "hq_attack_share": hq_share,
        "hq_attacks_in_contact_share": hq_contact_share,
    })
}

/// P(breach) forecasts in bins against how often the defender left its hex.
fn calibration(games: &[Game]) -> serde_json::Value {
    let bins = [(0.0, 0.5), (0.5, 0.7), (0.7, 0.9), (0.9, 1.01)];
    serde_json::Value::Array(
        bins.iter()
            .map(|&(lo, hi)| {
                let xs: Vec<(f64, bool)> = games
                    .iter()
                    .flat_map(|g| g.breaches.iter().copied())
                    .filter(|(p, _)| *p >= lo && *p < hi)
                    .collect();
                let n = xs.len().max(1) as f64;
                serde_json::json!({
                    "bin": [lo, hi.min(1.0)],
                    "count": xs.len(),
                    "forecast": xs.iter().map(|x| x.0).sum::<f64>() / n,
                    "actual": xs.iter().filter(|x| x.1).count() as f64 / n,
                })
            })
            .collect(),
    )
}

/// `--unit-log`: each scripted unit after its phase — where it went, AP and
/// strikes left, enemy ZOC around it, the distance to its order's hex.
fn unit_entries(
    s: &GameState,
    seed: u64,
    orders: &[crate::script::Order],
    starts: &BTreeMap<UnitId, (ceasefire_core::hex::Hex, u32)>,
) -> Vec<serde_json::Value> {
    use ceasefire_core::scenario::axial_to_offset;
    let min = s.data().rules.movement.zoc_min_strength;
    let side = SideId(0);
    let zoc = |h: ceasefire_core::hex::Hex| {
        s.units()
            .filter(|e| {
                e.side != side
                    && e.strength >= min
                    && s.data().units.get(&e.class).is_ok_and(|c| c.exerts_zoc)
                    && e.hex.is_adjacent(h)
            })
            .count()
    };
    let mut out = Vec::new();
    for (id, (from, ap0)) in starts {
        let order = s.unit(*id).and_then(|u| {
            orders
                .iter()
                .find(|o| u.formation.as_deref() == Some(o.formation.as_str()))
        });
        let Some(u) = s.unit(*id) else {
            out.push(serde_json::json!({"seed": seed, "turn": s.turn() - u32::from(s.active_side() == side), "unit": id.0, "destroyed": true}));
            continue;
        };
        let Some(order) = order else { continue };
        out.push(serde_json::json!({
            "seed": seed,
            // The phase just played (the side-1 phase of the same turn has begun).
            "turn": s.turn(),
            "night": s.is_night(),
            "formation": order.formation,
            "unit": id.0,
            "name": u.name,
            "class": u.class,
            "from": axial_to_offset(*from),
            "at": axial_to_offset(u.hex),
            "target": axial_to_offset(order.target),
            "dist_from": from.distance(order.target),
            "dist": u.hex.distance(order.target),
            "ap_start": f64::from(*ap0) / 2.0,
            "ap_left": f64::from(u.ap_halves) / 2.0,
            "strikes_left": u.strikes_left,
            "can_move": u.can_move,
            "moved": u.moved,
            "fought": u.fought,
            "breakthrough": u.breakthrough,
            "zoc_here": zoc(u.hex),
            "strength": u.strength,
            "supply": u.supply,
        }));
    }
    out
}
