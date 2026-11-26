//! R-MOVE-01..03: entry costs and zones of control. Full movement
//! (pathing, AP budget, stopping in ZOC) comes with `GameState`.

use std::collections::{BTreeMap, BTreeSet};

use crate::data::TerrainCatalog;
use crate::error::RuleError;
use crate::hex::Hex;
use crate::map::HexMap;
use crate::side::SideId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mobility {
    Tracked,
    Wheeled,
}

/// A ground unit in a hex, as seen by ZOC and supply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occupant {
    pub side: SideId,
    /// Class exerts ZOC and strength is above the threshold (R-MOVE-03).
    pub exerts_zoc: bool,
}

pub type Occupancy = BTreeMap<Hex, Occupant>;

/// R-MOVE-03: hexes in the ZOC of `side`'s enemies. ZOC does not reach
/// across a canal edge without a bridge (R-MOVE-02).
pub fn enemy_zoc(map: &HexMap, occupancy: &Occupancy, side: SideId) -> BTreeSet<Hex> {
    occupancy
        .iter()
        .filter(|(_, o)| o.side != side && o.exerts_zoc)
        .flat_map(|(&h, _)| {
            h.neighbors()
                .into_iter()
                .filter(move |&n| !map.has_canal(h, n) || map.has_bridge(h, n))
        })
        .collect()
}

/// R-MOVE-01, R-MOVE-02: AP (in half-points) to enter `to` from adjacent
/// `from`. `Ok(None)` means impassable: off-map, canal without bridge,
/// mountain without a pass, neutral closed territory (R-MOVE-06). Roads and
/// dirt roads (tracks) do not work into enemy ZOC.
pub fn entry_cost_halves(
    map: &HexMap,
    terrain: &TerrainCatalog,
    from: Hex,
    to: Hex,
    mobility: Mobility,
    to_in_enemy_zoc: bool,
) -> Result<Option<u32>, RuleError> {
    if !from.is_adjacent(to) {
        return Ok(None);
    }
    let Some(cell) = map.cell(to) else {
        return Ok(None);
    };
    if map.is_closed(to) {
        return Ok(None);
    }
    if map.has_canal(from, to) && !map.has_bridge(from, to) {
        return Ok(None);
    }
    let t = terrain.get(&cell.terrain)?;
    if t.def.impassable || (t.def.requires_pass && !cell.pass) {
        return Ok(None);
    }
    if map.has_road(from, to) && !to_in_enemy_zoc {
        return Ok(Some(terrain.road_ap_halves));
    }
    // A dirt road: the same AP whatever the terrain (also for supply paths).
    if map.has_track(from, to) && !to_in_enemy_zoc {
        return Ok(Some(terrain.track_ap_halves));
    }
    Ok(Some(match mobility {
        Mobility::Tracked => t.ap_halves,
        Mobility::Wheeled => t.wheeled_ap_halves,
    }))
}
