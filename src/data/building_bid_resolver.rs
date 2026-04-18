//! Resolve game building id (`bid`) to KOBAYASHI building `id` using
//! `translations-starbase_modules.json` plus [`BuildingIndex`](crate::data::building::BuildingIndex):
//! optional explicit [`BuildingIndexEntry::bid`](crate::data::building::BuildingIndexEntry::bid),
//! ids shaped `building_{n}`, and `{n}_*` [`BuildingIndexEntry::file`](crate::data::building::BuildingIndexEntry::file) stems.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::data::building::{BuildingIndex, BuildingIndexEntry};

const STARBASE_MODULE_NAME_KEY: &str = "starbase_module_name";

/// Default path for starbase module name translations (bid → display name).
pub const DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH: &str =
    "data/upstream/data-stfc-space/translations-starbase_modules.json";

#[derive(Debug, Deserialize)]
struct TranslationEntry {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

/// Load bid → building id map from translations file and building index.
/// Only returns entries where we found a matching building id in the index.
/// Returns None if translations or index cannot be loaded.
pub fn load_bid_to_building_id(
    translations_path: &str,
    building_index: &BuildingIndex,
) -> Option<HashMap<i64, String>> {
    let raw = fs::read_to_string(Path::new(translations_path)).ok()?;
    build_bid_to_building_id_from_json(&raw, building_index)
}

/// Build bid → building id map from translations JSON string. Used by [load_bid_to_building_id] and tests.
pub fn build_bid_to_building_id_from_json(
    raw: &str,
    building_index: &BuildingIndex,
) -> Option<HashMap<i64, String>> {
    let entries: Vec<TranslationEntry> = serde_json::from_str(raw).ok()?;

    let id_in_index: HashMap<String, String> = building_index
        .buildings
        .iter()
        .map(|e| (e.id.clone(), e.building_name.clone()))
        .collect();
    let name_to_id: HashMap<String, String> = building_index
        .buildings
        .iter()
        .map(|e| (normalize_name(&e.building_name), e.id.clone()))
        .collect();

    let mut out: HashMap<i64, String> = HashMap::new();
    for entry in entries {
        let Some(bid) = entry.id else {
            continue;
        };
        let Some(key) = entry.key.as_deref() else {
            continue;
        };
        if key != STARBASE_MODULE_NAME_KEY {
            continue;
        }
        let text = entry.text.as_deref().unwrap_or("").trim();
        if text.is_empty() {
            continue;
        }

        let resolved = resolve_one_bid(bid, text, &name_to_id, &id_in_index);
        if let Some(id) = resolved {
            out.insert(bid, id);
        }
    }

    // Supplement from index: explicit `bid`, id `building_{n}`, and/or `{bid}_*` file stem
    // (stfc.space exports use numeric-prefixed file stems even when `id` is a stable slug).
    for entry in &building_index.buildings {
        for bid in index_entry_candidate_bids(entry) {
            out.entry(bid).or_insert_with(|| entry.id.clone());
        }
    }

    Some(out)
}

/// All plausible upstream `bid` values for this index row (deduplicated).
fn index_entry_candidate_bids(entry: &BuildingIndexEntry) -> Vec<i64> {
    let mut v: Vec<i64> = Vec::new();
    if let Some(b) = entry.bid {
        v.push(b);
    }
    if let Some(b) = parse_building_id_as_bid(&entry.id) {
        if !v.contains(&b) {
            v.push(b);
        }
    }
    if let Some(b) = parse_bid_from_file_stem(entry.file.as_deref()) {
        if !v.contains(&b) {
            v.push(b);
        }
    }
    v
}

/// If `file` stem is `{digits}_…` (e.g. `50_parsteel_generator_d`), returns those digits as `bid`.
fn parse_bid_from_file_stem(file: Option<&str>) -> Option<i64> {
    let f = file?.trim();
    let (head, tail) = f.split_once('_')?;
    if head.is_empty() || !head.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if tail.is_empty() {
        return None;
    }
    head.parse().ok()
}

/// If id is "building_<number>", returns Some(bid); otherwise None.
fn parse_building_id_as_bid(id: &str) -> Option<i64> {
    let prefix = "building_";
    id.starts_with(prefix)
        .then(|| id[prefix.len()..].parse::<i64>().ok())?
}

fn normalize_name(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Resolve a single bid to our building id: name match, then "Operations" → ops_center, then building_{bid}.
fn resolve_one_bid(
    bid: i64,
    translation_text: &str,
    name_to_id: &HashMap<String, String>,
    id_in_index: &HashMap<String, String>,
) -> Option<String> {
    let normalized = normalize_name(translation_text);

    // Direct name match (case-insensitive).
    if let Some(id) = name_to_id.get(&normalized) {
        return Some(id.clone());
    }

    // Special case: "Operations" → building with name "OPERATIONS CENTER" (id ops_center).
    if normalized == "operations" {
        return id_in_index.iter().find_map(|(id, name)| {
            if normalize_name(name) == "operations center" {
                Some(id.clone())
            } else {
                None
            }
        });
    }

    // Fallback: building_{bid} if that id exists in the index.
    let fallback_id = format!("building_{}", bid);
    if id_in_index.contains_key(&fallback_id) {
        return Some(fallback_id);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::building::{BuildingIndex, BuildingIndexEntry};

    fn minimal_index() -> BuildingIndex {
        BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![
                BuildingIndexEntry {
                    id: "ops_center".to_string(),
                    building_name: "OPERATIONS CENTER".to_string(),
                    file: None,
                    bid: None,
                },
                BuildingIndexEntry {
                    id: "parsteel_generator_a".to_string(),
                    building_name: "Parsteel Generator A".to_string(),
                    file: None,
                    bid: None,
                },
                BuildingIndexEntry {
                    id: "building_50".to_string(),
                    building_name: "BUILDING 50".to_string(),
                    file: None,
                    bid: None,
                },
            ],
        }
    }

    #[test]
    fn resolve_operations_and_parsteel_and_building_n() {
        let translations = r##"[
            {"id": 0, "key": "starbase_module_name", "text": "Operations"},
            {"id": 1, "key": "starbase_module_name", "text": "Parsteel Generator A"},
            {"id": 50, "key": "starbase_module_name", "text": "Parsteel Generator D"}
        ]"##;
        let dir = std::env::temp_dir().join("kobayashi_bid_resolver_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("translations.json");
        std::fs::write(&path, translations).unwrap();
        let path_str = path.to_string_lossy();

        let index = minimal_index();
        let map = load_bid_to_building_id(&path_str, &index).unwrap();
        assert_eq!(map.get(&0), Some(&"ops_center".to_string()));
        assert_eq!(map.get(&1), Some(&"parsteel_generator_a".to_string()));
        // 50: no "Parsteel Generator D" in index, fallback to building_50
        assert_eq!(map.get(&50), Some(&"building_50".to_string()));
    }

    #[test]
    fn skip_non_starbase_module_name_and_null_id() {
        // Only entries with key "starbase_module_name" and numeric id are used; null id and other_key are skipped.
        // Index pass still adds building_50 (id "building_50" in index).
        let translations = r##"[
            {"id": null, "key": "starbase_module_name", "text": "Ignore"},
            {"id": 1, "key": "other_key", "text": "Parsteel Generator A"}
        ]"##;
        let index = minimal_index();
        let map = build_bid_to_building_id_from_json(translations, &index)
            .expect("build_bid_to_building_id_from_json should succeed with valid JSON");
        assert!(!map.contains_key(&0));
        assert!(!map.contains_key(&1));
        assert_eq!(map.get(&50), Some(&"building_50".to_string()));
    }

    #[test]
    fn index_building_n_resolved_without_translation() {
        // New-buildings strategy: index entries with id building_{bid} are included even without translations.
        let translations = r##"[]"##;
        let index = minimal_index();
        let map = build_bid_to_building_id_from_json(translations, &index).unwrap();
        assert_eq!(map.get(&50), Some(&"building_50".to_string()));
        assert!(!map.contains_key(&0));
        assert!(!map.contains_key(&1));
    }

    #[test]
    fn bid_from_file_stem_when_id_is_not_building_n() {
        let index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "custom_parsteel".to_string(),
                building_name: "Parsteel Generator A".to_string(),
                file: Some("1_parsteel_generator_a".to_string()),
                bid: None,
            }],
        };
        let map = build_bid_to_building_id_from_json("[]", &index).unwrap();
        assert_eq!(map.get(&1), Some(&"custom_parsteel".to_string()));
    }

    #[test]
    fn explicit_index_bid_is_used() {
        let index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "legacy_ops".to_string(),
                building_name: "Operations".to_string(),
                file: None,
                bid: Some(0),
            }],
        };
        let map = build_bid_to_building_id_from_json("[]", &index).unwrap();
        assert_eq!(map.get(&0), Some(&"legacy_ops".to_string()));
    }
}
