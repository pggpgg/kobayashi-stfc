//! Calibration scoreboard: composite accuracy over drift fixtures (+ recorded suite when populated).

use std::path::Path;

use kobayashi::calibration::{
    format_scoreboard_markdown, load_recorded_fight_suite, run_calibration_scoreboard,
    run_drift_suite, DRIFT_BAND_SIGMA_TARGET,
};

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn fixtures_dir() -> std::path::PathBuf {
    repo_root()
        .join("tests")
        .join("fixtures")
        .join("recorded_fights")
}

#[test]
fn calibration_scoreboard_drift_suite_all_in_band() {
    let reports = run_drift_suite(&fixtures_dir()).expect("run drift suite");
    assert!(
        reports.len() >= 20,
        "expected at least 20 drift fixtures, got {}",
        reports.len()
    );
    for report in &reports {
        assert!(report.all_ok, "fixture {} out of band", report.fixture_id);
        for row in &report.rows {
            assert!(
                row.in_band || row.sigma_from_mid <= DRIFT_BAND_SIGMA_TARGET + 1e-9,
                "fixture {} metric {} sigma={} (> {})",
                report.fixture_id,
                row.metric,
                row.sigma_from_mid,
                DRIFT_BAND_SIGMA_TARGET
            );
        }
    }
}

#[test]
fn calibration_scoreboard_composite_and_markdown() {
    let output = run_calibration_scoreboard(&repo_root()).expect("scoreboard run");
    assert!(output.composite.drift_all_ok());
    assert!(output.composite.metric_count > 0);
    assert!(output.composite.composite_score <= DRIFT_BAND_SIGMA_TARGET);
    let md = format_scoreboard_markdown(
        &output.drift_reports,
        output.recorded.as_ref(),
        &output.composite,
    );
    assert!(md.contains("Mean σ (composite)"));
    assert!(md.contains(&format!(
        "| Drift fixtures passed | {} |",
        output.drift_reports.len()
    )));
}

#[test]
fn run_recorded_suite_empty_manifest_ok() {
    let manifest = fixtures_dir().join("recorded_fight_suite.json");
    let run = kobayashi::calibration::run_recorded_suite(&repo_root(), &manifest)
        .expect("empty suite run");
    assert!(run.all_reports.is_empty());
    assert!(run.iteration_reports.is_empty());
}

#[test]
fn recorded_fight_suite_manifest_loads_empty() {
    let path = fixtures_dir().join("recorded_fight_suite.json");
    let suite = load_recorded_fight_suite(&path).expect("load manifest");
    assert!(suite.fights.is_empty());
    assert!(suite.profile_id.is_none());
}

#[test]
fn calibration_scoreboard_output_stable_composite() {
    let a = run_calibration_scoreboard(&repo_root()).expect("run a");
    let b = run_calibration_scoreboard(&repo_root()).expect("run b");
    assert_eq!(
        (a.composite.composite_score * 1e6) as i64,
        (b.composite.composite_score * 1e6) as i64
    );
    assert_eq!(
        a.composite.drift_fixtures_passed,
        b.composite.drift_fixtures_passed
    );
}

#[test]
fn officer_anchor_manifest_schema_accepts_known_anchors() {
    let json = r#"{
        "profile_id": "freeze_test",
        "fights": [{
            "id": "kirk_anchor",
            "fixture_csv": "fight samples/realta vs takret militia 10.csv",
            "ship_id": "realta",
            "captain": "kirk-1323b6",
            "bridge": [],
            "below_decks": [],
            "hostile_display_name": "Takret Militia",
            "hostile_level": 10,
            "officer_anchor": "kirk",
            "bands": {}
        }]
    }"#;
    let suite: kobayashi::calibration::RecordedFightSuite =
        serde_json::from_str(json).expect("parse");
    assert_eq!(suite.fights[0].officer_anchor.as_deref(), Some("kirk"));
}
