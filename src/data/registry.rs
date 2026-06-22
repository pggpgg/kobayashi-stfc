//! Data registry: versioning and source tracking for each dataset.
//! Written by the normalizer and spreadsheet importers; read by the app to show "data as of".

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSetEntry {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    pub path: String,
}

pub type Registry = HashMap<String, DataSetEntry>;

pub const DEFAULT_REGISTRY_PATH: &str = "data/registry.json";

/// Merge-update one dataset row in `data/registry.json`, preserving unrelated entries.
/// Source defaults to `"data-stfc-space"`; use [`merge_registry_entry_with_source`] for others.
pub fn merge_registry_entry(
    repo: &Path,
    key: &str,
    data_version: &str,
    index_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    merge_registry_entry_with_source(repo, key, "data-stfc-space", data_version, index_path)
}

/// Like [`merge_registry_entry`] but with an explicit `source` (e.g. `"community_spreadsheet"`).
pub fn merge_registry_entry_with_source(
    repo: &Path,
    key: &str,
    source: &str,
    data_version: &str,
    index_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let reg_path = repo.join(DEFAULT_REGISTRY_PATH);
    let mut reg: Registry = if reg_path.is_file() {
        let s = fs::read_to_string(&reg_path)?;
        serde_json::from_str(&s).unwrap_or_default()
    } else {
        Registry::default()
    };
    let last_updated = chrono::Utc::now().format("%Y-%m-%d").to_string();
    reg.insert(
        key.to_string(),
        DataSetEntry {
            source: source.to_string(),
            data_version: Some(data_version.to_string()),
            last_updated: Some(last_updated),
            path: index_path.to_string(),
        },
    );
    if let Some(parent) = reg_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(reg_path, serde_json::to_string_pretty(&reg)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn merge_registry_entry_updates_one_key_and_preserves_others() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("registry_merge_test_{nanos}"));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join("data")).unwrap();
        fs::write(
            repo.join(DEFAULT_REGISTRY_PATH),
            r#"{
  "hostiles": {
    "source": "data-stfc-space",
    "data_version": "stfcspace-hostiles-2026-01-01",
    "last_updated": "2026-01-01",
    "path": "hostiles/index.json"
  }
}"#,
        )
        .unwrap();

        merge_registry_entry(
            &repo,
            "ships",
            "stfcspace-ships-2026-06-14",
            "ships_extended/index.json",
        )
        .expect("merge ships");

        let reg: Registry =
            serde_json::from_str(&fs::read_to_string(repo.join(DEFAULT_REGISTRY_PATH)).unwrap())
                .unwrap();
        assert_eq!(
            reg.get("hostiles").unwrap().data_version.as_deref(),
            Some("stfcspace-hostiles-2026-01-01")
        );
        let ships = reg.get("ships").expect("ships row");
        assert_eq!(
            ships.data_version.as_deref(),
            Some("stfcspace-ships-2026-06-14")
        );
        assert_eq!(ships.path, "ships_extended/index.json");
        assert_eq!(ships.last_updated.as_deref().unwrap().len(), 10);

        let _ = fs::remove_dir_all(&repo);
    }
}
