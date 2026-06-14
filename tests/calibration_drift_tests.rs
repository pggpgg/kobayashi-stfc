//! Drift harness: load `tests/fixtures/recorded_fights/drift_*.json`, run combat, assert reference bands.
//!
//! ## Add a real fight log (~10 minutes)
//!
//! See [docs/CALIBRATION_ADD_FIGHT.md](../docs/CALIBRATION_ADD_FIGHT.md). Quick path:
//!
//! 1. Save TSV to `fight samples/<ship>_vs_<hostile>_<level>_<outcome>.csv`
//! 2. `cargo test --test recorded_fight_calibration_tests fight_export`
//! 3. Add a row to `recorded_fight_suite.json` (after snapshot freeze only)
//! 4. For synthetic mechanics, copy a `drift_*.json` template from [docs/combat_log_format.md](../docs/combat_log_format.md)
//! 5. `cargo xtask calibration-scoreboard --write`

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
fn drift_research_weapon_damage_pool_orders_below_layered_total_damage() {
    let dir = fixtures_dir();
    let pool_spec =
        load_drift_fixture(&dir.join("drift_research_weapon_damage_additive_pool.json"))
            .expect("load pooled research-style drift");
    let layered_spec =
        load_drift_fixture(&dir.join("drift_research_weapon_damage_layered_no_pool.json"))
            .expect("load layered research-style drift");
    let pooled = simulate_drift_fixture(&pool_spec);
    let layered = simulate_drift_fixture(&layered_spec);
    assert!(
        layered.total_damage > pooled.total_damage,
        "layered profile_weapon_damage path should deal strictly more total_damage than additive pool when pre_attack_multiplier>1 (layered={} pooled={})",
        layered.total_damage,
        pooled.total_damage
    );
    assert_eq!(pooled.rounds_simulated, 36);
    assert_eq!(layered.rounds_simulated, 34);
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
