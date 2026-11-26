//! AI v1 (R-AI-01), stage 4 step 2. One command per call, in the order of
//! an experienced player:
//!
//! 1. take a vacated hex if it is not a trap (R-COMBAT-06);
//! 2. economy before anyone acts: reinforce worn units, then a scenario boost;
//! 3. artillery bombards, then attacks with the best exchange in MPP from the
//!    core's forecast, with bonuses for likely retreats, objectives and
//!    finishing a unit already hit; the bar falls with aggressiveness;
//! 4. the move with the largest gain of a hex score: the operation's goal,
//!    terrain, entrenchment kept, enemy threat, support, captures;
//! 5. end of phase.
//!
//! Operations: a side attacks the most valuable objective it does not hold
//! when its aggressiveness reaches `attack_from_aggressiveness`; threatened
//! own objectives get the nearest defender first; other units hold.

use std::collections::{BTreeMap, BTreeSet};

use ceasefire_core::game::{Command, GameState, Unit, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::side::SideId;

use crate::params::{AiHints, AiParams};

pub struct AiV1 {
    params: AiParams,
    hints: AiHints,
}

/// Per-command view of the board for the active side.
struct Board<'a> {
    s: &'a GameState,
    side: SideId,
    own: Vec<&'a Unit>,
    enemy: Vec<&'a Unit>,
    /// Enemy and own power spread over hexes (influence maps).
    threat: BTreeMap<Hex, f64>,
    support: BTreeMap<Hex, f64>,
    objectives: BTreeMap<Hex, f64>,
}

impl AiV1 {
    pub fn new(params: AiParams, hints: AiHints) -> Self {
        Self { params, hints }
    }

    pub fn max_commands(&self) -> u32 {
        self.params.max_commands_per_phase
    }

    /// The next command for the active side.
    pub fn next(&mut self, s: &GameState) -> Command {
        let b = self.board(s);
        if let Some(c) = self.advance(&b) {
            return c;
        }
        if let Some(c) = self.economy(&b) {
            return c;
        }
        if let Some(c) = self.air(&b) {
            return c;
        }
        if let Some(c) = self.strike(&b) {
            return c;
        }
        if let Some(c) = self.best_move(&b) {
            return c;
        }
        Command::EndPhase
    }

    fn aggressiveness(&self, side: SideId) -> f64 {
        self.hints
            .aggressiveness(side, self.params.default_aggressiveness)
    }

    fn board<'a>(&self, s: &'a GameState) -> Board<'a> {
        let side = s.active_side();
        let mut own: Vec<&Unit> = s.units().filter(|u| u.side == side).collect();
        let mut enemy: Vec<&Unit> = s.units().filter(|u| u.side != side).collect();
        own.sort_by_key(|u| u.id);
        enemy.sort_by_key(|u| u.id);
        let radius = self.params.moves.influence_radius;
        let hexes: Vec<Hex> = s.map().hexes().collect();
        let spread = |units: &[&Unit]| {
            let mut m: BTreeMap<Hex, f64> = BTreeMap::new();
            for u in units {
                let p = power(s, u);
                for h in &hexes {
                    let d = h.distance(u.hex);
                    if d <= radius {
                        *m.entry(*h).or_insert(0.0) += p / f64::from(1 + d);
                    }
                }
            }
            m
        };
        let threat = spread(&enemy);
        let support = spread(&own);
        let hints = self.hints.sides.get(&side);
        let mut objectives: BTreeMap<Hex, f64> = s
            .objectives()
            .iter()
            .map(|o| (o.hex, f64::from(o.vp.max(1))))
            .collect();
        if let Some(h) = hints {
            for (hex, w) in &h.priorities {
                *objectives.entry(*hex).or_insert(0.0) += w;
            }
        }
        Board {
            s,
            side,
            own,
            enemy,
            threat,
            support,
            objectives,
        }
    }

    fn advance(&self, b: &Board) -> Option<Command> {
        let p = b.s.pending_advance()?;
        if !b.s.unit(p.unit).is_some_and(|u| u.side == b.side) {
            return None;
        }
        let adjacent = b
            .enemy
            .iter()
            .filter(|e| e.hex.distance(p.hex) == 1)
            .count();
        let objective = b.objectives.contains_key(&p.hex);
        (objective || adjacent <= self.params.advance.max_adjacent_enemies)
            .then_some(Command::Advance { unit: p.unit })
    }

    /// Reinforce worn units that have not acted, then buy a boost (R-ECO-02, R-TECH-02).
    fn economy(&self, b: &Board) -> Option<Command> {
        let e = &self.params.economy;
        let mut worn: Vec<&&Unit> = b
            .own
            .iter()
            .filter(|u| !u.acted && u.strength <= e.reinforce_at_or_below && u.strength < 10)
            .collect();
        worn.sort_by_key(|u| (u.strength, u.id));
        for u in worn {
            let cmd = Command::Reinforce {
                unit: u.id,
                steps: 1,
                elite: false,
            };
            if accepted(b.s, &cmd) {
                return Some(cmd);
            }
        }
        let mpp = b.s.side(b.side).map_or(0, |s| s.mpp);
        for boost in b.s.tech_boosts().iter().filter(|t| t.side == b.side) {
            if b.s.boost_bought(&boost.id) || mpp < boost.cost + e.boost_mpp_reserve {
                continue;
            }
            let cmd = Command::BuyTechBoost {
                id: boost.id.clone(),
            };
            if accepted(b.s, &cmd) {
                return Some(cmd);
            }
        }
        None
    }

    /// Bombardments first, then attacks; the best score above the side's bar.
    fn strike(&self, b: &Board) -> Option<Command> {
        let w = &self.params.attack;
        let aggr = self.aggressiveness(b.side);
        let bar = w.min_score_defensive + (w.min_score_aggressive - w.min_score_defensive) * aggr;
        let retreat_at = f64::from(b.s.data().rules.retreat.max_strength);
        let mut best: Option<(u8, f64, Command)> = None;
        for u in b.own.iter().filter(|u| u.strikes_left > 0) {
            let own_step = step_value(b.s, u);
            for e in &b.enemy {
                let def_step = step_value(b.s, e);
                let on_objective = if b.objectives.contains_key(&e.hex) {
                    w.objective_bonus * own_step
                } else {
                    0.0
                };
                let focus = if e.attacks_received > 0 {
                    w.focus_bonus * own_step
                } else {
                    0.0
                };
                // Rank 0: bombardment (no return fire) before attacks (rank 1).
                let (rank, score, cmd) = if let Ok(f) = b.s.forecast_attack(u.id, e.id) {
                    let left = f64::from(e.strength) - f.defender_losses.expected;
                    let retreat = if left <= retreat_at {
                        w.retreat_bonus * def_step
                    } else {
                        0.0
                    };
                    let exchange = f.defender_losses.expected * def_step
                        - f.attacker_losses.expected * own_step;
                    let score = exchange + retreat + on_objective + focus;
                    if score < bar * own_step {
                        continue;
                    }
                    (
                        1,
                        score,
                        Command::Attack {
                            unit: u.id,
                            target: e.id,
                        },
                    )
                } else if let Ok(f) = b.s.forecast_bombard(u.id, e.id) {
                    let score = f.expected * def_step + on_objective + focus;
                    if score <= 0.0 {
                        continue;
                    }
                    (
                        0,
                        score,
                        Command::Bombard {
                            unit: u.id,
                            target: e.id,
                        },
                    )
                } else {
                    continue;
                };
                let better = best
                    .as_ref()
                    .is_none_or(|(r, sc, _)| rank < *r || (rank == *r && score > *sc));
                if better {
                    best = Some((rank, score, cmd));
                }
            }
        }
        best.map(|(_, _, c)| c)
    }

    /// Goal hex of every own unit that may move (operations).
    fn goals(&self, b: &Board) -> BTreeMap<UnitId, Hex> {
        let m = &self.params.moves;
        let mut goals = BTreeMap::new();
        let movers: Vec<&&Unit> = b.own.iter().filter(|u| u.hq_rating.is_none()).collect();
        // Threatened own objectives: the nearest unit defends each.
        let mut taken: BTreeSet<UnitId> = BTreeSet::new();
        for hex in b
            .objectives
            .keys()
            .filter(|h| b.s.controller(**h) == Some(b.side))
        {
            let threatened = b
                .enemy
                .iter()
                .any(|e| e.hex.distance(*hex) <= m.defend_radius);
            let guarded = b.own.iter().any(|u| u.hex == *hex);
            if !threatened || guarded {
                continue;
            }
            if let Some(u) = movers
                .iter()
                .filter(|u| !taken.contains(&u.id))
                .min_by_key(|u| (u.hex.distance(*hex), u.id))
            {
                taken.insert(u.id);
                goals.insert(u.id, *hex);
            }
        }
        // Attack: the next objective of the side's sequence (operation «следующая
        // цель»: after taking one, the attack moves on), else the most valuable
        // objective not held, weighted by distance.
        let attack = self.aggressiveness(b.side) >= m.attack_from_aggressiveness;
        let next = self.hints.sides.get(&b.side).and_then(|h| {
            h.sequence
                .iter()
                .find(|hex| b.s.controller(**hex) != Some(b.side))
                .copied()
        });
        let target = next.or_else(|| {
            b.objectives
                .iter()
                .filter(|(h, _)| b.s.controller(**h) != Some(b.side))
                .map(|(h, v)| {
                    let d = movers.iter().map(|u| u.hex.distance(*h)).min().unwrap_or(0);
                    (v / f64::from(1 + d), *h)
                })
                .max_by(|a, c| a.0.total_cmp(&c.0).then(c.1.cmp(&a.1)))
                .map(|(_, h)| h)
        });
        for u in movers {
            if taken.contains(&u.id) {
                continue;
            }
            let goal = match (attack, target) {
                (true, Some(t)) => t,
                _ => u.hex,
            };
            goals.insert(u.id, goal);
        }
        goals
    }

    fn hex_score(&self, b: &Board, u: &Unit, h: Hex, goal: Hex) -> f64 {
        let m = &self.params.moves;
        let terrain =
            b.s.map()
                .cell(h)
                .and_then(|c| b.s.data().terrain.get(&c.terrain).ok())
                .map_or(1.0, |t| t.def.defense_mult);
        let mut score =
            -m.goal_weight * f64::from(h.distance(goal)) + m.terrain_weight * (terrain - 1.0);
        if h == u.hex {
            score += m.entrench_weight * f64::from(u.entrenchment);
        }
        score -= m.threat_weight * b.threat.get(&h).copied().unwrap_or(0.0);
        score += m.support_weight * b.support.get(&h).copied().unwrap_or(0.0);
        if b.objectives.contains_key(&h) && b.s.controller(h) != Some(b.side) {
            score += m.objective_bonus;
        }
        if b.s.controller(h).is_some_and(|c| c != b.side) {
            score += m.enemy_hex_bonus;
        }
        let contact = b
            .enemy
            .iter()
            .filter(|e| e.hex.distance(h) == 1 && e.strength < u.strength)
            .count();
        score + m.contact_bonus * contact as f64 * self.aggressiveness(b.side)
    }

    /// The single move with the largest gain over staying put.
    fn best_move(&self, b: &Board) -> Option<Command> {
        let goals = self.goals(b);
        let mut best: Option<(f64, UnitId, Hex)> = None;
        for u in &b.own {
            if u.moved || !u.can_move || u.ap_halves == 0 {
                continue;
            }
            let Some(goal) = goals.get(&u.id).copied() else {
                continue;
            };
            let Ok(reach) = b.s.reachable(u.id) else {
                continue;
            };
            let stay = self.hex_score(b, u, u.hex, goal);
            for h in reach.keys() {
                let gain = self.hex_score(b, u, *h, goal) - stay;
                if gain < self.params.moves.min_gain {
                    continue;
                }
                if best
                    .as_ref()
                    .is_none_or(|(g, id, bh)| gain > *g || (gain == *g && (u.id, *h) < (*id, *bh)))
                {
                    best = Some((gain, u.id, *h));
                }
            }
        }
        let (_, id, to) = best?;
        let path = b.s.path_to(id, to).ok().flatten()?;
        Some(Command::Move { unit: id, path })
    }
}

impl AiV1 {
    /// R-AIR-02: the sortie with the best expected exchange in MPP, before any
    /// ground action (R-AI-01: «сначала артиллерия и авиация»).
    fn air(&self, b: &Board) -> Option<Command> {
        if b.s.is_night() {
            return None;
        }
        let cost = |class: &str| {
            b.s.data()
                .units
                .get(class)
                .map_or(10.0, |c| f64::from(c.cost_mpp) / 10.0)
        };
        let expected = |p: &ceasefire_core::combat::FireParams, cap: u8| {
            (f64::from(p.n) * p.p).min(f64::from(cap))
        };
        let mut planes: Vec<&ceasefire_core::game::AirUnit> =
            b.s.air_units()
                .filter(|a| a.side == b.side && a.sorties == 0 && !a.rebased)
                .collect();
        planes.sort_by_key(|a| a.id);
        let mut fields: Vec<Hex> =
            b.s.air_units()
                .filter(|a| a.side != b.side)
                .filter_map(|a| match a.base {
                    ceasefire_core::game::AirBase::Hex { hex } => Some(hex),
                    _ => None,
                })
                .collect();
        fields.sort();
        fields.dedup();
        let mut best: Option<(f64, Command)> = None;
        for a in planes {
            let own = cost(&a.class);
            let own_loss = |f: &ceasefire_core::game::AirStrikeForecast| {
                let group: f64 = f
                    .group_duels
                    .iter()
                    .map(|d| expected(&d.a_fire, a.strength))
                    .sum();
                let back = f
                    .return_fire
                    .as_ref()
                    .map_or(0.0, |r| expected(r, a.strength));
                (group + back).min(f64::from(a.strength)) * own
            };
            for e in &b.enemy {
                let Ok(f) = b.s.forecast_air_strike(a.id, e.id) else {
                    continue;
                };
                let mut score = f.strike_losses.expected * step_value(b.s, e) - own_loss(&f);
                if b.objectives.contains_key(&e.hex) {
                    score += self.params.attack.objective_bonus * own;
                }
                if best.as_ref().is_none_or(|(s, _)| score > *s) && score > 0.0 {
                    best = Some((
                        score,
                        Command::AirStrike {
                            unit: a.id,
                            target: e.id,
                        },
                    ));
                }
            }
            for h in &fields {
                let Ok(f) = b.s.forecast_airfield_strike(a.id, *h) else {
                    continue;
                };
                let parked = b
                    .s
                    .air_units()
                    .filter(|p| {
                        p.side != b.side && p.base == ceasefire_core::game::AirBase::Hex { hex: *h }
                    })
                    .map(|p| cost(&p.class))
                    .fold(0.0, f64::max);
                let score = f.strike_losses.expected * parked - own_loss(&f);
                if best.as_ref().is_none_or(|(s, _)| score > *s) && score > 0.0 {
                    best = Some((
                        score,
                        Command::AirStrikeAirfield {
                            unit: a.id,
                            hex: *h,
                        },
                    ));
                }
            }
        }
        best.map(|(_, c)| c)
    }
}

/// Strength × readiness × best ground attack: a unit's weight on the influence maps.
fn power(s: &GameState, u: &Unit) -> f64 {
    let rules = &s.data().rules.readiness;
    let attack = s
        .data()
        .units
        .get(&u.class)
        .map(|c| c.attack.soft.max(c.attack.hard))
        .unwrap_or(1.0);
    f64::from(u.strength) * f64::from(u.readiness_pct(rules)) / 100.0 * attack
}

/// MPP of one step of the unit (R-ECO-02: a step is a tenth of the price).
fn step_value(s: &GameState, u: &Unit) -> f64 {
    s.data()
        .units
        .get(&u.class)
        .map_or(10.0, |c| f64::from(c.cost_mpp) / 10.0)
}

/// The core would accept the command (dry run on a copy).
fn accepted(s: &GameState, cmd: &Command) -> bool {
    s.clone().apply(cmd.clone()).is_ok()
}
