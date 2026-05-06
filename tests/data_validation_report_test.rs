//! Tests for strict `validate_data` / [`kobayashi::data::validate`] reporting.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use kobayashi::data::validate::{
    full_validation_report_to_json, hostile_ship_class_is_recognized, validate_all_data_for_report,
    validate_buildings_dataset, validate_registry_dataset, validate_support_buffs_catalog_data,
    validate_unmapped_canonical_officer_conditions, ValidationSeverity,
};

static CANONICAL_CONDITION_VALIDATE_TEST_LOCK: Mutex<()> = Mutex::new(());
static BUILDING_BONUS_GAPS_VALIDATE_TEST_LOCK: Mutex<()> = Mutex::new(());

fn write_building_gap_fixture(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("index.json"),
        r#"{"data_version":"test","buildings":[
            {"id":"alpha","building_name":"Alpha","file":"alpha","bid":900001},
            {"id":"beta","building_name":"Beta","file":"beta","bid":900002}
        ]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("alpha.json"),
        r#"{"id":"alpha","building_name":"Alpha","levels":[
            {"level":1,"bonuses":[
                {"stat":"buff_unknown_x","value":0.1,"operator":"add"},
                {"stat":"weapon_damage","value":0.05,"operator":"add",
                 "conditions":["mystery_condition"]}
            ]}
        ]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("beta.json"),
        r#"{"id":"beta","building_name":"Beta","levels":[
            {"level":1,"bonuses":[
                {"stat":"hull_hp","value":0.05,"operator":"add",
                 "conditions":["ship_combat_only"]}
            ]}
        ]}"#,
    )
    .unwrap();
}

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
        "support_buffs",
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
fn support_buff_validation_reports_catalog_errors() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("support_buff_validate_{nanos}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(
        tmp.join("support_buffs.json"),
        r#"{
          "buffs": {
            "bad_buff": {
              "id": "wrong_id",
              "display_name": "Bad Buff",
              "source": "test",
              "provenance_notes": ["test fixture"],
              "stat_targets": [
                {
                  "stat": "unknown_static_key",
                  "value": 2.0,
                  "stacking": "additive",
                  "layer": "static_bonuses"
                }
              ],
              "static_bonuses": {
                "unknown_static_key": 3.0
              }
            }
          }
        }"#,
    )
    .unwrap();

    let report = validate_support_buffs_catalog_data(&tmp).expect("validate support buffs");
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(report.has_errors());
    assert!(report.diagnostics.iter().any(|d| {
        d.severity == ValidationSeverity::Error && d.message.contains("must match map key")
    }));
    assert!(report.diagnostics.iter().any(|d| {
        d.severity == ValidationSeverity::Error
            && d.message
                .contains("not consumed by the static combat layer")
    }));
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
fn building_bonus_gaps_default_warns_per_row() {
    let _guard = BUILDING_BONUS_GAPS_VALIDATE_TEST_LOCK.lock().unwrap();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("buildings_validate_default_{nanos}"));
    let _ = std::fs::remove_dir_all(&tmp);
    write_building_gap_fixture(&tmp);

    std::env::remove_var("KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS");
    let report = validate_buildings_dataset(tmp.to_str().unwrap()).expect("validate buildings");
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(
        !report.has_errors(),
        "default mode should not produce errors, got {:?}",
        report.diagnostics
    );

    let buff_diags: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.context == "buildings.bonuses.opaque_buff")
        .collect();
    assert_eq!(
        buff_diags.len(),
        1,
        "one diagnostic per distinct buff_* stat"
    );
    let buff = buff_diags[0];
    assert_eq!(buff.severity, ValidationSeverity::Warning);
    assert!(buff.message.contains("buff_unknown_x"));
    assert!(buff.message.contains("alpha"));
    assert!(buff
        .message
        .contains("not merged via normalize_profile_combat_stat"));

    let cond_diags: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.context == "buildings.bonuses.unknown_condition")
        .collect();
    assert_eq!(
        cond_diags.len(),
        1,
        "one diagnostic per distinct unknown condition"
    );
    let cond = cond_diags[0];
    assert_eq!(cond.severity, ValidationSeverity::Warning);
    assert!(cond.message.contains("mystery_condition"));
    assert!(cond.message.contains("alpha"));
    assert!(cond.message.contains("not in is_known_building_condition"));
}

#[test]
fn building_bonus_gaps_strict_env_upgrades_to_error() {
    let _guard = BUILDING_BONUS_GAPS_VALIDATE_TEST_LOCK.lock().unwrap();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("buildings_validate_strict_{nanos}"));
    let _ = std::fs::remove_dir_all(&tmp);
    write_building_gap_fixture(&tmp);

    std::env::set_var("KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS", "1");
    let strict =
        validate_buildings_dataset(tmp.to_str().unwrap()).expect("validate buildings strict");
    std::env::remove_var("KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS");
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(
        strict.has_errors(),
        "strict env should upgrade gap warnings to errors, got {:?}",
        strict.diagnostics
    );
    assert!(strict.diagnostics.iter().any(|d| {
        d.severity == ValidationSeverity::Error
            && d.context == "buildings.bonuses.opaque_buff"
            && d.message.contains("buff_unknown_x")
    }));
    assert!(strict.diagnostics.iter().any(|d| {
        d.severity == ValidationSeverity::Error
            && d.context == "buildings.bonuses.unknown_condition"
            && d.message.contains("mystery_condition")
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
