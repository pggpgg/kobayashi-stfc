//! Syndicate reputation: level → stat bonuses (from community spreadsheet or manual).
//! Not in STFCcommunity; use e.g. Syndicate Progression spreadsheet export.

use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyndicateLevelEntry {
    pub level: u32,
    pub bonuses: Vec<SyndicateBonusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyndicateBonusEntry {
    pub stat: String,
    pub value: f64,
    #[serde(default)]
    pub operator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyndicateReputationList {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    pub levels: Vec<SyndicateLevelEntry>,
}

pub const DEFAULT_SYNDICATE_REPUTATION_PATH: &str = "data/syndicate_reputation.json";

pub fn load_syndicate_reputation(path: &str) -> Option<SyndicateReputationList> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
