//! Profile index: multi-profile support with per-profile paths and sync tokens.
//!
//! Each profile has: id, name, syncToken (UUID). Paths: profiles/{id}/profile.json, roster.imported.json, etc.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROFILES_DIR: &str = "profiles";
pub const PROFILE_INDEX_PATH: &str = "profiles/index.json";
pub const DEFAULT_PROFILE_ID: &str = "default";

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

/// Load the profile index from disk. Returns default (empty) if missing or invalid.
pub fn load_profile_index() -> ProfileIndex {
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

/// Save the profile index to disk.
pub fn save_profile_index(index: &ProfileIndex) -> std::io::Result<()> {
    if let Some(parent) = Path::new(PROFILE_INDEX_PATH).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(PROFILE_INDEX_PATH, serde_json::to_string_pretty(index).unwrap())
}

/// Get the effective profile id to use (from index default or fallback).
pub fn effective_profile_id(index: &ProfileIndex) -> String {
    index
        .default_id
        .clone()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string())
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
pub fn ensure_profile(index: &mut ProfileIndex, id: &str, name: Option<&str>) -> std::io::Result<()> {
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
                } else if c.is_whitespace() {
                    '_'
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

/// Migrate from legacy single-profile layout (data/profile.json, rosters/*) to profiles/default/.
/// Call once at startup; idempotent (skips if profiles/index.json already exists).
pub fn migrate_from_legacy_if_needed() -> std::io::Result<()> {
    if Path::new(PROFILE_INDEX_PATH).exists() {
        return Ok(());
    }
    let legacy_profile = Path::new("data/profile.json");
    let legacy_rosters = Path::new("rosters");
    if !legacy_profile.exists() && !legacy_rosters.exists() {
        // Nothing to migrate; create default profile
        let mut index = ProfileIndex::default();
        ensure_profile(&mut index, DEFAULT_PROFILE_ID, Some("Default"))?;
        return Ok(());
    }

    let dir = profile_data_dir(DEFAULT_PROFILE_ID);
    fs::create_dir_all(&dir)?;

    if legacy_profile.exists() {
        let dest = dir.join(PROFILE_JSON);
        fs::copy(legacy_profile, dest)?;
    } else {
        fs::write(dir.join(PROFILE_JSON), "{\"bonuses\":{}}")?;
    }

    for (src_name, dest_name) in [
        ("roster.imported.json", ROSTER_IMPORTED),
        ("research.imported.json", RESEARCH_IMPORTED),
        ("buildings.imported.json", BUILDINGS_IMPORTED),
        ("ships.imported.json", SHIPS_IMPORTED),
        ("forbidden_tech.imported.json", FORBIDDEN_TECH_IMPORTED),
    ] {
        let src = legacy_rosters.join(src_name);
        if src.exists() {
            fs::copy(&src, dir.join(dest_name))?;
        }
    }

    let sync_token = Uuid::new_v4().to_string();
    let index = ProfileIndex {
        profiles: vec![ProfileEntry {
            id: DEFAULT_PROFILE_ID.to_string(),
            name: "Default".to_string(),
            sync_token,
            is_default: Some(true),
        }],
        default_id: Some(DEFAULT_PROFILE_ID.to_string()),
    };
    save_profile_index(&index)?;
    fs::create_dir_all(dir.join(PRESETS_SUBDIR))?;

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
