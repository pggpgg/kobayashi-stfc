//! Building data: normalized level → bonuses (from STFCcommunity or manual).
//! For advanced player profile: building level → stat bonuses.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Normalized building record (KOBAYASHI schema). One per building.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingRecord {
    pub id: String,
    pub building_name: String,
    pub levels: Vec<BuildingLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingLevel {
    pub level: u32,
    pub bonuses: Vec<BonusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonusEntry {
    pub stat: String,
    pub value: f64,
    #[serde(default)]
    pub operator: String,
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
