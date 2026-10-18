use std::fs;
use std::path::PathBuf;

use kobayashi::server::profile_backup::{export_profiles_zip, import_profiles_zip};
use uuid::Uuid;

struct TempWorkingDir {
    original: PathBuf,
    root: PathBuf,
}

impl TempWorkingDir {
    fn new() -> Self {
        let original = std::env::current_dir().expect("current directory");
        let root =
            std::env::temp_dir().join(format!("kobayashi-profile-backup-tests-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create isolated directory");
        std::env::set_current_dir(&root).expect("enter isolated directory");
        Self { original, root }
    }
}

impl Drop for TempWorkingDir {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
        let _ = fs::remove_dir_all(&self.root);
    }
}

// This crate contains a single test so its temporary working directory is isolated
// from every other test process.
#[test]
fn profiles_zip_round_trip_restores_files_and_preserves_previous_tree() {
    let _dir = TempWorkingDir::new();
    fs::create_dir_all("profiles/demo").unwrap();
    let index = br#"{"profiles":[],"default_id":null}"#;
    let original = br#"{"name":"Demo","research":[1,2,3]}"#;
    fs::write("profiles/index.json", index).unwrap();
    fs::write("profiles/demo/profile.json", original).unwrap();
    fs::write("profiles/demo/binary.dat", [0, 128, 255]).unwrap();

    let archive = export_profiles_zip().expect("export compressed backup");
    fs::write("profiles/demo/profile.json", b"changed after export").unwrap();
    fs::write("profiles/new-file.txt", b"only in previous tree").unwrap();

    import_profiles_zip(&archive).expect("restore compressed backup");

    assert_eq!(fs::read("profiles/index.json").unwrap(), index);
    assert_eq!(fs::read("profiles/demo/profile.json").unwrap(), original);
    assert_eq!(fs::read("profiles/demo/binary.dat").unwrap(), [0, 128, 255]);
    assert!(!std::path::Path::new("profiles/new-file.txt").exists());
    assert_eq!(
        fs::read("profiles.before_restore/demo/profile.json").unwrap(),
        b"changed after export"
    );
    assert_eq!(
        fs::read("profiles.before_restore/new-file.txt").unwrap(),
        b"only in previous tree"
    );
}
