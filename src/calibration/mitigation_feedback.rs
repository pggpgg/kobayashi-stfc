//! Mitigation sensitivity feedback loop against drift fixtures.
//!
//! This compares mitigation/pierce-predicted damage-through factors to what the
//! traced combat engine actually emits (`pierce_calc.damage_through_factor`).

use std::path::Path;

use crate::calibration::{
    list_drift_fixture_paths, load_drift_fixture, simulate_drift_fixture_traced, DriftFixtureFile,
};
use crate::combat::direct_scalar_row;

#[derive(Debug, Clone)]
pub struct MitigationDamageThroughDriftRow {
    pub fixture_id: String,
    pub predicted_damage_through: f64,
    pub observed_damage_through: f64,
    pub abs_drift: f64,
    pub sample_count: usize,
    pub within_tolerance: bool,
}

#[derive(Debug, Clone)]
pub struct MitigationDamageThroughDriftReport {
    pub tolerance: f64,
    pub rows: Vec<MitigationDamageThroughDriftRow>,
    pub max_abs_drift: f64,
    pub all_ok: bool,
}

fn observed_damage_through_from_trace(spec: &DriftFixtureFile) -> Result<(f64, usize), String> {
    let result = simulate_drift_fixture_traced(spec);
    let mut sum = 0.0_f64;
    let mut n = 0usize;
    for event in &result.events {
        if event.event_type != "pierce_calc" || event.phase != "attack" {
            continue;
        }
        let Some(v) = event
            .values
            .get("damage_through_factor")
            .and_then(serde_json::Value::as_f64)
        else {
            return Err(format!(
                "{} has attack pierce_calc event missing damage_through_factor",
                spec.id
            ));
        };
        sum += v;
        n += 1;
    }
    if n == 0 {
        return Err(format!(
            "{} produced no attack pierce_calc events; cannot compute observed damage-through",
            spec.id
        ));
    }
    Ok((sum / n as f64, n))
}

fn predicted_damage_through_for_fixture(spec: &DriftFixtureFile) -> f64 {
    direct_scalar_row(
        "fixture_baseline",
        spec.defender.mitigation,
        spec.attacker.pierce,
        0.0,
    )
    .damage_through_factor
}

pub fn run_mitigation_damage_through_feedback(
    dir: &Path,
    tolerance: f64,
) -> Result<MitigationDamageThroughDriftReport, String> {
    let paths = list_drift_fixture_paths(dir).map_err(|e| format!("list fixtures: {e}"))?;
    let mut rows = Vec::with_capacity(paths.len());
    let mut max_abs_drift = 0.0_f64;

    for path in &paths {
        let spec = load_drift_fixture(path)?;
        let predicted = predicted_damage_through_for_fixture(&spec);
        let (observed, samples) = observed_damage_through_from_trace(&spec)?;
        let abs_drift = (observed - predicted).abs();
        max_abs_drift = max_abs_drift.max(abs_drift);
        rows.push(MitigationDamageThroughDriftRow {
            fixture_id: spec.id,
            predicted_damage_through: predicted,
            observed_damage_through: observed,
            abs_drift,
            sample_count: samples,
            within_tolerance: abs_drift <= tolerance,
        });
    }

    let all_ok = rows.iter().all(|r| r.within_tolerance);
    Ok(MitigationDamageThroughDriftReport {
        tolerance,
        rows,
        max_abs_drift,
        all_ok,
    })
}

pub fn format_mitigation_damage_through_drift_report(
    report: &MitigationDamageThroughDriftReport,
) -> String {
    let mut out = String::new();
    out.push_str("=== mitigation_sensitivity damage-through drift ===\n");
    out.push_str("fixture\tpredicted\tobserved\tabs_drift\tsamples\tstatus\n");
    let mut pass = 0usize;
    let mut fail = 0usize;
    for row in &report.rows {
        let status = if row.within_tolerance { "ok" } else { "DRIFT" };
        if row.within_tolerance {
            pass += 1;
        } else {
            fail += 1;
        }
        out.push_str(&format!(
            "{}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\n",
            row.fixture_id,
            row.predicted_damage_through,
            row.observed_damage_through,
            row.abs_drift,
            row.sample_count,
            status
        ));
    }
    out.push_str(&format!(
        "tolerance={:.6} max_abs_drift={:.6} fixtures_passed={} fixtures_failed={}\n",
        report.tolerance, report.max_abs_drift, pass, fail
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_report_format_includes_summary_tail() {
        let report = MitigationDamageThroughDriftReport {
            tolerance: 1e-6,
            rows: vec![MitigationDamageThroughDriftRow {
                fixture_id: "fixture_a".to_string(),
                predicted_damage_through: 0.75,
                observed_damage_through: 0.75,
                abs_drift: 0.0,
                sample_count: 5,
                within_tolerance: true,
            }],
            max_abs_drift: 0.0,
            all_ok: true,
        };
        let s = format_mitigation_damage_through_drift_report(&report);
        assert!(s.contains("fixtures_passed=1"));
        assert!(s.contains("fixtures_failed=0"));
        assert!(s.contains("tolerance=0.000001"));
    }
}
