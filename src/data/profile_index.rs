//! Profile index: multi-profile support with per-profile paths and sync tokens.
//!
//! Each profile has: id, name, syncToken (UUID). Paths: profiles/{id}/profile.json, roster.imported.json, etc.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

pub const PROFILES_DIR: &str = "profiles";
pub const PROFILE_INDEX_PATH: &str = "profiles/index.json";
pub const DEFAULT_PROFILE_ID: &str = "default";
/// Bundled sample profile directory in the repo (`profiles/demo/`). Not secret; index entries are local.
pub const DEMO_PROFILE_ID: &str = "demo";

/// One profile entry in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    profile_path_at(Path::new(PROFILES_DIR), profile_id, filename)
}

pub(crate) fn profile_path_at(root: &Path, profile_id: &str, filename: &str) -> PathBuf {
    root.join(sanitize_profile_id(profile_id)).join(filename)
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
/// Append-only per-crew optimizer observations when `KOBAYASHI_OPTIMIZE_OBSERVATIONS=1`.
pub const OPTIMIZE_OBSERVATIONS_JSONL: &str = "optimize_observations.jsonl";

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

/// Last valid index plus the mutex for complete index/restore transactions.
static PROFILE_INDEX_CACHE: OnceLock<Mutex<ProfileIndex>> = OnceLock::new();

static PROFILE_STORE_TRANSACTION: Mutex<()> = Mutex::new(());

/// Serialize complete profile-store transactions with tree restore/export.
/// Acquire this BEFORE the index mutex or any per-file/history mutex. Index-only
/// operations never acquire it while holding their index guard.
pub(crate) fn profile_store_transaction() -> std::io::Result<MutexGuard<'static, ()>> {
    PROFILE_STORE_TRANSACTION
        .lock()
        .map_err(|error| std::io::Error::other(format!("profile store lock poisoned: {error}")))
}

fn get_profile_index_cache<'a>(
    root: &Path,
    cache: &'a OnceLock<Mutex<ProfileIndex>>,
) -> &'a Mutex<ProfileIndex> {
    cache.get_or_init(|| Mutex::new(load_profile_index_from_disk_at(root)))
}

fn load_profile_index_from_disk_at(root: &Path) -> ProfileIndex {
    load_profile_index_checked_at(root).unwrap_or_default()
}

fn load_profile_index_checked_at(root: &Path) -> std::io::Result<ProfileIndex> {
    let path = root.join("index.json");
    let raw = match fs::read(&path) {
        Ok(raw) => raw,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && fs::symlink_metadata(&path).is_err() =>
        {
            return Ok(ProfileIndex::default())
        }
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&raw).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid {}: {error}", path.display()),
        )
    })
}

/// Read the current disk index. Retain a last valid read on an I/O/parse error;
/// mutating operations use the checked loader and refuse to overwrite that error.
pub fn load_profile_index() -> ProfileIndex {
    load_profile_index_at(Path::new(PROFILES_DIR), &PROFILE_INDEX_CACHE)
}

fn load_profile_index_at(root: &Path, cache: &OnceLock<Mutex<ProfileIndex>>) -> ProfileIndex {
    let mut current = get_profile_index_cache(root, cache)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    match load_profile_index_checked_at(root) {
        Ok(index) => *current = index,
        Err(error) => {
            tracing::error!(path=%root.display(),%error,"profile index could not be refreshed; writes remain disabled")
        }
    }
    current.clone()
}

pub(crate) fn profile_index_restore_guard() -> std::io::Result<MutexGuard<'static, ProfileIndex>> {
    lock_profile_index_at(Path::new(PROFILES_DIR), &PROFILE_INDEX_CACHE)
}

fn lock_profile_index_at<'a>(
    root: &Path,
    cache: &'a OnceLock<Mutex<ProfileIndex>>,
) -> std::io::Result<MutexGuard<'a, ProfileIndex>> {
    get_profile_index_cache(root, cache)
        .lock()
        .map_err(|error| std::io::Error::other(format!("profile index lock poisoned: {error}")))
}

/// Save the profile index to disk and update the in-memory cache.
pub fn save_profile_index(index: &ProfileIndex) -> std::io::Result<()> {
    save_profile_index_at(Path::new(PROFILES_DIR), &PROFILE_INDEX_CACHE, index)
}

fn save_profile_index_at(
    root: &Path,
    cache: &OnceLock<Mutex<ProfileIndex>>,
    index: &ProfileIndex,
) -> std::io::Result<()> {
    let mut state = lock_profile_index_at(root, cache)?;
    load_profile_index_checked_at(root)?;
    write_profile_index_at(root, index)?;
    *state = index.clone();
    Ok(())
}

fn write_profile_index_at(root: &Path, index: &ProfileIndex) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(index).map_err(std::io::Error::other)?;
    crate::data::atomic_file::write_atomic(&root.join("index.json"), &bytes)
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
    ensure_profile_at(
        Path::new(PROFILES_DIR),
        &PROFILE_INDEX_CACHE,
        index,
        id,
        name,
    )
}

fn ensure_profile_at(
    root: &Path,
    cache: &OnceLock<Mutex<ProfileIndex>>,
    index: &mut ProfileIndex,
    id: &str,
    name: Option<&str>,
) -> std::io::Result<()> {
    if id.is_empty() || sanitize_profile_id(id) != id {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid profile id",
        ));
    }
    let mut state = lock_profile_index_at(root, cache)?;
    *index = load_profile_index_checked_at(root)?;
    if index.profiles.iter().any(|p| p.id == id) {
        *state = index.clone();
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

    // Create profile directory and empty files if needed
    let dir = root.join(sanitize_profile_id(id));
    fs::create_dir_all(&dir)?;
    let profile_json = dir.join(PROFILE_JSON);
    if !profile_json.exists() {
        crate::data::atomic_file::write_atomic(&profile_json, b"{\"bonuses\":{}}")?;
    }
    let presets_dir = dir.join(PRESETS_SUBDIR);
    fs::create_dir_all(presets_dir)?;
    write_profile_index_at(root, index)?;
    *state = index.clone();

    Ok(())
}

/// Create a new profile with auto-generated id if not provided.
pub fn create_profile(
    index: &mut ProfileIndex,
    id: Option<&str>,
    name: &str,
) -> Result<ProfileEntry, String> {
    create_profile_at(
        Path::new(PROFILES_DIR),
        &PROFILE_INDEX_CACHE,
        index,
        id,
        name,
    )
}

fn create_profile_at(
    root: &Path,
    cache: &OnceLock<Mutex<ProfileIndex>>,
    index: &mut ProfileIndex,
    id: Option<&str>,
    name: &str,
) -> Result<ProfileEntry, String> {
    let mut state = lock_profile_index_at(root, cache).map_err(|error| error.to_string())?;
    *index = load_profile_index_checked_at(root).map_err(|error| error.to_string())?;
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
    if root.join(&id).exists() {
        return Err(format!(
            "Profile directory '{id}' already exists; existing data was preserved"
        ));
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

    let dir = root.join(sanitize_profile_id(&id));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let profile_json = dir.join(PROFILE_JSON);
    crate::data::atomic_file::write_atomic(&profile_json, b"{\"bonuses\":{}}")
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join(PRESETS_SUBDIR)).map_err(|e| e.to_string())?;
    write_profile_index_at(root, index).map_err(|e| e.to_string())?;
    *state = index.clone();

    Ok(entry)
}

/// Retained startup hook. A profile name/prefix cannot prove test ownership;
/// test fixtures must clean their own isolated roots instead of deleting user data.
pub fn prune_ephemeral_scenario_test_profiles() -> std::io::Result<()> {
    prune_ephemeral_scenario_test_profiles_at(Path::new(PROFILES_DIR), &PROFILE_INDEX_CACHE)
}

fn prune_ephemeral_scenario_test_profiles_at(
    _profiles_root: &Path,
    _cache: &OnceLock<Mutex<ProfileIndex>>,
) -> std::io::Result<()> {
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
    sync_profile_index_with_disk_at(Path::new(PROFILES_DIR), &PROFILE_INDEX_CACHE)
}

fn sync_profile_index_with_disk_at(
    profiles_root: &Path,
    cache: &OnceLock<Mutex<ProfileIndex>>,
) -> std::io::Result<()> {
    let mut state = lock_profile_index_at(profiles_root, cache)?;
    let mut index = load_profile_index_checked_at(profiles_root)?;
    if !profiles_root.is_dir() {
        return Ok(());
    }
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
        write_profile_index_at(profiles_root, &index)?;
        info!(
            profiles = ?added,
            "registered profile director(ies) that were on disk but missing from profiles/index.json"
        );
    }
    *state = index;
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
    ensure_profile_index_bootstrap_at(Path::new(PROFILES_DIR), &PROFILE_INDEX_CACHE)
}

fn ensure_profile_index_bootstrap_at(
    root: &Path,
    cache: &OnceLock<Mutex<ProfileIndex>>,
) -> std::io::Result<()> {
    let mut state = lock_profile_index_at(root, cache)?;
    let current = load_profile_index_checked_at(root)?;
    if root.join("index.json").exists() {
        *state = current;
        return Ok(());
    }
    let demo_dir = root.join(DEMO_PROFILE_ID);
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
        fs::create_dir_all(demo_dir.join(PRESETS_SUBDIR))?;
        write_profile_index_at(root, &index)?;
        *state = index;
        return Ok(());
    }
    drop(state);
    let mut index = ProfileIndex::default();
    ensure_profile_at(root, cache, &mut index, DEFAULT_PROFILE_ID, Some("Default"))?;
    Ok(())
}

/// Delete a profile and its data directory.
pub fn delete_profile(index: &mut ProfileIndex, id: &str) -> Result<(), String> {
    delete_profile_at(Path::new(PROFILES_DIR), &PROFILE_INDEX_CACHE, index, id)
}

fn delete_profile_at(
    root: &Path,
    cache: &OnceLock<Mutex<ProfileIndex>>,
    index: &mut ProfileIndex,
    id: &str,
) -> Result<(), String> {
    if id.is_empty() || sanitize_profile_id(id) != id {
        return Err("Invalid profile id".to_string());
    }
    let mut state = lock_profile_index_at(root, cache).map_err(|error| error.to_string())?;
    *index = load_profile_index_checked_at(root).map_err(|error| error.to_string())?;
    let pos = index
        .profiles
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("Profile '{}' not found", id))?;

    let dir = root.join(sanitize_profile_id(id));
    if dir.exists() {
        let resolved_root = root.canonicalize().map_err(|e| e.to_string())?;
        let resolved_dir = dir.canonicalize().map_err(|e| e.to_string())?;
        if resolved_dir == resolved_root || !resolved_dir.starts_with(&resolved_root) {
            return Err("Profile directory resolves outside the profiles root".to_string());
        }
    }
    index.profiles.remove(pos);
    if index.default_id.as_deref() == Some(id) {
        index.default_id = index.profiles.first().map(|p| p.id.clone());
    }
    write_profile_index_at(root, index).map_err(|error| error.to_string())?;
    *state = index.clone();

    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod isolated_persistence_tests {
    use super::*;
    struct TempIndex {
        root: PathBuf,
        cache: OnceLock<Mutex<ProfileIndex>>,
    }
    impl TempIndex {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("kobayashi-isolated-index-{}", Uuid::new_v4()));
            fs::create_dir_all(&root).unwrap();
            Self {
                root,
                cache: OnceLock::new(),
            }
        }
    }
    impl Drop for TempIndex {
        fn drop(&mut self) {
            assert!(self.root.is_absolute() && self.root.starts_with(std::env::temp_dir()));
            assert!(self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("kobayashi-isolated-index-"));
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn isolated_persistence_index_observes_a_restored_index_without_restart() {
        let t = TempIndex::new();
        let mut original = ProfileIndex::default();
        create_profile_at(&t.root, &t.cache, &mut original, Some("old"), "Original").unwrap();
        assert_eq!(
            load_profile_index_at(&t.root, &t.cache).profiles[0].id,
            "old"
        );
        let restored = ProfileIndex {
            profiles: vec![ProfileEntry {
                id: "restored".into(),
                name: "Restored".into(),
                sync_token: "retained-token".into(),
                is_default: None,
            }],
            default_id: Some("restored".into()),
        };
        fs::write(
            t.root.join("index.json"),
            serde_json::to_vec(&restored).unwrap(),
        )
        .unwrap();
        let current = load_profile_index_at(&t.root, &t.cache);
        assert_eq!(
            profile_id_by_sync_token(&current, "retained-token").as_deref(),
            Some("restored")
        );
    }
    #[test]
    fn isolated_persistence_index_creates_do_not_overwrite_a_stale_snapshot() {
        let t = TempIndex::new();
        let mut first = load_profile_index_at(&t.root, &t.cache);
        let mut second = first.clone();
        create_profile_at(&t.root, &t.cache, &mut first, Some("first"), "First").unwrap();
        create_profile_at(&t.root, &t.cache, &mut second, Some("second"), "Second").unwrap();
        let disk = load_profile_index_from_disk_at(&t.root);
        assert_eq!(disk.profiles.len(), 2);
    }
    #[test]
    fn isolated_persistence_index_delete_preserves_newer_entries() {
        let t = TempIndex::new();
        let mut first = ProfileIndex::default();
        create_profile_at(&t.root, &t.cache, &mut first, Some("first"), "First").unwrap();
        let mut stale = first.clone();
        create_profile_at(&t.root, &t.cache, &mut first, Some("second"), "Second").unwrap();
        delete_profile_at(&t.root, &t.cache, &mut stale, "first").unwrap();
        assert_eq!(
            load_profile_index_from_disk_at(&t.root)
                .profiles
                .first()
                .map(|p| p.id.as_str()),
            Some("second")
        );
    }
    #[test]
    fn isolated_persistence_index_corruption_is_not_replaced_by_create() {
        let t = TempIndex::new();
        let original = b"{broken";
        fs::write(t.root.join("index.json"), original).unwrap();
        let result = create_profile_at(
            &t.root,
            &t.cache,
            &mut ProfileIndex::default(),
            Some("new"),
            "New",
        );
        assert!(result.is_err());
        assert_eq!(fs::read(t.root.join("index.json")).unwrap(), original);
    }
    #[test]
    fn isolated_persistence_index_corruption_is_not_replaced_by_directory_discovery() {
        let t = TempIndex::new();
        let original = b"{broken";
        fs::write(t.root.join("index.json"), original).unwrap();
        fs::create_dir(t.root.join("copied")).unwrap();
        fs::write(t.root.join("copied/profile.json"), b"{}").unwrap();
        assert!(sync_profile_index_with_disk_at(&t.root, &t.cache).is_err());
        assert_eq!(fs::read(t.root.join("index.json")).unwrap(), original);
    }
    #[test]
    fn isolated_persistence_startup_never_deletes_a_user_named_scenario_research() {
        let t = TempIndex::new();
        let mut index = ProfileIndex::default();
        let entry = create_profile_at(
            &t.root,
            &t.cache,
            &mut index,
            None,
            "Scenario Research Notes",
        )
        .unwrap();
        assert_eq!(entry.id, "scenario_research");
        let sentinel = t.root.join(&entry.id).join("research.imported.json");
        fs::write(&sentinel, b"precious user research").unwrap();
        prune_ephemeral_scenario_test_profiles_at(&t.root, &t.cache).unwrap();
        assert_eq!(fs::read(&sentinel).unwrap(), b"precious user research");
        assert_eq!(load_profile_index_from_disk_at(&t.root).profiles.len(), 1);
    }
    #[test]
    fn isolated_persistence_parallel_creates_retain_every_entry() {
        let t = TempIndex::new();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for i in 0..8 {
                let t = &t;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    create_profile_at(
                        &t.root,
                        &t.cache,
                        &mut ProfileIndex::default(),
                        Some(&format!("p{i}")),
                        "Same name",
                    )
                    .unwrap();
                });
            }
        });
        assert_eq!(load_profile_index_from_disk_at(&t.root).profiles.len(), 8);
    }
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
