//! T-SCALE-01, T-TERR-01, T-LOS-01: the tactical map (10 × 14, portrait,
//! flat-top offset coordinates of the core) and lines of sight.

use std::collections::{BTreeMap, BTreeSet};

use ceasefire_core::hex::Hex;
use ceasefire_core::scenario::{axial_to_offset, offset_to_axial};
use serde::{Deserialize, Serialize};

use crate::rules::TacRules;
use crate::TacError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub terrain: String,
    /// Height level 0–3 for lines of sight.
    pub level: u8,
    /// T-SUP-01: a minefield (hidden until found).
    pub mine: bool,
    pub mine_known: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacMap {
    pub cols: i32,
    pub rows: i32,
    pub cells: BTreeMap<Hex, Cell>,
    /// T-SCALE-01: 2–3 key points in the defender's (top) half.
    pub key_points: Vec<Hex>,
    /// T-SUP-01: canal sides (a river or canal across the map); crossed by a
    /// bridge, or by infantry with all its move.
    pub canal: BTreeSet<(Hex, Hex)>,
    pub bridges: BTreeSet<(Hex, Hex)>,
}

fn side_key(a: Hex, b: Hex) -> (Hex, Hex) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

impl TacMap {
    pub fn at(col: i32, row: i32) -> Hex {
        offset_to_axial([col, row])
    }

    pub fn offset(h: Hex) -> (i32, i32) {
        let [c, r] = axial_to_offset(h);
        (c, r)
    }

    pub fn cell(&self, h: Hex) -> Option<&Cell> {
        self.cells.get(&h)
    }

    pub fn has_canal(&self, a: Hex, b: Hex) -> bool {
        self.canal.contains(&side_key(a, b))
    }

    pub fn has_bridge(&self, a: Hex, b: Hex) -> bool {
        self.bridges.contains(&side_key(a, b))
    }

    pub fn add_canal(&mut self, a: Hex, b: Hex) {
        self.canal.insert(side_key(a, b));
    }

    pub fn add_bridge(&mut self, a: Hex, b: Hex) {
        self.bridges.insert(side_key(a, b));
    }

    pub fn neighbors(&self, h: Hex) -> Vec<Hex> {
        h.neighbors()
            .into_iter()
            .filter(|n| self.cells.contains_key(n))
            .collect()
    }

    /// Height level of a hex (0 off the map).
    pub fn level(&self, h: Hex) -> u8 {
        self.cell(h).map_or(0, |c| c.level)
    }

    /// T-LOS-01: the hexes a center-to-center line passes, ends included
    /// (cube interpolation with a small nudge, Red Blob Games).
    pub fn line(a: Hex, b: Hex) -> Vec<Hex> {
        let n = a.distance(b);
        if n == 0 {
            return vec![a];
        }
        let (aq, ar) = (f64::from(a.q) + 1e-6, f64::from(a.r) + 1e-6);
        let (bq, br) = (f64::from(b.q) + 1e-6, f64::from(b.r) + 1e-6);
        (0..=n)
            .map(|i| {
                let t = f64::from(i) / f64::from(n);
                cube_round(aq + (bq - aq) * t, ar + (br - ar) * t)
            })
            .collect()
    }

    /// T-LOS-01: line of sight between hexes. Blocked by an intermediate hex
    /// higher than both ends, or by blocking terrain (village, orchard) or
    /// smoke at a level not below both ends — so an observer on a height sees
    /// over villages and orchards below it (night interpretation, see
    /// docs/decisions-tactical.md).
    pub fn sees(&self, rules: &TacRules, a: Hex, b: Hex, smoke: &BTreeSet<Hex>) -> bool {
        let (la, lb) = (self.level(a), self.level(b));
        let line = Self::line(a, b);
        for h in line.iter().skip(1).take(line.len().saturating_sub(2)) {
            let Some(cell) = self.cell(*h) else {
                return false;
            };
            let l = cell.level;
            if l > la && l > lb {
                return false;
            }
            let blocks =
                smoke.contains(h) || rules.terrain.get(&cell.terrain).is_some_and(|t| t.blocks);
            if blocks && l >= la && l >= lb {
                return false;
            }
        }
        true
    }

    /// Defender's half: rows 0 .. rows/2 (top).
    pub fn in_defender_half(&self, h: Hex) -> bool {
        Self::offset(h).1 < self.rows / 2
    }

    pub fn validate(&self, rules: &TacRules) -> Result<(), TacError> {
        for c in self.cells.values() {
            rules.terrain(&c.terrain)?;
        }
        Ok(())
    }
}

fn cube_round(q: f64, r: f64) -> Hex {
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    Hex::new(rq as i32, rr as i32)
}
