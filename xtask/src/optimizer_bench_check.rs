//! Regression gate for the cross-method optimizer benchmark (roadmap §1.4).
//!
//! Runs `optimizer_method_bench` with the configuration recorded in the baseline file, reads the
//! `stability` records it emits, and compares them to committed per-method expectations.
//!
//! Three rules, all on search quality rather than speed — timings on shared CI runners move for
//! reasons that have nothing to do with the optimizer:
//!
//! 1. **Recall floor** — a method may not lose more than `recall_drop` of its mean reference
//!    top-K recall.
//! 2. **Regret ceiling** — a method's mean ranking-score regret vs the reference sweep may not
//!    grow by more than `score_regret_increase`.
//! 3. **Control ordering** — every search method must stay at least as good as the stratified
//!    random control, within `control_margin`. This is roadmap principle 2: a lane that cannot
//!    beat random is not earning its complexity.
//!
//! Score regret rather than win-rate regret is the primary signal because PvE win rates saturate:
//! in most matchups every legal crew wins or every legal crew loses, and only hull remaining and
//! round-1 kills separate them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Bumped when the baseline file's shape changes.
const BASELINE_SCHEMA_VERSION: u8 = 1;

/// The stratified random baseline every other lane is measured against.
const CONTROL_METHOD: &str = "random_stratified";

/// Bench invocation the baseline was measured with. Re-running with a different configuration
/// would compare numbers that were never comparable, so this travels with the baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchConfig {
    pub case: String,
    pub methods: Vec<String>,
    pub budget_mode: String,
    pub trial_budget: u64,
    pub seed_panel: Vec<u64>,
    pub reference_sims: usize,
    pub reference_max_crews: usize,
    pub recall_top_k: usize,
    #[serde(default)]
    pub prefilter_keep: Vec<usize>,
    pub profile: String,
}

impl BenchConfig {
    fn to_args(&self) -> Vec<String> {
        let mut args = vec![
            "--profile".into(),
            self.profile.clone(),
            "--case".into(),
            self.case.clone(),
            "--methods".into(),
            self.methods.join(","),
            "--budget-mode".into(),
            self.budget_mode.clone(),
            "--trial-budget".into(),
            self.trial_budget.to_string(),
            "--seed-panel".into(),
            self.seed_panel
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            "--reference-sweep".into(),
            "--reference-sims".into(),
            self.reference_sims.to_string(),
            "--reference-max-crews".into(),
            self.reference_max_crews.to_string(),
            "--recall-top-k".into(),
            self.recall_top_k.to_string(),
        ];
        if !self.prefilter_keep.is_empty() {
            args.push("--prefilter-keep".into());
            args.push(
                self.prefilter_keep
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        args
    }
}

/// How far a method may drift before the gate fails. Deliberately loose: these are Monte Carlo
/// measurements over a handful of seeds, and a gate that fires on noise gets ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tolerances {
    pub recall_drop: f64,
    pub score_regret_increase: f64,
    pub control_margin: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MethodBaseline {
    pub top_k_recall_mean: Option<f64>,
    pub score_regret_mean: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub schema_version: u8,
    pub note: String,
    pub config: BenchConfig,
    pub tolerances: Tolerances,
    pub methods: BTreeMap<String, MethodBaseline>,
}

/// One `stability` record from the bench output, reduced to the fields the gate reads.
#[derive(Debug, Clone, Deserialize)]
struct StabilityRow {
    record_kind: String,
    case: String,
    method: String,
    seeds: usize,
    top_k_recall_mean: Option<f64>,
    score_regret_mean: Option<f64>,
}

#[derive(Debug, Clone)]
struct Failure {
    method: String,
    rule: &'static str,
    detail: String,
}

pub struct RunArgs {
    pub repo: PathBuf,
    pub baseline: PathBuf,
    pub input: Option<PathBuf>,
    pub jsonl_out: Option<PathBuf>,
    pub write_baseline: bool,
    pub markdown_out: Option<PathBuf>,
}

pub fn run(args: RunArgs) -> Result<()> {
    let baseline = read_baseline(&args.baseline)?;
    let jsonl = match &args.input {
        Some(path) => std::fs::read_to_string(path)
            .with_context(|| format!("read bench output {}", path.display()))?,
        None => run_bench(&args.repo, &baseline.config)?,
    };
    if let Some(path) = &args.jsonl_out {
        std::fs::write(path, &jsonl)
            .with_context(|| format!("write bench output {}", path.display()))?;
        eprintln!("Wrote {}", path.display());
    }
    let observed = parse_stability(&jsonl, &baseline.config.case)?;
    if observed.is_empty() {
        bail!(
            "no stability records for case {:?} in the bench output",
            baseline.config.case
        );
    }

    if args.write_baseline {
        let updated = Baseline {
            methods: observed
                .iter()
                .map(|(method, row)| {
                    (
                        method.clone(),
                        MethodBaseline {
                            top_k_recall_mean: row.top_k_recall_mean,
                            score_regret_mean: row.score_regret_mean,
                        },
                    )
                })
                .collect(),
            ..baseline
        };
        let text = serde_json::to_string_pretty(&updated)? + "\n";
        std::fs::write(&args.baseline, text)
            .with_context(|| format!("write baseline {}", args.baseline.display()))?;
        eprintln!(
            "Wrote {} with {} method(s).",
            args.baseline.display(),
            updated.methods.len()
        );
        return Ok(());
    }

    let failures = compare(&baseline, &observed);
    let report = markdown_report(&baseline, &observed, &failures);
    print!("{report}");
    if let Some(path) = &args.markdown_out {
        std::fs::write(path, &report)
            .with_context(|| format!("write markdown {}", path.display()))?;
        eprintln!("Wrote {}", path.display());
    }
    if !failures.is_empty() {
        bail!(
            "optimizer method bench gate failed: {} rule violation(s)",
            failures.len()
        );
    }
    Ok(())
}

fn read_baseline(path: &Path) -> Result<Baseline> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read baseline {}", path.display()))?;
    let baseline: Baseline = serde_json::from_str(&text)
        .with_context(|| format!("parse baseline {}", path.display()))?;
    if baseline.schema_version != BASELINE_SCHEMA_VERSION {
        bail!(
            "baseline {} has schema_version {}, expected {}",
            path.display(),
            baseline.schema_version,
            BASELINE_SCHEMA_VERSION
        );
    }
    Ok(baseline)
}

fn run_bench(repo: &Path, config: &BenchConfig) -> Result<String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        "--release".into(),
        "--bin".into(),
        "optimizer_method_bench".into(),
        "--".into(),
    ];
    args.extend(config.to_args());
    eprintln!("+ cargo {}", args.join(" "));
    let output = Command::new("cargo")
        .args(&args)
        .current_dir(repo)
        .output()
        .context("failed to spawn `cargo run --bin optimizer_method_bench`")?;
    if !output.status.success() {
        bail!(
            "optimizer_method_bench exited with {:?}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_stability(jsonl: &str, case: &str) -> Result<BTreeMap<String, StabilityRow>> {
    let mut out = BTreeMap::new();
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        // Lane and reference records share the stream; only stability rows deserialize cleanly
        // into StabilityRow, and non-stability kinds are skipped by tag.
        let Ok(row) = serde_json::from_str::<StabilityRow>(line) else {
            continue;
        };
        if row.record_kind != "stability" || row.case != case {
            continue;
        }
        out.insert(row.method.clone(), row);
    }
    Ok(out)
}

fn compare(baseline: &Baseline, observed: &BTreeMap<String, StabilityRow>) -> Vec<Failure> {
    let mut failures = Vec::new();
    for (method, expected) in &baseline.methods {
        let Some(row) = observed.get(method) else {
            failures.push(Failure {
                method: method.clone(),
                rule: "missing",
                detail: "baseline expects this method but the run produced no stability record"
                    .to_string(),
            });
            continue;
        };
        if let (Some(expected_recall), Some(actual)) =
            (expected.top_k_recall_mean, row.top_k_recall_mean)
        {
            let floor = expected_recall - baseline.tolerances.recall_drop;
            if actual < floor {
                failures.push(Failure {
                    method: method.clone(),
                    rule: "recall floor",
                    detail: format!(
                        "recall {actual:.3} below floor {floor:.3} (baseline {expected_recall:.3})"
                    ),
                });
            }
        }
        if let (Some(expected_regret), Some(actual)) =
            (expected.score_regret_mean, row.score_regret_mean)
        {
            let ceiling = expected_regret + baseline.tolerances.score_regret_increase;
            if actual > ceiling {
                failures.push(Failure {
                    method: method.clone(),
                    rule: "regret ceiling",
                    detail: format!(
                        "score regret {actual:.5} above ceiling {ceiling:.5} (baseline {expected_regret:.5})"
                    ),
                });
            }
        }
    }
    if let Some(control) = observed
        .get(CONTROL_METHOD)
        .and_then(|r| r.score_regret_mean)
    {
        for (method, row) in observed {
            if method == CONTROL_METHOD {
                continue;
            }
            let Some(actual) = row.score_regret_mean else {
                continue;
            };
            let limit = control + baseline.tolerances.control_margin;
            if actual > limit {
                failures.push(Failure {
                    method: method.clone(),
                    rule: "control ordering",
                    detail: format!(
                        "score regret {actual:.5} worse than the random control {control:.5} (+{:.5} allowed)",
                        baseline.tolerances.control_margin
                    ),
                });
            }
        }
    }
    failures
}

fn fmt_opt(value: Option<f64>, places: usize) -> String {
    value.map_or_else(|| "—".to_string(), |v| format!("{v:.places$}"))
}

fn markdown_report(
    baseline: &Baseline,
    observed: &BTreeMap<String, StabilityRow>,
    failures: &[Failure],
) -> String {
    let mut s = String::new();
    s.push_str("## Optimizer method bench\n\n");
    s.push_str(&format!(
        "Case `{}`, {} mode, {} trials/lane, seeds {:?}, reference {} sims × ≤{} crews, recall@{}.\n\n",
        baseline.config.case,
        baseline.config.budget_mode,
        baseline.config.trial_budget,
        baseline.config.seed_panel,
        baseline.config.reference_sims,
        baseline.config.reference_max_crews,
        baseline.config.recall_top_k,
    ));
    s.push_str("| method | seeds | recall@K | baseline | score regret | baseline | |\n");
    s.push_str("|---|---:|---:|---:|---:|---:|:--:|\n");
    for (method, row) in observed {
        let expected = baseline.methods.get(method).cloned().unwrap_or_default();
        let ok = !failures.iter().any(|f| &f.method == method);
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} |\n",
            method,
            row.seeds,
            fmt_opt(row.top_k_recall_mean, 3),
            fmt_opt(expected.top_k_recall_mean, 3),
            fmt_opt(row.score_regret_mean, 5),
            fmt_opt(expected.score_regret_mean, 5),
            if ok { "✅" } else { "❌" },
        ));
    }
    s.push_str(&format!(
        "\nTolerances: recall may drop {:.3}, score regret may rise {:.5}, and every lane must stay within {:.5} of the `{}` control.\n",
        baseline.tolerances.recall_drop,
        baseline.tolerances.score_regret_increase,
        baseline.tolerances.control_margin,
        CONTROL_METHOD,
    ));
    if failures.is_empty() {
        s.push_str("\nAll rules passed.\n");
    } else {
        s.push_str("\n**Failures**\n\n");
        for f in failures {
            s.push_str(&format!("- `{}` — {}: {}\n", f.method, f.rule, f.detail));
        }
    }
    s.push_str("\n_Workflow: `optimizer-method-bench.yml`. Refresh with `cargo xtask optimizer-bench-check --write-baseline`._\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> Baseline {
        Baseline {
            schema_version: BASELINE_SCHEMA_VERSION,
            note: "test".into(),
            config: BenchConfig {
                case: "case_a".into(),
                methods: vec!["tiered".into(), CONTROL_METHOD.into()],
                budget_mode: "equal-trials".into(),
                trial_budget: 40_000,
                seed_panel: vec![7, 11],
                reference_sims: 400,
                reference_max_crews: 800,
                recall_top_k: 10,
                prefilter_keep: vec![],
                profile: "demo".into(),
            },
            tolerances: Tolerances {
                recall_drop: 0.2,
                score_regret_increase: 0.01,
                control_margin: 0.005,
            },
            methods: BTreeMap::from([(
                "tiered".to_string(),
                MethodBaseline {
                    top_k_recall_mean: Some(0.5),
                    score_regret_mean: Some(0.002),
                },
            )]),
        }
    }

    fn row(method: &str, recall: Option<f64>, regret: Option<f64>) -> StabilityRow {
        StabilityRow {
            record_kind: "stability".into(),
            case: "case_a".into(),
            method: method.into(),
            seeds: 2,
            top_k_recall_mean: recall,
            score_regret_mean: regret,
        }
    }

    #[test]
    fn matching_run_passes() {
        let observed = BTreeMap::from([
            ("tiered".to_string(), row("tiered", Some(0.5), Some(0.002))),
            (
                CONTROL_METHOD.to_string(),
                row(CONTROL_METHOD, Some(0.1), Some(0.010)),
            ),
        ]);
        assert!(compare(&baseline(), &observed).is_empty());
    }

    #[test]
    fn recall_within_tolerance_passes_but_below_it_fails() {
        let ok = BTreeMap::from([("tiered".to_string(), row("tiered", Some(0.31), Some(0.002)))]);
        assert!(compare(&baseline(), &ok).is_empty());
        let bad = BTreeMap::from([("tiered".to_string(), row("tiered", Some(0.29), Some(0.002)))]);
        let failures = compare(&baseline(), &bad);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].rule, "recall floor");
    }

    #[test]
    fn regret_growth_beyond_tolerance_fails() {
        let bad = BTreeMap::from([("tiered".to_string(), row("tiered", Some(0.5), Some(0.02)))]);
        let failures = compare(&baseline(), &bad);
        assert!(failures.iter().any(|f| f.rule == "regret ceiling"));
    }

    #[test]
    fn a_lane_worse_than_the_random_control_fails() {
        let observed = BTreeMap::from([
            ("tiered".to_string(), row("tiered", Some(0.5), Some(0.009))),
            (
                CONTROL_METHOD.to_string(),
                row(CONTROL_METHOD, Some(0.1), Some(0.001)),
            ),
        ]);
        let failures = compare(&baseline(), &observed);
        assert!(
            failures.iter().any(|f| f.rule == "control ordering"),
            "{failures:?}"
        );
    }

    #[test]
    fn a_method_the_baseline_expects_but_the_run_skipped_fails() {
        let failures = compare(&baseline(), &BTreeMap::new());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].rule, "missing");
    }

    #[test]
    fn parse_skips_lane_and_reference_records() {
        let jsonl = concat!(
            r#"{"record_kind":"lane","case":"case_a","method":"tiered","seed":7}"#,
            "\n",
            r#"{"record_kind":"reference","case":"case_a","seed":7}"#,
            "\n",
            r#"{"record_kind":"stability","case":"case_a","method":"tiered","seeds":2,"top_k_recall_mean":0.45,"score_regret_mean":0.003}"#,
            "\n",
            r#"{"record_kind":"stability","case":"other","method":"tiered","seeds":2,"top_k_recall_mean":0.9,"score_regret_mean":0.0}"#,
            "\n",
        );
        let parsed = parse_stability(jsonl, "case_a").expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["tiered"].top_k_recall_mean, Some(0.45));
    }
}
