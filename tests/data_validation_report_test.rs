//! Tests for strict `validate_data` / [`kobayashi::data::validate`] reporting.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use kobayashi::data::validate::{
    full_validation_report_to_json, hostile_ship_class_is_recognized, validate_all_data_for_report,
    validate_registry_dataset, validate_unmapped_canonical_officer_conditions, ValidationSeverity,
};

static CANONICAL_CONDITION_VALIDATE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn hostile_ship_class_recognition() {
    assert!(hostile_ship_class_is_recognized("battleship"));
    assert!(hostile_ship_class_is_recognized("Explorer"));
    assert!(!hostile_ship_class_is_recognized("dreadnought"));
    assert!(!hostile_ship_class_is_recognized(""));
}

#[test]
fn full_report_includes_core_categories() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let report = validate_all_data_for_report(manifest);
    let names: Vec<&str> = report.categories.iter().map(|c| c.name.as_str()).collect();
    for required in [
        "registry",
        "officers_canonical",
        "officers_lcars",
        "forbidden_chaos",
        "canonical_conditions",
    ] {
        assert!(
            names.contains(&required),
            "missing category {required:?}, have {names:?}"
        );
    }
    let json = full_validation_report_to_json(&report).expect("serialize");
    assert!(json.contains("\"summary\""));
    assert!(json.contains("\"errors\""));
}

#[test]
fn strict_canonical_condition_maps_upgrade_unmapped_to_error() {
    let _guard = CANONICAL_CONDITION_VALIDATE_TEST_LOCK.lock().unwrap();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("canonical_condition_validate_{nanos}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("officers")).unwrap();
    let canon_path = tmp.join("officers/officers.canonical.json");
    std::fs::write(
        &canon_path,
        r#"{"officers":[{"id":"999","name":"Test","abilities":[{"slot":"captain","conditions":["TotallyUnknownConditionToken"]}]}]}"#,
    )
    .unwrap();

    let relaxed = validate_unmapped_canonical_officer_conditions(&tmp).unwrap();
    assert!(
        !relaxed.has_errors(),
        "unmapped canonical conditions should be warnings by default"
    );
    assert!(
        relaxed.diagnostics.iter().any(|d| {
            d.severity == ValidationSeverity::Warning && d.context == "canonical.unmapped_condition"
        }),
        "expected a warning diagnostic, got {:?}",
        relaxed.diagnostics
    );

    std::env::set_var("KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS", "1");
    let strict = validate_unmapped_canonical_officer_conditions(&tmp).unwrap();
    std::env::remove_var("KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS");
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(
        strict.has_errors(),
        "strict mode should treat unmapped tokens as errors, got {:?}",
        strict.diagnostics
    );
    assert!(strict.diagnostics.iter().any(|d| {
        d.severity == ValidationSeverity::Error && d.context == "canonical.unmapped_condition"
    }));
}

#[test]
fn registry_validation_emits_error_when_registry_missing() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("validate_registry_test_{nanos}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let r = validate_registry_dataset(&tmp).expect("validate");
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(r.has_errors());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.severity == ValidationSeverity::Error && d.context == "registry"));
}
