//! Scenario and map loading (`data/scenarios/<id>/scenario.json`, `map.json`)
//! and scenario validation. JSON comes in as strings; the core does no I/O.
//!
//! Hexes in the files are offset coordinates `[col, row]` (flat-top, odd-q:
//! odd columns are shifted down), converted to axial on load.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::data::{GameData, TargetClass, UnitTag};
use crate::error::RuleError;
use crate::game::{
    CityValue, Effect, KeyHex, NeutralZone, Objective, Predicate, ScenarioEvent, ScenarioSetup,
    ScheduledIncome, SideState, TechBoost, UnitSetup, VictoryThreshold,
};
use crate::hex::Hex;
use crate::map::{HexCell, HexMap};
use crate::side::SideId;
use crate::supply::SupplySource;
use crate::tech::TechLevels;

/// Offset `[col, row]` (odd-q) → axial.
pub fn offset_to_axial([col, row]: [i32; 2]) -> Hex {
    Hex::new(col, row - (col - (col & 1)) / 2)
}

/// Axial → offset `[col, row]` (odd-q).
pub fn axial_to_offset(h: Hex) -> [i32; 2] {
    [h.q, h.r + (h.q - (h.q & 1)) / 2]
}

fn parse<'a, T: Deserialize<'a>>(json: &'a str, what: &str) -> Result<T, RuleError> {
    serde_json::from_str(json).map_err(|e| RuleError::Parse(format!("{what}: {e}")))
}

fn invalid(msg: String) -> RuleError {
    RuleError::InvalidData(msg)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegendEntry {
    terrain: String,
    #[serde(default)]
    pass: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapFile {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    width: usize,
    height: usize,
    legend: BTreeMap<String, LegendEntry>,
    terrain: Vec<String>,
    /// Character → side id; any other character means "nobody".
    control_legend: BTreeMap<String, u8>,
    control: Vec<String>,
    #[serde(default)]
    roads: Vec<Vec<[i32; 2]>>,
    /// R-MOVE-01: dirt roads, polylines of hexes like `roads`.
    #[serde(default)]
    tracks: Vec<Vec<[i32; 2]>>,
    /// R-AIR-01: airfield hexes.
    #[serde(default)]
    airfields: Vec<[i32; 2]>,
    #[serde(default)]
    canals: Vec<[[i32; 2]; 2]>,
    #[serde(default)]
    bridges: Vec<[[i32; 2]; 2]>,
    /// R-MOVE-02: destroyed bridges (remains on the canal edge).
    #[serde(default)]
    destroyed_bridges: Vec<[[i32; 2]; 2]>,
    /// R-MOVE-02: ferries — supply across the canal only.
    #[serde(default)]
    ferries: Vec<[[i32; 2]; 2]>,
    /// Sources of bridges and ferries (map, date) or `"assumption": true`;
    /// the rules ignore them.
    #[serde(default)]
    #[allow(dead_code)]
    crossing_sources: Option<serde_json::Value>,
    /// Where a generated map came from (mapgen); not used by the rules.
    #[serde(default)]
    #[allow(dead_code)]
    geo: Option<GeoFile>,
    /// R-UI-01: historical road layer with sources (mapgen `roads`); the rules
    /// use `roads`, which mapgen derives from it when `roads_from` says so.
    #[serde(default)]
    #[allow(dead_code)]
    historical_roads: Vec<serde_json::Value>,
    #[serde(default)]
    #[allow(dead_code)]
    roads_from: Option<String>,
    /// OSM-derived roads, bridges and terrain kept while the historical layer
    /// is in use, so it can be switched back.
    #[serde(default)]
    #[allow(dead_code)]
    osm_backup: Option<serde_json::Value>,
    /// R-AIR-01: the airfields' lon/lat and sources behind `airfields` (mapgen).
    #[serde(default)]
    #[allow(dead_code)]
    airfield_sources: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct GeoFile {
    /// West, south, east, north (degrees).
    bbox: [f64; 4],
    /// Distance between neighbouring hex centres.
    hex_km: f64,
    #[serde(default)]
    hillshade: Option<String>,
    #[serde(default)]
    sources: Vec<String>,
    /// Named places from OSM, for the editor.
    #[serde(default)]
    places: Vec<PlaceFile>,
    /// Extent of the bbox in km (east, south).
    #[serde(default)]
    size_km: Option<[f64; 2]>,
    /// R-UI-01: made by `mapgen procgen` (fictional land, no geodata).
    #[serde(default)]
    procedural: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct PlaceFile {
    name: String,
    kind: String,
    hex: [i32; 2],
    #[serde(default)]
    population: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SideFile {
    id: u8,
    /// Short tag for logs and localisation keys.
    #[allow(dead_code)]
    key: String,
    #[serde(default)]
    tech: TechLevels,
    national_morale_pct: u8,
    #[serde(default)]
    mpp: u32,
    #[serde(default)]
    base_income: u32,
    #[serde(default = "one")]
    loss_sensitivity: u8,
    /// R-ECO-03: class → count, or `{count, alt}` — `alt: true` marks a
    /// historical unit that fought elsewhere («альтернатива», R-UI-07).
    #[serde(default)]
    purchase_limits: BTreeMap<String, LimitFile>,
    /// R-TURN-02: unit classes of the side that fight at night without penalties.
    #[serde(default)]
    night_combat: Vec<String>,
    /// Where `night_combat` comes from (a reference or «предположение»); the rules ignore it.
    #[serde(default)]
    #[allow(dead_code)]
    night_combat_source: Option<String>,
    /// R-UI-03: ring and control colour ("#rrggbb"); client data, the rules ignore it.
    #[serde(default)]
    #[allow(dead_code)]
    color: Option<String>,
    /// R-UI-03: default facing of the side's models ("left" | "right", towards
    /// the enemy); client data, the rules ignore it.
    #[serde(default)]
    #[allow(dead_code)]
    facing: Option<String>,
    /// R-UI-03: horizontal stripes of the small flag on the unit's plate,
    /// `[["#rrggbb", share], …]` top to bottom; client data.
    #[serde(default)]
    #[allow(dead_code)]
    flag: Option<serde_json::Value>,
}

fn one() -> u8 {
    1
}

/// R-ECO-03 limit entry: a bare count, or a count with the «альтернатива» flag.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LimitFile {
    Count(u32),
    Entry {
        count: u32,
        #[serde(default)]
        alt: bool,
    },
}

impl LimitFile {
    fn count(&self) -> u32 {
        match self {
            Self::Count(n) | Self::Entry { count: n, .. } => *n,
        }
    }

    fn alt(&self) -> bool {
        matches!(self, Self::Entry { alt: true, .. })
    }
}

fn full_strength() -> u8 {
    10
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitFile {
    class: String,
    side: u8,
    hex: [i32; 2],
    #[serde(default = "full_strength")]
    strength: u8,
    morale_pct: u8,
    #[serde(default)]
    exp: u16,
    #[serde(default)]
    entrenchment: u8,
    #[serde(default)]
    supply: Option<u8>,
    #[serde(default)]
    hq_rating: Option<u8>,
    #[serde(default)]
    formation: Option<String>,
    #[serde(default)]
    nation: Option<String>,
    /// R-UI-03: optional sprite model id; does not change unit statistics.
    #[serde(default)]
    model: Option<String>,
    /// R-UI-04: the formation's name, a locale key (`formation.*`).
    #[serde(default)]
    name: Option<String>,
    /// R-AIR-01: aircraft based off the map (an `offmap_bases` id); `hex` is ignored then.
    #[serde(default)]
    offmap_base: Option<String>,
    /// The unit exists only when every listed scenario option has this value
    /// (e.g. `{"first_strike": "free"}`).
    #[serde(default)]
    when: BTreeMap<String, String>,
    /// Where the unit and its position come from, and whether a source
    /// confirms it (section 14: unconfirmed data is marked); the rules ignore them.
    #[serde(default)]
    #[allow(dead_code)]
    source: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    confirmed: Option<bool>,
    #[serde(default, rename = "_comment")]
    #[allow(dead_code)]
    comment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HexValue {
    hex: [i32; 2],
    value: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectiveFile {
    hex: [i32; 2],
    nm_value: u8,
    vp: i32,
    /// R-ECO-01: MPP each own phase while held.
    #[serde(default)]
    income: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyHexFile {
    hex: [i32; 2],
    side: u8,
    nm_per_turn: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VictoryFile {
    side: u8,
    decisive: i32,
    victory: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IncomeFile {
    side: u8,
    from_turn: u32,
    #[serde(default)]
    until_turn: Option<u32>,
    amount: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioFile {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    #[allow(dead_code)]
    id: String,
    hex_km: f64,
    #[serde(default)]
    supply_decay_ap: Option<f64>,
    seed: u64,
    #[serde(default = "first_turn")]
    start_turn: u32,
    #[serde(default)]
    last_turn: Option<u32>,
    sides: Vec<SideFile>,
    #[serde(default)]
    sources: Vec<HexValue>,
    #[serde(default)]
    cities: Vec<HexValue>,
    #[serde(default)]
    objectives: Vec<ObjectiveFile>,
    #[serde(default)]
    key_hexes: Vec<KeyHexFile>,
    #[serde(default)]
    victory: Vec<VictoryFile>,
    #[serde(default)]
    scheduled_income: Vec<IncomeFile>,
    #[serde(default)]
    tech_boosts: Vec<BoostFile>,
    /// R-MOVE-06: neutral closed territory by country, offset hexes.
    #[serde(default)]
    neutral: Vec<NeutralFile>,
    /// R-UI-04: calendar date of the first turn ("YYYY-MM-DD"); client data,
    /// the rules ignore it.
    #[serde(default)]
    #[allow(dead_code)]
    start_date: Option<String>,
    /// R-AI-01: hints for the computer opponent (priorities, aggressiveness);
    /// read by the AI crate, the rules ignore them.
    #[serde(default)]
    #[allow(dead_code)]
    ai: Option<serde_json::Value>,
    /// R-AIR-01: bases off the map.
    #[serde(default)]
    offmap_bases: Vec<OffMapBaseFile>,
    /// Scenario options and their chosen values (e.g. `first_strike`:
    /// `historical` | `free`); units carry `when` conditions on them.
    #[serde(default)]
    options: BTreeMap<String, String>,
    /// Rule variants on or off for this scenario (names of `rules.variants`).
    #[serde(default)]
    variants: BTreeMap<String, bool>,
    /// R-NM-02: sudden-death threshold per side (`null` — never breaks).
    #[serde(default)]
    sudden_death: Vec<SuddenDeathFile>,
    /// R-NM-02: exit hexes per side.
    #[serde(default)]
    exits: Vec<ExitFile>,
    /// R-NM-02: VP per enemy step lost, per side.
    #[serde(default)]
    vp_per_enemy_step: Vec<SideVpFile>,
    units: Vec<UnitFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SuddenDeathFile {
    side: u8,
    threshold: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExitFile {
    side: u8,
    hexes: Vec<[i32; 2]>,
    vp_per_step: i32,
    #[serde(default)]
    open: bool,
    /// Where the hexes come from (data notes).
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SideVpFile {
    side: u8,
    vp: i32,
    /// R-NM-02: `vp` per this many enemy steps (default 1).
    #[serde(default = "one_step")]
    per_steps: u32,
}

fn one_step() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OffMapBaseFile {
    id: String,
    side: u8,
    edge: crate::game::MapEdge,
    distance_km: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NeutralFile {
    id: String,
    hexes: Vec<[i32; 2]>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoostFile {
    id: String,
    side: u8,
    branch: crate::tech::TechBranch,
    cost: u32,
    turns: u32,
}

fn first_turn() -> u32 {
    1
}

fn unit_setup(u: UnitFile) -> UnitSetup {
    UnitSetup {
        class: u.class,
        side: SideId(u.side),
        hex: offset_to_axial(u.hex),
        strength: u.strength,
        morale_pct: u.morale_pct,
        exp: u.exp,
        entrenchment: u.entrenchment,
        supply: u.supply,
        hq_rating: u.hq_rating,
        formation: u.formation,
        nation: u.nation,
        model: u.model,
        name: u.name,
        offmap_base: u.offmap_base,
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PredicateFile {
    Turn { turn: u32 },
    HexControl { hex: [i32; 2], side: u8 },
    NmAtMost { side: u8, pct: u8 },
    LossesAtLeast { side: u8, steps: u32 },
    UnitNear { side: u8, hex: [i32; 2], km: u32 },
    AnyOf { of: Vec<PredicateFile> },
    AllOf { of: Vec<PredicateFile> },
}

impl From<PredicateFile> for Predicate {
    fn from(p: PredicateFile) -> Self {
        match p {
            PredicateFile::Turn { turn } => Predicate::Turn { turn },
            PredicateFile::HexControl { hex, side } => Predicate::HexControl {
                hex: offset_to_axial(hex),
                side: SideId(side),
            },
            PredicateFile::NmAtMost { side, pct } => Predicate::NmAtMost {
                side: SideId(side),
                pct,
            },
            PredicateFile::LossesAtLeast { side, steps } => Predicate::LossesAtLeast {
                side: SideId(side),
                steps,
            },
            PredicateFile::UnitNear { side, hex, km } => Predicate::UnitNear {
                side: SideId(side),
                hex: offset_to_axial(hex),
                km,
            },
            PredicateFile::AnyOf { of } => Predicate::AnyOf {
                of: of.into_iter().map(Into::into).collect(),
            },
            PredicateFile::AllOf { of } => Predicate::AllOf {
                of: of.into_iter().map(Into::into).collect(),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum EffectFile {
    Reinforcements { units: Vec<UnitFile> },
    Mpp { side: u8, amount: i32 },
    Nm { side: u8, change: i32 },
    ShiftLastTurn { turns: i32 },
    SetLastTurn { turn: u32 },
    Message { key: String },
    UnitsMorale { side: u8, change: i32 },
    UnitsEntrenchment { side: u8, change: i32 },
    OpenExits { side: u8 },
    UnitsReadinessPenalty { side: u8, pct: u8 },
    DestroyBridge { a: [i32; 2], b: [i32; 2] },
}

impl From<EffectFile> for Effect {
    fn from(e: EffectFile) -> Self {
        match e {
            EffectFile::Reinforcements { units } => Effect::Reinforcements {
                units: units.into_iter().map(unit_setup).collect(),
            },
            EffectFile::Mpp { side, amount } => Effect::Mpp {
                side: SideId(side),
                amount,
            },
            EffectFile::Nm { side, change } => Effect::Nm {
                side: SideId(side),
                change,
            },
            EffectFile::ShiftLastTurn { turns } => Effect::ShiftLastTurn { turns },
            EffectFile::SetLastTurn { turn } => Effect::SetLastTurn { turn },
            EffectFile::Message { key } => Effect::Message { key },
            EffectFile::UnitsMorale { side, change } => Effect::UnitsMorale {
                side: SideId(side),
                change,
            },
            EffectFile::UnitsEntrenchment { side, change } => Effect::UnitsEntrenchment {
                side: SideId(side),
                change,
            },
            EffectFile::OpenExits { side } => Effect::OpenExits { side: SideId(side) },
            EffectFile::UnitsReadinessPenalty { side, pct } => Effect::UnitsReadinessPenalty {
                side: SideId(side),
                pct,
            },
            EffectFile::DestroyBridge { a, b } => Effect::DestroyBridge {
                a: offset_to_axial(a),
                b: offset_to_axial(b),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventFile {
    id: String,
    /// A note for designers; ignored.
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    #[serde(default)]
    phase_of: Option<u8>,
    /// R-UI-07: show as a window until clicked.
    #[serde(default)]
    important: bool,
    /// R-UI-07: the window's title and text keys and illustration id (client data).
    #[serde(default)]
    #[allow(dead_code)]
    window: Option<serde_json::Value>,
    trigger: PredicateFile,
    #[serde(default)]
    conditions: Vec<PredicateFile>,
    effects: Vec<EffectFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventsFile {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    events: Vec<EventFile>,
}

/// R-EVT-01: `events.json`.
fn parse_events(json: &str) -> Result<Vec<ScenarioEvent>, RuleError> {
    let file: EventsFile = parse(json, "events.json")?;
    Ok(file
        .events
        .into_iter()
        .map(|e| ScenarioEvent {
            id: e.id,
            important: e.important,
            phase_of: e.phase_of.map(SideId),
            trigger: e.trigger.into(),
            conditions: e.conditions.into_iter().map(Into::into).collect(),
            effects: e.effects.into_iter().map(Into::into).collect(),
        })
        .collect())
}

/// `map.json` → map and initial hex control.
pub fn load_map(json: &str) -> Result<(HexMap, BTreeMap<Hex, SideId>), RuleError> {
    let file: MapFile = parse(json, "map.json")?;
    if file.terrain.len() != file.height || file.control.len() != file.height {
        return Err(invalid(format!(
            "map: need {} terrain and control rows",
            file.height
        )));
    }
    let mut map = HexMap::default();
    let mut control = BTreeMap::new();
    for (row, (t_line, c_line)) in file.terrain.iter().zip(&file.control).enumerate() {
        let t: Vec<char> = t_line.chars().collect();
        let c: Vec<char> = c_line.chars().collect();
        if t.len() != file.width || c.len() != file.width {
            return Err(invalid(format!(
                "map row {row}: need {} characters",
                file.width
            )));
        }
        for col in 0..file.width {
            let hex = offset_to_axial([col as i32, row as i32]);
            let entry = file
                .legend
                .get(&t[col].to_string())
                .ok_or_else(|| invalid(format!("map [{col},{row}]: no legend for '{}'", t[col])))?;
            map.insert(
                hex,
                HexCell {
                    terrain: entry.terrain.clone(),
                    pass: entry.pass,
                },
            );
            if let Some(&side) = file.control_legend.get(&c[col].to_string()) {
                control.insert(hex, SideId(side));
            }
        }
    }
    for road in &file.roads {
        for pair in road.windows(2) {
            map.add_road(offset_to_axial(pair[0]), offset_to_axial(pair[1]))
                .map_err(|_| {
                    invalid(format!("road {:?} → {:?}: not adjacent", pair[0], pair[1]))
                })?;
        }
    }
    for track in &file.tracks {
        for pair in track.windows(2) {
            map.add_track(offset_to_axial(pair[0]), offset_to_axial(pair[1]))
                .map_err(|_| {
                    invalid(format!("track {:?} → {:?}: not adjacent", pair[0], pair[1]))
                })?;
        }
    }
    for [a, b] in &file.canals {
        map.add_canal(offset_to_axial(*a), offset_to_axial(*b))?;
    }
    for h in &file.airfields {
        if map.cell(offset_to_axial(*h)).is_none() {
            return Err(invalid(format!("airfield {h:?} is off the map")));
        }
        map.add_airfield(offset_to_axial(*h));
    }
    for [a, b] in &file.bridges {
        map.add_bridge(offset_to_axial(*a), offset_to_axial(*b))?;
    }
    for [a, b] in &file.destroyed_bridges {
        map.destroy_bridge(offset_to_axial(*a), offset_to_axial(*b))?;
    }
    for [a, b] in &file.ferries {
        map.add_ferry(offset_to_axial(*a), offset_to_axial(*b))?;
    }
    Ok((map, control))
}

/// Builds a [`ScenarioSetup`] from `scenario.json`, `map.json` and the
/// optional `events.json` texts.
pub fn load_scenario(
    scenario_json: &str,
    map_json: &str,
    events_json: Option<&str>,
) -> Result<ScenarioSetup, RuleError> {
    let file: ScenarioFile = parse(scenario_json, "scenario.json")?;
    let (map, control) = load_map(map_json)?;
    let hex = offset_to_axial;
    Ok(ScenarioSetup {
        hex_km: Some(file.hex_km),
        supply_decay_ap: file.supply_decay_ap,
        seed: file.seed,
        start_turn: file.start_turn,
        sides: file
            .sides
            .into_iter()
            .map(|s| SideState {
                id: SideId(s.id),
                tech: s.tech,
                national_morale_pct: s.national_morale_pct,
                mpp: s.mpp,
                base_income: s.base_income,
                purchase_alt: s
                    .purchase_limits
                    .iter()
                    .filter(|(_, l)| l.alt())
                    .map(|(c, _)| c.clone())
                    .collect(),
                purchase_limits: s
                    .purchase_limits
                    .iter()
                    .map(|(c, l)| (c.clone(), l.count()))
                    .collect(),
                purchased: BTreeMap::new(),
                loss_sensitivity: s.loss_sensitivity,
                steps_lost: 0,
                night_combat: s.night_combat,
            })
            .collect(),
        map,
        control,
        sources: file
            .sources
            .iter()
            .map(|s| {
                let value = u8::try_from(s.value)
                    .map_err(|_| invalid(format!("source {:?}: value", s.hex)))?;
                Ok(SupplySource {
                    hex: hex(s.hex),
                    value,
                })
            })
            .collect::<Result<_, RuleError>>()?,
        // Units of the chosen scenario options only (`when`).
        units: file
            .units
            .into_iter()
            .filter(|u| u.when.iter().all(|(k, v)| file.options.get(k) == Some(v)))
            .map(unit_setup)
            .collect(),
        cities: file
            .cities
            .iter()
            .map(|c| CityValue {
                hex: hex(c.hex),
                value: c.value,
            })
            .collect(),
        scheduled_income: file
            .scheduled_income
            .iter()
            .map(|e| ScheduledIncome {
                side: SideId(e.side),
                from_turn: e.from_turn,
                until_turn: e.until_turn,
                amount: e.amount,
            })
            .collect(),
        objectives: file
            .objectives
            .iter()
            .map(|o| Objective {
                hex: hex(o.hex),
                nm_value: o.nm_value,
                vp: o.vp,
                income: o.income,
            })
            .collect(),
        key_hexes: file
            .key_hexes
            .iter()
            .map(|k| KeyHex {
                hex: hex(k.hex),
                side: SideId(k.side),
                nm_per_turn: k.nm_per_turn,
            })
            .collect(),
        last_turn: file.last_turn,
        victory: file
            .victory
            .iter()
            .map(|v| VictoryThreshold {
                side: SideId(v.side),
                decisive: v.decisive,
                victory: v.victory,
            })
            .collect(),
        events: events_json
            .map(parse_events)
            .transpose()?
            .unwrap_or_default(),
        tech_boosts: file
            .tech_boosts
            .into_iter()
            .map(|b| TechBoost {
                id: b.id,
                side: SideId(b.side),
                branch: b.branch,
                cost: b.cost,
                turns: b.turns,
            })
            .collect(),
        variants: file.variants.clone(),
        conditions: crate::game::VictoryConditions {
            sudden_death: file
                .sudden_death
                .iter()
                .map(|x| (SideId(x.side), x.threshold))
                .collect(),
            exits: file
                .exits
                .iter()
                .map(|x| crate::game::ExitZone {
                    side: SideId(x.side),
                    hexes: x.hexes.iter().map(|h| hex(*h)).collect(),
                    vp_per_step: x.vp_per_step,
                    open: x.open,
                })
                .collect(),
            vp_per_enemy_step: file
                .vp_per_enemy_step
                .iter()
                .map(|x| {
                    (
                        SideId(x.side),
                        crate::game::StepVp {
                            vp: x.vp,
                            per_steps: x.per_steps,
                        },
                    )
                })
                .collect(),
        },
        offmap_bases: file
            .offmap_bases
            .into_iter()
            .map(|b| crate::game::OffMapBase {
                id: b.id,
                side: SideId(b.side),
                edge: b.edge,
                distance_km: b.distance_km,
            })
            .collect(),
        neutral: file
            .neutral
            .into_iter()
            .map(|n| NeutralZone {
                id: n.id,
                hexes: n.hexes.into_iter().map(offset_to_axial).collect(),
            })
            .collect(),
    })
}

/// Problems that make a scenario unplayable. Hexes are reported in offset
/// coordinates, as in the files.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScenarioIssue {
    UnknownSide {
        unit: usize,
        side: u8,
    },
    UnknownClass {
        unit: usize,
        class: String,
    },
    UnknownTerrain {
        hex: [i32; 2],
        terrain: String,
    },
    OffMap {
        unit: usize,
        hex: [i32; 2],
    },
    /// Ground unit on terrain it cannot enter (mountains without a pass).
    Impassable {
        unit: usize,
        hex: [i32; 2],
    },
    /// R-TURN-03: more than one unit of a layer in a hex.
    LayerClash {
        hex: [i32; 2],
        layer: &'static str,
    },
    /// R-HQ-02: a side with line units but no HQ.
    NoHq {
        side: u8,
    },
    /// Sources, cities, objectives or key hexes off the map.
    MarkerOffMap {
        what: &'static str,
        hex: [i32; 2],
    },
    /// R-MOVE-01: water belongs to nobody.
    ControlledWater {
        hex: [i32; 2],
    },
    /// R-MOVE-06: neutral closed territory holds no units, control or
    /// scenario markers (`what`: unit, control, source, city, objective,
    /// key hex, event).
    Neutral {
        what: &'static str,
        hex: [i32; 2],
    },
    /// R-TECH-02.
    DuplicateBoost {
        id: String,
    },
    BoostUnknownSide {
        boost: String,
        side: u8,
    },
    /// R-EVT-01.
    DuplicateEvent {
        id: String,
    },
    EventUnknownSide {
        event: String,
        side: u8,
    },
    EventUnknownClass {
        event: String,
        class: String,
    },
    EventHexOffMap {
        event: String,
        hex: [i32; 2],
    },
}

/// Scenario checks beyond what `GameState::new` enforces, reported all at once.
pub fn validate(data: &GameData, setup: &ScenarioSetup) -> Vec<ScenarioIssue> {
    let mut issues = Vec::new();
    let off = axial_to_offset;
    for hex in setup.map.hexes() {
        if let Some(cell) = setup.map.cell(hex) {
            if data.terrain.get(&cell.terrain).is_err() {
                issues.push(ScenarioIssue::UnknownTerrain {
                    hex: off(hex),
                    terrain: cell.terrain.clone(),
                });
            }
        }
    }
    for hex in setup.control.keys() {
        let water = setup
            .map
            .cell(*hex)
            .and_then(|c| data.terrain.get(&c.terrain).ok())
            .is_some_and(|t| t.def.impassable);
        if water {
            issues.push(ScenarioIssue::ControlledWater { hex: off(*hex) });
        }
    }
    let sides: BTreeSet<SideId> = setup.sides.iter().map(|s| s.id).collect();
    let mut layers: BTreeMap<(Hex, &'static str), usize> = BTreeMap::new();
    let mut line_units: BTreeSet<SideId> = BTreeSet::new();
    let mut hqs: BTreeSet<SideId> = BTreeSet::new();
    for (i, u) in setup.units.iter().enumerate() {
        if !sides.contains(&u.side) {
            issues.push(ScenarioIssue::UnknownSide {
                unit: i,
                side: u.side.0,
            });
        }
        let Ok(class) = data.units.get(&u.class) else {
            issues.push(ScenarioIssue::UnknownClass {
                unit: i,
                class: u.class.clone(),
            });
            continue;
        };
        // R-AIR-01: aircraft sit on bases (several per base, or off the map)
        // and need no HQ; `GameState::new` checks their base.
        if class.target_class == TargetClass::Air {
            continue;
        }
        let layer = match class.target_class {
            TargetClass::Air => "air",
            TargetClass::Naval => "naval",
            _ => "ground",
        };
        *layers.entry((u.hex, layer)).or_insert(0) += 1;
        if class.has_tag(UnitTag::Hq) {
            hqs.insert(u.side);
        } else {
            line_units.insert(u.side);
        }
        let Some(cell) = setup.map.cell(u.hex) else {
            issues.push(ScenarioIssue::OffMap {
                unit: i,
                hex: off(u.hex),
            });
            continue;
        };
        let blocked = data
            .terrain
            .get(&cell.terrain)
            .is_ok_and(|t| t.def.impassable || (t.def.requires_pass && !cell.pass));
        if layer == "ground" && blocked {
            issues.push(ScenarioIssue::Impassable {
                unit: i,
                hex: off(u.hex),
            });
        }
    }
    for ((hex, layer), n) in layers {
        if n > 1 {
            issues.push(ScenarioIssue::LayerClash {
                hex: off(hex),
                layer,
            });
        }
    }
    for side in line_units.difference(&hqs) {
        issues.push(ScenarioIssue::NoHq { side: side.0 });
    }
    let markers = setup
        .sources
        .iter()
        .map(|s| ("source", s.hex))
        .chain(setup.cities.iter().map(|c| ("city", c.hex)))
        .chain(setup.objectives.iter().map(|o| ("objective", o.hex)))
        .chain(setup.key_hexes.iter().map(|k| ("key hex", k.hex)));
    for (what, hex) in markers {
        if setup.map.cell(hex).is_none() {
            issues.push(ScenarioIssue::MarkerOffMap {
                what,
                hex: off(hex),
            });
        }
    }
    let mut boost_ids = BTreeSet::new();
    for b in &setup.tech_boosts {
        if !boost_ids.insert(b.id.as_str()) {
            issues.push(ScenarioIssue::DuplicateBoost { id: b.id.clone() });
        }
        if !sides.contains(&b.side) {
            issues.push(ScenarioIssue::BoostUnknownSide {
                boost: b.id.clone(),
                side: b.side.0,
            });
        }
    }
    validate_events(data, setup, &sides, &mut issues);
    validate_neutral(setup, &mut issues);
    issues
}

/// R-MOVE-06: nothing of the scenario inside neutral territory.
fn validate_neutral(setup: &ScenarioSetup, issues: &mut Vec<ScenarioIssue>) {
    let closed: BTreeSet<Hex> = setup
        .neutral
        .iter()
        .flat_map(|z| z.hexes.iter().copied())
        .collect();
    for h in &closed {
        if setup.map.cell(*h).is_none() {
            issues.push(ScenarioIssue::MarkerOffMap {
                what: "neutral",
                hex: axial_to_offset(*h),
            });
        }
    }
    let mut hit = |what: &'static str, hex: Hex| {
        if closed.contains(&hex) {
            issues.push(ScenarioIssue::Neutral {
                what,
                hex: axial_to_offset(hex),
            });
        }
    };
    for u in &setup.units {
        hit("unit", u.hex);
    }
    for h in setup.control.keys() {
        hit("control", *h);
    }
    for s in &setup.sources {
        hit("source", s.hex);
    }
    for c in &setup.cities {
        hit("city", c.hex);
    }
    for o in &setup.objectives {
        hit("objective", o.hex);
    }
    for k in &setup.key_hexes {
        hit("key hex", k.hex);
    }
    for e in &setup.events {
        for effect in &e.effects {
            if let Effect::Reinforcements { units } = effect {
                for u in units {
                    hit("event", u.hex);
                }
            }
        }
    }
}

/// R-EVT-01: unique ids, known sides and classes, hexes on the map.
fn validate_events(
    data: &GameData,
    setup: &ScenarioSetup,
    sides: &BTreeSet<SideId>,
    issues: &mut Vec<ScenarioIssue>,
) {
    let mut ids = BTreeSet::new();
    for e in &setup.events {
        let ev = || e.id.clone();
        if !ids.insert(e.id.as_str()) {
            issues.push(ScenarioIssue::DuplicateEvent { id: ev() });
        }
        let mut side_refs: Vec<SideId> = e.phase_of.into_iter().collect();
        let mut hexes = Vec::new();
        // Nested any_of / all_of: every leaf is checked.
        let mut stack: Vec<&Predicate> = std::iter::once(&e.trigger).chain(&e.conditions).collect();
        while let Some(p) = stack.pop() {
            match p {
                Predicate::HexControl { hex, side } => {
                    side_refs.push(*side);
                    hexes.push(*hex);
                }
                Predicate::NmAtMost { side, .. } | Predicate::LossesAtLeast { side, .. } => {
                    side_refs.push(*side)
                }
                Predicate::UnitNear { side, hex, .. } => {
                    side_refs.push(*side);
                    hexes.push(*hex);
                }
                Predicate::Turn { .. } => {}
                Predicate::AnyOf { of } | Predicate::AllOf { of } => stack.extend(of),
            }
        }
        for effect in &e.effects {
            match effect {
                Effect::Reinforcements { units } => {
                    for u in units {
                        side_refs.push(u.side);
                        hexes.push(u.hex);
                        if data.units.get(&u.class).is_err() {
                            issues.push(ScenarioIssue::EventUnknownClass {
                                event: ev(),
                                class: u.class.clone(),
                            });
                        }
                    }
                }
                Effect::Mpp { side, .. }
                | Effect::Nm { side, .. }
                | Effect::UnitsMorale { side, .. }
                | Effect::UnitsEntrenchment { side, .. }
                | Effect::UnitsReadinessPenalty { side, .. } => side_refs.push(*side),
                _ => {}
            }
        }
        for side in side_refs.into_iter().filter(|s| !sides.contains(s)) {
            issues.push(ScenarioIssue::EventUnknownSide {
                event: ev(),
                side: side.0,
            });
        }
        for hex in hexes.into_iter().filter(|h| setup.map.cell(*h).is_none()) {
            issues.push(ScenarioIssue::EventHexOffMap {
                event: ev(),
                hex: axial_to_offset(hex),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_axial_round_trip_and_adjacency() {
        for col in 0..6 {
            for row in 0..6 {
                assert_eq!(axial_to_offset(offset_to_axial([col, row])), [col, row]);
                // Same row, next column, and same column, next row, are neighbours.
                assert!(offset_to_axial([col, row]).is_adjacent(offset_to_axial([col + 1, row])));
                assert!(offset_to_axial([col, row]).is_adjacent(offset_to_axial([col, row + 1])));
            }
        }
    }
}
