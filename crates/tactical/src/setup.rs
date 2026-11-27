//! R-TAC-03 → a tactical battle: the strategic snapshot (plus the support
//! the core's snapshot does not carry yet) becomes a 10 × 14 battle.
//! T-SCALE-01/02, T-UNIT-01/02, T-SUP-01/02.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ceasefire_core::data::GameData;
use ceasefire_core::hex::Hex;
use ceasefire_core::rng::Pcg32;
use ceasefire_core::tactical::{Origin, Role, TacticalScenario, TacticalUnit};
use ceasefire_core::tech::{TechBranch, TechLevels};
use serde::{Deserialize, Serialize};

use crate::battle::{Battle, Support, TacUnit, TurnMode};
use crate::map::{Cell, TacMap};
use crate::rules::TacRules;
use crate::TacError;

/// What one side brings beyond its brigade (T-SUP-01). The core's snapshot
/// (R-TAC-03) does not carry these yet: `sim --dump-battles` fills them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SideSupport {
    #[serde(default)]
    pub artillery_supply: Option<u8>,
    #[serde(default)]
    pub air_supply: Option<u8>,
    #[serde(default)]
    pub air_readiness_pct: u8,
    #[serde(default)]
    pub hq_rating: Option<u8>,
    #[serde(default)]
    pub reserve_class: Option<String>,
    #[serde(default)]
    pub engineers: bool,
    #[serde(default)]
    pub enemy_sam: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SupportInfo {
    #[serde(default)]
    pub attacker: SideSupport,
    #[serde(default)]
    pub defender: SideSupport,
}

/// The input of a tactical battle: the core's snapshot and the support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleInput {
    pub scenario: TacticalScenario,
    #[serde(default)]
    pub support: SupportInfo,
}

fn role_ix(r: Role) -> usize {
    match r {
        Role::Attacker => 0,
        Role::Defender => 1,
    }
}

fn pick<'a>(rng: &mut Pcg32, mix: &'a [(String, u32)]) -> &'a str {
    let total: u32 = mix.iter().map(|m| m.1).sum::<u32>().max(1);
    let mut x = rng.next_u32() % total;
    for (t, w) in mix {
        if x < *w {
            return t;
        }
        x -= w;
    }
    mix.first().map_or("open", |m| m.0.as_str())
}

/// T-SCALE-01: a synthetic map by seed (night interpretation): the top half
/// from the defender's hex terrain, the bottom half from the attacker's, the
/// side columns from the neighbours; 2–3 key points in the top half (highest,
/// then fortified and villages, 2 apart); a canal across the middle if the
/// attack crosses one.
pub fn synthetic_map(rules: &TacRules, sc: &TacticalScenario, rng: &mut Pcg32) -> TacMap {
    let (cols, rows) = (rules.map.cols, rules.map.rows);
    let terrain_at = |dq: i32, dr: i32| {
        sc.area
            .iter()
            .find(|a| a.dq == dq && a.dr == dr)
            .map_or("plain".to_owned(), |a| a.terrain.clone())
    };
    let att = sc.units.iter().find(|u| u.role == Role::Attacker);
    let def_t = terrain_at(0, 0);
    let att_t = att.map_or(def_t.clone(), |u| terrain_at(u.dq, u.dr));
    let neighbours: Vec<String> = sc
        .area
        .iter()
        .filter(|a| a.dq != 0 || a.dr != 0)
        .map(|a| a.terrain.clone())
        .collect();
    let default_mix = rules
        .terrain_mix
        .get("default")
        .cloned()
        .unwrap_or_default();
    let mix_of = |t: &str| {
        rules
            .terrain_mix
            .get(t)
            .cloned()
            .unwrap_or_else(|| default_mix.clone())
    };
    let levels_of = |t: &str| {
        rules
            .hill_levels
            .get(t)
            .or_else(|| rules.hill_levels.get("default"))
            .copied()
            .unwrap_or((1, 1))
    };
    let mut cells = BTreeMap::new();
    for row in 0..rows {
        for col in 0..cols {
            let strategic = if col == 0 || col == cols - 1 {
                neighbours
                    .get(((row + col) as usize) % neighbours.len().max(1))
                    .cloned()
                    .unwrap_or_else(|| def_t.clone())
            } else if row < rows / 2 {
                def_t.clone()
            } else {
                att_t.clone()
            };
            let mix = mix_of(&strategic);
            let t = pick(rng, &mix).to_owned();
            let level = if t == "hill" {
                let (a, b) = levels_of(&strategic);
                a + (rng.next_u32() % u32::from(b.saturating_sub(a) + 1)) as u8
            } else {
                0
            };
            cells.insert(
                TacMap::at(col, row),
                Cell {
                    terrain: t,
                    level,
                    mine: false,
                    mine_known: false,
                },
            );
        }
    }
    let mut map = TacMap {
        cols,
        rows,
        cells,
        key_points: Vec::new(),
        canal: BTreeSet::new(),
        bridges: BTreeSet::new(),
    };
    let span = rules.map.key_points_max - rules.map.key_points_min + 1;
    let k = rules.map.key_points_min + (rng.next_u32() as usize % span);
    let mut cand: Vec<(i32, Hex)> = map
        .cells
        .iter()
        .filter(|(h, _)| {
            let (c, r) = TacMap::offset(**h);
            r >= 1 && r < rows / 2 && c >= 1 && c < cols - 1
        })
        .map(|(h, c)| {
            let bonus = match c.terrain.as_str() {
                "fortified" => 3,
                "village" => 2,
                "hill" => 1,
                _ => 0,
            };
            (i32::from(c.level) * 2 + bonus, *h)
        })
        .collect();
    cand.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, h) in cand {
        if map.key_points.len() >= k {
            break;
        }
        if map.key_points.iter().all(|p| p.distance(h) >= 2) {
            map.key_points.push(h);
        }
    }
    if sc.across_canal {
        for col in 0..cols {
            let a = TacMap::at(col, rows / 2 - 1);
            for n in a.neighbors() {
                if TacMap::offset(n).1 >= rows / 2 && map.cells.contains_key(&n) {
                    map.add_canal(a, n);
                }
            }
        }
    }
    map
}

/// T-UNIT-01: a brigade → battalions and companies. Strength split evenly,
/// the remainder to the first ones (10 → 4, 3, 3); extra companies 1 step.
pub fn breakdown(rules: &TacRules, u: &TacticalUnit, tech: &TechLevels) -> Vec<(String, u8)> {
    let Some(b) = rules.breakdown.get(&u.class) else {
        return Vec::new();
    };
    let n = b.battalions.len().max(1) as u8;
    let base = u.strength / n;
    let rest = u.strength % n;
    let mut out: Vec<(String, u8)> = b
        .battalions
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), base + u8::from((i as u8) < rest)))
        .filter(|(_, s)| *s > 0)
        .collect();
    for k in &b.extra {
        out.push((k.clone(), rules.extra_steps));
    }
    if let Some(x) = &b.extra_if_antitank {
        if tech.level(TechBranch::Antitank).unwrap_or(1) >= x.level {
            out.push((x.kind.clone(), rules.extra_steps));
        }
    }
    out
}

/// Starting hexes: the defender in the top half around the key points, the
/// attacker on the bottom rows.
fn spots(map: &TacMap, role: Role, n: usize) -> Vec<Hex> {
    let (cols, rows) = (map.cols, map.rows);
    let mut cand: Vec<Hex> = map.cells.keys().copied().collect();
    match role {
        Role::Defender => {
            let kp = map.key_points.clone();
            cand.retain(|h| TacMap::offset(*h).1 < rows / 2);
            cand.sort_by_key(|h| (kp.iter().map(|k| k.distance(*h)).min().unwrap_or(0), *h));
        }
        Role::Attacker => {
            cand.retain(|h| TacMap::offset(*h).1 >= rows - 3);
            cand.sort_by_key(|h| {
                let (c, r) = TacMap::offset(*h);
                ((rows - 1 - r) * cols + (c - cols / 2).abs(), *h)
            });
        }
    }
    cand.into_iter().take(n).collect()
}

struct Builder<'a> {
    rules: &'a TacRules,
    data: &'a GameData,
    sc: &'a TacticalScenario,
    units: BTreeMap<u32, TacUnit>,
    next: u32,
}

impl Builder<'_> {
    fn tech(&self, role: Role) -> TechLevels {
        self.sc
            .sides
            .iter()
            .find(|s| s.role == role)
            .map(|s| s.tech.clone())
            .unwrap_or_default()
    }

    fn stats(&self, class: &str, role: Role) -> Result<ceasefire_core::tech::UnitStats, TacError> {
        let c = self
            .data
            .units
            .get(class)
            .map_err(|e| TacError::Data(e.to_string()))?;
        self.data
            .tech
            .unit_stats(c, &self.tech(role))
            .map_err(|e| TacError::Data(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        map: &TacMap,
        role: Role,
        kind: &str,
        steps: u8,
        src: &TacticalUnit,
        hex: Hex,
        arrives: Option<u32>,
    ) -> Result<(), TacError> {
        let k = self.rules.kind(kind)?;
        let st = self.stats(&k.class, role)?;
        let start = map
            .cell(hex)
            .and_then(|c| self.rules.terrain.get(&c.terrain))
            .map_or(0, |t| t.start_entrench);
        let entrench = if role == Role::Defender && arrives.is_none() {
            src.entrenchment.min(self.rules.entrench.max).max(start)
        } else {
            start
        };
        let id = self.next;
        self.next += 1;
        self.units.insert(
            id,
            TacUnit {
                id,
                role,
                kind: kind.to_owned(),
                attack_soft: if k.has("armor_only") {
                    0.0
                } else {
                    st.attack.soft
                },
                attack_hard: st.attack.hard,
                defense: st.defense.ground.unwrap_or(1.0),
                air_defense: st.defense.air,
                armor: k.has("armor"),
                steps,
                damage: 0,
                readiness_pct: src.readiness_pct,
                exp_stars: (src.exp / 100) as u8,
                entrench,
                hex,
                arrives,
                revealed_until: 0,
                suppressed: false,
                hit_this_turn: false,
                moved_this_turn: false,
                overwatch: false,
                activated: false,
            },
        );
        Ok(())
    }
}

/// Builds the battle from a snapshot; `seed` fixes the map and the dice
/// (T-RES-04).
pub fn build_battle(
    rules: Arc<TacRules>,
    data: &GameData,
    input: &BattleInput,
    seed: u64,
    mode: TurnMode,
) -> Result<Battle, TacError> {
    let sc = &input.scenario;
    let mut rng = Pcg32::new(seed, 0x7ac7);
    let mut map = synthetic_map(&rules, sc, &mut rng);
    let mut b = Builder {
        rules: &rules,
        data,
        sc,
        units: BTreeMap::new(),
        next: 1,
    };
    let mut support = [Support::default(), Support::default()];
    for role in [Role::Attacker, Role::Defender] {
        let Some(src) = sc.units.iter().find(|u| u.role == role) else {
            continue;
        };
        let tech = b.tech(role);
        let parts = breakdown(&rules, src, &tech);
        let places = spots(&map, role, parts.len());
        for ((kind, steps), hex) in parts.iter().zip(places) {
            b.add(&map, role, kind, *steps, src, hex, None)?;
        }
        let side = match role {
            Role::Attacker => &input.support.attacker,
            Role::Defender => &input.support.defender,
        };
        // T-SUP-01: a reserve battalion from an adjacent brigade, side edge, turn 3.
        if let Some(class) = &side.reserve_class {
            let fake = TacticalUnit {
                class: class.clone(),
                strength: 10,
                ..src.clone()
            };
            if let Some((kind, steps)) = breakdown(&rules, &fake, &tech).into_iter().next() {
                let row = match role {
                    Role::Attacker => map.rows - 3,
                    Role::Defender => 2,
                };
                b.add(
                    &map,
                    role,
                    &kind,
                    steps,
                    src,
                    TacMap::at(0, row),
                    Some(rules.reserve.arrival_turn),
                )?;
            }
        }
        // T-SUP-02: calls by the sources' supply; +1 any from an HQ rating ≥ 8.
        let (arty, _) = side
            .artillery_supply
            .map_or((0, 0), |x| rules.support.band(x));
        let (_, air) = side.air_supply.map_or((0, 0), |x| rules.support.band(x));
        let art = b.stats("artillery", role)?;
        let air_st = b.stats("strike", role)?;
        let inf = b.stats("infantry", role)?;
        let s = &mut support[role_ix(role)];
        s.artillery = arty;
        s.air = if sc.night || side.air_readiness_pct < rules.support.air_min_readiness_pct {
            0
        } else {
            air
        };
        s.mortar = rules.breakdown.get(&src.class).map_or(0, |bd| {
            bd.support.iter().filter(|x| *x == "mortar").count() as u8
        });
        s.any = u8::from(
            side.hq_rating
                .is_some_and(|r| r >= rules.support.hq_bonus_rating),
        );
        s.enemy_sam = side.enemy_sam;
        s.artillery_soft = art.attack.soft;
        s.artillery_hard = art.attack.hard;
        s.air_soft = air_st.attack.soft;
        s.air_hard = air_st.attack.hard;
        s.mortar_soft = inf.attack.soft;
        s.mortar_hard = inf.attack.soft * 0.5;
    }
    // T-SUP-01: the defender's engineers lay 1–2 hidden minefields on the approach.
    if input.support.defender.engineers {
        let span = rules.mines.fields_max - rules.mines.fields_min + 1;
        let n = rules.mines.fields_min + (rng.next_u32() as usize % span);
        let row = map.rows / 2;
        let mut cols: Vec<i32> = (1..map.cols - 1).collect();
        for i in 0..n {
            if cols.is_empty() {
                break;
            }
            let c = cols.remove(rng.next_u32() as usize % cols.len());
            let h = TacMap::at(c, row + (i as i32 % 2));
            if b.units.values().all(|u| u.hex != h) {
                if let Some(cell) = map.cells.get_mut(&h) {
                    cell.mine = true;
                }
            }
        }
    }
    // The attacker's engineers can bridge the canal.
    if input.support.attacker.engineers && sc.across_canal {
        if let Some(src) = sc.units.iter().find(|u| u.role == Role::Attacker) {
            let taken: BTreeSet<Hex> = b.units.values().map(|u| u.hex).collect();
            let hex = spots(&map, Role::Attacker, 30)
                .into_iter()
                .find(|h| !taken.contains(h))
                .unwrap_or(TacMap::at(map.cols - 1, map.rows - 1));
            b.add(
                &map,
                Role::Attacker,
                "engineer_co",
                rules.extra_steps,
                src,
                hex,
                None,
            )?;
        }
    }
    let max_turns = match sc.origin {
        Origin::Campaign { .. } => rules.turns.campaign,
        Origin::Standalone { .. } => rules.turns.historical,
    };
    let control = map
        .key_points
        .iter()
        .map(|h| (*h, Role::Defender))
        .collect();
    let units = b.units;
    map.validate(&rules)?;
    let mut battle = Battle {
        rules: Arc::clone(&rules),
        map,
        units,
        turn: 1,
        max_turns,
        night: sc.night,
        mode,
        active: Role::Attacker,
        passed: [false; 2],
        activations: 0,
        smoke: BTreeMap::new(),
        lit: BTreeMap::new(),
        fired_from: BTreeMap::new(),
        control,
        steps_lost: [0, 0],
        support,
        pending_bridges: Vec::new(),
        rng: Pcg32::new(seed, 0xba77),
        result: None,
        log: Vec::new(),
    };
    let mut ev = Vec::new();
    battle.start_turn(&mut ev);
    Ok(battle)
}
