//! Calibration helpers: drift fixtures vs reference bands (recorded-fight regression guardrails).

mod drift;
mod trace_invariants;

pub use drift::{
    drift_report, format_drift_summary, list_drift_fixture_paths, load_drift_fixture,
    run_drift_fixture_path, simulate_drift_fixture, simulate_drift_fixture_traced,
    DriftFixtureFile, DriftMetricRow, DriftRunReport, DriftSyntheticCrew, FixtureCombatant,
    FixtureSimulation, FixtureWeapon, MetricBands,
};
pub use trace_invariants::{
    check_trace_invariants, TraceInvariantContext, TraceInvariantViolation,
};
