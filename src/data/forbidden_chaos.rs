//! Forbidden tech and Chaos tech: name + stat bonuses (from community spreadsheet or manual).
//! For advanced player profile when implemented.

use std::collections::HashMap;
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

/// Human-readable issues for catalog maintenance: entries without `fid` cannot match sync payloads;
/// duplicate `fid` values make merge behavior ambiguous.
pub fn forbidden_chaos_sync_readiness_issues(list: &ForbiddenChaosList) -> Vec<String> {
    let mut issues = Vec::new();
    let mut seen_fid: HashMap<i64, usize> = HashMap::new();
    for (i, item) in list.items.iter().enumerate() {
        if item.fid.is_none() {
            issues.push(format!(
                "item[{i}] {:?}: missing fid (sync cannot match this row)",
                item.name
            ));
        }
        if let Some(fid) = item.fid {
            if let Some(prev) = seen_fid.insert(fid, i) {
                issues.push(format!(
                    "duplicate fid {fid}: catalog items at index {prev} and {i}"
                ));
            }
        }
    }
    issues
}

#[cfg(test)]
mod sync_readiness_tests {
    use super::*;

    #[test]
    fn sync_readiness_flags_missing_and_duplicate_fid() {
        let list = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![
                ForbiddenChaosRecord {
                    fid: None,
                    name: "No Fid".into(),
                    tech_type: String::new(),
                    tier: None,
                    bonuses: vec![],
                },
                ForbiddenChaosRecord {
                    fid: Some(100),
                    name: "A".into(),
                    tech_type: String::new(),
                    tier: None,
                    bonuses: vec![],
                },
                ForbiddenChaosRecord {
                    fid: Some(100),
                    name: "B".into(),
                    tech_type: String::new(),
                    tier: None,
                    bonuses: vec![],
                },
            ],
        };
        let issues = forbidden_chaos_sync_readiness_issues(&list);
        assert!(issues.iter().any(|s| s.contains("missing fid")));
        assert!(issues.iter().any(|s| s.contains("duplicate fid 100")));
    }

    #[test]
    fn repo_forbidden_chaos_catalog_has_no_duplicate_fids() {
        let Some(list) = load_forbidden_chaos(DEFAULT_FORBIDDEN_CHAOS_PATH) else {
            return;
        };
        let issues: Vec<_> = forbidden_chaos_sync_readiness_issues(&list)
            .into_iter()
            .filter(|s| s.contains("duplicate fid"))
            .collect();
        assert!(
            issues.is_empty(),
            "duplicate fids in {}: {issues:?}",
            DEFAULT_FORBIDDEN_CHAOS_PATH
        );
    }
}
