//! Zip export/import for the entire `profiles/` tree (audit: backup/restore UX).

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

use crate::data::profile_index::{
    profile_index_restore_guard, ProfileIndex, PROFILES_DIR, PROFILE_INDEX_PATH,
};

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
        let meta = backup_entry_metadata(&entry.path())?;
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

fn backup_entry_metadata(path: &Path) -> std::io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    let linked = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        linked || metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT, including junctions.
    };
    if linked {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "linked profile entries cannot be exported: {}",
                path.display()
            ),
        ));
    }
    Ok(metadata)
}

/// Build a zip archive containing every file under `profiles/` (paths like `profiles/index.json`).
pub fn export_profiles_zip() -> Result<Vec<u8>, String> {
    let _transaction = crate::data::profile_index::profile_store_transaction()
        .map_err(|error| error.to_string())?;
    let base = Path::new(PROFILES_DIR);
    export_profiles_zip_at(base)
}

fn export_profiles_zip_at(base: &Path) -> Result<Vec<u8>, String> {
    backup_entry_metadata(base).map_err(|error| error.to_string())?;
    if !base.exists() {
        return Err("profiles directory not found".to_string());
    }

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    collect_profile_files(base, Path::new(""), &mut entries).map_err(|e| e.to_string())?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

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
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let _transaction = crate::data::profile_index::profile_store_transaction()
        .map_err(|error| error.to_string())?;
    let mut index = profile_index_restore_guard().map_err(|e| e.to_string())?;
    *index = import_profiles_zip_inner_at(&root, zip_bytes, RestoreLimits::default(), || {})?;
    Ok(())
}

#[derive(Clone, Copy)]
struct RestoreLimits {
    entry_bytes: u64,
    total_bytes: u64,
    entries: usize,
}
impl Default for RestoreLimits {
    fn default() -> Self {
        Self {
            entry_bytes: 256 * 1024 * 1024,
            total_bytes: 1024 * 1024 * 1024,
            entries: 10_000,
        }
    }
}

struct RestoreStaging {
    path: PathBuf,
    parent: PathBuf,
}

impl Drop for RestoreStaging {
    fn drop(&mut self) {
        // This is the absolute, randomly named directory created by this import,
        // never an archive-provided path or the live profiles directory.
        if self.path.is_absolute()
            && self.path.parent() == Some(self.parent.as_path())
            && self
                .path
                .canonicalize()
                .is_ok_and(|path| path.starts_with(&self.parent))
            && self.path.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with(".kobayashi_profiles_restore_staging_")
            })
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
fn import_profiles_zip_at(
    root: &Path,
    zip_bytes: &[u8],
    limits: RestoreLimits,
) -> Result<ProfileIndex, String> {
    import_profiles_zip_with_hook_at(root, zip_bytes, limits, || {})
}

#[cfg(test)]
fn import_profiles_zip_with_hook_at(
    root: &Path,
    zip_bytes: &[u8],
    limits: RestoreLimits,
    after_backup: impl FnOnce(),
) -> Result<ProfileIndex, String> {
    let _transaction = crate::data::profile_index::profile_store_transaction()
        .map_err(|error| error.to_string())?;
    import_profiles_zip_inner_at(root, zip_bytes, limits, after_backup)
}

fn import_profiles_zip_inner_at(
    root: &Path,
    zip_bytes: &[u8],
    limits: RestoreLimits,
    after_backup: impl FnOnce(),
) -> Result<ProfileIndex, String> {
    if zip_bytes.is_empty() {
        return Err("empty zip body".to_string());
    }

    let reader = Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("invalid zip: {e}"))?;
    if archive.len() > limits.entries {
        return Err(format!("backup exceeds the {} entry limit", limits.entries));
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let staging = RestoreStaging {
        path: root.join(format!(
            ".kobayashi_profiles_restore_staging_{}",
            Uuid::new_v4()
        )),
        parent: root.clone(),
    };
    fs::create_dir(&staging.path).map_err(|e| e.to_string())?;
    let staging_root = &staging.path;
    let mut total_bytes = 0u64;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        if file.is_dir() {
            continue;
        }
        let name = file
            .name()
            .map_err(|e| format!("invalid backup entry name: {e}"))?
            .into_owned();
        let rel = normalize_zip_entry_name(&name)
            .ok_or_else(|| format!("unsafe backup entry: {name}"))?;
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("symbolic links are not allowed in backups: {name}"));
        }
        // Bound the index before reading it into memory for JSON validation.
        let entry_limit = if rel == Path::new(PROFILE_INDEX_PATH) {
            limits.entry_bytes.min(4 * 1024 * 1024)
        } else {
            limits.entry_bytes
        };
        let remaining = limits.total_bytes.saturating_sub(total_bytes);
        let allowed = entry_limit.min(remaining);
        if file.size() > allowed {
            return Err(format!("backup expansion limit exceeded by {name}"));
        }
        let out_path = staging_root.join(&rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out_path)
            .map_err(|e| e.to_string())?;
        let copied = std::io::copy(
            &mut (&mut file).take(allowed.saturating_add(1)),
            &mut output,
        )
        .map_err(|e| e.to_string())?;
        if copied > allowed {
            return Err(format!("backup expansion limit exceeded by {name}"));
        }
        total_bytes += copied;
        output.sync_all().map_err(|e| e.to_string())?;
    }

    let index_path = staging_root.join(PROFILE_INDEX_PATH);
    if !index_path.is_file() {
        return Err(
                "backup zip must contain profiles/index.json (export from Kobayashi or use a full profiles backup)"
                    .to_string(),
            );
    }
    let raw = fs::read(&index_path).map_err(|e| e.to_string())?;
    let index: ProfileIndex = parse_index(&raw)?;

    let staged_profiles = staging_root.join(PROFILES_DIR);
    if !staged_profiles.is_dir() {
        return Err("invalid backup: profiles directory missing after extract".to_string());
    }

    let live = root.join(PROFILES_DIR);
    let backup_path = unique_backup_path(&root)?;

    let had_live = live.exists();
    if had_live {
        fs::rename(&live, &backup_path).map_err(|e| e.to_string())?;
    }
    after_backup();

    match fs::rename(&staged_profiles, &live) {
        Ok(()) => Ok(index),
        Err(e) => {
            if had_live {
                let _ = fs::rename(&backup_path, &live);
            }
            Err(format!("failed to install restored profiles: {e}"))
        }
    }
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

#[cfg(test)]
mod isolated_persistence_tests {
    use super::*;
    struct TempRoot(PathBuf);
    impl TempRoot {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("kobayashi-isolated-restore-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
            assert!(self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("kobayashi-isolated-restore-"));
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn zip_fixture(extra: &[(&str, usize)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("profiles/index.json", options).unwrap();
        writer.write_all(b"{\"profiles\":[]}").unwrap();
        for (name, size) in extra {
            writer.start_file(*name, options).unwrap();
            writer.write_all(&vec![b'x'; *size]).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
    fn rejects_without_replacing_live(extra: &[(&str, usize)], limits: RestoreLimits) {
        let root = TempRoot::new();
        fs::create_dir(root.0.join("profiles")).unwrap();
        let sentinel = root.0.join("profiles/sentinel");
        fs::write(&sentinel, b"existing precious data").unwrap();
        assert!(import_profiles_zip_at(&root.0, &zip_fixture(extra), limits).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"existing precious data");
        assert!(!fs::read_dir(&root.0).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("staging")));
    }
    #[test]
    fn isolated_persistence_restore_limits_each_entry() {
        rejects_without_replacing_live(
            &[("profiles/large.json", 65)],
            RestoreLimits {
                entry_bytes: 64,
                total_bytes: 1024,
                entries: 20,
            },
        );
    }
    #[test]
    fn isolated_persistence_restore_limits_total_expansion() {
        rejects_without_replacing_live(
            &[("profiles/a.json", 24), ("profiles/b.json", 24)],
            RestoreLimits {
                entry_bytes: 64,
                total_bytes: 48,
                entries: 20,
            },
        );
    }
    #[test]
    fn isolated_persistence_restore_limits_entry_count() {
        rejects_without_replacing_live(
            &[("profiles/a.json", 1), ("profiles/b.json", 1)],
            RestoreLimits {
                entry_bytes: 64,
                total_bytes: 1024,
                entries: 2,
            },
        );
    }

    #[test]
    fn isolated_persistence_restore_installs_a_valid_archive_and_keeps_the_previous_tree() {
        let root = TempRoot::new();
        fs::create_dir(root.0.join("profiles")).unwrap();
        fs::write(root.0.join("profiles/sentinel"), b"previous data").unwrap();
        let index = import_profiles_zip_at(
            &root.0,
            &zip_fixture(&[("profiles/new/data.json", 3)]),
            RestoreLimits::default(),
        )
        .unwrap();
        assert!(index.profiles.is_empty());
        assert_eq!(
            fs::read(root.0.join("profiles/new/data.json")).unwrap(),
            b"xxx"
        );
        assert_eq!(
            fs::read(root.0.join("profiles.before_restore/sentinel")).unwrap(),
            b"previous data"
        );
        assert!(!fs::read_dir(&root.0).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("staging")));
    }

    #[test]
    fn isolated_persistence_restore_rejects_an_invalid_index_without_replacing_live_data() {
        let root = TempRoot::new();
        fs::create_dir(root.0.join("profiles")).unwrap();
        fs::write(root.0.join("profiles/sentinel"), b"previous data").unwrap();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("profiles/index.json", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"{broken").unwrap();
        let archive = writer.finish().unwrap().into_inner();
        assert!(import_profiles_zip_at(&root.0, &archive, RestoreLimits::default()).is_err());
        assert_eq!(
            fs::read(root.0.join("profiles/sentinel")).unwrap(),
            b"previous data"
        );
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }

    #[test]
    fn isolated_persistence_export_rejects_linked_directories_instead_of_disclosing_external_files()
    {
        let root = TempRoot::new();
        let profiles = root.0.join("profiles");
        let outside = root.0.join("outside");
        fs::create_dir(&profiles).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"not profile backup data").unwrap();
        let link = profiles.join("linked");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    link.to_str().unwrap(),
                    outside.to_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(
            export_profiles_zip_at(&profiles).is_err(),
            "linked files outside profiles must never enter the archive"
        );
        assert_eq!(
            fs::read(outside.join("secret.txt")).unwrap(),
            b"not profile backup data"
        );
    }

    #[test]
    fn isolated_persistence_restore_serializes_inflight_sync_and_rechecks_tokens_after_install() {
        use std::sync::mpsc;
        use std::time::Duration;
        let root = TempRoot::new();
        let profiles = root.0.join("profiles");
        fs::create_dir(&profiles).unwrap();
        let old = serde_json::json!({"profiles":[{"id":"old","name":"Old","sync_token":"old-token"}],"default_id":"old"});
        fs::write(
            profiles.join("index.json"),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("profiles/index.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"{"profiles":[{"id":"new","name":"New","sync_token":"new-token"}],"default_id":"new"}"#).unwrap();
        zip.start_file(
            "profiles/new/research.imported.json",
            SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(br#"{"research":[{"rid":2,"level":5}]}"#)
            .unwrap();
        let archive = zip.finish().unwrap().into_inner();
        let (loaded_tx, loaded_rx) = mpsc::channel();
        let (resume_sync_tx, resume_sync_rx) = mpsc::channel();
        let sync_root = root.0.clone();
        let writer = std::thread::spawn(move || {
            let profiles = sync_root.join("profiles");
            crate::server::sync::ingress_payload_at(
                r#"[{"type":"research","rid":1,"level":3}]"#,
                Some("old-token"),
                &profiles,
                &sync_root.join("sync.log"),
                || {
                    let index =
                        serde_json::from_slice(&fs::read(profiles.join("index.json")).unwrap())
                            .unwrap();
                    loaded_tx.send(()).unwrap();
                    resume_sync_rx.recv().unwrap();
                    index
                },
            )
        });
        loaded_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (moved_tx, moved_rx) = mpsc::channel();
        let (resume_restore_tx, resume_restore_rx) = mpsc::channel();
        let restore_root = root.0.clone();
        let restore = std::thread::spawn(move || {
            import_profiles_zip_with_hook_at(
                &restore_root,
                &archive,
                RestoreLimits::default(),
                || {
                    moved_tx.send(()).unwrap();
                    resume_restore_rx.recv().unwrap();
                },
            )
        });
        let moved_before_sync_finished = moved_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        resume_sync_tx.send(()).unwrap();
        assert_eq!(writer.join().unwrap().0, axum::http::StatusCode::OK);
        if !moved_before_sync_finished {
            moved_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        resume_restore_tx.send(()).unwrap();
        let restored = restore.join().unwrap();
        assert!(
            restored.is_ok(),
            "restore must complete without an inflight sync recreating the live tree: {restored:?}"
        );
        assert!(
            !moved_before_sync_finished,
            "restore must wait for the authorized sync generation to finish"
        );
        let old_research: serde_json::Value = serde_json::from_slice(
            &fs::read(
                root.0
                    .join("profiles.before_restore/old/research.imported.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(old_research["research"][0]["rid"], 1);
        let new_research = fs::read(profiles.join("new/research.imported.json")).unwrap();
        let (status, _) = crate::server::sync::ingress_payload_at(
            r#"[{"type":"research","rid":9,"level":1}]"#,
            Some("old-token"),
            &profiles,
            &root.0.join("sync.log"),
            || serde_json::from_slice(&fs::read(profiles.join("index.json")).unwrap()).unwrap(),
        );
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(
            fs::read(profiles.join("new/research.imported.json")).unwrap(),
            new_research
        );
        assert!(!profiles.join("old").exists());
    }
}
