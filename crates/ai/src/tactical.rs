//! Tactical AI (docs/tactical.md, T-TECH-01): utilitarian, one command per
//! call, the same forecast as the player; weights with the standard seeded
//! jitter. Order of preference: smoke and artillery → fire at visible targets
//! with the best expected damage → moves toward key points through cover →
//! overwatch. No deep search.

use std::collections::{BTreeMap, VecDeque};

use ceasefire_core::hex::Hex;
use ceasefire_core::tactical::Role;
use ceasefire_tactical::battle::{other, Battle, TacUnit};
use ceasefire_tactical::{Action, SupportCall, TacCommand};

/// SplitMix64 → [0, 1).
fn unit_float(seed: u64, salt: u64) -> f64 {
    let mut z = seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

#[derive(Debug, Clone, Copy)]
struct Weights {
    fire: f64,
    key_point: f64,
    cover: f64,
    exposure: f64,
}

pub struct TacticalAi {
    seed: u64,
}

impl TacticalAi {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    fn weights(&self, b: &Battle, role: Role) -> Weights {
        let a = &b.rules.ai;
        let j = |k: u64| {
            1.0 + a.jitter
                * (unit_float(self.seed, k + if role == Role::Attacker { 0 } else { 10 }) * 2.0
                    - 1.0)
        };
        Weights {
            fire: a.fire_weight * j(1),
            key_point: a.key_point_weight * j(2),
            cover: a.cover_weight * j(3),
            exposure: a.exposure_weight * j(4),
        }
    }

    /// The next command for the active side.
    pub fn next(&self, b: &Battle) -> TacCommand {
        let me = b.active;
        let w = self.weights(b, me);
        let mut options: Vec<(f64, u32, Action)> = Vec::new();
        let mut ready: Vec<&TacUnit> = b.alive(me).filter(|u| !u.activated).collect();
        ready.sort_by_key(|u| u.id);
        for u in &ready {
            options.extend(self.unit_options(b, u, &w));
        }
        options.sort_by(|a, c| c.0.total_cmp(&a.0).then(a.1.cmp(&c.1)));
        let support = self.support(b, me);
        for (_, unit, action) in options {
            // The first legal one (a trial on a copy).
            for call in [support.clone(), None] {
                let cmd = TacCommand::Act {
                    unit,
                    support: call,
                    action: action.clone(),
                };
                if b.clone().apply(cmd.clone()).is_ok() {
                    return cmd;
                }
            }
        }
        match ready.first() {
            Some(u) => TacCommand::Act {
                unit: u.id,
                support: None,
                action: Action::Wait,
            },
            None => TacCommand::Pass,
        }
    }

    /// Support first (T-SUP-01): artillery or aircraft on the most valuable
    /// visible enemy; smoke for an attacker that sees nobody yet.
    fn support(&self, b: &Battle, me: Role) -> Option<SupportCall> {
        let mut seen: Vec<&TacUnit> = b.alive(other(me)).filter(|t| b.visible_to(me, t)).collect();
        seen.sort_by_key(|t| (std::cmp::Reverse(value(b, t)), t.id));
        let art = SupportCall::Artillery {
            hex: Hex::new(0, 0),
        };
        let air = SupportCall::Air { target: 0 };
        if let Some(t) = seen.first() {
            if !b.night
                && b.calls_left(me, &air) > 0
                && b.support[if me == Role::Attacker { 0 } else { 1 }].air > 0
            {
                return Some(SupportCall::Air { target: t.id });
            }
            if b.calls_left(me, &art) > 0 {
                return Some(SupportCall::Artillery { hex: t.hex });
            }
            if b.calls_left(me, &SupportCall::Mortar { hex: t.hex }) > 0 {
                return Some(SupportCall::Mortar { hex: t.hex });
            }
            return None;
        }
        if me == Role::Attacker && b.calls_left(me, &art) >= 2 {
            // Smoke ahead of the front unit, toward the nearest key point.
            let kp = b.map.key_points.first().copied()?;
            let front = b.alive(me).min_by_key(|u| (u.hex.distance(kp), u.id))?;
            let step = b
                .map
                .neighbors(front.hex)
                .into_iter()
                .min_by_key(|h| (h.distance(kp), *h))?;
            if b.rules.ai.smoke_min_threat <= 0.0 || front.hex.distance(kp) > 2 {
                return Some(SupportCall::Smoke { hex: step });
            }
        }
        None
    }

    fn unit_options(&self, b: &Battle, u: &TacUnit, w: &Weights) -> Vec<(f64, u32, Action)> {
        let me = u.role;
        let mut out = Vec::new();
        // Fire and assault.
        for t in b.alive(other(me)).filter(|t| b.visible_to(me, t)) {
            if let Some(f) = b.forecast(u.id, t.id, false, false) {
                let bonus = if b.map.key_points.contains(&t.hex) {
                    1.5
                } else {
                    1.0
                };
                out.push((
                    w.fire * f.expected * bonus * f64::from(value(b, t)) / 10.0 + 0.1,
                    u.id,
                    Action::Fire { target: t.id },
                ));
            }
            if !u.suppressed && u.hex.is_adjacent(t.hex) {
                if let (Some(a), Some(d)) = (
                    b.forecast(u.id, t.id, false, true),
                    b.forecast(t.id, u.id, false, true),
                ) {
                    let kp = if b.map.key_points.contains(&t.hex) {
                        w.key_point
                    } else {
                        0.0
                    };
                    let gain = w.fire * (a.expected - d.expected) + kp;
                    if gain > 0.0 {
                        out.push((gain, u.id, Action::Assault { target: t.id }));
                    }
                }
            }
        }
        // Moves toward key points through cover.
        let here = position_score(b, u, u.hex, w);
        for (h, path) in reachable(b, u) {
            let gain = position_score(b, u, h, w) - here;
            if gain > 0.05 {
                out.push((gain, u.id, Action::Move { path }));
            }
        }
        // Overwatch, entrenching.
        if !u.suppressed && b.alive(other(me)).any(|e| e.hex.distance(u.hex) <= 5) {
            out.push((0.3, u.id, Action::Overwatch));
        }
        if me == Role::Defender && u.entrench < b.rules.entrench.max {
            out.push((0.2, u.id, Action::Entrench));
        }
        out.push((0.0, u.id, Action::Wait));
        out
    }
}

/// Value of a target: its steps (armour × 1.5).
fn value(_b: &Battle, t: &TacUnit) -> u32 {
    u32::from(t.steps) * if t.armor { 3 } else { 2 }
}

/// A hex for unit `u`: closer to the key points the side wants (the attacker
/// those it does not hold, the defender those it holds), cover, and fewer
/// enemies seeing it.
fn position_score(b: &Battle, u: &TacUnit, h: Hex, w: &Weights) -> f64 {
    let me = u.role;
    let wanted: Vec<Hex> = b
        .map
        .key_points
        .iter()
        .filter(|k| match me {
            Role::Attacker => b.control.get(k) != Some(&Role::Attacker),
            Role::Defender => true,
        })
        .copied()
        .collect();
    let d = wanted.iter().map(|k| k.distance(h)).min().unwrap_or(0);
    let mut s = -w.key_point * f64::from(d) / 3.0;
    if wanted.contains(&h) {
        s += w.key_point;
    }
    let cell = b.map.cell(h);
    if let Some(t) = cell.and_then(|c| b.rules.terrain.get(&c.terrain)) {
        s += w.cover * (t.defense - 1.0) * 2.0;
    }
    let seen_by = b
        .alive(other(me))
        .filter(|e| e.hex.distance(h) <= 4 && b.map.sees(&b.rules, e.hex, h, &Default::default()))
        .count();
    s - w.exposure * seen_by as f64 * 0.3
}

/// Hexes the unit can end its move in this activation, with a path: terrain
/// costs, no occupied hexes or known mines, canals only by bridge, stop next
/// to a visible enemy.
fn reachable(b: &Battle, u: &TacUnit) -> Vec<(Hex, Vec<Hex>)> {
    let Some(kind) = b.rules.units.get(&u.kind) else {
        return Vec::new();
    };
    let mp = kind.move_points;
    let enemy_adjacent = |h: Hex| {
        b.alive(other(u.role))
            .any(|e| e.hex.is_adjacent(h) && b.visible_to(u.role, e))
    };
    let mut best: BTreeMap<Hex, (u32, Vec<Hex>)> = BTreeMap::new();
    let mut q = VecDeque::from([(u.hex, 0u32, Vec::new())]);
    while let Some((h, spent, path)) = q.pop_front() {
        if h != u.hex && enemy_adjacent(h) {
            continue;
        }
        for n in b.map.neighbors(h) {
            if b.unit_at(n).is_some() {
                continue;
            }
            let Some(cell) = b.map.cell(n) else { continue };
            if cell.mine && cell.mine_known {
                continue;
            }
            if b.map.has_canal(h, n) && !b.map.has_bridge(h, n) {
                continue;
            }
            let cost = b
                .rules
                .terrain
                .get(&cell.terrain)
                .map_or(1, |t| t.move_cost);
            let s = spent + cost;
            if s > mp {
                continue;
            }
            if best.get(&n).is_some_and(|(c, _)| *c <= s) {
                continue;
            }
            let mut p = path.clone();
            p.push(n);
            best.insert(n, (s, p.clone()));
            q.push_back((n, s, p));
        }
    }
    best.into_iter().map(|(h, (_, p))| (h, p)).collect()
}
