//! Faction reputation: tiers and points (from STFCcommunity or manual).
//! For advanced player profile: faction tier → combat bonuses (when implemented).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Normalized faction reputation (one file per faction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionReputationRecord {
    pub faction: String,
    pub reputation: Vec<ReputationTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationTier {
    pub points_min: i64,
    pub reputation_id: u32,
    pub reputation_name: String,
}

/// Index listing available factions. Includes data_version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionReputationIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub factions: Vec<String>,
}

pub const DEFAULT_FACTION_REPUTATION_INDEX_PATH: &str = "data/faction_reputation/index.json";

pub fn load_faction_reputation_index(path: &str) -> Option<FactionReputationIndex> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn load_faction_reputation_record(data_dir: &Path, faction: &str) -> Option<FactionReputationRecord> {
    let path = data_dir.join(format!("{}.json", faction));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
