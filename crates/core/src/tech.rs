//! R-TECH-01: technology branches and levels. Units keep level-1 values in
//! data; tech effects add per-level bonuses on top.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::data::{AttackValues, DefenseValues, UnitCatalog, UnitClass};
use crate::error::RuleError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechBranch {
    Armor,
    Antitank,
    AirDefense,
    Aviation,
    Ew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitStat {
    AttackSoft,
    AttackHard,
    AttackAir,
    AttackNaval,
    DefenseGround,
    DefenseAir,
    SamRadiusKm,
}

/// Side-wide modifiers that are not unit stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideModifier {
    /// Subtracted from SAM hit chance against this side's aircraft.
    SamHitPenalty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitEffect {
    pub branch: TechBranch,
    pub units: Vec<String>,
    pub stats: Vec<UnitStat>,
    /// Bonus at levels 1, 2, 3.
    pub by_level: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideEffect {
    pub branch: TechBranch,
    pub modifier: SideModifier,
    pub by_level: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechCatalog {
    pub unit_effects: Vec<UnitEffect>,
    pub side_effects: Vec<SideEffect>,
}

/// Tech level per branch for one side; missing branches are level 1.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TechLevels(BTreeMap<TechBranch, u8>);

impl TechLevels {
    pub fn with(mut self, branch: TechBranch, level: u8) -> Self {
        self.0.insert(branch, level);
        self
    }

    /// R-TECH-02: one level up.
    pub fn raise(&mut self, branch: TechBranch) -> Result<u8, RuleError> {
        let level = self.level(branch)? + 1;
        self.0.insert(branch, level);
        self.level(branch)
    }

    pub fn level(&self, branch: TechBranch) -> Result<u8, RuleError> {
        let level = self.0.get(&branch).copied().unwrap_or(1);
        if !(1..=3).contains(&level) {
            return Err(RuleError::InvalidTechLevel {
                branch: format!("{branch:?}"),
                level,
            });
        }
        Ok(level)
    }

    fn index(&self, branch: TechBranch) -> Result<usize, RuleError> {
        Ok(usize::from(self.level(branch)? - 1))
    }
}

/// Unit values after technology bonuses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitStats {
    pub attack: AttackValues,
    pub defense: DefenseValues,
    pub sam_radius_km: Option<f64>,
}

impl UnitStats {
    pub fn base(class: &UnitClass) -> Self {
        Self {
            attack: class.attack,
            defense: class.defense,
            sam_radius_km: class.sam_radius_km,
        }
    }

    fn add(&mut self, stat: UnitStat, bonus: f64) {
        match stat {
            UnitStat::AttackSoft => self.attack.soft += bonus,
            UnitStat::AttackHard => self.attack.hard += bonus,
            UnitStat::AttackAir => self.attack.air += bonus,
            UnitStat::AttackNaval => self.attack.naval += bonus,
            UnitStat::DefenseGround => {
                if let Some(d) = self.defense.ground.as_mut() {
                    *d += bonus;
                }
            }
            UnitStat::DefenseAir => self.defense.air += bonus,
            UnitStat::SamRadiusKm => {
                if let Some(r) = self.sam_radius_km.as_mut() {
                    *r += bonus;
                }
            }
        }
    }
}

impl TechCatalog {
    pub fn from_json(json: &str) -> Result<Self, RuleError> {
        serde_json::from_str(json).map_err(|e| RuleError::Parse(e.to_string()))
    }

    /// Every unit id referenced by an effect must exist.
    pub fn validate(&self, units: &UnitCatalog) -> Result<(), RuleError> {
        for effect in &self.unit_effects {
            for id in &effect.units {
                units.get(id)?;
            }
        }
        Ok(())
    }

    /// R-TECH-01: unit values at the side's tech levels.
    pub fn unit_stats(
        &self,
        class: &UnitClass,
        levels: &TechLevels,
    ) -> Result<UnitStats, RuleError> {
        let mut stats = UnitStats::base(class);
        for effect in &self.unit_effects {
            if !effect.units.contains(&class.id) {
                continue;
            }
            let bonus = effect.by_level[levels.index(effect.branch)?];
            for &stat in &effect.stats {
                stats.add(stat, bonus);
            }
        }
        Ok(stats)
    }

    /// R-TECH-01: sum of a side-wide modifier at the side's tech levels.
    pub fn side_modifier(
        &self,
        modifier: SideModifier,
        levels: &TechLevels,
    ) -> Result<f64, RuleError> {
        let mut total = 0.0;
        for effect in self.side_effects.iter().filter(|e| e.modifier == modifier) {
            total += effect.by_level[levels.index(effect.branch)?];
        }
        Ok(total)
    }
}
