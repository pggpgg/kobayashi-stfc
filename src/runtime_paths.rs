//! Locate runtime assets for both source checkouts and extracted release bundles.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const KOBAYASHI_HOME_ENV: &str = "KOBAYASHI_HOME";

static ASSET_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn is_runtime_root(path: &Path) -> bool {
    path.join("data/officers/officers.canonical.json").is_file()
        && path.join("data/ships_extended/index.json").is_file()
        && path.join("data/hostiles/index.json").is_file()
}

fn discover_asset_root() -> Option<PathBuf> {
    let explicit = std::env::var_os(KOBAYASHI_HOME_ENV).map(PathBuf::from);
    let cwd = std::env::current_dir().ok();
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let build_root = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    [explicit, cwd, executable_dir, build_root]
        .into_iter()
        .flatten()
        .find(|candidate| is_runtime_root(candidate))
}

/// Root containing `data/`, `profiles/`, and `frontend/dist/`.
pub fn asset_root() -> &'static Path {
    ASSET_ROOT
        .get_or_init(|| discover_asset_root().unwrap_or_else(|| PathBuf::from(".")))
        .as_path()
}

/// Resolve a repository-relative runtime path against [`asset_root`].
pub fn resolve(relative: impl AsRef<Path>) -> PathBuf {
    asset_root().join(relative)
}

/// Switch to the discovered asset root when launched outside the extracted bundle directory.
///
/// Existing loaders are intentionally CWD-relative. This preserves their behavior while making a
/// release binary runnable through an absolute path, shell shortcut, or file-manager launch.
pub fn activate_asset_root() -> std::io::Result<()> {
    let root = asset_root();
    if is_runtime_root(root) && std::env::current_dir().ok().as_deref() != Some(root) {
        std::env::set_current_dir(root)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_runtime_root;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runtime_root_requires_core_catalogs() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("kobayashi-runtime-root-{nonce}"));
        std::fs::create_dir_all(&dir).expect("create tempdir");
        assert!(!is_runtime_root(&dir));

        for relative in [
            "data/officers/officers.canonical.json",
            "data/ships_extended/index.json",
            "data/hostiles/index.json",
        ] {
            let path = dir.join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
            std::fs::write(path, b"{}").expect("write marker");
        }

        assert!(is_runtime_root(&dir));
        std::fs::remove_dir_all(dir).expect("remove tempdir");
    }
}
