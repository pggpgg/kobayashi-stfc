//! Building data: normalized level → bonuses (from STFCcommunity or manual).
//! For advanced player profile: building level → stat bonuses.
//! Each bonus uses engine/LCARS stat keys and additive vs multiplicative
//! semantics consistent with syndicate combat bonuses.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Normalized building record (KOBAYASHI schema). One per building.
/// Stats are stored as fractional bonuses where applicable (e.g. 0.35 = +35%).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingRecord {
    pub id: String,
    pub building_name: String,
    /// Optional provenance for this building file (e.g. "stfccommunity-main").
    #[serde(default)]
    pub data_version: Option<String>,
    /// Human-readable source description (e.g. "STFCcommunity baseline", "Manual corrections").
    #[serde(default)]
    pub source_note: Option<String>,
    pub levels: Vec<BuildingLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingLevel {
    pub level: u32,
    /// Optional Operations level band for which this row applies.
    #[serde(default)]
    pub ops_min: Option<u32>,
    #[serde(default)]
    pub ops_max: Option<u32>,
    pub bonuses: Vec<BonusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonusEntry {
    /// Engine/LCARS stat key (e.g. "weapon_damage", "hull_hp").
    pub stat: String,
    /// Bonus value (fractional where applicable: 0.35 = +35%).
    pub value: f64,
    /// Aggregation operator. "add" sums linearly; "multiply" stacks like
    /// (1 + current) * (1 + value) - 1. Empty defaults to "add".
    #[serde(default)]
    pub operator: String,
    /// Optional tags describing when this bonus applies (e.g. "defense_platform_only").
    #[serde(default)]
    pub conditions: Vec<String>,
    /// Optional free-form notes for edge cases or non-combat effects.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Index of all buildings. Includes data_version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub buildings: Vec<BuildingIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingIndexEntry {
    pub id: String,
    pub building_name: String,
}

pub const DEFAULT_BUILDINGS_INDEX_PATH: &str = "data/buildings/index.json";

pub fn load_building_index(path: &str) -> Option<BuildingIndex> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn load_building_record(data_dir: &Path, id: &str) -> Option<BuildingRecord> {
    let path = data_dir.join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn accumulate_bonus(
    out: &mut HashMap<String, f64>,
    stat: &str,
    operator: &str,
    value: f64,
) {
    let key = stat.to_string();
    let current = out.get(&key).copied().unwrap_or(0.0);
    let is_multiply = operator.eq_ignore_ascii_case("multiply")
        || operator.eq_ignore_ascii_case("mul")
        || operator.eq_ignore_ascii_case("mult");
    let new_value = if is_multiply {
        (1.0 + current) * (1.0 + value) - 1.0
    } else {
        current + value
    };
    out.insert(key, new_value);
}

/// Returns cumulative bonuses from a single building up to and including the
/// specified level. Levels outside the record are ignored.
pub fn cumulative_building_level_bonuses(
    record: &BuildingRecord,
    level: u32,
) -> HashMap<String, f64> {
    let mut out: HashMap<String, f64> = HashMap::new();

    for lvl in record.levels.iter().filter(|l| l.level <= level) {
        for bonus in &lvl.bonuses {
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            accumulate_bonus(&mut out, &bonus.stat, op, bonus.value);
        }
    }

    out
}

/// Returns cumulative bonuses from multiple buildings, given a mapping from
/// building id → level. Missing buildings or levels are ignored.
pub fn cumulative_building_bonuses(
    records: &[BuildingRecord],
    levels_by_id: &HashMap<String, u32>,
) -> HashMap<String, f64> {
    let mut out: HashMap<String, f64> = HashMap::new();
    let index: HashMap<&str, &BuildingRecord> =
        records.iter().map(|r| (r.id.as_str(), r)).collect();

    for (id, level) in levels_by_id {
        let Some(rec) = index.get(id.as_str()) else {
            continue;
        };
        for lvl in rec.levels.iter().filter(|l| l.level <= *level) {
            for bonus in &lvl.bonuses {
                let op = if bonus.operator.is_empty() {
                    "add"
                } else {
                    bonus.operator.as_str()
                };
                accumulate_bonus(&mut out, &bonus.stat, op, bonus.value);
            }
        }
    }

    out
}
