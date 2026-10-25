use kobayashi::data::import::{import_roster_csv_to, import_spocks_export_to};
use kobayashi::server::api::officers_import_payload;
use serde_json::Value;
use std::fs;

struct TestDirectory(std::path::PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kobayashi_roster_import_safety_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn empty_csv_rejects_without_replacing_existing_roster() {
    let dir = TestDirectory::new();
    let source = dir.0.join("empty.csv");
    let output = dir.0.join("roster.imported.json");
    let existing = r#"{"officers":[{"canonical_officer_id":"odo-04a97d","canonical_name":"Odo","rank":5,"tier":5,"level":50}]}"#;
    fs::write(&source, "name,tier,level\n\n").unwrap();
    fs::write(&output, existing).unwrap();

    assert!(import_roster_csv_to(source.to_str().unwrap(), output.to_str().unwrap()).is_err());
    assert_eq!(fs::read_to_string(output).unwrap(), existing);
}

#[test]
fn spocks_id_only_record_resolves_and_retains_source_id() {
    let dir = TestDirectory::new();
    let source = dir.0.join("source.json");
    let output = dir.0.join("roster.imported.json");
    fs::write(
        &source,
        r#"{"officers":[{"officerId":"1028718336","rank":5,"tier":5,"level":50}]}"#,
    )
    .unwrap();

    let report = import_spocks_export_to(source.to_str().unwrap(), output.to_str().unwrap())
        .expect("known officer ID should resolve");
    let roster: Value = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(report.matched_records, 1);
    assert_eq!(roster["officers"][0]["canonical_officer_id"], "odo-04a97d");
    assert_eq!(roster["officers"][0]["source_oid"], "1028718336");
}

#[test]
fn malformed_upload_does_not_leave_temporary_import_file() {
    let prefix = format!("kobayashi_import_{}_", std::process::id());
    let temp_dir = std::env::temp_dir();
    let before: std::collections::HashSet<_> = fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().starts_with(&prefix))
        .collect();

    assert!(officers_import_payload("{malformed", None).is_err());

    let after: std::collections::HashSet<_> = fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().starts_with(&prefix))
        .collect();
    assert_eq!(after, before, "failed upload must clean up its temp file");
}

#[test]
fn mixed_unresolved_import_preserves_existing_roster() {
    let dir = TestDirectory::new();
    let source = dir.0.join("mixed.json");
    let output = dir.0.join("roster.imported.json");
    let existing =
        r#"{"officers":[{"canonical_officer_id":"keep-this-roster","canonical_name":"Existing"}]}"#;
    fs::write(&source, r#"{"officers":[{"officerId":"1028718336","rank":5,"tier":5,"level":50},{"name":"Not A Canonical Officer","rank":2,"tier":2,"level":20}]}"#).unwrap();
    fs::write(&output, existing).unwrap();

    let report =
        import_spocks_export_to(source.to_str().unwrap(), output.to_str().unwrap()).unwrap();
    assert!(report.has_critical_failures());
    assert_eq!(report.matched_records, 1);
    assert_eq!(
        fs::read_to_string(output).unwrap(),
        existing,
        "a partial import must not replace the last complete roster"
    );
    assert_eq!(report.roster_entries_written, 0);
}

#[test]
fn conflicting_duplicate_import_preserves_existing_roster() {
    let dir = TestDirectory::new();
    let source = dir.0.join("conflict.json");
    let output = dir.0.join("roster.imported.json");
    let existing =
        r#"{"officers":[{"canonical_officer_id":"keep-this-roster","canonical_name":"Existing"}]}"#;
    fs::write(&source, r#"{"officers":[{"officerId":"1028718336","rank":5,"tier":5,"level":50},{"officerId":"1028718336","rank":4,"tier":5,"level":50}]}"#).unwrap();
    fs::write(&output, existing).unwrap();

    let report =
        import_spocks_export_to(source.to_str().unwrap(), output.to_str().unwrap()).unwrap();
    assert!(report.has_critical_failures());
    assert_eq!(report.conflict_records, 1);
    assert_eq!(
        fs::read_to_string(output).unwrap(),
        existing,
        "conflicting canonical states must not replace the last complete roster"
    );
    assert_eq!(report.roster_entries_written, 0);
}
