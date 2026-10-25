//! Filesystem replacement helper shared by cumulative JSON savepoints.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_with(path, |file| file.write_all(bytes))
}

fn write_with(path: &Path, writer: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "replacement path has no filename",
        )
    })?;
    let temporary_path = parent.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&temporary_path)?;
    let mut temporary = TemporaryFile {
        path: temporary_path,
        file: Some(file),
    };
    if let Ok(existing) = fs::symlink_metadata(path) {
        if existing.is_file() {
            temporary
                .file
                .as_ref()
                .unwrap()
                .set_permissions(existing.permissions())?;
        }
    }
    let file = temporary.file.as_mut().unwrap();
    writer(file)?;
    file.sync_all()?;
    drop(temporary.file.take());
    // The sibling temporary file is on the same filesystem. Rename replaces
    // the directory entry only after a complete, flushed write.
    fs::rename(&temporary.path, path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

struct TemporaryFile {
    path: PathBuf,
    file: Option<File>,
}
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod isolated_persistence_tests {
    use super::*;
    #[test]
    fn isolated_persistence_atomic_replacement_keeps_old_bytes_after_partial_write_failure() {
        let root = std::env::temp_dir().join(format!(
            "kobayashi-isolated-atomic-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("savepoint.json");
        fs::write(&path, b"original complete data").unwrap();
        let result = write_with(&path, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("injected disk-write failure"))
        });
        assert!(result.is_err());
        let actual = fs::read(&path).unwrap();
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            1,
            "failed temporary writes must be removed"
        );
        assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(actual, b"original complete data");
    }

    #[test]
    fn isolated_persistence_atomic_replacement_installs_complete_bytes_without_temporary_files() {
        let root = std::env::temp_dir().join(format!(
            "kobayashi-isolated-atomic-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("savepoint.json");
        write_atomic(&path, b"first complete data").unwrap();
        write_atomic(&path, b"second complete data").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second complete data");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(&root).unwrap();
    }
}
