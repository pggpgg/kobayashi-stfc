//! Ship data: normalized combat stats for player ships (from STFCcommunity or manual).
//! Used to build AttackerStats + Combatant when resolving by name/id.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::{AttackerStats, ShipType};

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: String,
}

/// Normalized ship record (KOBAYASHI schema). Written by normalizer, loaded at runtime.
/// Stats are for a chosen tier/level (e.g. tier 1, level 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipRecord {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
    /// Aggregated armor piercing (from weapon components).
    pub armor_piercing: f64,
    /// Aggregated shield piercing (from weapon components).
    pub shield_piercing: f64,
    /// Aggregated accuracy (from weapon components).
    pub accuracy: f64,
    /// Representative attack/damage (e.g. damage_per_round from primary weapon).
    pub attack: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub hull_health: f64,
    pub shield_health: f64,
    /// Apex Shred: reduces defender's effective Apex Barrier. Stored as decimal (1.0 = 100%).
    #[serde(default)]
    pub apex_shred: f64,
}

/// Index of all ships for name resolution. Includes data_version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub ships: Vec<ShipIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipIndexEntry {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
}

impl ShipRecord {
    pub fn to_attacker_stats(&self) -> AttackerStats {
        AttackerStats {
            armor_piercing: self.armor_piercing,
            shield_piercing: self.shield_piercing,
            accuracy: self.accuracy,
        }
    }

    pub fn ship_type(&self) -> ShipType {
        crate::data::hostile::ship_class_to_type(&self.ship_class)
    }
}

pub const DEFAULT_SHIPS_INDEX_PATH: &str = "data/ships/index.json";

/// Load ship index from data/ships/index.json. Returns None if file missing.
pub fn load_ship_index(path: &str) -> Option<ShipIndex> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Load a single ship record by id from data/ships/<id>.json.
pub fn load_ship_record(data_dir: &Path, id: &str) -> Option<ShipRecord> {
    let path = data_dir.join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
