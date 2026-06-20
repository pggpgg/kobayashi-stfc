//! Calibration helpers: drift fixtures vs reference bands (recorded-fight regression guardrails).

mod drift;
mod mitigation_feedback;
mod recorded;
mod scoreboard;
mod trace_invariants;

pub use drift::{
    drift_report, format_drift_summary, list_drift_fixture_paths, load_drift_fixture,
    run_drift_fixture_path, simulate_drift_fixture, simulate_drift_fixture_traced,
    simulation_band_report, DriftFixtureFile, DriftMetricRow, DriftRunReport, DriftSyntheticCrew,
    FixtureCombatant, FixtureSimulation, FixtureWeapon, MetricBands,
};
pub use mitigation_feedback::{
    format_mitigation_damage_through_drift_report, run_mitigation_damage_through_feedback,
    MitigationDamageThroughDriftReport, MitigationDamageThroughDriftRow,
};
pub use recorded::{
    load_recorded_fight_suite, run_recorded_suite, RecordedFightEntry, RecordedFightSuite,
    RecordedSuiteRun,
};
pub use scoreboard::{
    composite_from_drift_reports, composite_from_suite_run, default_fixtures_dir,
    default_manifest_path, format_scoreboard_markdown, format_scoreboard_text,
    run_calibration_scoreboard, run_drift_suite, CalibrationScoreboardOutput, CompositeScoreReport,
    DRIFT_BAND_SIGMA_TARGET, DRIFT_COMPOSITE_MEAN_SIGMA_TARGET,
    RECORDED_COMPOSITE_MEAN_SIGMA_TARGET,
};
pub use trace_invariants::{
    check_trace_invariants, TraceInvariantContext, TraceInvariantViolation,
};
