//! Tests for strict `validate_data` / [`kobayashi::data::validate`] reporting.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use kobayashi::data::validate::{
    full_validation_report_to_json, hostile_ship_class_is_recognized, validate_all_data_for_report,
    validate_buildings_dataset, validate_forbidden_tech_bonus_gaps, validate_registry_dataset,
    validate_research_mapping_gaps, validate_ships_extended_dataset,
    validate_support_buffs_catalog_data, validate_unmapped_canonical_officer_conditions,
    ValidationSeverity,
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
fn forbidden_tech_bonus_routing_reports_summary_when_catalog_present() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data_root = manifest.join("data");
    let catalog = data_root.join("forbidden_chaos_tech.json");
    if !catalog.is_file() {
        eprintln!("skipping forbidden_tech_bonus_routing test: no forbidden_chaos_tech.json");
        return;
    }

    let report = validate_forbidden_tech_bonus_gaps(&data_root).expect("validate ft gaps");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.context == "forbidden_tech.bonus_routing.summary"),
        "expected summary diagnostic, got {:?}",
        report.diagnostics
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.context == "forbidden_tech.bonus_routing.gap"),
        "repo catalog should have zero actionable FT routing gaps, got {:?}",
        report.diagnostics
    );
}

#[test]
fn research_mapping_gaps_reports_summary_when_catalog_present() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = manifest.join("data/research_catalog.json");
    if !catalog.is_file() {
        eprintln!("skipping research_mapping_gaps test: no research_catalog.json");
        return;
    }

    let report = validate_research_mapping_gaps(manifest).expect("validate research gaps");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.context == "research.mapping_gaps.summary"),
        "expected summary diagnostic, got {:?}",
        report.diagnostics
    );
    assert!(
        !report.has_errors(),
        "default mode should not error on baseline-matched gaps, got {:?}",
        report.diagnostics
    );
}

fn write_ships_extended_fixture_ship(dir: &Path, id: &str, deflection_t1: f64, deflection_t2: f64) {
    let tier = |tier: u32, deflection: f64| {
        format!(
            r#"{{"tier":{tier},"armor_piercing":1.0,"shield_piercing":1.0,"accuracy":1.0,
                "armor":100.0,"shield_deflection":{deflection},"dodge":100.0,
                "attack":100.0,"crit_chance":0.1,"crit_damage":1.5,
                "hull_health":1000.0,"shield_health":500.0}}"#
        )
    };
    std::fs::write(
        dir.join(format!("{id}.json")),
        format!(
            r#"{{"id":"{id}","ship_name":"{id}","ship_class":"explorer",
                "tiers":[{},{}],
                "levels":[{{"level":1,"shield":0.0,"health":0.0}}]}}"#,
            tier(1, deflection_t1),
            tier(2, deflection_t2),
        ),
    )
    .unwrap();
}

/// `shield_deflection` guards: the legacy `Deflector.deflection` signature (constant 120 on every
/// tier) is an error; an explorer with zero Shield Deflection everywhere (dead officer-defense
/// channel, OFFICER_STAT_FORMULA.md §2c) is a warning; real per-tier values pass clean.
#[test]
fn ships_extended_validation_flags_stale_and_zero_shield_deflection() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("ships_extended_validate_{nanos}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(
        tmp.join("index.json"),
        r#"{"data_version":"test","ships":[
            {"id":"stale_explorer","ship_name":"stale_explorer","ship_class":"explorer"},
            {"id":"zero_explorer","ship_name":"zero_explorer","ship_class":"explorer"},
            {"id":"good_explorer","ship_name":"good_explorer","ship_class":"explorer"}
        ]}"#,
    )
    .unwrap();
    write_ships_extended_fixture_ship(&tmp, "stale_explorer", 120.0, 120.0);
    write_ships_extended_fixture_ship(&tmp, "zero_explorer", 0.0, 0.0);
    write_ships_extended_fixture_ship(&tmp, "good_explorer", 6667.0, 13338.0);

    let report = validate_ships_extended_dataset(tmp.to_str().unwrap()).expect("validate ships");
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(report.diagnostics.iter().any(|d| {
        d.severity == ValidationSeverity::Error
            && d.context.contains("stale_explorer")
            && d.context.ends_with(".shield_deflection")
            && d.message.contains("Deflector.deflection")
    }));
    assert!(report.diagnostics.iter().any(|d| {
        d.severity == ValidationSeverity::Warning
            && d.context.contains("zero_explorer")
            && d.context.ends_with(".shield_deflection")
            && d.message.contains("officer-defense")
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.context.contains("good_explorer")),
        "real per-tier Shield Deflection values should pass clean, got {:?}",
        report.diagnostics
    );
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
