//! Compare Criterion `target/criterion/**/new/estimates.json` medians to `benchmark_results.log`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_HEADER: &str = "# schema=1";

#[derive(Deserialize)]
struct EstimatesFile {
    median: PointEstimate,
}

#[derive(Deserialize)]
struct PointEstimate {
    point_estimate: f64,
}

/// Collect `bench_id -> median_ns` from Criterion output (`…/<bench_id>/new/estimates.json`).
pub fn collect_criterion_medians(criterion_root: &Path) -> Result<BTreeMap<String, u64>> {
    if !criterion_root.is_dir() {
        bail!(
            "Criterion directory missing: {} (run `cargo bench` first)",
            criterion_root.display()
        );
    }
    let mut out = BTreeMap::new();
    collect_recursive(criterion_root, criterion_root, &mut out)?;
    if out.is_empty() {
        bail!(
            "no `new/estimates.json` under {}; run `cargo bench --bench simulator --bench monte_carlo_parallel`",
            criterion_root.display()
        );
    }
    Ok(out)
}

fn collect_recursive(root: &Path, dir: &Path, out: &mut BTreeMap<String, u64>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("new") {
                let estimates = path.join("estimates.json");
                if estimates.is_file() {
                    let rel = path
                        .parent()
                        .and_then(|p| p.strip_prefix(root).ok())
                        .context("strip criterion root")?;
                    let id = rel.to_string_lossy().replace('\\', "/");
                    let text = fs::read_to_string(&estimates)
                        .with_context(|| format!("read {}", estimates.display()))?;
                    let parsed: EstimatesFile = serde_json::from_str(&text)
                        .with_context(|| format!("parse {}", estimates.display()))?;
                    let ns = parsed.median.point_estimate.round().max(0.0) as u64;
                    out.insert(id, ns);
                }
            } else {
                collect_recursive(root, &path, out)?;
            }
        }
    }
    Ok(())
}

fn parse_baseline_log(text: &str) -> Result<BTreeMap<String, u64>> {
    let mut m = BTreeMap::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            bail!(
                "benchmark_results.log:{}: expected `<bench_id> <median_ns>`, got {:?}",
                lineno + 1,
                line
            );
        }
        let ns_str = parts.pop().unwrap();
        let ns: u64 = ns_str
            .parse()
            .with_context(|| format!("benchmark_results.log:{}: bad median_ns {:?}", lineno + 1, ns_str))?;
        let id = parts.join(" ");
        if m.insert(id.clone(), ns).is_some() {
            bail!("benchmark_results.log: duplicate bench_id {:?}", id);
        }
    }
    if m.is_empty() {
        bail!("benchmark_results.log: no data rows");
    }
    Ok(m)
}

pub fn write_baseline_file(path: &Path, medians: &BTreeMap<String, u64>, note: &str) -> Result<()> {
    let mut buf = String::new();
    buf.push_str(SCHEMA_HEADER);
    buf.push('\n');
    buf.push_str("# Kobayashi Criterion regression baseline: median time (ns) per benchmark id.\n");
    buf.push_str("# Refresh: see docs/PERFORMANCE.md § Regression gate.\n");
    if !note.is_empty() {
        buf.push_str("# ");
        buf.push_str(note);
        buf.push('\n');
    }
    for (id, ns) in medians {
        buf.push_str(id);
        buf.push(' ');
        buf.push_str(&ns.to_string());
        buf.push('\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    fs::write(path, buf).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub struct CompareOutcome {
    pub ok: bool,
    pub rows: Vec<CompareRow>,
}

pub struct CompareRow {
    pub bench_id: String,
    pub baseline_ns: u64,
    pub current_ns: u64,
    /// (current - baseline) / baseline; None if not comparable
    pub delta_pct: Option<f64>,
    pub status: RowStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RowStatus {
    Ok,
    Regressed,
    /// In log but no Criterion output for this id
    MissingFromRun,
    /// In Criterion output but not in log
    MissingFromLog,
}

/// Fail if any current median is strictly worse than `baseline * (1 + regression_fraction)`.
pub fn compare(
    baseline: &BTreeMap<String, u64>,
    current: &BTreeMap<String, u64>,
    regression_fraction: f64,
) -> CompareOutcome {
    let b_keys: BTreeSet<_> = baseline.keys().cloned().collect();
    let c_keys: BTreeSet<_> = current.keys().cloned().collect();
    let mut rows = Vec::new();
    let mut ok = b_keys == c_keys;

    for id in b_keys.symmetric_difference(&c_keys) {
        let in_b = baseline.contains_key(id);
        let status = if in_b {
            ok = false;
            RowStatus::MissingFromRun
        } else {
            ok = false;
            RowStatus::MissingFromLog
        };
        rows.push(CompareRow {
            bench_id: id.clone(),
            baseline_ns: *baseline.get(id).unwrap_or(&0),
            current_ns: *current.get(id).unwrap_or(&0),
            delta_pct: None,
            status,
        });
    }

    for id in b_keys.intersection(&c_keys) {
        let b = baseline[id];
        let c = current[id];
        let delta_pct = if b > 0 {
            Some((c as f64 - b as f64) / b as f64 * 100.0)
        } else {
            None
        };
        let threshold = b as f64 * (1.0 + regression_fraction);
        let regressed = b > 0 && c as f64 > threshold + f64::EPSILON;
        let status = if regressed {
            ok = false;
            RowStatus::Regressed
        } else {
            RowStatus::Ok
        };
        rows.push(CompareRow {
            bench_id: id.clone(),
            baseline_ns: b,
            current_ns: c,
            delta_pct,
            status,
        });
    }

    rows.sort_by(|a, b| a.bench_id.cmp(&b.bench_id));
    CompareOutcome { ok, rows }
}

fn status_label(s: RowStatus) -> &'static str {
    match s {
        RowStatus::Ok => "ok",
        RowStatus::Regressed => "FAIL",
        RowStatus::MissingFromRun => "MISSING_RUN",
        RowStatus::MissingFromLog => "MISSING_LOG",
    }
}

pub fn print_text_table(outcome: &CompareOutcome) {
    eprintln!(
        "{:<45} {:>12} {:>12} {:>10} {}",
        "benchmark", "baseline_ns", "current_ns", "delta_%", "status"
    );
    for r in &outcome.rows {
        let delta = r
            .delta_pct
            .map(|d| format!("{d:+.1}"))
            .unwrap_or_else(|| "n/a".into());
        eprintln!(
            "{:<45} {:>12} {:>12} {:>10} {}",
            r.bench_id,
            r.baseline_ns,
            r.current_ns,
            delta,
            status_label(r.status)
        );
    }
}

pub fn markdown_summary(outcome: &CompareOutcome, regression_pct: f64) -> String {
    let title = if outcome.ok {
        "### Benchmark regression gate: passed"
    } else {
        "### Benchmark regression gate: failed"
    };
    let mut s = String::new();
    s.push_str(title);
    s.push_str(&format!(
        "\n\nCompared medians (ns) to `benchmark_results.log`; regression if current > baseline × {:.2}.\n\n",
        1.0 + regression_pct
    ));
    s.push_str("| benchmark | baseline (ns) | current (ns) | Δ% | |\n");
    s.push_str("| --- | ---: | ---: | ---: | --- |\n");
    for r in &outcome.rows {
        let delta = r
            .delta_pct
            .map(|d| format!("{d:+.1}"))
            .unwrap_or_else(|| "—".into());
        let mark = status_label(r.status);
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            r.bench_id, r.baseline_ns, r.current_ns, delta, mark
        ));
    }
    s.push_str("\n_Workflow: `benchmark-regression.yml`._\n");
    s
}

pub fn load_baseline(path: &Path) -> Result<BTreeMap<String, u64>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if !text.lines().any(|l| l.trim() == SCHEMA_HEADER) {
        eprintln!("warning: {} missing `{}` header", path.display(), SCHEMA_HEADER);
    }
    parse_baseline_log(&text)
}

pub fn run(
    repo: &Path,
    baseline_path: PathBuf,
    criterion_dir: Option<PathBuf>,
    write_baseline: bool,
    markdown_out: Option<PathBuf>,
) -> Result<()> {
    let criterion_root = criterion_dir.unwrap_or_else(|| repo.join("target/criterion"));
    let regression_fraction = 0.10_f64;

    let current = collect_criterion_medians(&criterion_root)?;

    if write_baseline {
        let note = format!(
            "generated {}",
            chrono_nowish()
        );
        write_baseline_file(&baseline_path, &current, &note)?;
        eprintln!(
            "Wrote {} with {} benchmarks.",
            baseline_path.display(),
            current.len()
        );
        return Ok(());
    }

    let baseline = load_baseline(&baseline_path)?;
    let outcome = compare(&baseline, &current, regression_fraction);
    print_text_table(&outcome);

    if let Some(md_path) = markdown_out {
        let md = markdown_summary(&outcome, regression_fraction);
        fs::write(&md_path, md).with_context(|| format!("write {}", md_path.display()))?;
        eprintln!("Wrote {}", md_path.display());
    }

    if !outcome.ok {
        bail!(
            "benchmark regression or benchmark set mismatch vs {} (see table above)",
            baseline_path.display()
        );
    }
    Ok(())
}

fn chrono_nowish() -> String {
    use std::time::SystemTime;
    let t = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix_{t}")
}
