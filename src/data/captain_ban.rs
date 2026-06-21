//! Optimizer captain ban list: canonical officer ids excluded from captain enumeration.
//!
//! Loaded from [`DEFAULT_CAPTAIN_BAN_LIST_PATH`]. Bridge and below-decks pools are unaffected.

use std::collections::HashSet;
use std::fs;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::data::officer::{
    load_canonical_officers, normalize_officer_lookup_key, DEFAULT_CANONICAL_OFFICERS_PATH,
};

pub const DEFAULT_CAPTAIN_BAN_LIST_PATH: &str = "data/optimizer/captain_ban_list.json";

#[derive(Debug, Default, Deserialize)]
struct CaptainBanListFile {
    #[serde(default)]
    canonical_ids: Vec<String>,
    #[serde(default)]
    names: Vec<String>,
}

static BANNED_CAPTAIN_IDS: OnceLock<HashSet<String>> = OnceLock::new();

fn resolve_name_to_canonical_id(name: &str, by_lookup: &HashMapLite) -> Option<String> {
    let key = normalize_officer_lookup_key(name);
    if key.is_empty() {
        return None;
    }
    by_lookup.get(&key).cloned()
}

/// Minimal name → canonical id map built from the canonical officer catalog.
struct HashMapLite {
    by_key: std::collections::HashMap<String, String>,
}

impl HashMapLite {
    fn from_officers(officers: &[crate::data::officer::Officer]) -> Self {
        let mut by_key = std::collections::HashMap::new();
        for o in officers {
            let key = normalize_officer_lookup_key(&o.name);
            if !key.is_empty() {
                by_key.entry(key).or_insert_with(|| o.id.clone());
            }
        }
        Self { by_key }
    }

    fn get(&self, key: &str) -> Option<&String> {
        self.by_key.get(key)
    }
}

fn load_banned_captain_ids() -> HashSet<String> {
    let raw = fs::read_to_string(DEFAULT_CAPTAIN_BAN_LIST_PATH).unwrap_or_default();
    let file: CaptainBanListFile = serde_json::from_str(&raw).unwrap_or_default();

    let mut ids: HashSet<String> = file
        .canonical_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();

    if !file.names.is_empty() {
        let officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH).unwrap_or_default();
        let by_lookup = HashMapLite::from_officers(&officers);
        for name in file.names {
            if let Some(id) = resolve_name_to_canonical_id(&name, &by_lookup) {
                ids.insert(id);
            }
        }
    }

    ids
}

fn banned_captain_ids() -> &'static HashSet<String> {
    BANNED_CAPTAIN_IDS.get_or_init(load_banned_captain_ids)
}

/// True when `officer_id` is on the optimizer captain ban list.
pub fn is_captain_banned(officer_id: &str) -> bool {
    banned_captain_ids().contains(officer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quark_is_banned_by_canonical_id() {
        assert!(is_captain_banned("quark-2fd57b"));
    }

    #[test]
    fn airiam_is_banned_by_canonical_id() {
        assert!(is_captain_banned("airiam-9265fc"));
    }
}
