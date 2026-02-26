//! Forbidden tech and Chaos tech: name + stat bonuses (from community spreadsheet or manual).
//! For advanced player profile when implemented.

use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForbiddenChaosRecord {
    /// Game ID (fid) from sync payload; when set, used to match imported forbidden tech.
    #[serde(default)]
    pub fid: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub tech_type: String,
    #[serde(default)]
    pub tier: Option<u32>,
    pub bonuses: Vec<BonusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonusEntry {
    pub stat: String,
    pub value: f64,
    #[serde(default)]
    pub operator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForbiddenChaosList {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    pub items: Vec<ForbiddenChaosRecord>,
}

pub const DEFAULT_FORBIDDEN_CHAOS_PATH: &str = "data/forbidden_chaos_tech.json";

pub fn load_forbidden_chaos(path: &str) -> Option<ForbiddenChaosList> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
