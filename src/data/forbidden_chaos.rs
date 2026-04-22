//! Forbidden tech and Chaos tech: name + stat bonuses (from community spreadsheet or manual).
//! For advanced player profile when implemented.

use std::collections::{HashMap, HashSet};
use std::fs;

use serde::{Deserialize, Serialize};

use crate::data::import;

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

/// `fid` values present in a synced `forbidden_tech.imported.json` payload that have **no** matching
/// catalog row. Merge applies no combat bonuses for those entries.
pub fn forbidden_chaos_unresolved_import_fids(
    catalog: &ForbiddenChaosList,
    imported: &[import::ForbiddenTechEntry],
) -> Vec<i64> {
    let catalog_fids: HashSet<i64> = catalog.items.iter().filter_map(|r| r.fid).collect();
    let mut missing: Vec<i64> = imported
        .iter()
        .map(|e| e.fid)
        .filter(|fid| !catalog_fids.contains(fid))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    missing
}

#[cfg(test)]
mod sync_readiness_tests {
    use std::path::Path;

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

    /// Every catalog row must have `fid` so stfc-mod sync (`forbidden_tech.imported.json`) can merge bonuses.
    #[test]
    fn repo_forbidden_chaos_catalog_items_have_fid_for_sync_match() {
        let Some(list) = load_forbidden_chaos(DEFAULT_FORBIDDEN_CHAOS_PATH) else {
            return;
        };
        let issues: Vec<_> = forbidden_chaos_sync_readiness_issues(&list)
            .into_iter()
            .filter(|s| s.contains("missing fid"))
            .collect();
        assert!(
            issues.is_empty(),
            "catalog rows missing fid (sync cannot match) in {}: {issues:?}\n\
             Fix: set `fid` in data/import/forbidden_chaos_tech.csv or run `cargo run --bin import_forbidden_chaos` \
             with data/upstream/data-stfc-space/summary-forbidden_tech.json + translations-forbidden_tech.json present.",
            DEFAULT_FORBIDDEN_CHAOS_PATH
        );
    }

    #[test]
    fn repo_demo_synced_forbidden_tech_fids_resolve_in_catalog() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let ft_path = Path::new(&manifest_dir).join("profiles/demo/forbidden_tech.imported.json");
        let Some(imported) =
            import::load_imported_forbidden_tech(ft_path.to_str().expect("utf8 path"))
        else {
            panic!(
                "missing or invalid {}",
                ft_path.display()
            );
        };
        let Some(catalog) = load_forbidden_chaos(DEFAULT_FORBIDDEN_CHAOS_PATH) else {
            panic!("missing catalog {}", DEFAULT_FORBIDDEN_CHAOS_PATH);
        };
        let missing = forbidden_chaos_unresolved_import_fids(&catalog, &imported);
        assert!(
            missing.is_empty(),
            "demo profile forbidden_tech fids missing from {} (sync bonuses would not apply): {missing:?}",
            DEFAULT_FORBIDDEN_CHAOS_PATH
        );
    }

    /// Source CSV must carry `fid` on every row so the catalog stays sync-ready without relying on
    /// optional upstream join during `import_forbidden_chaos`.
    #[test]
    fn repo_forbidden_chaos_csv_has_fid_on_every_data_row() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let csv_path = Path::new(&manifest_dir).join("data/import/forbidden_chaos_tech.csv");
        let data = fs::read_to_string(&csv_path).unwrap_or_else(|e| {
            panic!("read {}: {e}", csv_path.display());
        });
        let mut reader = csv::Reader::from_reader(data.as_bytes());
        let mut missing_rows: Vec<String> = Vec::new();
        for (i, result) in reader.records().enumerate() {
            let record = result.unwrap_or_else(|e| panic!("csv record {i}: {e}"));
            if i == 0
                && record
                    .get(0)
                    .map(|s| s.eq_ignore_ascii_case("name"))
                    .unwrap_or(false)
            {
                continue;
            }
            if record.len() < 7 {
                missing_rows.push(format!("row {}: expected >=7 columns, got {}", i + 1, record.len()));
                continue;
            }
            let fid_col = record.get(3).unwrap_or("").trim();
            if fid_col.is_empty() || fid_col.parse::<i64>().is_err() {
                let name = record.get(0).unwrap_or("").to_string();
                missing_rows.push(format!("row {}: name={name:?} bad fid={fid_col:?}", i + 1));
            }
        }
        assert!(
            missing_rows.is_empty(),
            "data/import/forbidden_chaos_tech.csv rows with missing/invalid fid:\n{}",
            missing_rows.join("\n")
        );
    }
}
