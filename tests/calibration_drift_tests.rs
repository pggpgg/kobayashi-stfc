//! Drift harness: load `tests/fixtures/recorded_fights/drift_*.json`, run combat, assert reference bands.

use std::path::Path;

use kobayashi::calibration::{
    drift_report, format_drift_summary, list_drift_fixture_paths, load_drift_fixture,
    run_drift_fixture_path, simulate_drift_fixture,
};

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("recorded_fights")
}

#[test]
fn drift_fixture_paths_exist() {
    let dir = fixtures_dir();
    let paths = list_drift_fixture_paths(&dir).expect("read fixtures dir");
    assert!(
        paths.len() >= 3,
        "expected at least three drift_*.json fixtures, found {:?}",
        paths
    );
}

#[test]
fn drift_harness_all_fixtures_within_bands() {
    let dir = fixtures_dir();
    let paths = list_drift_fixture_paths(&dir).expect("list drift fixtures");
    let mut reports = Vec::new();

    for path in &paths {
        let (report, _result) =
            run_drift_fixture_path(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            report.all_ok,
            "drift fixture {} failed:\n{}",
            path.display(),
            format_drift_summary(std::slice::from_ref(&report))
        );
        reports.push(report);
    }

    let summary = format_drift_summary(&reports);
    assert!(
        summary.contains(&format!("fixtures_passed={}", reports.len()))
            && summary.contains("fixtures_failed=0"),
        "unexpected summary tail:\n{summary}"
    );
}

#[test]
fn drift_harness_summary_stable_across_runs() {
    let path = fixtures_dir().join("drift_survey_soak.json");
    let spec = load_drift_fixture(&path).expect("load");
    let r1 = simulate_drift_fixture(&spec);
    let r2 = simulate_drift_fixture(&spec);
    assert_eq!(r1.total_damage, r2.total_damage);
    assert_eq!(r1.rounds_simulated, r2.rounds_simulated);
    assert_eq!(r1.attacker_won, r2.attacker_won);
    let rep1 = drift_report(&spec, &r1);
    let rep2 = drift_report(&spec, &r2);
    assert_eq!(rep1.all_ok, rep2.all_ok);
    for (a, b) in rep1.rows.iter().zip(rep2.rows.iter()) {
        assert_eq!(a.actual, b.actual);
        assert_eq!(a.sigma_from_mid, b.sigma_from_mid);
    }
}
