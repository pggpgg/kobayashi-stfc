//! Calibration scoreboard: drift suite runner, composite accuracy score, and report formatters.

use std::path::{Path, PathBuf};

use crate::calibration::drift::{
    format_drift_summary, list_drift_fixture_paths, run_drift_fixture_path, DriftMetricRow,
    DriftRunReport,
};
use crate::calibration::recorded::{run_recorded_suite, RecordedSuiteRun};

/// Target: every drift metric should stay in band (σ ≤ 1.0).
pub const DRIFT_BAND_SIGMA_TARGET: f64 = 1.0;
/// Informational composite target for the drift layer (mean σ across metrics).
pub const DRIFT_COMPOSITE_MEAN_SIGMA_TARGET: f64 = 0.35;
/// Initial recorded-fight composite target (post-freeze; holdout excluded from iteration composite).
pub const RECORDED_COMPOSITE_MEAN_SIGMA_TARGET: f64 = 2.0;

/// Aggregate accuracy over drift and/or recorded fight reports.
#[derive(Debug, Clone)]
pub struct CompositeScoreReport {
    pub drift_fixtures_passed: usize,
    pub drift_fixtures_failed: usize,
    pub recorded_fights_passed: usize,
    pub recorded_fights_failed: usize,
    pub recorded_iteration_passed: usize,
    pub recorded_iteration_failed: usize,
    pub metric_count: usize,
    pub mean_sigma: f64,
    pub max_sigma: f64,
    /// Same as `mean_sigma` — lower is better; 0 = all metrics at band center.
    pub composite_score: f64,
    pub worst_metric: Option<(String, String, f64)>,
}

impl CompositeScoreReport {
    pub fn drift_all_ok(&self) -> bool {
        self.drift_fixtures_failed == 0
    }

    pub fn recorded_iteration_all_ok(&self) -> bool {
        self.recorded_iteration_failed == 0
    }

    pub fn all_ok(&self) -> bool {
        self.drift_all_ok() && self.recorded_iteration_all_ok()
    }
}

/// Run all `drift_*.json` fixtures under `fixtures_dir`.
pub fn run_drift_suite(
    fixtures_dir: &Path,
) -> Result<Vec<DriftRunReport>, String> {
    let paths = list_drift_fixture_paths(fixtures_dir)
        .map_err(|e| format!("list drift fixtures in {}: {e}", fixtures_dir.display()))?;
    let mut reports = Vec::with_capacity(paths.len());
    for path in &paths {
        let (report, _) = run_drift_fixture_path(path)?;
        reports.push(report);
    }
    Ok(reports)
}

pub fn composite_from_drift_reports(reports: &[DriftRunReport]) -> CompositeScoreReport {
    let drift_fixtures_passed = reports.iter().filter(|r| r.all_ok).count();
    let drift_fixtures_failed = reports.len().saturating_sub(drift_fixtures_passed);
    composite_from_metric_rows(
        reports.iter().flat_map(|r| r.rows.iter()),
        drift_fixtures_passed,
        drift_fixtures_failed,
        0,
        0,
        0,
        0,
    )
}

pub fn composite_from_suite_run(
    drift_reports: &[DriftRunReport],
    recorded: &RecordedSuiteRun,
) -> CompositeScoreReport {
    let drift_fixtures_passed = drift_reports.iter().filter(|r| r.all_ok).count();
    let drift_fixtures_failed = drift_reports.len().saturating_sub(drift_fixtures_passed);
    let recorded_fights_passed = recorded.all_reports.iter().filter(|r| r.all_ok).count();
    let recorded_fights_failed = recorded.all_reports.len().saturating_sub(recorded_fights_passed);
    let recorded_iteration_passed = recorded
        .iteration_reports
        .iter()
        .filter(|r| r.all_ok)
        .count();
    let recorded_iteration_failed = recorded
        .iteration_reports
        .len()
        .saturating_sub(recorded_iteration_passed);

    let drift_rows = drift_reports.iter().flat_map(|r| r.rows.iter());
    let recorded_rows = recorded.iteration_reports.iter().flat_map(|r| r.rows.iter());
    composite_from_metric_rows(
        drift_rows.chain(recorded_rows),
        drift_fixtures_passed,
        drift_fixtures_failed,
        recorded_fights_passed,
        recorded_fights_failed,
        recorded_iteration_passed,
        recorded_iteration_failed,
    )
}

fn composite_from_metric_rows<'a>(
    rows: impl Iterator<Item = &'a DriftMetricRow>,
    drift_fixtures_passed: usize,
    drift_fixtures_failed: usize,
    recorded_fights_passed: usize,
    recorded_fights_failed: usize,
    recorded_iteration_passed: usize,
    recorded_iteration_failed: usize,
) -> CompositeScoreReport {
    let row_vec: Vec<&DriftMetricRow> = rows.collect();
    let metric_count = row_vec.len();
    let mean_sigma = if metric_count == 0 {
        0.0
    } else {
        row_vec.iter().map(|r| r.sigma_from_mid).sum::<f64>() / metric_count as f64
    };
    let max_sigma = row_vec
        .iter()
        .map(|r| r.sigma_from_mid)
        .fold(0.0_f64, f64::max);
    let worst_metric = row_vec
        .iter()
        .max_by(|a, b| {
            a.sigma_from_mid
                .partial_cmp(&b.sigma_from_mid)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| (r.fixture_id.clone(), r.metric.to_string(), r.sigma_from_mid));

    CompositeScoreReport {
        drift_fixtures_passed,
        drift_fixtures_failed,
        recorded_fights_passed,
        recorded_fights_failed,
        recorded_iteration_passed,
        recorded_iteration_failed,
        metric_count,
        mean_sigma,
        max_sigma,
        composite_score: mean_sigma,
        worst_metric,
    }
}

fn scoreboard_header_markdown() -> &'static str {
    "# Calibration scoreboard

> **Generated.** Do not edit by hand. Regenerate with:
>
> ```bash
> cargo xtask calibration-scoreboard --write docs/CALIBRATION_SCOREBOARD.md
> ```

Measures how closely the combat engine matches reference bands on synthetic drift fixtures and (when populated) snapshot-bound recorded fights.

## Band-width targets

| Layer | Target | Notes |
| --- | --- | --- |
| Drift synthetic | all metrics σ ≤ 1.0 (in band) | CI gate |
| Drift composite | mean σ ≤ 0.35 | Informational |
| Recorded (post-freeze) | outcome exact; mean σ ≤ 2.0 initially | Holdout excluded from iteration composite |

## Iterate rule (post-freeze)

Engine changes are accepted when drift + non-holdout recorded composite improves **and** no non-holdout fight regresses beyond band. Run holdout fights before release; never tune directly to holdout.

See [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md) and [CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md).

"
}

/// Plain-text scoreboard for CI artifacts.
pub fn format_scoreboard_text(
    drift_reports: &[DriftRunReport],
    recorded: Option<&RecordedSuiteRun>,
    composite: &CompositeScoreReport,
) -> String {
    let mut out = String::new();
    out.push_str("=== Kobayashi calibration scoreboard ===\n\n");
    out.push_str(&format_drift_summary(drift_reports));
    if let Some(rec) = recorded {
        if !rec.all_reports.is_empty() {
            out.push_str("\n=== Recorded fight suite ===\n");
            out.push_str(&format_drift_summary(&rec.all_reports));
            out.push_str(&format!(
                "recorded_iteration_passed={} recorded_iteration_failed={} (holdout excluded)\n",
                composite.recorded_iteration_passed, composite.recorded_iteration_failed
            ));
        } else {
            out.push_str("\nrecorded_fight_suite: empty (awaiting snapshot freeze)\n");
        }
    }
    out.push_str(&format_composite_footer(composite));
    out
}

/// Markdown scoreboard for committed documentation.
pub fn format_scoreboard_markdown(
    drift_reports: &[DriftRunReport],
    recorded: Option<&RecordedSuiteRun>,
    composite: &CompositeScoreReport,
) -> String {
    let mut out = scoreboard_header_markdown().to_string();
    out.push_str("## Summary\n\n");
    out.push_str(&format_composite_markdown(composite));
    out.push_str("\n## Drift layer (synthetic)\n\n");
    out.push_str(&format_drift_markdown_table(drift_reports));
    if let Some(rec) = recorded {
        out.push_str("\n## Recorded layer (snapshot-bound)\n\n");
        if rec.all_reports.is_empty() {
            out.push_str(
                "_No recorded fights in manifest yet — populate after snapshot freeze._\n",
            );
        } else {
            out.push_str(&format!(
                "Iteration composite uses **{}** non-holdout fights ({} holdout).\n\n",
                rec.iteration_reports.len(),
                rec.all_reports.len().saturating_sub(rec.iteration_reports.len())
            ));
            out.push_str(&format_drift_markdown_table(&rec.all_reports));
            if !rec.axis_coverage.is_empty() {
                out.push_str("\n### Axis coverage\n\n");
                out.push_str("| axis | fights |\n| --- | ---: |\n");
                for (axis, count) in &rec.axis_coverage {
                    out.push_str(&format!("| {axis} | {count} |\n"));
                }
            }
        }
    }
    out
}

fn format_composite_footer(composite: &CompositeScoreReport) -> String {
    let mut out = String::new();
    out.push_str("\n--- composite ---\n");
    out.push_str(&format!(
        "metric_count={} mean_sigma={:.4} max_sigma={:.4} composite_score={:.4}\n",
        composite.metric_count, composite.mean_sigma, composite.max_sigma, composite.composite_score
    ));
    if let Some((id, metric, sigma)) = &composite.worst_metric {
        out.push_str(&format!("worst_metric={id} {metric} sigma={sigma:.4}\n"));
    }
    out.push_str(&format!(
        "drift_passed={} drift_failed={}\n",
        composite.drift_fixtures_passed, composite.drift_fixtures_failed
    ));
    out
}

fn format_composite_markdown(composite: &CompositeScoreReport) -> String {
    let mut s = format!(
        "| Metric | Value |\n| --- | ---: |\n\
         | Drift fixtures passed | {} |\n\
         | Drift fixtures failed | {} |\n\
         | Recorded fights passed | {} |\n\
         | Recorded fights failed | {} |\n\
         | Recorded iteration passed | {} |\n\
         | Recorded iteration failed | {} |\n\
         | Metrics scored | {} |\n\
         | Mean σ (composite) | {:.4} |\n\
         | Max σ | {:.4} |\n",
        composite.drift_fixtures_passed,
        composite.drift_fixtures_failed,
        composite.recorded_fights_passed,
        composite.recorded_fights_failed,
        composite.recorded_iteration_passed,
        composite.recorded_iteration_failed,
        composite.metric_count,
        composite.composite_score,
        composite.max_sigma,
    );
    if let Some((id, metric, sigma)) = &composite.worst_metric {
        s.push_str(&format!("| Worst metric | `{id}` `{metric}` σ={sigma:.4} |\n"));
    }
    s
}

fn format_drift_markdown_table(reports: &[DriftRunReport]) -> String {
    let mut out = String::new();
    for report in reports {
        out.push_str(&format!("### `{}`\n\n", report.fixture_id));
        if let Some(ref d) = report.description {
            out.push_str(&format!("{d}\n\n"));
        }
        if report.rows.is_empty() && report.attacker_won_ok.is_none() {
            out.push_str("_No bands configured._\n\n");
            continue;
        }
        out.push_str(
            "| metric | actual | band | σ | status |\n| --- | ---: | --- | ---: | --- |\n",
        );
        for row in &report.rows {
            let status = if row.in_band { "ok" } else { "OUT" };
            out.push_str(&format!(
                "| {} | {:.4} | [{:.4}, {:.4}] | {:.3} | {} |\n",
                row.metric, row.actual, row.low, row.high, row.sigma_from_mid, status
            ));
        }
        if let Some(ok) = report.attacker_won_ok {
            out.push_str(&format!(
                "\n**attacker_won:** {}\n\n",
                if ok { "ok" } else { "MISMATCH" }
            ));
        } else {
            out.push('\n');
        }
        out.push_str(&format!(
            "**fixture_ok:** {}\n\n",
            if report.all_ok { "yes" } else { "no" }
        ));
    }
    out
}

/// Default fixtures directory relative to repo root.
pub fn default_fixtures_dir(repo_root: &Path) -> PathBuf {
    repo_root
        .join("tests")
        .join("fixtures")
        .join("recorded_fights")
}

pub fn default_manifest_path(repo_root: &Path) -> PathBuf {
    default_fixtures_dir(repo_root).join("recorded_fight_suite.json")
}

/// Full scoreboard run: drift suite + optional recorded manifest.
pub fn run_calibration_scoreboard(
    repo_root: &Path,
) -> Result<CalibrationScoreboardOutput, String> {
    let fixtures_dir = default_fixtures_dir(repo_root);
    let drift_reports = run_drift_suite(&fixtures_dir)?;
    let manifest_path = default_manifest_path(repo_root);
    let recorded = if manifest_path.is_file() {
        Some(run_recorded_suite(repo_root, &manifest_path)?)
    } else {
        None
    };
    let composite = match &recorded {
        Some(rec) => composite_from_suite_run(&drift_reports, rec),
        None => composite_from_drift_reports(&drift_reports),
    };
    Ok(CalibrationScoreboardOutput {
        drift_reports,
        recorded,
        composite,
    })
}

#[derive(Debug)]
pub struct CalibrationScoreboardOutput {
    pub drift_reports: Vec<DriftRunReport>,
    pub recorded: Option<RecordedSuiteRun>,
    pub composite: CompositeScoreReport,
}

impl CalibrationScoreboardOutput {
    pub fn text_report(&self) -> String {
        format_scoreboard_text(
            &self.drift_reports,
            self.recorded.as_ref(),
            &self.composite,
        )
    }

    pub fn markdown_report(&self) -> String {
        format_scoreboard_markdown(
            &self.drift_reports,
            self.recorded.as_ref(),
            &self.composite,
        )
    }
}
