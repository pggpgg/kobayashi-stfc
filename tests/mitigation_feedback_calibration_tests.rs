//! Mitigation feedback loop: predicted damage-through vs traced fixture observations.

use std::path::Path;

use kobayashi::calibration::{
    format_mitigation_damage_through_drift_report, run_mitigation_damage_through_feedback,
};

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("recorded_fights")
}

#[test]
fn mitigation_sensitivity_feedback_matches_drift_fixtures() {
    // Trace rounds to 6 decimals; keep a small buffer over that quantization.
    let tolerance = 1e-5;
    let report =
        run_mitigation_damage_through_feedback(&fixtures_dir(), tolerance).expect("run feedback");
    let body = format_mitigation_damage_through_drift_report(&report);
    println!("{body}");
    assert!(
        report.all_ok,
        "mitigation sensitivity drift exceeded tolerance:\n{body}"
    );
}
