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

/// Reference classes for the combat value (decision A, R-ECO-02 prices).
const REFERENCE: [&str; 3] = ["infantry", "mech", "tank"];
/// Classes whose prices are searched, with the price grid (min, max, step).
const PRICED: [(&str, u32, u32); 4] = [
    ("infantry", 50, 70),
    ("mech", 70, 90),
    ("tank", 100, 140),
    ("artillery", 50, 70),
];
const PRICE_STEP: usize = 5;

/// Entrenchment after one own turn without moving (R-SUP-03: class growth,
/// capped by the terrain).
fn one_turn_entrench(class: &UnitClass, max: u8) -> u8 {
    u8::try_from(class.entrench_per_turn)
        .unwrap_or(u8::MAX)
        .min(max)
}

/// Combat value of a class (decision A): mean over passable terrains of
/// (a) the net exchange of its attacks on each reference defender and
/// (b) its net exchange when each reference attacker attacks it; both per
/// phase (all strikes, fatigue on the second), entrenchment of one turn.
fn combat_value(data: &GameData, id: &str, level: u8) -> Result<(f64, f64), String> {
    let tech = tech_at(level);
    let class = data.units.get(id).map_err(|e| e.to_string())?;
    let (mut att_sum, mut def_sum, mut n) = (0.0, 0.0, 0u32);
    for terrain in data.terrain.iter().filter(|t| !t.def.impassable) {
        let ctx = GroundAttack {
            terrain_mult: terrain.def.defense_mult,
            night: false,
            across_canal: false,
        };
        for r in REFERENCE {
            let rc = data.units.get(r).map_err(|e| e.to_string())?;
            // (a) the class attacks a dug-in reference defender.
            let att = unit(data, id, &tech, STRENGTH, READINESS, 0, 0)?;
            let def = unit(
                data,
                r,
                &tech,
                STRENGTH,
                READINESS,
                0,
                one_turn_entrench(rc, terrain.def.max_entrench),
            )?;
            let (i, rec) = exchange_per_phase(data, &att, &def, &ctx)?;
            att_sum += i - rec;
            // (b) a reference attacker attacks the dug-in class.
            let att = unit(data, r, &tech, STRENGTH, READINESS, 0, 0)?;
            let def = unit(
                data,
                id,
                &tech,
                STRENGTH,
                READINESS,
                0,
                one_turn_entrench(class, terrain.def.max_entrench),
            )?;
            let (i, rec) = exchange_per_phase(data, &att, &def, &ctx)?;
            def_sum += rec - i;
            n += 1;
        }
    }
    let n = f64::from(n.max(1));
    Ok((att_sum / n, def_sum / n))
}

/// Steps exchanged between a class and each reference class, summed over
/// passable terrains: `[attack_inflicted, attack_received, defence_inflicted,
/// defence_received]` per reference, same setting as [`combat_value`].
fn exchange_steps(data: &GameData, id: &str, level: u8) -> Result<Vec<[f64; 4]>, String> {
    let tech = tech_at(level);
    let class = data.units.get(id).map_err(|e| e.to_string())?;
    let mut out = vec![[0.0; 4]; REFERENCE.len()];
    for terrain in data.terrain.iter().filter(|t| !t.def.impassable) {
        let ctx = GroundAttack {
            terrain_mult: terrain.def.defense_mult,
            night: false,
            across_canal: false,
        };
        for (k, r) in REFERENCE.iter().enumerate() {
            let rc = data.units.get(r).map_err(|e| e.to_string())?;
            let att = unit(data, id, &tech, STRENGTH, READINESS, 0, 0)?;
            let def = unit(
                data,
                r,
                &tech,
                STRENGTH,
                READINESS,
                0,
                one_turn_entrench(rc, terrain.def.max_entrench),
            )?;
            let (i, rec) = exchange_per_phase(data, &att, &def, &ctx)?;
            out[k][0] += i;
            out[k][1] += rec;
            let att = unit(data, r, &tech, STRENGTH, READINESS, 0, 0)?;
            let def = unit(
                data,
                id,
                &tech,
                STRENGTH,
                READINESS,
                0,
                one_turn_entrench(class, terrain.def.max_entrench),
            )?;
            let (i, rec) = exchange_per_phase(data, &att, &def, &ctx)?;
            out[k][2] += rec;
            out[k][3] += i;
        }
    }
    Ok(out)
}

/// MPP exchange ratio: enemy MPP destroyed / own MPP lost (a step is 1/10 of
/// the price), attack and defence together. `prices[k]` is the price of
/// `PRICED[k]`; reference prices are looked up there.
fn mpp_ratio(steps: &[[f64; 4]], own_price: u32, prices: &[u32]) -> f64 {
    let price_of = |id: &str| {
        PRICED
            .iter()
            .position(|p| p.0 == id)
            .map_or(0.0, |k| f64::from(prices[k]))
    };
    let (mut destroyed, mut lost) = (0.0, 0.0);
    for (k, r) in REFERENCE.iter().enumerate() {
        destroyed += (steps[k][0] + steps[k][2]) * price_of(r) / 10.0;
        lost += (steps[k][1] + steps[k][3]) * f64::from(own_price) / 10.0;
    }
    if lost > 0.0 {
        destroyed / lost
    } else {
        f64::INFINITY
    }
}

/// For discussion (not the criterion): the MPP exchange ratio is positive by
/// construction, so «±25% of the mean» is meaningful and prices matter.
fn ratio_variant(data: &GameData, ids: &[&str], current: &[u32]) -> Result<String, String> {
    let mut steps = Vec::new();
    for level in [1u8, 2] {
        steps.push(
            ids.iter()
                .map(|id| exchange_steps(data, id, level))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let devs = |prices: &[u32]| -> Vec<Vec<f64>> {
        steps
            .iter()
            .map(|lvl| {
                let r: Vec<f64> = lvl
                    .iter()
                    .zip(prices)
                    .map(|(s, p)| mpp_ratio(s, *p, prices))
                    .collect();
                let mean = r.iter().sum::<f64>() / r.len() as f64;
                r.iter().map(|v| v / mean - 1.0).collect()
            })
            .collect()
    };
    let worst = |d: &[Vec<f64>]| d.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs()));
    let grids: Vec<Vec<u32>> = PRICED
        .iter()
        .map(|(_, lo, hi)| (*lo..=*hi).step_by(PRICE_STEP).collect())
        .collect();
    let mut best_ok: Option<(u32, Vec<u32>)> = None;
    let mut best_any: Option<(f64, Vec<u32>)> = None;
    let mut idx = vec![0usize; grids.len()];
    loop {
        let prices: Vec<u32> = idx.iter().zip(&grids).map(|(i, g)| g[*i]).collect();
        let w = worst(&devs(&prices));
        let distance: u32 = prices
            .iter()
            .zip(current)
            .map(|(p, c)| p.abs_diff(*c))
            .sum();
        if w <= COST_SPREAD && best_ok.as_ref().is_none_or(|(d, _)| distance < *d) {
            best_ok = Some((distance, prices.clone()));
        }
        if best_any.as_ref().is_none_or(|(b, _)| w < *b) {
            best_any = Some((w, prices.clone()));
        }
        let mut k = 0;
        while k < idx.len() {
            idx[k] += 1;
            if idx[k] < grids[k].len() {
                break;
            }
            idx[k] = 0;
            k += 1;
        }
        if k == idx.len() {
            break;
        }
    }
    let mut s = String::from("\n### Для обсуждения: отношение «уничтожено MPP противника / потеряно своих MPP»\n\nНе критерий. Шаг силы стоит 10% цены; атака и оборона против тех же эталонных классов и при тех же условиях суммируются. Отношение всегда положительно, поэтому «±25% от среднего» имеет смысл, а цена влияет и на числитель (цены эталонов), и на знаменатель (своя цена).\n\n");
    let mut table = |title: String, prices: &[u32]| {
        let d = devs(prices);
        let _ = writeln!(s, "{title}\n\n| тех. | класс | цена | отношение | отклонение |\n| --- | --- | --- | --- | --- |");
        for (li, level) in [1u8, 2].iter().enumerate() {
            for (k, id) in ids.iter().enumerate() {
                let r = mpp_ratio(&steps[li][k], prices[k], prices);
                let _ = writeln!(
                    s,
                    "| {level} | {id} | {} | {r:.2} | {:+.0}%{} |",
                    prices[k],
                    d[li][k] * 100.0,
                    if d[li][k].abs() <= COST_SPREAD {
                        ""
                    } else {
                        " ✗"
                    }
                );
            }
        }
        s.push('\n');
    };
    table("Текущие цены:".into(), current);
    match (best_ok, best_any) {
        (Some((dist, p)), _) => table(
            format!("Ближайший к текущим проходящий набор (сумма |Δ| = {dist} MPP):"),
            &p,
        ),
        (None, Some((w, p))) => table(
            format!(
                "Ни один набор не проходит; лучший по наибольшему отклонению ({:.0}%):",
                w * 100.0
            ),
            &p,
        ),
        (None, None) => {}
    }
    Ok(s)
}

/// Deviation of each class's value per MPP from the mean over the priced
/// classes, at one tech level.
fn deviations(values: &[f64], prices: &[u32]) -> Vec<f64> {
    let per: Vec<f64> = values
        .iter()
        .zip(prices)
        .map(|(v, p)| v / f64::from(*p))
        .collect();
    let mean = per.iter().sum::<f64>() / per.len() as f64;
    per.iter().map(|v| v / mean - 1.0).collect()
}

/// «Стоимость юнитов» (decision A): combat value per MPP within ±25% of the
/// mean, tech 1 and 2 separately; otherwise a search over the price grid.
fn check_cost(data: &GameData) -> Result<Check, String> {
    let ids: Vec<&str> = PRICED.iter().map(|p| p.0).collect();
    let current: Vec<u32> = ids
        .iter()
        .map(|id| {
            data.units
                .get(id)
                .map(|c| c.cost_mpp)
                .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    // values[level][class] = (attack, defence, combat value)
    let mut values = Vec::new();
    for level in [1u8, 2] {
        let mut row = Vec::new();
        for id in &ids {
            let (a, d) = combat_value(data, id, level)?;
            row.push((a, d, (a + d) / 2.0));
        }
        values.push(row);
    }
    let mut body = String::from(
        "Боевая ценность за MPP (решение A): среднее (а) чистого размена в атаке против эталонных защитников (пехота, мех. бригада, танки) и (б) чистого размена в обороне, когда класс атакуют те же эталонные классы. Размен — ожидаемые потери противника минус свои за все удары фазы (второй удар с усталостью обеих сторон; артиллерия в атаке — обстрелом без ответа). Окопы обороняющегося — за один ход без движения (`entrench_per_turn`: пехота 2, остальные 1, не больше максимума местности). Среднее по проходимой местности. Сравниваются пехота, мех. бригада, танки, артиллерия; тех. 1 и 2 отдельно.\n\n| тех. | класс | цена | атака (а) | оборона (б) | ценность | на 100 MPP | отклонение |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    let value = |level: usize| -> Vec<f64> { values[level].iter().map(|v| v.2).collect() };
    let mut pass = true;
    for (li, level) in [1u8, 2].iter().enumerate() {
        let dev = deviations(&value(li), &current);
        for (k, id) in ids.iter().enumerate() {
            let (a, d, v) = values[li][k];
            let ok = dev[k].abs() <= COST_SPREAD;
            pass &= ok;
            let _ = writeln!(
                body,
                "| {level} | {id} | {} | {a:.2} | {d:.2} | {v:.2} | {:.2} | {:+.0}%{} |",
                current[k],
                v / f64::from(current[k]) * 100.0,
                dev[k] * 100.0,
                if ok { "" } else { " ✗" }
            );
        }
    }
    let _ = writeln!(body, "\nДопуск ±{:.0}%.", COST_SPREAD * 100.0);
    if !pass {
        // Price grid: every set, the worst deviation over both tech levels.
        let grids: Vec<Vec<u32>> = PRICED
            .iter()
            .map(|(_, lo, hi)| (*lo..=*hi).step_by(PRICE_STEP).collect())
            .collect();
        let mut best_ok: Option<(u32, Vec<u32>)> = None;
        let mut best_any: Option<(f64, Vec<u32>)> = None;
        let mut sets = 0;
        let mut idx = vec![0usize; grids.len()];
        loop {
            let prices: Vec<u32> = idx.iter().zip(&grids).map(|(i, g)| g[*i]).collect();
            sets += 1;
            let worst = (0..2)
                .flat_map(|l| deviations(&value(l), &prices))
                .fold(0.0f64, |m, d| m.max(d.abs()));
            let distance: u32 = prices
                .iter()
                .zip(&current)
                .map(|(p, c)| p.abs_diff(*c))
                .sum();
            if worst <= COST_SPREAD && best_ok.as_ref().is_none_or(|(d, _)| distance < *d) {
                best_ok = Some((distance, prices.clone()));
            }
            if best_any.as_ref().is_none_or(|(w, _)| worst < *w) {
                best_any = Some((worst, prices.clone()));
            }
            // Next combination (odometer).
            let mut k = 0;
            while k < idx.len() {
                idx[k] += 1;
                if idx[k] < grids[k].len() {
                    break;
                }
                idx[k] = 0;
                k += 1;
            }
            if k == idx.len() {
                break;
            }
        }
        let grid_desc: Vec<String> = PRICED
            .iter()
            .map(|(id, lo, hi)| format!("{id} {lo}–{hi}"))
            .collect();
        let _ = writeln!(
            body,
            "\nТекущие цены не проходят. Перебор цен с шагом {PRICE_STEP} ({}): {sets} наборов.\n",
            grid_desc.join(", ")
        );
        let (title, prices) = match (&best_ok, &best_any) {
            (Some((d, p)), _) => (
                format!("Проходят; ближайший к текущим набор (сумма |Δ| = {d} MPP)"),
                p.clone(),
            ),
            (None, Some((w, p))) => (
                format!(
                    "Ни один набор не проходит. Лучший по наибольшему отклонению ({:.0}%)",
                    w * 100.0
                ),
                p.clone(),
            ),
            (None, None) => ("Нет наборов".into(), current.clone()),
        };
        let _ = writeln!(body, "{title}:\n\n| тех. | класс | цена сейчас → предложено | на 100 MPP | отклонение |\n| --- | --- | --- | --- | --- |");
        for (li, level) in [1u8, 2].iter().enumerate() {
            let dev = deviations(&value(li), &prices);
            for (k, id) in ids.iter().enumerate() {
                let _ = writeln!(
                    body,
                    "| {level} | {id} | {} → {} | {:.2} | {:+.0}%{} |",
                    current[k],
                    prices[k],
                    values[li][k].2 / f64::from(prices[k]) * 100.0,
                    dev[k] * 100.0,
                    if dev[k].abs() <= COST_SPREAD {
                        ""
                    } else {
                        " ✗"
                    }
                );
            }
        }
    }
    if !pass {
        let _ = writeln!(
            body,
            "\nПочему не проходит ни один набор: чистый размен у классов близок к нулю и разного знака (пехота и артиллерия — в минусе, танки — в плюсе), среднее по классам около нуля; цена делит ценность, но не меняет её знак, поэтому отклонения от среднего остаются сотнями процентов при любых ценах."
        );
        body.push_str(&ratio_variant(data, &ids, &current)?);
    }
    Ok(Check {
        name: "Боевая ценность за MPP",
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
