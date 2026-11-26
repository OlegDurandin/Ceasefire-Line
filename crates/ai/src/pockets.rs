//! R-AI-01 «поиск котлов» (stage 4 step 7): where the enemy's supply can be
//! cut. Once per phase, at the operational level.
//!
//! 1. The enemy's supply tree (R-SUP-02, the core's own code) gives every
//!    enemy unit its best path from a source.
//! 2. The dependency of a hex D(h) is the sum over the enemy units whose path
//!    runs through h of strength × price / 10.
//! 3. Chokepoints are the free hexes with the largest D(h).

use std::collections::BTreeMap;

use ceasefire_core::error::RuleError;
use ceasefire_core::game::GameState;
use ceasefire_core::hex::Hex;
use ceasefire_core::side::SideId;

/// Weight of a unit in the pocket sums: strength × price / 10.
pub fn unit_weight(s: &GameState, class: &str, strength: u8) -> f64 {
    let price = s.data().units.get(class).map_or(0, |c| c.cost_mpp);
    f64::from(strength) * f64::from(price) / 10.0
}

/// Per hex of the enemy's supply paths: D(h), the units depending on it
/// (ids, sorted) and its path cost from the source.
struct Dependency {
    weight: f64,
    units: Vec<u32>,
    cost: u32,
}

fn dependency(s: &GameState, enemy: SideId) -> Result<BTreeMap<Hex, Dependency>, RuleError> {
    let tree = s.supply_tree(enemy)?;
    let mut d: BTreeMap<Hex, Dependency> = BTreeMap::new();
    let mut units: Vec<_> = s.units().filter(|u| u.side == enemy).collect();
    units.sort_by_key(|u| u.id);
    for u in units {
        if !tree.cost.contains_key(&u.hex) {
            continue; // already cut off
        }
        let w = unit_weight(s, &u.class, u.strength);
        for h in tree.path_to(u.hex) {
            if h == u.hex {
                continue;
            }
            let e = d.entry(h).or_insert(Dependency {
                weight: 0.0,
                units: Vec::new(),
                cost: tree.cost[&h],
            });
            e.weight += w;
            e.units.push(u.id.0);
        }
    }
    Ok(d)
}

/// D(h) for the units of `enemy`: how much of it depends on each hex of
/// its supply paths (units' own hexes excluded).
pub fn supply_dependency(s: &GameState, enemy: SideId) -> Result<BTreeMap<Hex, f64>, RuleError> {
    Ok(dependency(s, enemy)?
        .into_iter()
        .map(|(h, d)| (h, d.weight))
        .collect())
}

/// The `k` chokepoints: free hexes (no unit) with the largest D(h). Hexes
/// that cut the very same units (one trunk of the tree) give one candidate,
/// the one farthest along the path — nearest the units it cuts off. Largest
/// first; ties by hex order (deterministic).
pub fn chokepoints(s: &GameState, enemy: SideId, k: usize) -> Result<Vec<(Hex, f64)>, RuleError> {
    let occupied: std::collections::BTreeSet<Hex> = s.units().map(|u| u.hex).collect();
    let mut by_units: BTreeMap<Vec<u32>, (Hex, f64, u32)> = BTreeMap::new();
    for (h, d) in dependency(s, enemy)? {
        if d.weight <= 0.0 || occupied.contains(&h) {
            continue;
        }
        let e = by_units.entry(d.units).or_insert((h, d.weight, d.cost));
        if d.cost > e.2 {
            *e = (h, d.weight, d.cost);
        }
    }
    let mut c: Vec<(Hex, f64)> = by_units.into_values().map(|(h, w, _)| (h, w)).collect();
    c.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    c.truncate(k);
    Ok(c)
}
