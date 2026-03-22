//! Hostile data: normalized combat stats for hostiles (from STFCcommunity, data.stfc.space, or manual).
//! Used to build DefenderStats + hull + ShipType when resolving by name/id.
//!
//! **Display names:** `normalize_hostiles_stfc_space` sets `hostile_name` to `Hostile {id}` until a
//! `loca_id` → string map (e.g. `translations-hostiles` from data.stfc.space) is wired into that tool.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::combat::{DefenderStats, ShipType};

#[derive(Debug, Clone)]
pub struct Hostile {
    pub id: String,
}

/// Faction reference from upstream hostile detail (`faction` object).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostileFactionRef {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub loca_id: Option<u64>,
}

/// Resource drop range from upstream `resources[]` (min/max may be negative in game data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileResourceDrop {
    pub resource_id: u64,
    pub min: i64,
    pub max: i64,
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
    /// Fraction of incoming damage to shield (rest to hull). Base 0.8 for most; some hostiles/ships (e.g. Sarcophagus) use 0.2.
    #[serde(default)]
    pub shield_mitigation: Option<f64>,
    /// Apex Barrier: true damage mitigation applied after other mitigation.
    #[serde(default)]
    pub apex_barrier: f64,
    /// Isolytic defense: flat reduction to isolytic damage taken.
    #[serde(default)]
    pub isolytic_defense: f64,
    /// Mitigation floor (e.g. 0.16). When absent, engine uses [`crate::combat::MITIGATION_FLOOR`].
    #[serde(default)]
    pub mitigation_floor: Option<f64>,
    /// Mitigation ceiling (e.g. 0.72). When absent, engine uses [`crate::combat::MITIGATION_CEILING`].
    #[serde(default)]
    pub mitigation_ceiling: Option<f64>,
    /// "Mystery" mitigation factor X: formula becomes 1 - (1-X)*(1-A)*(1-S)*(1-D). Used rarely by game for some hostiles.
    #[serde(default)]
    pub mystery_mitigation_factor: Option<f64>,

    // --- data.stfc.space / modern upstream (all optional for STFCcommunity JSON) ---
    /// Display-name localization key from upstream (`loca_id`).
    #[serde(default)]
    pub loca_id: Option<u64>,
    #[serde(default)]
    pub faction: Option<HostileFactionRef>,
    /// Upstream ship category enum (not hull class); distinct from `ship_class` string.
    #[serde(default)]
    pub upstream_ship_type: u32,
    /// Raw `hull_type` from upstream before mapping to `ship_class`.
    #[serde(default)]
    pub hull_type_raw: u32,
    #[serde(default)]
    pub rarity: u32,
    #[serde(default)]
    pub is_scout: bool,
    #[serde(default)]
    pub is_outpost: bool,
    #[serde(default)]
    pub strength: u64,
    #[serde(default)]
    pub systems: Vec<u64>,
    #[serde(default)]
    pub xp_amount: u32,
    #[serde(default)]
    pub warp: u32,
    #[serde(default)]
    pub warp_with_superhighway: u32,

    /// Upstream composite `stats.health`.
    #[serde(default)]
    pub stat_health: f64,
    /// Upstream composite `stats.defense`.
    #[serde(default)]
    pub stat_defense: f64,
    /// Upstream `stats.attack`.
    #[serde(default)]
    pub stat_attack: f64,
    /// Upstream `stats.dpr`.
    #[serde(default)]
    pub dpr: f64,
    /// Upstream `stats.strength` (aggregated); may mirror top-level `strength` loosely as f64.
    #[serde(default)]
    pub stat_strength: f64,

    #[serde(default)]
    pub accuracy: f64,
    #[serde(default)]
    pub armor_piercing: f64,
    #[serde(default)]
    pub shield_piercing: f64,
    #[serde(default)]
    pub crit_chance: f64,
    #[serde(default)]
    pub crit_damage: f64,

    /// Full upstream `components` array (warp, weapons, shield, etc.).
    #[serde(default)]
    pub components: Vec<Value>,
    /// Full upstream `ability` array.
    #[serde(default, rename = "ability")]
    pub ability: Vec<Value>,
    #[serde(default)]
    pub resources: Vec<HostileResourceDrop>,
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
    #[serde(default)]
    pub rarity: Option<u32>,
    #[serde(default)]
    pub upstream_ship_type: Option<u32>,
    #[serde(default)]
    pub loca_id: Option<u64>,
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

/// Maps data.stfc.space `hull_type` to KOBAYASHI `ship_class` string.
pub fn hull_type_raw_to_ship_class(hull_type: u32) -> Option<&'static str> {
    match hull_type {
        0 => Some("battleship"),
        1 => Some("survey"),
        2 => Some("interceptor"),
        3 => Some("explorer"),
        5 => Some("survey"),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_record_deserializes_legacy_minimal_json() {
        let j = r#"{"id":"actian_apex_33_interceptor","hostile_name":"Actian Apex","level":33,"ship_class":"interceptor","armor":1.0,"shield_deflection":2.0,"dodge":3.0,"hull_health":100.0,"shield_health":50.0}"#;
        let r: HostileRecord = serde_json::from_str(j).expect("legacy hostile JSON");
        assert_eq!(r.id, "actian_apex_33_interceptor");
        assert!(r.components.is_empty() && r.ability.is_empty());
        assert_eq!(r.upstream_ship_type, 0);
    }

    #[test]
    fn hostile_record_deserializes_extended_fields() {
        let j = r#"{"id":"2918121098","hostile_name":"Hostile 2918121098","level":81,"ship_class":"explorer","armor":1.0,"shield_deflection":2.0,"dodge":3.0,"hull_health":10.0,"shield_health":5.0,"upstream_ship_type":2,"hull_type_raw":3,"components":[{"k":1}],"ability":[]}"#;
        let r: HostileRecord = serde_json::from_str(j).expect("extended hostile JSON");
        assert_eq!(r.components.len(), 1);
    }

    #[test]
    fn hull_type_raw_mapping_known_values() {
        assert_eq!(hull_type_raw_to_ship_class(0), Some("battleship"));
        assert_eq!(hull_type_raw_to_ship_class(3), Some("explorer"));
        assert_eq!(hull_type_raw_to_ship_class(99), None);
    }
}
