//! Profile index: multi-profile support with per-profile paths and sync tokens.
//!
//! Each profile has: id, name, syncToken (UUID). Paths: profiles/{id}/profile.json, roster.imported.json, etc.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

pub const PROFILES_DIR: &str = "profiles";
pub const PROFILE_INDEX_PATH: &str = "profiles/index.json";
pub const DEFAULT_PROFILE_ID: &str = "default";
/// Bundled sample profile directory in the repo (`profiles/demo/`). Not secret; index entries are local.
pub const DEMO_PROFILE_ID: &str = "demo";

/// One profile entry in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileEntry {
    pub id: String,
    pub name: String,
    pub sync_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

/// Profile index: list of profiles and default id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileIndex {
    #[serde(default)]
    pub profiles: Vec<ProfileEntry>,
    #[serde(default)]
    pub default_id: Option<String>,
}

/// Returns the data directory for a profile: profiles/{id}/
pub fn profile_data_dir(id: &str) -> PathBuf {
    Path::new(PROFILES_DIR).join(sanitize_profile_id(id))
}

/// Returns the path for a file within a profile's directory.
pub fn profile_path(profile_id: &str, filename: &str) -> PathBuf {
    profile_data_dir(profile_id).join(filename)
}

/// Profile-specific filenames (relative to profile dir).
pub const PROFILE_JSON: &str = "profile.json";
pub const ROSTER_IMPORTED: &str = "roster.imported.json";
pub const RESEARCH_IMPORTED: &str = "research.imported.json";
pub const BUILDINGS_IMPORTED: &str = "buildings.imported.json";
pub const SHIPS_IMPORTED: &str = "ships.imported.json";
pub const FORBIDDEN_TECH_IMPORTED: &str = "forbidden_tech.imported.json";
pub const BUFFS_IMPORTED: &str = "buffs.imported.json";
pub const BATTLELOGS_IMPORTED: &str = "battlelogs.imported.json";
/// Written when the STFC Community Mod persists data via `POST /api/sync/ingress` (not manual UI import).
pub const LAST_MOD_SYNC_JSON: &str = "last_mod_sync.json";
/// Cross-session optimize winners / warm-start stats (per `optimize_cache_key` from the SPA).
pub const OPTIMIZE_HISTORY_JSON: &str = "optimize_history.json";
/// Per-profile learned officer scores (`optimizer::officer_learning::OfficerPerformanceScores`).
pub const OFFICER_LEARNING_JSON: &str = "officer_learning.json";
/// Optional JSON overrides for scout/coarse budget heuristics (operator-tuned; see `optimizer::budget_hints`).
pub const OPTIMIZER_BUDGET_HINTS_JSON: &str = "optimizer_budget_hints.json";
/// Append-only optimize budget telemetry when `KOBAYASHI_BUDGET_TELEMETRY=1` (see `budget_telemetry` module).
pub const BUDGET_TELEMETRY_JSONL: &str = "budget_telemetry.jsonl";

/// Resolve profile id for optimizer/simulate; uses default when None.
pub fn resolve_profile_id_for_api(profile_id: Option<&str>) -> String {
    let index = load_profile_index();
    profile_id
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| effective_profile_id(&index))
}
pub const PRESETS_SUBDIR: &str = "presets";

/// Sanitize profile id for use in paths (alphanumeric, hyphen, underscore only).
fn sanitize_profile_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Process-wide cache to avoid re-reading profiles/index.json on every request.
static PROFILE_INDEX_CACHE: OnceLock<Mutex<ProfileIndex>> = OnceLock::new();

fn get_profile_index_cache() -> &'static Mutex<ProfileIndex> {
    PROFILE_INDEX_CACHE.get_or_init(|| Mutex::new(load_profile_index_from_disk()))
}

/// Load the profile index from disk. Returns default (empty) if missing or invalid.
fn load_profile_index_from_disk() -> ProfileIndex {
    let path = Path::new(PROFILE_INDEX_PATH);
    if !path.exists() {
        return ProfileIndex::default();
    }
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        _ => return ProfileIndex::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Load the profile index, using the process-wide cache.
pub fn load_profile_index() -> ProfileIndex {
    get_profile_index_cache().lock().unwrap().clone()
}

/// Save the profile index to disk and update the in-memory cache.
pub fn save_profile_index(index: &ProfileIndex) -> std::io::Result<()> {
    if let Some(parent) = Path::new(PROFILE_INDEX_PATH).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        PROFILE_INDEX_PATH,
        serde_json::to_string_pretty(index).unwrap(),
    )?;
    // Update the process-wide cache so subsequent reads don't hit disk.
    if let Some(cache) = PROFILE_INDEX_CACHE.get() {
        *cache.lock().unwrap() = index.clone();
    }
    Ok(())
}

/// Get the effective profile id to use (from index default or fallback).
pub fn effective_profile_id(index: &ProfileIndex) -> String {
    if let Some(id) = index.default_id.as_ref().filter(|id| !id.is_empty()) {
        return id.clone();
    }

    index
        .profiles
        .iter()
        .find_map(|p| (!p.id.is_empty()).then(|| p.id.clone()))
        .unwrap_or_else(|| {
            let demo_json = profile_data_dir(DEMO_PROFILE_ID).join(PROFILE_JSON);
            if demo_json.is_file() {
                DEMO_PROFILE_ID.to_string()
            } else {
                DEFAULT_PROFILE_ID.to_string()
            }
        })
}

/// Look up profile by sync token. Returns Some(profile_id) if found.
pub fn profile_id_by_sync_token(index: &ProfileIndex, token: &str) -> Option<String> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    index
        .profiles
        .iter()
        .find(|p| p.sync_token == token)
        .map(|p| p.id.clone())
}

/// Build a map of sync_token -> profile_id for fast lookup.
pub fn sync_token_to_profile_id(index: &ProfileIndex) -> HashMap<String, String> {
    index
        .profiles
        .iter()
        .map(|p| (p.sync_token.clone(), p.id.clone()))
        .collect()
}

/// Ensure a profile exists in the index and on disk. Creates with new sync token if missing.
pub fn ensure_profile(
    index: &mut ProfileIndex,
    id: &str,
    name: Option<&str>,
) -> std::io::Result<()> {
    if index.profiles.iter().any(|p| p.id == id) {
        return Ok(());
    }
    let name = name.unwrap_or(id);
    let sync_token = Uuid::new_v4().to_string();
    let entry = ProfileEntry {
        id: id.to_string(),
        name: name.to_string(),
        sync_token,
        is_default: None,
    };
    index.profiles.push(entry.clone());

    // Set as default if first profile
    if index.profiles.len() == 1 {
        index.default_id = Some(id.to_string());
    }

    save_profile_index(index)?;

    // Create profile directory and empty files if needed
    let dir = profile_data_dir(id);
    fs::create_dir_all(&dir)?;
    let profile_json = dir.join(PROFILE_JSON);
    if !profile_json.exists() {
        fs::write(profile_json, "{\"bonuses\":{}}")?;
    }
    let presets_dir = dir.join(PRESETS_SUBDIR);
    fs::create_dir_all(presets_dir)?;

    Ok(())
}

/// Create a new profile with auto-generated id if not provided.
pub fn create_profile(
    index: &mut ProfileIndex,
    id: Option<&str>,
    name: &str,
) -> Result<ProfileEntry, String> {
    let id = id.map(|s| s.to_string()).unwrap_or_else(|| {
        let slug = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .take(2)
            .collect::<Vec<_>>()
            .join("_");
        if slug.is_empty() {
            format!("profile_{}", Uuid::new_v4().as_simple())
        } else {
            slug
        }
    });

    let id = sanitize_profile_id(&id);
    if id.is_empty() {
        return Err("Invalid profile id".to_string());
    }
    if index.profiles.iter().any(|p| p.id == id) {
        return Err(format!("Profile '{}' already exists", id));
    }

    let sync_token = Uuid::new_v4().to_string();
    let entry = ProfileEntry {
        id: id.clone(),
        name: name.to_string(),
        sync_token,
        is_default: None,
    };
    index.profiles.push(entry.clone());

    if index.profiles.len() == 1 {
        index.default_id = Some(id.clone());
    }

    save_profile_index(index).map_err(|e| e.to_string())?;

    let dir = profile_data_dir(&id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let profile_json = dir.join(PROFILE_JSON);
    fs::write(profile_json, "{\"bonuses\":{}}").map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join(PRESETS_SUBDIR)).map_err(|e| e.to_string())?;

    Ok(entry)
}

/// Ephemeral profile ids used by research scenario tests under `profiles/`.
///
/// - `tests/scenario_research_integration_tests.rs` creates `scenario_research` (slug from "Scenario Research Test").
/// - `src/optimizer/monte_carlo/scenario.rs` uses `scenario_research_{uuid}`.
///
/// These are not end-user profiles; cleanup may miss them if a test run aborts.
fn is_ephemeral_scenario_test_profile_id(id: &str) -> bool {
    id == "scenario_research" || id.starts_with("scenario_research_")
}

/// Removes ephemeral scenario-test profiles from `profiles/index.json` and deletes matching
/// directories under `profiles/`, including orphans not listed in the index.
pub fn prune_ephemeral_scenario_test_profiles() -> std::io::Result<()> {
    let profiles_root = Path::new(PROFILES_DIR);
    let mut index = load_profile_index();
    let removed_from_index: Vec<String> = index
        .profiles
        .iter()
        .filter(|p| is_ephemeral_scenario_test_profile_id(&p.id))
        .map(|p| p.id.clone())
        .collect();

    if !removed_from_index.is_empty() {
        index
            .profiles
            .retain(|p| !is_ephemeral_scenario_test_profile_id(&p.id));
        if index
            .default_id
            .as_deref()
            .is_some_and(|d| removed_from_index.iter().any(|r| r == d))
        {
            index.default_id = index.profiles.first().map(|p| p.id.clone());
        }
        save_profile_index(&index)?;
        info!(
            ?removed_from_index,
            "removed ephemeral scenario research test profile(s) from profiles/index.json"
        );
    }

    if profiles_root.is_dir() {
        for entry in fs::read_dir(profiles_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            if is_ephemeral_scenario_test_profile_id(&id) {
                let path = entry.path();
                if let Err(e) = fs::remove_dir_all(&path) {
                    tracing::warn!(path = %path.display(), error = %e, "failed to remove ephemeral scenario test profile directory");
                } else {
                    info!(%id, "removed ephemeral scenario research test profile directory");
                }
            }
        }
    }

    Ok(())
}

/// Human-readable display name from a directory id (`higgs_bozo` → `Higgs Bozo`).
fn pretty_profile_name_from_id(id: &str) -> String {
    let mut out = String::new();
    for part in id.split('_').filter(|p| !p.is_empty()) {
        let mut ch = part.chars();
        if let Some(c) = ch.next() {
            let rest: String = ch.collect();
            out.extend(c.to_uppercase());
            out.push_str(&rest.to_lowercase());
        }
        out.push(' ');
    }
    let s = out.trim_end().to_string();
    if s.is_empty() {
        id.to_string()
    } else {
        s
    }
}

/// Registers `profiles/<id>/` directories that contain `profile.json` but are not listed in
/// `profiles/index.json`, then saves the index.
///
/// This covers copied profile trees, gitignored folders, or an index reset that still leaves data
/// on disk. Each newly listed profile gets a **new** sync token; point the STFC Community Mod at
/// that token if you use sync for that profile.
pub fn sync_profile_index_with_disk() -> std::io::Result<()> {
    let profiles_root = Path::new(PROFILES_DIR);
    if !profiles_root.is_dir() {
        return Ok(());
    }
    let mut index = load_profile_index();
    let indexed: HashSet<String> = index.profiles.iter().map(|p| p.id.clone()).collect();
    let mut added: Vec<String> = Vec::new();

    for entry in fs::read_dir(profiles_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.starts_with('.') {
            continue;
        }
        let sanitized = sanitize_profile_id(&id);
        if sanitized != id {
            continue;
        }
        if indexed.contains(&id) {
            continue;
        }
        if is_ephemeral_scenario_test_profile_id(&id) {
            continue;
        }
        let profile_json = entry.path().join(PROFILE_JSON);
        if !profile_json.is_file() {
            continue;
        }
        let sync_token = Uuid::new_v4().to_string();
        let name = pretty_profile_name_from_id(&id);
        index.profiles.push(ProfileEntry {
            id: id.clone(),
            name,
            sync_token,
            is_default: None,
        });
        added.push(id);
    }

    if !added.is_empty() {
        save_profile_index(&index)?;
        info!(
            profiles = ?added,
            "registered profile director(ies) that were on disk but missing from profiles/index.json"
        );
    }
    Ok(())
}

/// Create `profiles/index.json` when missing (first run or fresh clone).
///
/// Idempotent: no-op if the index file already exists.
///
/// If the shipped [`DEMO_PROFILE_ID`] tree exists, registers it with a **fresh** sync token
/// (so repo contents are not treated as a shared secret).
/// Otherwise creates a single [`DEFAULT_PROFILE_ID`] profile via [`ensure_profile`].
pub fn ensure_profile_index_bootstrap() -> std::io::Result<()> {
    if Path::new(PROFILE_INDEX_PATH).exists() {
        return Ok(());
    }
    let demo_dir = profile_data_dir(DEMO_PROFILE_ID);
    let demo_profile_json = demo_dir.join(PROFILE_JSON);
    if demo_dir.is_dir() && demo_profile_json.is_file() {
        let mut index = ProfileIndex::default();
        let sync_token = Uuid::new_v4().to_string();
        index.profiles.push(ProfileEntry {
            id: DEMO_PROFILE_ID.to_string(),
            name: "Demo".to_string(),
            sync_token,
            is_default: None,
        });
        index.default_id = Some(DEMO_PROFILE_ID.to_string());
        save_profile_index(&index)?;
        fs::create_dir_all(demo_dir.join(PRESETS_SUBDIR))?;
        return Ok(());
    }
    let mut index = ProfileIndex::default();
    ensure_profile(&mut index, DEFAULT_PROFILE_ID, Some("Default"))?;
    Ok(())
}

/// Delete a profile and its data directory.
pub fn delete_profile(index: &mut ProfileIndex, id: &str) -> Result<(), String> {
    let pos = index
        .profiles
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("Profile '{}' not found", id))?;

    index.profiles.remove(pos);
    if index.default_id.as_deref() == Some(id) {
        index.default_id = index.profiles.first().map(|p| p.id.clone());
    }
    save_profile_index(index).map_err(|e| e.to_string())?;

    let dir = profile_data_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_profile_uses_explicit_default() {
        let index = ProfileIndex {
            profiles: vec![ProfileEntry {
                id: "demo".to_string(),
                name: "Demo".to_string(),
                sync_token: "token".to_string(),
                is_default: None,
            }],
            default_id: Some("higgsbozo".to_string()),
        };

        assert_eq!(effective_profile_id(&index), "higgsbozo");
    }

    #[test]
    fn effective_profile_falls_back_to_first_indexed_profile() {
        let index = ProfileIndex {
            profiles: vec![ProfileEntry {
                id: "higgsbozo".to_string(),
                name: "HiggsBozo".to_string(),
                sync_token: "token".to_string(),
                is_default: None,
            }],
            default_id: None,
        };

        assert_eq!(effective_profile_id(&index), "higgsbozo");
    }

    #[test]
    fn effective_profile_falls_back_to_demo_when_index_empty_and_demo_on_disk() {
        let demo_json = profile_data_dir(DEMO_PROFILE_ID).join(PROFILE_JSON);
        assert!(
            demo_json.is_file(),
            "repo ships profiles/demo/profile.json; missing demo breaks empty-index fallback"
        );
        assert_eq!(
            effective_profile_id(&ProfileIndex::default()),
            DEMO_PROFILE_ID
        );
    }
}
