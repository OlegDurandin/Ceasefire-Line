//! `sim ai-vs-ai --dump-battles DIR`: every ground attack of the games as a
//! tactical battle input (R-TAC-03 snapshot of the core + the support the
//! snapshot does not carry yet, T-SUP-01), for `sim tactical`.

use std::fs;
use std::path::Path;

use ceasefire_core::data::UnitTag;
use ceasefire_core::game::{Command, Event, GameState, Unit};
use ceasefire_core::hex::Hex;
use ceasefire_core::scale::range_in_hexes;
use ceasefire_core::side::SideId;
use ceasefire_tactical::{BattleInput, SideSupport, SupportInfo};

/// The support one side brings to a battle at `hex` (T-SUP-01), from the
/// strategic state through public APIs.
fn side_support(s: &GameState, side: SideId, unit: &Unit, hex: Hex) -> SideSupport {
    let data = s.data();
    let km = s.hex_km();
    let class = |c: &str| data.units.get(c).ok();
    let artillery_supply = s
        .units()
        .filter(|u| u.side == side && u.id != unit.id)
        .filter(|u| class(&u.class).is_some_and(|c| c.has_tag(UnitTag::Bombard)))
        .filter(|u| {
            class(&u.class)
                .and_then(|c| c.range_km)
                .is_some_and(|r| u.hex.distance(hex) <= range_in_hexes(r, km))
        })
        .map(|u| u.supply)
        .max();
    let strike = s
        .air_units()
        .filter(|a| a.side == side && a.class == "strike")
        .filter(|a| {
            let radius = class(&a.class).and_then(|c| c.range_km).unwrap_or(0.0);
            s.base_distance(&a.base, hex)
                .is_some_and(|d| d <= range_in_hexes(radius, km))
        })
        .max_by_key(|a| a.readiness_pct(data));
    let air_supply = strike.map(|a| match &a.base {
        ceasefire_core::game::AirBase::Hex { hex } => s.hex_supply(side).map_or(0, |m| m.get(*hex)),
        _ => data.rules.supply.max,
    });
    let hq_rating = unit
        .attached_hq
        .and_then(|id| s.unit(id))
        .and_then(|h| h.hq_rating);
    let reserve_class = s
        .units()
        .filter(|u| {
            u.side == side && u.id != unit.id && u.hex.is_adjacent(hex) && u.hq_rating.is_none()
        })
        .filter(|u| class(&u.class).is_some_and(|c| !c.has_tag(UnitTag::Bombard) && c.strikes > 0))
        .min_by_key(|u| u.id)
        .map(|u| u.class.clone());
    let engineers = s.units().any(|u| {
        u.side == side
            && u.hex.distance(hex) <= 2
            && class(&u.class).is_some_and(|c| c.has_tag(UnitTag::BuildBridge))
    });
    let enemy_sam = s.units().any(|u| {
        u.side != side
            && class(&u.class)
                .and_then(|c| c.sam_radius_km)
                .is_some_and(|r| u.hex.distance(hex) <= range_in_hexes(r, km))
    });
    SideSupport {
        artillery_supply,
        air_supply,
        air_readiness_pct: strike.map_or(0, |a| a.readiness_pct(data)),
        hq_rating,
        reserve_class,
        engineers,
        enemy_sam,
    }
}

/// If `cmd` is a ground attack, the battle input it would give (declared on
/// a copy of the state; the game itself is not touched).
pub fn battle_of(s: &GameState, cmd: &Command) -> Option<BattleInput> {
    let Command::Attack { unit, target } = cmd else {
        return None;
    };
    let att = s.unit(*unit)?.clone();
    let def = s.unit(*target)?.clone();
    let mut trial = s.clone();
    let events = trial
        .apply(Command::DeclareAttack {
            unit: *unit,
            target: *target,
        })
        .ok()?;
    let scenario = events.into_iter().find_map(|e| match e {
        Event::CombatDeclared { snapshot, .. } => Some(*snapshot),
        _ => None,
    })?;
    Some(BattleInput {
        scenario,
        support: SupportInfo {
            attacker: side_support(s, att.side, &att, def.hex),
            defender: side_support(s, def.side, &def, def.hex),
        },
    })
}

pub fn write(dir: &Path, name: &str, input: &BattleInput) -> Result<(), String> {
    let path = dir.join(format!("{name}.json"));
    let text = serde_json::to_string(input).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}
