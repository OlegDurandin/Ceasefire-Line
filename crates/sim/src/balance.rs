//! `sim balance`: GDD section 17 checks that need no AI. Writes a Markdown
//! report and fails if any check fails. Pace and outcome checks (AI vs AI)
//! belong to stage 4.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use ceasefire_core::air::sam_barrage;
use ceasefire_core::combat::{
    fire_params, forecast_fire, forecast_ground, Combatant, FireInput, GroundAttack,
};
use ceasefire_core::data::{GameData, UnitClass, UnitTag};
use ceasefire_core::prob::{hits_distribution, LossForecast};
use ceasefire_core::tech::{SideModifier, TechBranch, TechLevels};

use crate::combat_matrix::{load, matrix, tech_at, MatrixRow};

/// Section 17, «Эффект ПТУР»: tank losses / infantry losses.
const ATGM_1967_MAX_RATIO: f64 = 1.0 / 3.0;
const ATGM_1973_MIN_RATIO: f64 = 0.5;
/// Section 17, «Стоимость юнитов»: damage per MPP within ±25% of the mean.
const COST_SPREAD: f64 = 0.25;
/// Section 17 + designer decision B: aircraft MPP lost / target MPP destroyed.
const UMBRELLA_ONE_RANGE: (f64, f64) = (0.8, 1.2);
const UMBRELLA_ONE_EW_MAX: f64 = 0.8;
const UMBRELLA_TWO_MIN: f64 = 1.5;
/// Unit classes with a special role, left out of the cost comparison.
const SPECIAL_ROLE: [&str; 5] = ["commando", "engineer", "hq", "sam_static", "sam_mobile"];
const STRENGTH: u8 = 10;
const READINESS: u8 = 80;

pub struct Options {
    data: PathBuf,
    report: PathBuf,
    runs: u32,
    seed: u64,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        data: PathBuf::from("data"),
        report: PathBuf::from("docs/balance-report.md"),
        runs: 10_000,
        seed: 1,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--data" => o.data = PathBuf::from(value),
            "--report" => o.report = PathBuf::from(value),
            "--runs" => o.runs = value.parse().map_err(|e| format!("--runs: {e}"))?,
            "--seed" => o.seed = value.parse().map_err(|e| format!("--seed: {e}"))?,
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(o)
}

struct Check {
    name: &'static str,
    pass: bool,
    body: String,
}

fn unit<'a>(
    data: &'a GameData,
    id: &str,
    tech: &TechLevels,
    strength: u8,
    readiness: u8,
    exp: u8,
    entrench: u8,
) -> Result<Combatant<'a>, String> {
    let class = data.units.get(id).map_err(|e| e.to_string())?;
    Ok(Combatant {
        class,
        stats: data
            .tech
            .unit_stats(class, tech)
            .map_err(|e| e.to_string())?,
        strength,
        readiness_pct: readiness,
        exp_stars: exp,
        entrenchment: entrench,
    })
}

fn plain(data: &GameData) -> Result<GroundAttack, String> {
    Ok(GroundAttack {
        terrain_mult: data
            .terrain
            .get("plain")
            .map_err(|e| e.to_string())?
            .def
            .defense_mult,
        night: false,
        across_canal: false,
    })
}

/// Matrix: no attack always free or always suicidal. Attacks on a class that
/// cannot fire back at the attacker (attack 0) are free by design.
fn check_matrix(data: &GameData, rows: &[MatrixRow]) -> Result<Check, String> {
    let mut by_design = 0;
    let mut bad = Vec::new();
    for r in rows.iter().filter(|r| !r.flag().is_empty()) {
        let attacker = data.units.get(&r.attacker).map_err(|e| e.to_string())?;
        let defender = data.units.get(&r.defender).map_err(|e| e.to_string())?;
        let stats = data
            .tech
            .unit_stats(defender, &tech_at(r.tech))
            .map_err(|e| e.to_string())?;
        if r.flag() == "FREE" && stats.attack.against(attacker.target_class) <= 0.0 {
            by_design += 1;
        } else {
            bad.push(r);
        }
    }
    let mut body = format!(
        "{} сочетаний (класс × класс × местность × окопы 0/2/4 × технологии 1/2), Монте-Карло. «Бесплатно» или «самоубийство» — ≥ {:.0}% прогонов.\n\n- По замыслу бесплатны (цель не может стрелять по атакующему): {by_design}\n- Нарушений: {}\n",
        rows.len(),
        crate::combat_matrix::FLAG_SHARE * 100.0,
        bad.len()
    );
    if !bad.is_empty() {
        body.push_str("\n| тех. | атакующий | цель | местность | окопы | флаг |\n| --- | --- | --- | --- | --- | --- |\n");
        for r in bad.iter().take(30) {
            let _ = writeln!(
                body,
                "| {} | {} | {} | {} | {} | {} |",
                r.tech,
                r.attacker,
                r.defender,
                r.terrain,
                r.entrench,
                r.flag()
            );
        }
    }
    Ok(Check {
        name: "Матрица боёв",
        pass: bad.is_empty(),
        body,
    })
}

/// «Эффект ПТУР»: the GDD example (tanks 10/90%/1★ vs infantry 8/80%, entrenchment 2, plain).
fn check_atgm(data: &GameData) -> Result<Check, String> {
    let ctx = plain(data)?;
    let ratio = |level: u8| -> Result<(f64, f64), String> {
        let tanks_tech = TechLevels::default().with(TechBranch::Armor, level);
        let inf_tech = TechLevels::default().with(TechBranch::Antitank, level);
        let tanks = unit(data, "tank", &tanks_tech, 10, 90, 1, 0)?;
        let inf = unit(data, "infantry", &inf_tech, 8, 80, 0, 2)?;
        let f =
            forecast_ground(&data.rules.combat, &tanks, &inf, &ctx).map_err(|e| e.to_string())?;
        Ok((f.attacker_losses.expected, f.defender_losses.expected))
    };
    let (t67, i67) = ratio(1)?;
    let (t73, i73) = ratio(2)?;
    let (r67, r73) = (t67 / i67, t73 / i73);
    let pass = r67 <= ATGM_1967_MAX_RATIO && r73 >= ATGM_1973_MIN_RATIO;
    let body = format!(
        "| год | уровни | потери танков | потери пехоты | отношение | критерий |\n| --- | --- | --- | --- | --- | --- |\n| 1967 | броня 1, ПТУР 1 | {t67:.2} | {i67:.2} | {r67:.2} | ≤ {:.2} |\n| 1973 | броня 2, ПТУР 2 | {t73:.2} | {i73:.2} | {r73:.2} | ≥ {:.2} |\n",
        ATGM_1967_MAX_RATIO, ATGM_1973_MIN_RATIO
    );
    Ok(Check {
        name: "Эффект ПТУР",
        pass,
        body,
    })
}

/// One phase of attacks by `att` on `def`: (steps inflicted, steps received).
/// Each further strike is made with fatigue on both sides (R-UNIT-01:
/// −5% per own attack, −10% per attack received); artillery bombards
/// (R-COMBAT-04) and takes no return fire.
fn exchange_per_phase(
    data: &GameData,
    att: &Combatant<'_>,
    def: &Combatant<'_>,
    ctx: &GroundAttack,
) -> Result<(f64, f64), String> {
    let rules = &data.rules;
    let fatigue_att = rules.readiness.fatigue_per_attack_pct;
    let fatigue_def = rules.readiness.fatigue_per_attacked_pct;
    let (mut inflicted, mut received) = (0.0, 0.0);
    for k in 0..att.class.strikes {
        let k8 = u8::try_from(k).unwrap_or(u8::MAX);
        let mut a = att.clone();
        let mut d = def.clone();
        a.readiness_pct = a
            .readiness_pct
            .saturating_sub(k8.saturating_mul(fatigue_att));
        d.readiness_pct = d
            .readiness_pct
            .saturating_sub(k8.saturating_mul(fatigue_def));
        if a.class.has_tag(UnitTag::Bombard) {
            let params = fire_params(
                &rules.combat,
                &FireInput {
                    strength: a.strength,
                    readiness_pct: a.readiness_pct,
                    shot_fraction: rules.bombard.shot_fraction,
                    attack: a.stats.attack.against(d.class.target_class),
                    shooter_exp: 0,
                    attack_mult: 1.0,
                    defense: d.stats.defense.ground.unwrap_or(0.0),
                    target_exp: 0,
                    terrain_mult: ctx.terrain_mult,
                    entrenchment: d.entrenchment,
                    defense_mult: 1.0,
                    p_shift: 0.0,
                },
            );
            inflicted += forecast_fire(&rules.combat, &params, d.strength).expected;
        } else {
            let f = forecast_ground(&rules.combat, &a, &d, ctx).map_err(|e| e.to_string())?;
            inflicted += f.defender_losses.expected;
            received += f.attacker_losses.expected;
        }
    }
    Ok((inflicted, received))
}

/// «Стоимость юнитов»: net exchange (inflicted − received) per MPP by class,
/// within ±25% of the mean over the compared classes.
fn check_cost(data: &GameData) -> Result<Check, String> {
    let compared: Vec<&UnitClass> = data
        .units
        .iter()
        .filter(|u| {
            u.target_class.is_ground() && u.strikes > 0 && !SPECIAL_ROLE.contains(&u.id.as_str())
        })
        .collect();
    let defenders: Vec<&UnitClass> = data
        .units
        .iter()
        .filter(|u| u.target_class.is_ground())
        .collect();
    let mut body = String::from("Чистый размен за фазу: ожидаемые потери противника минус свои, по всем ударам фазы (второй удар — с усталостью обеих сторон; артиллерия — обстрелом без ответа), в среднем по всем наземным целям, местностям и окопам 0/2/4, делённый на цену.\n\n| тех. | класс | цена | нанесено | получено | размен | размен / 100 MPP | отклонение |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    let mut pass = true;
    let mut defense_rows: Vec<(u8, String, f64)> = Vec::new();
    for level in [1u8, 2] {
        let tech = tech_at(level);
        let mut rows = Vec::new();
        for att_class in &compared {
            let (mut inf, mut rec, mut n) = (0.0, 0.0, 0u32);
            for def_class in &defenders {
                for terrain in data.terrain.iter().filter(|t| !t.def.impassable) {
                    let mut levels: Vec<u8> = [0u8, 2, 4]
                        .iter()
                        .map(|&e| e.min(terrain.def.max_entrench))
                        .collect();
                    levels.dedup();
                    for entrench in levels {
                        let att = unit(data, &att_class.id, &tech, STRENGTH, READINESS, 0, 0)?;
                        let def =
                            unit(data, &def_class.id, &tech, STRENGTH, READINESS, 0, entrench)?;
                        let ctx = GroundAttack {
                            terrain_mult: terrain.def.defense_mult,
                            night: false,
                            across_canal: false,
                        };
                        let (i, r) = exchange_per_phase(data, &att, &def, &ctx)?;
                        inf += i;
                        rec += r;
                        n += 1;
                    }
                }
            }
            let n = f64::from(n.max(1));
            let (inf, rec) = (inf / n, rec / n);
            // For reference: the same class defending against every compared attacker.
            let (mut d_inf, mut d_rec, mut dn) = (0.0, 0.0, 0u32);
            for other in &compared {
                for terrain in data.terrain.iter().filter(|t| !t.def.impassable) {
                    let mut levels: Vec<u8> = [0u8, 2, 4]
                        .iter()
                        .map(|&e| e.min(terrain.def.max_entrench))
                        .collect();
                    levels.dedup();
                    for entrench in levels {
                        let att = unit(data, &other.id, &tech, STRENGTH, READINESS, 0, 0)?;
                        let def =
                            unit(data, &att_class.id, &tech, STRENGTH, READINESS, 0, entrench)?;
                        let ctx = GroundAttack {
                            terrain_mult: terrain.def.defense_mult,
                            night: false,
                            across_canal: false,
                        };
                        let (i, r) = exchange_per_phase(data, &att, &def, &ctx)?;
                        d_inf += r;
                        d_rec += i;
                        dn += 1;
                    }
                }
            }
            let defense = (d_inf - d_rec) / f64::from(dn.max(1)) / f64::from(att_class.cost_mpp);
            defense_rows.push((level, att_class.id.clone(), defense));
            rows.push((
                att_class.id.clone(),
                att_class.cost_mpp,
                inf,
                rec,
                (inf - rec) / f64::from(att_class.cost_mpp),
            ));
        }
        let mean = rows.iter().map(|r| r.4).sum::<f64>() / rows.len().max(1) as f64;
        for (id, cost, inf, rec, v) in &rows {
            let dev = v / mean - 1.0;
            let ok = dev.abs() <= COST_SPREAD;
            pass &= ok;
            let mark = if ok { "" } else { " ✗" };
            let _ = writeln!(
                body,
                "| {level} | {id} | {cost} | {inf:.2} | {rec:.2} | {:.2} | {:.2} | {:+.0}%{mark} |",
                inf - rec,
                v * 100.0,
                dev * 100.0
            );
        }
    }
    let _ = writeln!(
        body,
        "\nДопуск ±{:.0}%. Не сравниваются (особая роль): {}.",
        COST_SPREAD * 100.0,
        SPECIAL_ROLE.join(", ")
    );
    body.push_str("\nДля справки (в критерий не входит) — размен того же класса в обороне против всех сравниваемых атакующих, на 100 MPP:\n\n| тех. | класс | оборона / 100 MPP |\n| --- | --- | --- |\n");
    for (level, id, v) in &defense_rows {
        let _ = writeln!(body, "| {level} | {id} | {:.2} |", v * 100.0);
    }
    Ok(Check {
        name: "Размен за MPP",
        pass,
        body,
    })
}

/// One air strike under `sams` umbrellas: (aircraft steps lost, target steps lost).
fn strike_exchange(data: &GameData, sams: usize, ew_level: u8) -> Result<(f64, f64), String> {
    let rules = &data.rules;
    let air_tech = TechLevels::default()
        .with(TechBranch::Aviation, 2)
        .with(TechBranch::Ew, ew_level);
    let sam_tech = TechLevels::default().with(TechBranch::AirDefense, 2);
    let jet = unit(data, "strike", &air_tech, STRENGTH, 90, 0, 0)?;
    let target = unit(
        data,
        "infantry",
        &TechLevels::default(),
        STRENGTH,
        READINESS,
        0,
        2,
    )?;
    let sam = unit(data, "sam_mobile", &sam_tech, 8, 90, 0, 0)?;
    let ew = data
        .tech
        .side_modifier(SideModifier::SamHitPenalty, &air_tech)
        .map_err(|e| e.to_string())?;
    let all: Vec<Combatant<'_>> = (0..sams).map(|_| sam.clone()).collect();
    let shots: Vec<(u32, f64)> = sam_barrage(rules, &all, &jet, ew)
        .iter()
        .map(|s| (s.params.n, s.params.p))
        .collect();
    let sam_dist = hits_distribution(&shots, u32::from(jet.strength));
    let (mut jet_loss, mut target_loss) = (0.0, 0.0);
    for (k, &w) in sam_dist.iter().enumerate() {
        let left = jet.strength.saturating_sub(k as u8);
        let mut survivor = jet.clone();
        survivor.strength = left;
        // R-AIR-02: combat formula, D = target's air defense; return fire by its air attack only.
        let hit = fire_params(
            &rules.combat,
            &FireInput {
                strength: left,
                readiness_pct: survivor.readiness_pct,
                shot_fraction: 1.0,
                attack: survivor.stats.attack.against(target.class.target_class),
                shooter_exp: 0,
                attack_mult: 1.0,
                defense: target.stats.defense.air,
                target_exp: 0,
                terrain_mult: 1.0,
                entrenchment: 0,
                defense_mult: 1.0,
                p_shift: 0.0,
            },
        );
        let back = fire_params(
            &rules.combat,
            &FireInput {
                strength: target.strength,
                readiness_pct: target.readiness_pct,
                shot_fraction: 1.0,
                attack: target.stats.attack.air,
                shooter_exp: 0,
                attack_mult: 1.0,
                defense: survivor.stats.defense.air,
                target_exp: 0,
                terrain_mult: 1.0,
                entrenchment: 0,
                defense_mult: 1.0,
                p_shift: 0.0,
            },
        );
        let LossForecast { expected: t, .. } = forecast_fire(&rules.combat, &hit, target.strength);
        let LossForecast { expected: j, .. } = forecast_fire(&rules.combat, &back, left);
        target_loss += w * t;
        jet_loss += w * (k as f64 + j);
    }
    Ok((jet_loss, target_loss))
}

/// «Зонтики ПВО»: under two umbrellas a strike costs more MPP than it destroys,
/// unless EW (level ≥ 2) helps.
fn check_umbrellas(data: &GameData) -> Result<Check, String> {
    let step = |id: &str| -> Result<f64, String> {
        Ok(
            f64::from(data.units.get(id).map_err(|e| e.to_string())?.cost_mpp)
                / f64::from(STRENGTH),
        )
    };
    let (jet_step, target_step) = (step("strike")?, step("infantry")?);
    let mut body = String::from("Ударная авиация ур. 2 (сила 10, 90%) бьёт пехоту (сила 10, 80%) под зонтиками мобильных ЗРК ур. 2 (сила 8, 90%). Обмен в MPP: потерянные шаги × цена шага.\n\n| зонтиков | РЭБ | потери самолётов, шагов | потери цели, шагов | потеряно MPP | уничтожено MPP | потеряно / уничтожено |\n| --- | --- | --- | --- | --- | --- | --- |\n");
    let mut results = std::collections::BTreeMap::new();
    for sams in [0usize, 1, 2] {
        for ew in [1u8, 2, 3] {
            let (j, t) = strike_exchange(data, sams, ew)?;
            let (lost, killed) = (j * jet_step, t * target_step);
            results.insert((sams, ew), lost / killed);
            let _ = writeln!(
                body,
                "| {sams} | {ew} | {j:.2} | {t:.2} | {lost:.1} | {killed:.1} | {:.2} |",
                lost / killed
            );
        }
    }
    let r = |s: usize, e: u8| results.get(&(s, e)).copied().unwrap_or(f64::NAN);
    let targets = [
        (
            "1 зонтик, без РЭБ",
            r(1, 1),
            UMBRELLA_ONE_RANGE.0 <= r(1, 1) && r(1, 1) <= UMBRELLA_ONE_RANGE.1,
            format!("{:.1}–{:.1}", UMBRELLA_ONE_RANGE.0, UMBRELLA_ONE_RANGE.1),
        ),
        (
            "1 зонтик, РЭБ ур. 2",
            r(1, 2),
            r(1, 2) <= UMBRELLA_ONE_EW_MAX,
            format!("≤ {UMBRELLA_ONE_EW_MAX:.1}"),
        ),
        (
            "2 зонтика, без РЭБ",
            r(2, 1),
            r(2, 1) >= UMBRELLA_TWO_MIN,
            format!("≥ {UMBRELLA_TWO_MIN:.1}"),
        ),
    ];
    body.push_str("\nЦелевые соотношения «потеряно MPP / уничтожено MPP»:\n\n| случай | соотношение | цель | итог |\n| --- | --- | --- | --- |\n");
    let mut pass = true;
    for (name, v, ok, goal) in &targets {
        pass &= *ok;
        let _ = writeln!(
            body,
            "| {name} | {v:.2} | {goal} | {} |",
            if *ok { "да" } else { "**нет**" }
        );
    }
    Ok(Check {
        name: "Зонтики ПВО",
        pass,
        body,
    })
}

pub fn run(o: &Options) -> Result<(), String> {
    let data = load(&o.data)?;
    let rows = matrix(&data, o.runs, o.seed, STRENGTH, READINESS)?;
    let checks = [
        check_matrix(&data, &rows)?,
        check_atgm(&data)?,
        check_cost(&data)?,
        check_umbrellas(&data)?,
    ];
    let mut s = String::from("# Баланс: проверки раздела 17\n\n");
    let _ = writeln!(
        s,
        "Сгенерирован `sim balance` (прогонов на ячейку матрицы: {}, сид {}). Юниты по умолчанию: сила {STRENGTH}, готовность {READINESS}%, без опыта. Проверки темпа и исхода сценария (ИИ против ИИ) — этап 4.\n",
        o.runs, o.seed
    );
    s.push_str("| проверка | итог |\n| --- | --- |\n");
    for c in &checks {
        let _ = writeln!(
            s,
            "| {} | {} |",
            c.name,
            if c.pass {
                "пройдена"
            } else {
                "**не пройдена**"
            }
        );
    }
    for c in &checks {
        let _ = write!(s, "\n## {}\n\n{}", c.name, c.body);
    }
    if let Some(dir) = o.report.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&o.report, s).map_err(|e| format!("{}: {e}", o.report.display()))?;
    let failed: Vec<&str> = checks.iter().filter(|c| !c.pass).map(|c| c.name).collect();
    eprintln!("report: {}; failed: {failed:?}", o.report.display());
    if failed.is_empty() {
        Ok(())
    } else {
        Err("balance checks failed".into())
    }
}
