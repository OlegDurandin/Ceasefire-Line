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

use ceasefire_core::data::UnitTag;
use ceasefire_core::game::{Command, GameState, Unit, UnitId};
use ceasefire_core::hex::Hex;
use ceasefire_core::side::SideId;
use ceasefire_core::supply::SupplyMap;

use crate::operations::{is_mobile, op_units, reach_to, reconcile, Breach, OpStats, Operation};
use crate::params::{AiHints, AiParams};
use crate::pockets::{
    best_pocket, chokepoints, own_chokepoints, pocket_value, pocket_vp, supply_dependency, Pocket,
};

mod economy;
mod retreat;
mod transfer;

pub struct AiV1 {
    params: AiParams,
    hints: AiHints,
    /// The game's seed: the per-game spread of the weights comes from it.
    seed: u64,
    /// R-AI-01 operational level, once per phase: pockets and roles.
    plan: Option<Plan>,
    /// Operations carried to each side's next phase (inertia).
    carry: BTreeMap<u8, Vec<Operation>>,
    /// Breakthrough forecasts waiting for their outcome: (P, defender, its hex).
    pending: Vec<(f64, UnitId, Hex)>,
    /// Breakthrough defenders fired at this phase.
    attacked: BTreeSet<UnitId>,
    /// For `sim`: kept operations and breakthrough calibration.
    pub stats: OpStats,
    /// R-AI-01 п. 3: the phase's economy commands, decided at its start.
    eco: Option<((u32, u8), std::collections::VecDeque<Command>)>,
    /// R-MOVE-05: transfers made this phase.
    transfers_done: u32,
}

/// A chokepoint and the unit sent to it (`None`: an own unit already holds it).
type Role = (Hex, Option<UnitId>);

/// R-AI-01 «поиск котлов»: the phase's pocket operations and unit roles.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// (turn, side) the plan was made for.
    key: (u32, u8),
    /// Pockets chosen as operations, most valuable first.
    pub pockets: Vec<Pocket>,
    /// «Охват»: tank and mech brigades sent to a chokepoint.
    pub envelop: BTreeMap<UnitId, Hex>,
    /// «Горловина»: units standing on a chokepoint hold it.
    pub hold: BTreeSet<UnitId>,
    /// «Сковывание»: infantry and artillery keep contact with a cut unit.
    pub pin: BTreeMap<UnitId, Hex>,
    /// D(h) of the enemy, normalised to 0..1.
    pub dependency: BTreeMap<Hex, f64>,
    /// Chokepoint hexes of the operations.
    pub chokes: BTreeSet<Hex>,
    /// Breakthrough sectors of the operations (R-AI-01, step 7).
    pub breaches: Vec<Breach>,
    /// The operations themselves.
    pub ops: Vec<Operation>,
    /// Step 7 item 5 «защита своих горловин»: reserves sent to (or holding)
    /// threatened own chokepoints.
    pub guard: BTreeMap<UnitId, Hex>,
    /// Units a closing enemy pocket would cut off, and the hex they withdraw to.
    pub withdraw: BTreeMap<UnitId, Hex>,
    /// Defended own chokepoints: the VP the enemy's pocket through each would take.
    pub guard_vp: BTreeMap<Hex, f64>,
    /// Step 7 item 6: own hexes whose supply drops to the cut band if the
    /// enemy takes a threatened own chokepoint.
    pub risk: BTreeSet<Hex>,
    /// Own chokepoints the side depends on enough (`defend.min_share`).
    pub own_chokes: BTreeSet<Hex>,
    /// R-AI-01 «Отход»: units taking their steps out through an exit, and where.
    pub exit: BTreeMap<UnitId, Hex>,
    /// Rearguards: they stay to win time for the withdrawing units.
    pub rearguard: BTreeSet<UnitId>,
    /// Own hex supply now (for the «no stranding» rule).
    own_supply: SupplyMap,
}

/// SplitMix64 → [0, 1): a fixed, platform-independent spread from the seed.
fn unit_float(seed: u64, salt: u64) -> f64 {
    let mut z = seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
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
    /// `seed`: the game's seed (the same game always gets the same spread).
    pub fn new(params: AiParams, hints: AiHints, seed: u64) -> Self {
        Self {
            params,
            hints,
            seed,
            plan: None,
            carry: BTreeMap::new(),
            pending: Vec::new(),
            attacked: BTreeSet::new(),
            stats: OpStats::default(),
            eco: None,
            transfers_done: 0,
        }
    }

    /// The last plan made (R-AI-01 debugging layer «Намерения ИИ»).
    pub fn last_plan(&self) -> Option<&Plan> {
        self.plan.as_ref()
    }

    /// The phase's plan (made on the first command of the phase).
    pub fn plan_for(&mut self, s: &GameState) -> &Plan {
        let key = (s.turn(), s.active_side().0);
        if self.plan.as_ref().is_none_or(|p| p.key != key) {
            // Calibration: did last phase's breakthrough defenders leave their hexes?
            // Only sectors actually attacked count (the defender took fire).
            let hit = std::mem::take(&mut self.attacked);
            for (p, id, hex) in std::mem::take(&mut self.pending) {
                if hit.contains(&id) {
                    let left = s.unit(id).is_none_or(|u| u.hex != hex);
                    self.stats.breaches.push((p, left));
                }
            }
            let (mut plan, fresh) = self.make_plan(s, key);
            let carried = self.carry.get(&key.1).cloned().unwrap_or_default();
            let ops = if carried.is_empty() {
                fresh
            } else {
                let (ops, kept) = reconcile(s, &carried, fresh, &self.params.pockets);
                self.stats.carried_phases += 1;
                if kept {
                    self.stats.kept_phases += 1;
                }
                ops
            };
            self.fill_plan(s, &mut plan, ops.clone());
            self.defend(s, &mut plan);
            self.position_maps(s, &mut plan);
            self.retreat(s, &mut plan);
            let d = self.stats.defend.entry(key.1).or_default();
            d.0 += plan.guard.len() as u32;
            d.1 += plan.withdraw.len() as u32;
            self.pending = plan
                .breaches
                .iter()
                .map(|b| (b.p, b.defender, b.hex))
                .collect();
            self.carry.insert(key.1, ops);
            self.plan = Some(plan);
        }
        self.plan.as_ref().expect("just made")
    }

    /// R-AI-01 common currency: expected VP of the objective the side attacks —
    /// its margin (VP to both holders, NM lost and half gained) × the chance
    /// to take it, by own and enemy power within 3 hexes. 0 when the side holds.
    fn objective_vp(&self, s: &GameState, us: SideId) -> f64 {
        let m = &self.params.moves;
        if self.aggressiveness(us) < m.attack_from_aggressiveness {
            return 0.0;
        }
        let next = self.hints.sides.get(&us).and_then(|h| {
            h.sequence
                .iter()
                .find(|hex| s.controller(**hex) != Some(us))
                .copied()
        });
        let target = next.or_else(|| {
            s.objectives()
                .iter()
                .filter(|o| s.controller(o.hex) != Some(us))
                .max_by_key(|o| (o.vp, std::cmp::Reverse(o.hex)))
                .map(|o| o.hex)
        });
        let Some(t) = target else {
            return 0.0;
        };
        let Some(o) = s.objectives().iter().find(|o| o.hex == t) else {
            return 0.0;
        };
        let margin = objective_margin(s, o);
        let near = |own: bool| -> f64 {
            s.units()
                .filter(|u| (u.side == us) == own && u.hex.distance(t) <= 3)
                .map(|u| power(s, u))
                .sum()
        };
        let (a, d) = (near(true), near(false));
        let p = if a + d > 0.0 { a / (a + d) } else { 0.0 };
        margin * p
    }

    /// R-AI-01 «поиск котлов»: the plan's base (dependency, own supply) and
    /// the fresh operations worth taking this phase.
    fn make_plan(&self, s: &GameState, key: (u32, u8)) -> (Plan, Vec<Operation>) {
        let w = &self.params.pockets;
        let us = s.active_side();
        let mut plan = Plan {
            key,
            own_supply: s.hex_supply(us).unwrap_or_default(),
            ..Plan::default()
        };
        let Some(enemy) = s.units().map(|u| u.side).find(|x| *x != us) else {
            return (plan, Vec::new());
        };
        let dep = supply_dependency(s, enemy).unwrap_or_default();
        let top = dep.values().copied().fold(0.0, f64::max);
        if top > 0.0 {
            plan.dependency = dep.into_iter().map(|(h, v)| (h, v / top)).collect();
        }
        if !w.enabled {
            return (plan, Vec::new());
        }
        // Pockets through each candidate; the same hexes count once.
        let mut pockets: Vec<Pocket> = Vec::new();
        for (c, _) in chokepoints(s, enemy, w.candidates).unwrap_or_default() {
            // The chokepoint alone and its best pincers: the pair may be out of reach.
            let single = pocket_value(s, us, enemy, &[c], w).map(|(value, cut)| Pocket {
                hexes: vec![c],
                value,
                cut,
            });
            for p in [single, best_pocket(s, us, enemy, c, w)]
                .into_iter()
                .flatten()
            {
                let mut hexes = p.hexes.clone();
                hexes.sort();
                if p.value >= w.min_value
                    && !pockets.iter().any(|q| {
                        let mut h = q.hexes.clone();
                        h.sort();
                        h == hexes
                    })
                {
                    pockets.push(p);
                }
            }
        }
        pockets.sort_by(|a, b| b.value.total_cmp(&a.value).then(a.hexes.cmp(&b.hexes)));
        let mut own: Vec<&Unit> = s.units().filter(|u| u.side == us).collect();
        own.sort_by_key(|u| u.id);
        let zoc: BTreeSet<Hex> = s.zoc_against(us);
        // Own losses in VP: steps × own sensitivity / steps per NM point.
        let own_step_vp = s.side(us).map_or(1.0, |x| f64::from(x.loss_sensitivity))
            / f64::from(s.data().rules.national_morale.steps_per_loss.max(1));
        // R-AI-01 common currency: the objective the side attacks is worth its
        // VP margin × the chance to take it; an operation takes a share of it.
        let objective_vp = self.objective_vp(s, us);
        let mobile_count = own.iter().filter(|u| is_mobile(s, u)).count().max(1) as f64;
        // «Охват»: every chokepoint needs a unit that gets there — freely now,
        // through a free corridor in two phases, or through a breakthrough
        // sector; utility = expected VP × Π reach − own losses − objective share.
        let mut busy: BTreeSet<UnitId> = BTreeSet::new();
        let mut chosen: Vec<Operation> = Vec::new();
        for p in pockets {
            let mut taken = busy.clone();
            let mut roles: Vec<Role> = Vec::new();
            let mut breaches = Vec::new();
            let (mut reach, mut now, mut used, mut losses) = (1.0, true, 0.0, 0.0);
            for &c in &p.hexes {
                if let Some(u) = own.iter().find(|u| u.hex == c) {
                    taken.insert(u.id);
                    roles.push((c, None));
                    continue;
                }
                let best = own
                    .iter()
                    .filter(|u| is_mobile(s, u) && !taken.contains(&u.id))
                    .filter(|u| u.supply > w.drop_supply && u.strength > w.drop_strength)
                    .filter_map(|u| reach_to(s, u, c, enemy, p.value, w, &zoc).map(|r| (r, u.id)))
                    .max_by(|a, b| a.0 .0.total_cmp(&b.0 .0).then(b.1.cmp(&a.1)));
                let Some(((r, closes, breach), id)) = best else {
                    reach = 0.0;
                    break;
                };
                taken.insert(id);
                reach *= r;
                now &= closes;
                used += 1.0;
                if let Some(b) = breach {
                    losses += b.losses * own_step_vp;
                    breaches.push(b);
                }
                roles.push((c, Some(id)));
            }
            if reach <= 0.0 {
                continue;
            }
            let delay = if now { 0 } else { 1 };
            let vp = pocket_vp(s, us, enemy, &p.hexes, delay, w).unwrap_or(0.0);
            let utility = vp * reach - losses - objective_vp * used / mobile_count;
            if utility >= w.min_vp {
                busy = taken;
                chosen.push(Operation {
                    pocket: p,
                    roles,
                    breaches,
                    utility,
                });
                if chosen.len() >= w.max_operations {
                    break;
                }
            }
        }
        chosen.sort_by(|a, b| b.utility.total_cmp(&a.utility));
        (plan, chosen)
    }

    /// Roles of the operations in force: envelopment, holders, breakthrough
    /// sectors, and «сковывание» for infantry and artillery near cut units.
    fn fill_plan(&self, s: &GameState, plan: &mut Plan, ops: Vec<Operation>) {
        let w = &self.params.pockets;
        let us = s.active_side();
        let mut busy: BTreeSet<UnitId> = BTreeSet::new();
        for op in &ops {
            for (id, c) in op_units(op) {
                plan.envelop.insert(id, c);
                busy.insert(id);
            }
            for (c, unit) in &op.roles {
                plan.chokes.insert(*c);
                if unit.is_none() {
                    if let Some(u) = s.unit_at(*c).filter(|u| u.side == us) {
                        plan.hold.insert(u.id);
                        busy.insert(u.id);
                    }
                }
            }
            // A breach still worth trying: its defender is still there.
            for b in &op.breaches {
                if s.unit(b.defender).is_some_and(|u| u.hex == b.hex) {
                    plan.breaches.push(b.clone());
                }
            }
        }
        let cut: Vec<Hex> = ops
            .iter()
            .flat_map(|o| o.pocket.cut.iter())
            .filter_map(|id| s.unit(*id).map(|u| u.hex))
            .collect();
        let mut own: Vec<&Unit> = s.units().filter(|u| u.side == us).collect();
        own.sort_by_key(|u| u.id);
        for u in own
            .iter()
            .filter(|u| !is_mobile(s, u) && u.hq_rating.is_none() && !busy.contains(&u.id))
        {
            if let Some(h) = cut
                .iter()
                .filter(|h| h.distance(u.hex) <= w.pin_radius)
                .min_by_key(|h| (h.distance(u.hex), **h))
            {
                plan.pin.insert(u.id, *h);
            }
        }
        plan.pockets = ops.iter().map(|o| o.pocket.clone()).collect();
        plan.ops = ops;
    }

    /// Step 7 item 5 «защита своих горловин»: the pocket search from the own
    /// side. An own chokepoint the side depends on enough (`defend.min_share`)
    /// with an enemy ground unit near gets the nearest free reserve; when none
    /// is free, the pocket is closing and the units it would cut off (projected
    /// supply ≤ `pockets.cut_supply_max` with the enemy on the chokepoint)
    /// withdraw to the nearest hex that keeps supply. Garrisons stay: units
    /// on own objectives and, with `fortified_hold`, in cities and fortified areas.
    fn defend(&self, s: &GameState, plan: &mut Plan) {
        let d = &self.params.defend;
        let w = &self.params.pockets;
        if !d.enabled {
            return;
        }
        let us = s.active_side();
        let Some(enemy) = s.units().map(|u| u.side).find(|x| *x != us) else {
            return;
        };
        let mut own: Vec<&Unit> = s.units().filter(|u| u.side == us).collect();
        own.sort_by_key(|u| u.id);
        let total: f64 = own
            .iter()
            .map(|u| crate::pockets::unit_weight(s, &u.class, u.strength))
            .sum();
        if total <= 0.0 {
            return;
        }
        let ground_enemy: Vec<&Unit> = s
            .units()
            .filter(|u| u.side == enemy && u.hq_rating.is_none() && !is_air(s, u))
            .collect();
        let mut busy: BTreeSet<UnitId> = plan
            .envelop
            .keys()
            .chain(plan.pin.keys())
            .chain(plan.hold.iter())
            .copied()
            .collect();
        let objectives: BTreeSet<Hex> = s.objectives().iter().map(|o| o.hex).collect();
        let garrison = |u: &Unit| -> bool {
            objectives.contains(&u.hex)
                || (s.variants().fortified_hold
                    && s.map()
                        .cell(u.hex)
                        .and_then(|c| s.data().terrain.get(&c.terrain).ok())
                        .is_some_and(|t| t.def.stronghold))
        };
        let mut guarded = 0;
        for (c, dep) in own_chokepoints(s, us, d.candidates).unwrap_or_default() {
            if dep / total < d.min_share
                || !ground_enemy
                    .iter()
                    .any(|e| e.hex.distance(c) <= d.threat_radius)
            {
                continue;
            }
            // Only a real pocket: the trial recount with the enemy on `c` cuts someone.
            let Ok((_, cut)) = pocket_value(s, enemy, us, &[c], w) else {
                continue;
            };
            if cut.is_empty() {
                continue;
            }
            // A free reserve: not in an operation, not in contact, fit; the one
            // already on the chokepoint first, then the nearest.
            let reserve = (guarded < d.max_guards)
                .then(|| {
                    own.iter()
                        .filter(|u| u.hq_rating.is_none() && !is_air(s, u) && !busy.contains(&u.id))
                        .filter(|u| u.supply > w.drop_supply && u.strength > w.drop_strength)
                        .filter(|u| !garrison(u) || u.hex == c)
                        .filter(|u| {
                            u.hex == c || !ground_enemy.iter().any(|e| e.hex.distance(u.hex) <= 1)
                        })
                        .filter(|u| u.hex.distance(c) <= d.guard_radius)
                        .min_by_key(|u| (u.hex.distance(c), u.id))
                        .map(|u| (u.id, u.hex))
                })
                .flatten();
            if let Some((id, at)) = reserve {
                busy.insert(id);
                guarded += 1;
                plan.chokes.insert(c);
                let delay = 1;
                plan.guard_vp
                    .insert(c, pocket_vp(s, enemy, us, &[c], delay, w).unwrap_or(0.0));
                if at == c {
                    plan.hold.insert(id);
                } else {
                    plan.guard.insert(id, c);
                }
                continue;
            }
            // No reserve: the pocket closes — those it cuts off withdraw.
            let Ok(after) = s.hex_supply_if(us, &[(c, enemy)]) else {
                continue;
            };
            for id in cut {
                let Some(u) = s.unit(id) else { continue };
                // Envelopment and chokepoint holders stay in their operation (it
                // drops them itself at `pockets.drop_supply`); leaving the pocket
                // comes before «сковывание».
                let assigned = plan.envelop.contains_key(&id)
                    || plan.hold.contains(&id)
                    || plan.guard.contains_key(&id)
                    || plan.withdraw.contains_key(&id);
                if assigned || garrison(u) || u.hq_rating.is_some() || is_air(s, u) {
                    continue;
                }
                let out = s
                    .map()
                    .hexes()
                    .filter(|h| *h != c && after.get(*h) > w.cut_supply_max)
                    .filter(|h| s.controller(*h) == Some(us) && s.unit_at(*h).is_none())
                    .min_by_key(|h| (h.distance(u.hex), *h));
                if let Some(h) = out {
                    busy.insert(id);
                    plan.pin.remove(&id);
                    plan.withdraw.insert(id, h);
                }
            }
        }
    }

    pub fn max_commands(&self) -> u32 {
        self.params.max_commands_per_phase
    }

    /// The next command for the active side.
    pub fn next(&mut self, s: &GameState) -> Command {
        self.plan_for(s);
        // R-AI-01 п. 3: economy decided once, after the plan and before any action.
        let key = (s.turn(), s.active_side().0);
        if self.eco.as_ref().is_none_or(|(k, _)| *k != key) {
            let queue = self.economy_plan(s);
            self.eco = Some((key, queue.into()));
            self.transfers_done = 0;
        }
        let b = self.board(s);
        if let Some(c) = self.advance(&b) {
            return c;
        }
        if let Some((_, queue)) = self.eco.as_mut() {
            while let Some(c) = queue.pop_front() {
                if accepted(s, &c) {
                    return c;
                }
            }
        }
        // R-AI-01 «сектора»: a reserve to the threatened sector before the fighting.
        if let Some(c) = self.transfer(&b).filter(|c| accepted(s, c)) {
            self.transfers_done += 1;
            return c;
        }
        // R-AI-01 step 7: the breakthrough operations first, in order.
        if let Some(c) = self.breakthrough(&b) {
            if let Command::Attack { target, .. }
            | Command::Bombard { target, .. }
            | Command::AirStrike { target, .. } = &c
            {
                self.attacked.insert(*target);
            }
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
        // HQs last: the line has moved first.
        if let Some(c) = self.hq_move(&b) {
            return c;
        }
        Command::EndPhase
    }

    /// The side's aggressiveness: the scenario's hint (or the default) moved by
    /// up to ± `aggressiveness_jitter`, fixed for the game by its seed.
    fn aggressiveness(&self, side: SideId) -> f64 {
        let base = self
            .hints
            .aggressiveness(side, self.params.default_aggressiveness);
        let spread = (unit_float(self.seed, u64::from(side.0) + 1) * 2.0 - 1.0)
            * self.params.aggressiveness_jitter;
        (base + spread).clamp(0.0, 1.0)
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
        let breach = self
            .plan
            .as_ref()
            .is_some_and(|pl| pl.breaches.iter().any(|x| x.hex == p.hex));
        (objective || breach || adjacent <= self.params.advance.max_adjacent_enemies)
            .then_some(Command::Advance { unit: p.unit })
    }

    /// Bombardments first, then attacks; the best score above the side's bar.
    fn strike(&self, b: &Board) -> Option<Command> {
        let mut best: Option<(u8, f64, Command)> = None;
        for (rank, score, cmd) in self.strike_options(b) {
            let better = best
                .as_ref()
                .is_none_or(|(r, sc, _)| rank < *r || (rank == *r && score > *sc));
            if better {
                best = Some((rank, score, cmd));
            }
        }
        best.map(|(_, _, c)| c)
    }

    /// Every bombardment and attack worth making now: (rank 0 bombardment /
    /// 1 attack, score, command).
    fn strike_options(&self, b: &Board) -> Vec<(u8, f64, Command)> {
        let w = &self.params.attack;
        let aggr = self.aggressiveness(b.side);
        let bar = w.min_score_defensive + (w.min_score_aggressive - w.min_score_defensive) * aggr;
        let retreat_at = f64::from(b.s.data().rules.retreat.max_strength);
        let mut options: Vec<(u8, f64, Command)> = Vec::new();
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
                } + if e.supply == 0 {
                    // «Добивание»: cut-off units first (R-COMBAT-05 ×1,5 makes it pay).
                    self.params.pockets.finish_bonus * own_step
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
                options.push((rank, score, cmd));
            }
        }
        options
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
        // R-AI-01 «поиск котлов»: operations override the default goals, but
        // not the defenders of threatened own objectives.
        if let Some(plan) = &self.plan {
            for (id, h) in plan.envelop.iter().chain(plan.pin.iter()) {
                if !taken.contains(id) {
                    goals.insert(*id, *h);
                }
            }
            // While a breakthrough sector holds, the tanks of its operation
            // close up to it (1–2 hexes) instead of heading for the chokepoint.
            for op in &plan.ops {
                for br in op
                    .breaches
                    .iter()
                    .filter(|br| b.s.unit(br.defender).is_some_and(|u| u.hex == br.hex))
                {
                    for (id, _) in op_units(op) {
                        if !taken.contains(&id) {
                            goals.insert(id, br.hex);
                        }
                    }
                }
            }
            // Step 7 item 5: reserves to own chokepoints, cut units out of the pocket.
            for (id, h) in plan
                .guard
                .iter()
                .chain(plan.withdraw.iter())
                .chain(plan.exit.iter())
            {
                if !taken.contains(id) {
                    goals.insert(*id, *h);
                }
            }
            for id in plan.hold.iter().chain(plan.rearguard.iter()) {
                if let Some(u) = b.s.unit(*id) {
                    goals.insert(*id, u.hex);
                }
            }
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
        score += m.contact_bonus * contact as f64 * self.aggressiveness(b.side);
        // R-AI-01: tanks and mechs value hexes the enemy's supply depends on.
        let mobile =
            b.s.data()
                .units
                .get(&u.class)
                .is_ok_and(|c| c.has_tag(UnitTag::Exploitation));
        // Step 7 item 6: the position terms.
        let p = &self.params.position;
        if let Some(plan) = &self.plan {
            if mobile {
                score += self.weight(p.w_cut, 1, b.side)
                    * plan.dependency.get(&h).copied().unwrap_or(0.0);
            }
            let cut_off: f64 = b
                .enemy
                .iter()
                .filter(|e| e.supply == 0 && e.hex.distance(h) == 1)
                .map(|e| f64::from(e.strength) / 10.0)
                .sum();
            score += self.weight(p.w_pocket, 2, b.side) * cut_off;
            let holds = plan.guard.contains_key(&u.id) || plan.hold.contains(&u.id);
            if plan.risk.contains(&h) && !holds {
                score -= self.weight(p.w_risk, 3, b.side) * f64::from(u.strength) / 10.0;
            }
            if plan.own_chokes.contains(&h) && u.hq_rating.is_none() {
                score += self.weight(p.w_choke, 4, b.side);
            }
        }
        score
    }

    /// A position weight with the game's seeded spread (± `position.jitter`).
    fn weight(&self, w: f64, salt: u64, side: SideId) -> f64 {
        let u = unit_float(self.seed, 0x9051_0000 + salt * 8 + u64::from(side.0));
        w * (1.0 + self.params.position.jitter * (u * 2.0 - 1.0))
    }

    /// Step 7 item 6: own chokepoints the side depends on, and the hexes a
    /// threatened one would cut (trial recount with the enemy on it).
    fn position_maps(&self, s: &GameState, plan: &mut Plan) {
        let p = &self.params.position;
        let d = &self.params.defend;
        if p.w_risk == 0.0 && p.w_choke == 0.0 {
            return;
        }
        let us = s.active_side();
        let Some(enemy) = s.units().map(|u| u.side).find(|x| *x != us) else {
            return;
        };
        let total: f64 = s
            .units()
            .filter(|u| u.side == us)
            .map(|u| crate::pockets::unit_weight(s, &u.class, u.strength))
            .sum();
        if total <= 0.0 {
            return;
        }
        let band = self.params.pockets.cut_supply_max;
        let enemy_ground: Vec<Hex> = s
            .units()
            .filter(|e| e.side == enemy && e.hq_rating.is_none() && !is_air(s, e))
            .map(|e| e.hex)
            .collect();
        for (c, dep) in own_chokepoints(s, us, d.candidates).unwrap_or_default() {
            if dep / total < d.min_share {
                continue;
            }
            plan.own_chokes.insert(c);
            if p.w_risk == 0.0
                || !enemy_ground
                    .iter()
                    .any(|e| e.distance(c) <= d.threat_radius)
            {
                continue;
            }
            let Ok(after) = s.hex_supply_if(us, &[(c, enemy)]) else {
                continue;
            };
            for (h, v) in plan.own_supply.iter() {
                if v > band && after.get(h) <= band {
                    plan.risk.insert(h);
                }
            }
        }
    }

    /// The single move with the largest gain over staying put.
    fn best_move(&self, b: &Board) -> Option<Command> {
        let goals = self.goals(b);
        let mut best: Option<(f64, UnitId, Hex)> = None;
        let plan = self.plan.as_ref();
        let drop = b.s.data().rules.supply.max_drop_per_phase;
        let min_supply = self.params.pockets.min_own_supply;
        for u in &b.own {
            if u.moved || !u.can_move || u.ap_halves == 0 {
                continue;
            }
            // «Горловина»: a unit on a chokepoint stays and digs in.
            if plan.is_some_and(|p| p.hold.contains(&u.id) || p.rearguard.contains(&u.id)) {
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
                // No stranding: own supply a phase later below the floor, unless a chokepoint.
                if let Some(p) = plan {
                    let ahead = p.own_supply.get(*h).max(u.supply.saturating_sub(drop));
                    if ahead < min_supply && !p.chokes.contains(h) {
                        continue;
                    }
                }
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
/// R-AI-01 common currency: what an objective is worth between the sides —
/// its VP to both holders, NM lost and the share gained.
fn objective_margin(s: &GameState, o: &ceasefire_core::game::Objective) -> f64 {
    let share = f64::from(s.data().rules.national_morale.objective_taken_share_pct) / 100.0;
    2.0 * f64::from(o.vp) + f64::from(o.nm_value) * (1.0 + share)
}

/// Air (and naval) units are not ground reserves and are not cut off.
fn is_air(s: &GameState, u: &Unit) -> bool {
    s.data()
        .units
        .get(&u.class)
        .is_ok_and(|c| !c.target_class.is_ground())
}

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

impl AiV1 {
    /// R-AI-01 step 7, «прорыв к горловине»: per breakthrough sector, in
    /// order — artillery on the defender, then aircraft, then attacks from
    /// place (infantry, R-COMBAT-07), then tank and mech strikes; the advance
    /// into the vacated hex is taken by `advance`; then the operation's tanks
    /// with AP left go on to the chokepoint. Other operations come after.
    fn breakthrough(&self, b: &Board) -> Option<Command> {
        let plan = self.plan.as_ref()?;
        for op in &plan.ops {
            for br in &op.breaches {
                let Some(def) = b.s.unit(br.defender).filter(|u| u.hex == br.hex) else {
                    continue;
                };
                // 1. Artillery.
                for u in b.own.iter().filter(|u| u.strikes_left > 0) {
                    let cmd = Command::Bombard {
                        unit: u.id,
                        target: def.id,
                    };
                    if b.s.forecast_bombard(u.id, def.id).is_ok() && accepted(b.s, &cmd) {
                        return Some(cmd);
                    }
                }
                // 2. Aircraft (day only; the core refuses at night).
                let mut planes: Vec<_> =
                    b.s.air_units()
                        .filter(|a| a.side == b.side && a.sorties == 0 && !a.rebased)
                        .collect();
                planes.sort_by_key(|a| a.id);
                for a in planes {
                    if b.s.forecast_air_strike(a.id, def.id).is_ok() {
                        let cmd = Command::AirStrike {
                            unit: a.id,
                            target: def.id,
                        };
                        if accepted(b.s, &cmd) {
                            return Some(cmd);
                        }
                    }
                }
                // 3. Infantry from place, then 4. tanks and mechs.
                for mobile in [false, true] {
                    for u in b
                        .own
                        .iter()
                        .filter(|u| u.strikes_left > 0 && is_mobile(b.s, u) == mobile)
                    {
                        if b.s.forecast_attack(u.id, def.id).is_ok() {
                            let cmd = Command::Attack {
                                unit: u.id,
                                target: def.id,
                            };
                            if accepted(b.s, &cmd) {
                                return Some(cmd);
                            }
                        }
                    }
                }
            }
            // 5. The vacated sector is ours: the operation's tanks go on.
            let units = op_units(op);
            for (id, c) in &units {
                let Some(u) = b.s.unit(*id) else { continue };
                if !u.can_move || u.ap_halves == 0 || u.hex == *c {
                    continue;
                }
                let reach = b.s.reachable(*id).unwrap_or_default();
                let to = if reach.contains_key(c) {
                    Some(*c)
                } else if op
                    .breaches
                    .iter()
                    .any(|br| b.s.unit(br.defender).is_none_or(|d| d.hex != br.hex))
                {
                    // Through the breach: the reachable hex nearest the chokepoint.
                    reach
                        .keys()
                        .filter(|h| h.distance(*c) < u.hex.distance(*c))
                        .min_by_key(|h| (h.distance(*c), **h))
                        .copied()
                } else {
                    None
                };
                if let Some(to) = to {
                    if let Some(path) = b.s.path_to(*id, to).ok().flatten() {
                        return Some(Command::Move { unit: *id, path });
                    }
                }
            }
        }
        None
    }
}

impl AiV1 {
    /// Step 7, «оборона»: an own HQ closer than `hq_min_enemy_distance` to an
    /// enemy ground unit steps back to a reachable hex at that distance from
    /// all of them, keeping the most of its attached units within the
    /// attachment radius (R-HQ-01); ties — the shorter move, then hex order.
    fn hq_move(&self, b: &Board) -> Option<Command> {
        let min = self.params.moves.hq_min_enemy_distance;
        let radius = ceasefire_core::scale::range_in_hexes(
            b.s.data().rules.hq.attach_radius_km,
            b.s.hex_km(),
        );
        let far_enough = |h: Hex| b.enemy.iter().all(|e| e.hex.distance(h) >= min);
        for hq in b
            .own
            .iter()
            .filter(|u| u.hq_rating.is_some() && u.can_move && !u.moved)
        {
            // An HQ on an own objective is its garrison, and one in a line of
            // own combat units is part of it: both stay.
            let in_line = b
                .own
                .iter()
                .any(|u| u.hq_rating.is_none() && u.hex.is_adjacent(hq.hex));
            if far_enough(hq.hex) || b.objectives.contains_key(&hq.hex) || in_line {
                continue;
            }
            let subs: Vec<Hex> = b
                .own
                .iter()
                .filter(|u| u.attached_hq == Some(hq.id))
                .map(|u| u.hex)
                .collect();
            // The HQ's supply bonus (R-SUP-02, 20 km) must reach as many of its
            // units as now: a step back may not cost the front its supply.
            let bonus_radius = ceasefire_core::scale::range_in_hexes(
                b.s.data().rules.supply.hq_bonus_radius_km,
                b.s.hex_km(),
            );
            let in_bonus = |h: Hex| {
                subs.iter()
                    .filter(|s| s.distance(h) <= bonus_radius)
                    .count()
            };
            let now = in_bonus(hq.hex);
            let reach = b.s.reachable(hq.id).unwrap_or_default();
            let best = reach
                .keys()
                .filter(|h| far_enough(**h) && in_bonus(**h) >= now)
                .map(|h| {
                    let kept = subs.iter().filter(|s| s.distance(*h) <= radius).count();
                    (std::cmp::Reverse(kept), h.distance(hq.hex), *h)
                })
                .min();
            if let Some((_, _, to)) = best {
                if let Some(path) = b.s.path_to(hq.id, to).ok().flatten() {
                    return Some(Command::Move { unit: hq.id, path });
                }
            }
        }
        None
    }
}
