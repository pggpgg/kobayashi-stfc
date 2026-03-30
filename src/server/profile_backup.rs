//! Zip export/import for the entire `profiles/` tree (audit: backup/restore UX).

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zip::write::FileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

use crate::data::profile_index::{ProfileIndex, PROFILES_DIR, PROFILE_INDEX_PATH};

/// Returns a relative path under the project root only if `raw` is a safe `profiles/...` path.
pub(crate) fn normalize_zip_entry_name(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim().replace('\\', "/");
    if raw.is_empty() || raw.contains("..") {
        return None;
    }
    if raw.ends_with('/') {
        return None;
    }
    let p = Path::new(&raw);
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::Normal(part) => out.push(part),
            _ => return None,
        }
    }
    let mut comps = out.components();
    let first = comps.next()?;
    let first = first.as_os_str().to_str()?;
    if first != PROFILES_DIR {
        return None;
    }
    Some(out)
}

fn collect_profile_files(
    base: &Path,
    rel: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(base.join(rel))? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let file_name = entry.file_name();
        let rel_path = rel.join(&file_name);
        if meta.is_dir() {
            collect_profile_files(base, &rel_path, out)?;
        } else if meta.is_file() {
            let path = base.join(&rel_path);
            let data = fs::read(&path)?;
            let zip_name = Path::new(PROFILES_DIR).join(&rel_path);
            let name = zip_name.to_string_lossy().replace('\\', "/");
            out.push((name, data));
        }
    }
    Ok(())
}

/// Build a zip archive containing every file under `profiles/` (paths like `profiles/index.json`).
pub fn export_profiles_zip() -> Result<Vec<u8>, String> {
    let base = Path::new(PROFILES_DIR);
    if !base.exists() {
        return Err("profiles directory not found".to_string());
    }

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    collect_profile_files(base, Path::new(""), &mut entries).map_err(|e| e.to_string())?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = FileOptions::default().compression_method(CompressionMethod::Deflated);

    for (name, data) in entries {
        zip.start_file(&name, opts).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }

    zip.finish()
        .map_err(|e| e.to_string())
        .map(|c| c.into_inner())
}

fn parse_index(bytes: &[u8]) -> Result<ProfileIndex, String> {
    serde_json::from_slice(bytes)
        .map_err(|e| format!("invalid profiles/index.json in archive: {e}"))
}

fn unique_backup_path(cwd: &Path) -> Result<PathBuf, String> {
    for i in 0..100u32 {
        let name = if i == 0 {
            "profiles.before_restore".to_string()
        } else {
            format!("profiles.before_restore.{i}")
        };
        let p = cwd.join(&name);
        if !p.exists() {
            return Ok(p);
        }
    }
    Err("could not allocate a backup directory name".to_string())
}

/// Replace the current `profiles/` tree with the contents of a zip produced by [`export_profiles_zip`].
/// The archive must contain `profiles/index.json` and only entries under `profiles/`.
/// On success, the previous `profiles` directory is renamed to `profiles.before_restore` (or suffixed) when it existed.
pub fn import_profiles_zip(zip_bytes: &[u8]) -> Result<(), String> {
    if zip_bytes.is_empty() {
        return Err("empty zip body".to_string());
    }

    let reader = Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("invalid zip: {e}"))?;

    let staging_root =
        std::env::temp_dir().join(format!("kobayashi_profiles_restore_{}", Uuid::new_v4()));
    fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;

    let result = (|| -> Result<(), String> {
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_owned();
            let Some(rel) = normalize_zip_entry_name(&name) else {
                continue;
            };
            let out_path = staging_root.join(&rel);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            fs::write(&out_path, buf).map_err(|e| e.to_string())?;
        }

        let index_path = staging_root.join(PROFILE_INDEX_PATH);
        if !index_path.is_file() {
            return Err(
                "backup zip must contain profiles/index.json (export from Kobayashi or use a full profiles backup)"
                    .to_string(),
            );
        }
        let raw = fs::read(&index_path).map_err(|e| e.to_string())?;
        let _index: ProfileIndex = parse_index(&raw)?;

        let staged_profiles = staging_root.join(PROFILES_DIR);
        if !staged_profiles.is_dir() {
            return Err("invalid backup: profiles directory missing after extract".to_string());
        }

        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let live = cwd.join(PROFILES_DIR);
        let backup_path = unique_backup_path(&cwd)?;

        let had_live = live.exists();
        if had_live {
            fs::rename(&live, &backup_path).map_err(|e| e.to_string())?;
        }

        match fs::rename(&staged_profiles, &live) {
            Ok(()) => Ok(()),
            Err(e) => {
                if had_live {
                    let _ = fs::rename(&backup_path, &live);
                }
                Err(format!("failed to install restored profiles: {e}"))
            }
        }
    })();

    let _ = fs::remove_dir_all(&staging_root);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_accepts_profiles_only() {
        assert!(normalize_zip_entry_name("profiles/index.json").is_some());
        assert!(normalize_zip_entry_name("profiles/demo/profile.json").is_some());
        assert!(normalize_zip_entry_name("profiles\\demo\\x.json").is_some());
    }

    #[test]
    fn normalize_rejects_traversal_and_other_roots() {
        assert!(normalize_zip_entry_name("../profiles/x").is_none());
        assert!(normalize_zip_entry_name("profiles/../x").is_none());
        assert!(normalize_zip_entry_name("data/x").is_none());
        assert!(normalize_zip_entry_name("profiles/").is_none());
    }
}
