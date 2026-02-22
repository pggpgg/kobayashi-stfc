//! Hostile data: normalized combat stats for hostiles (from STFCcommunity or manual).
//! Used to build DefenderStats + hull + ShipType when resolving by name/id.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::{DefenderStats, ShipType};

#[derive(Debug, Clone)]
pub struct Hostile {
    pub id: String,
}

/// Normalized hostile record (KOBAYASHI schema). Written by normalizer, loaded at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileRecord {
    pub id: String,
    pub hostile_name: String,
    pub level: u32,
    pub ship_class: String,
    pub armor: f64,
    pub shield_deflection: f64,
    pub dodge: f64,
    pub hull_health: f64,
    pub shield_health: f64,
}

/// Index of all hostiles for name/level resolution. Includes data_version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub hostiles: Vec<HostileIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileIndexEntry {
    pub id: String,
    pub hostile_name: String,
    pub level: u32,
    pub ship_class: String,
}

impl HostileRecord {
    pub fn to_defender_stats(&self) -> DefenderStats {
        DefenderStats {
            armor: self.armor,
            shield_deflection: self.shield_deflection,
            dodge: self.dodge,
        }
    }

    pub fn ship_type(&self) -> ShipType {
        ship_class_to_type(&self.ship_class)
    }
}

pub fn ship_class_to_type(ship_class: &str) -> ShipType {
    match ship_class.to_lowercase().as_str() {
        "battleship" => ShipType::Battleship,
        "explorer" => ShipType::Explorer,
        "interceptor" => ShipType::Interceptor,
        "survey" => ShipType::Survey,
        "armada" => ShipType::Armada,
        _ => ShipType::Battleship,
    }
}

pub const DEFAULT_HOSTILES_INDEX_PATH: &str = "data/hostiles/index.json";

/// Load hostile index from data/hostiles/index.json. Returns None if file missing.
pub fn load_hostile_index(path: &str) -> Option<HostileIndex> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Load a single hostile record by id from data/hostiles/<id>.json.
pub fn load_hostile_record(data_dir: &Path, id: &str) -> Option<HostileRecord> {
    let path = data_dir.join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
