//! LCARS malformed-input robustness (roadmap task 8).
//!
//! The LCARS monolith is user-editable, so the load chain has a contract:
//! - **Runtime** ([`load_lcars_dir`]): lenient but loud — a malformed file is skipped with a
//!   stderr warning while the remaining files still load.
//! - **Validation** ([`validate_lcars_dir`]): strict — the same malformed file is a hard Error,
//!   so `kobayashi validate` and the CI `validate_data` gate catch what the runtime tolerates.
//! - **Resolution** ([`collect_lcars_drops`]): unknown effect types degrade gracefully into
//!   drop-report entries, never panics.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use kobayashi::data::validate::{validate_lcars_dir, ValidationSeverity};
use kobayashi::lcars::{
    collect_lcars_drops, load_lcars_dir, LcarsAbility, LcarsEffect, LcarsFile, LcarsOfficer,
};
use proptest::prelude::*;

const VALID_LCARS: &str = r#"
officers:
  - id: valid_officer
    name: Valid Officer
    captain_ability:
      name: Steady Hand
      effects:
        - type: stat_modify
          stat: weapon_damage
          operator: add
          value: 0.1
"#;

const CORRUPT_LCARS: &str = "officers:\n  - id: broken\n    name: [unterminated\n";

fn temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("{label}_{nanos}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    tmp
}

/// One corrupt file in the directory must not take down the valid ones at runtime — and must
/// fail validation.
#[test]
fn corrupt_file_is_skipped_by_loader_but_fails_validation() {
    let tmp = temp_dir("lcars_robustness");
    std::fs::write(tmp.join("valid.lcars.yaml"), VALID_LCARS).unwrap();
    std::fs::write(tmp.join("corrupt.lcars.yaml"), CORRUPT_LCARS).unwrap();

    // Runtime loader: lenient — the valid officer still loads.
    let officers = load_lcars_dir(&tmp).expect("dir load succeeds");
    assert_eq!(officers.len(), 1);
    assert_eq!(officers[0].id, "valid_officer");

    // Validation: strict — the corrupt file is a hard Error naming the file.
    let report = validate_lcars_dir(tmp.to_str().unwrap()).expect("validation runs");
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(report.has_errors());
    assert!(
        report.diagnostics.iter().any(|d| {
            d.severity == ValidationSeverity::Error
                && d.context == "lcars.parse"
                && d.message.contains("corrupt.lcars.yaml")
        }),
        "expected an lcars.parse Error for corrupt.lcars.yaml, got {:?}",
        report.diagnostics
    );
}

/// A directory of only-valid files stays error-free under the stricter validation path.
#[test]
fn valid_only_directory_passes_validation_without_parse_errors() {
    let tmp = temp_dir("lcars_robustness_valid");
    std::fs::write(tmp.join("valid.lcars.yaml"), VALID_LCARS).unwrap();
    let report = validate_lcars_dir(tmp.to_str().unwrap()).expect("validation runs");
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.context == "lcars.parse"),
        "no parse diagnostics expected, got {:?}",
        report.diagnostics
    );
}

fn fuzz_effect(effect_type: String, field: Option<String>, value: f64) -> LcarsEffect {
    LcarsEffect {
        effect_type,
        stat: field.clone(),
        target: field.clone(),
        operator: field.clone(),
        value: Some(value),
        trigger: field,
        duration: None,
        scaling: None,
        condition: None,
        chance: None,
        multiplier: None,
        tag: None,
        accumulate: None,
        decay: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Arbitrary strings in effect fields round-trip through YAML and resolve without panicking;
    /// the (guaranteed-unknown) effect type surfaces as a drop-report entry, never a crash.
    #[test]
    fn arbitrary_effect_fields_round_trip_and_degrade_to_drops(
        suffix in ".{0,40}",
        field in proptest::option::of(".{0,24}"),
        value in -1e12..1e12f64,
    ) {
        let officer = LcarsOfficer {
            id: "fuzz".to_string(),
            name: "Fuzz".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(LcarsAbility {
                name: "Fuzzed".to_string(),
                effects: vec![fuzz_effect(format!("zzz_unknown_{suffix}"), field, value)],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let yaml = serde_yaml::to_string(&LcarsFile { officers: vec![officer] })
            .expect("serialize fuzzed officer");
        let parsed: LcarsFile = serde_yaml::from_str(&yaml).expect("round-trip parse");
        prop_assert_eq!(parsed.officers.len(), 1);

        let drops = collect_lcars_drops(&parsed.officers);
        prop_assert!(
            !drops.drops.is_empty(),
            "unknown effect type should be recorded as a drop"
        );
    }

    /// Hostile raw text in the value position parses to Ok or Err — never a panic. (Structure
    /// breakage is expected and fine; this pins the absence of a panicking path.)
    #[test]
    fn arbitrary_raw_yaml_never_panics_the_parser(payload in ".{0,80}") {
        let doc = format!(
            "officers:\n  - id: x\n    name: X\n    captain_ability:\n      name: A\n      effects:\n        - type: {payload}\n"
        );
        let _ = serde_yaml::from_str::<LcarsFile>(&doc);
    }
}
