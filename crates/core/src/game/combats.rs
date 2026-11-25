//! Tactical battles: the combat queue. `DeclareAttack` fixes a combat and
//! exports its snapshot; `ResolveCombat` applies a result — dice or a
//! tactical outcome. A phase cannot end while combats wait.

use serde::{Deserialize, Serialize};

use crate::combat::ExchangeForecast;
use crate::error::RuleError;
use crate::hex::Hex;
use crate::tactical::{
    AreaEdge, AreaHex, CombatSource, Origin, Role, TacticalOutcome, TacticalScenario, TacticalSide,
    TacticalUnit,
};

use super::{Command, Event, GameState, UnitId};

/// Everything fixed when an attack is declared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclaredAttack {
    pub attacker: UnitId,
    pub defender: UnitId,
    pub forecast: ExchangeForecast,
    /// R-COMBAT-06 uses readiness at the start of the combat.
    pub defender_readiness_pct: u8,
    pub attacker_strength: u8,
    pub defender_strength: u8,
    pub defender_hex: Hex,
    pub occupies: bool,
    pub by_boat: bool,
}

/// A declared combat waiting for its result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingCombat {
    pub id: u32,
    pub declared: DeclaredAttack,
    pub snapshot: TacticalScenario,
}

impl GameState {
    pub fn pending_combats(&self) -> &[PendingCombat] {
        &self.pending_combats
    }

    /// Units locked by a combat waiting for its result.
    pub(super) fn in_combat(&self, id: UnitId) -> bool {
        self.pending_combats
            .iter()
            .any(|c| c.declared.attacker == id || c.declared.defender == id)
    }

    /// Unit ids a command acts on (for the combat lock).
    pub(super) fn command_units(c: &Command) -> Vec<UnitId> {
        match c {
            Command::Move { unit, .. }
            | Command::Advance { unit }
            | Command::Reinforce { unit, .. }
            | Command::Transfer { unit, .. }
            | Command::PinToHq { unit, .. }
            | Command::BuildBridge { unit, .. }
            | Command::BombardBridge { unit, .. } => vec![*unit],
            Command::Attack { unit, target }
            | Command::Bombard { unit, target }
            | Command::DeclareAttack { unit, target } => {
                vec![*unit, *target]
            }
            Command::Purchase { .. }
            | Command::BuyTechBoost { .. }
            | Command::ResolveCombat { .. }
            | Command::EndPhase => vec![],
        }
    }

    pub(super) fn declare_attack(
        &mut self,
        id: UnitId,
        target: UnitId,
    ) -> Result<Vec<Event>, RuleError> {
        let declared = self.declare(id, target)?;
        let combat = self.next_combat_id;
        self.next_combat_id += 1;
        let snapshot = self.snapshot(combat, &declared)?;
        self.pending_combats.push(PendingCombat {
            id: combat,
            declared,
            snapshot: snapshot.clone(),
        });
        Ok(vec![Event::CombatDeclared {
            combat,
            attacker: id,
            defender: target,
            snapshot: Box::new(snapshot),
        }])
    }

    /// `outcome: None` = calculate automatically (dice).
    pub(super) fn resolve_combat(
        &mut self,
        combat: u32,
        outcome: Option<TacticalOutcome>,
    ) -> Result<Vec<Event>, RuleError> {
        let pos = self
            .pending_combats
            .iter()
            .position(|c| c.id == combat)
            .ok_or(RuleError::UnknownCombat(combat))?;
        let pending = self.pending_combats.remove(pos);
        let source = outcome.map_or(CombatSource::Auto, |outcome| CombatSource::Tactical {
            outcome,
        });
        self.conclude(&pending.declared, source)
    }

    /// The battle as a self-contained mini-scenario: the defender's hex and
    /// its six neighbours, both units, day/night, tech, the forecast.
    fn snapshot(&self, combat: u32, c: &DeclaredAttack) -> Result<TacticalScenario, RuleError> {
        let center = c.defender_hex;
        let rel = |h: Hex| [h.q - center.q, h.r - center.r];
        let area_hexes: Vec<Hex> = std::iter::once(center)
            .chain(center.neighbors())
            .filter(|h| self.map.cell(*h).is_some())
            .collect();
        let area = area_hexes
            .iter()
            .filter_map(|&h| {
                self.map.cell(h).map(|cell| {
                    let [dq, dr] = rel(h);
                    AreaHex {
                        dq,
                        dr,
                        terrain: cell.terrain.clone(),
                        pass: cell.pass,
                        owner: self.control.get(&h).map(|s| s.0),
                    }
                })
            })
            .collect();
        let mut edges = Vec::new();
        for (i, &a) in area_hexes.iter().enumerate() {
            for &b in &area_hexes[i + 1..] {
                if !a.is_adjacent(b) {
                    continue;
                }
                let (road, canal, bridge) = (
                    self.map.has_road(a, b),
                    self.map.has_canal(a, b),
                    self.map.bridge_state(a, b),
                );
                if road || canal || bridge.is_some() {
                    edges.push(AreaEdge {
                        a: rel(a),
                        b: rel(b),
                        road,
                        canal,
                        bridge,
                    });
                }
            }
        }
        let rules = &self.data.rules;
        let mut units = Vec::new();
        let mut sides = Vec::new();
        for (id, role) in [(c.attacker, Role::Attacker), (c.defender, Role::Defender)] {
            let u = self.get_unit(id)?;
            let [dq, dr] = rel(u.hex);
            units.push(TacticalUnit {
                side: u.side.0,
                role,
                class: u.class.clone(),
                nation: u.nation.clone(),
                strength: if role == Role::Attacker {
                    c.attacker_strength
                } else {
                    c.defender_strength
                },
                readiness_pct: u.readiness_pct(&rules.readiness),
                morale_pct: u.morale_pct,
                exp: u.exp,
                entrenchment: u.entrenchment,
                dq,
                dr,
            });
            sides.push(TacticalSide {
                side: u.side.0,
                role,
                tech: self.side_state(u.side)?.tech.clone(),
            });
        }
        // Readiness above is after the attacker's own fatigue from declaring;
        // the forecast is what counts and was taken before it.
        let attacker_hex = self.get_unit(c.attacker)?.hex;
        Ok(TacticalScenario {
            format: rules.tactical.snapshot_format,
            origin: Origin::Campaign {
                combat,
                turn: self.turn,
            },
            night: self.is_night(),
            hex_km: self.hex_km,
            across_canal: self.map.has_canal(attacker_hex, center),
            area,
            edges,
            sides,
            units,
            forecast: c.forecast,
            outcome_percentiles: rules.tactical.percentiles.clone(),
        })
    }
}
