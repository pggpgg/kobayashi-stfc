//! Calibration helpers: drift fixtures vs reference bands (recorded-fight regression guardrails).

mod drift;

pub use drift::{
    drift_report, format_drift_summary, list_drift_fixture_paths, load_drift_fixture,
    run_drift_fixture_path, simulate_drift_fixture, DriftFixtureFile, DriftMetricRow,
    DriftRunReport, FixtureCombatant, FixtureSimulation, FixtureWeapon, MetricBands,
};
