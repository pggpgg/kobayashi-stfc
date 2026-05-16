//! Validate committed `data/` artifacts: registry paths, officers (canonical + LCARS), ships,
//! hostiles, buildings, forbidden/chaos catalog, and unmapped canonical condition tokens.
//!
//! Run from repo root: `cargo run --bin validate_data`
//!
//! Exit code **1** if any diagnostic has severity `error`. Warnings do not fail the process.
//!
//! ## Strict mode
//!
//! Pass `--strict` to upgrade mapping-coverage warnings to errors. Today this covers:
//! - Building bonus mapping gaps (opaque `buff_*` stats and unknown `conditions` tokens), via
//!   `KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS=1` (see `validate_buildings_dataset`).
//! - Unmapped canonical officer `conditions` tokens, via
//!   `KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS=1` (see `validate_unmapped_canonical_officer_conditions`).
//!
//! Strict mode therefore causes exit code `1` until the relevant catalog or mapping table is
//! extended; use it as an opt-in gate while iterating on coverage.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use kobayashi::data::validate::{
    full_validation_report_to_json, full_validation_report_to_markdown,
    validate_all_data_for_report, ValidationSeverity,
};
use kobayashi::lcars::{collect_lcars_drops, LcarsDropReport};

#[derive(Parser, Debug)]
#[command(name = "validate_data")]
struct Args {
    /// Emit JSON (pretty) for CI / tooling.
    #[arg(long, value_name = "PATH")]
    json_out: Option<PathBuf>,

    /// Emit Markdown tables for human triage.
    #[arg(long, value_name = "PATH")]
    markdown_out: Option<PathBuf>,

    /// Which machine-readable formats to write when output paths are set (default: both if both paths given).
    #[arg(long, value_enum, default_value = "both")]
    format: ReportFormat,

    /// Crate / repo root containing `data/` (default: `CARGO_MANIFEST_DIR`).
    #[arg(long)]
    manifest_dir: Option<PathBuf>,

    /// Promote mapping-coverage warnings (building bonus gaps, unmapped canonical conditions)
    /// to errors. Sets `KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS=1` and
    /// `KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS=1` for this process.
    #[arg(long)]
    strict: bool,

    /// Generate an LCARS coverage report (per-category drop counts + top reasons + top officers)
    /// from `data/officers/officers.lcars.yaml` and write it to `coverage_report.json` /
    /// `coverage_report.md` in the manifest dir. Respects `--format`.
    #[arg(long)]
    coverage: bool,
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
enum ReportFormat {
    #[default]
    Both,
    Json,
    Markdown,
}

fn main() {
    let args = Args::parse();
    let manifest_dir = args
        .manifest_dir
        .clone()
        .or_else(|| std::env::var("CARGO_MANIFEST_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));

    if args.strict {
        // Set before `validate_all_data_for_report` so the per-category validators observe the
        // strict env vars. Both validators read these via their own helpers.
        std::env::set_var("KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS", "1");
        std::env::set_var("KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS", "1");
    }

    let report = validate_all_data_for_report(&manifest_dir);

    let write_json = matches!(args.format, ReportFormat::Both | ReportFormat::Json);
    let write_md = matches!(args.format, ReportFormat::Both | ReportFormat::Markdown);

    if let Some(path) = &args.json_out {
        if write_json {
            let json = full_validation_report_to_json(&report).unwrap_or_else(|e| {
                eprintln!("validate_data: JSON serialize failed: {e}");
                std::process::exit(2);
            });
            if let Err(e) = fs::write(path, json.as_bytes()) {
                eprintln!("validate_data: write {}: {e}", path.display());
                std::process::exit(2);
            }
            eprintln!("Wrote {}", path.display());
        }
    }

    if let Some(path) = &args.markdown_out {
        if write_md {
            let md = full_validation_report_to_markdown(&report);
            if let Err(e) = fs::write(path, md.as_bytes()) {
                eprintln!("validate_data: write {}: {e}", path.display());
                std::process::exit(2);
            }
            eprintln!("Wrote {}", path.display());
        }
    }

    println!(
        "Data validation: {} error(s), {} warning(s), {} info (manifest {})",
        report.summary.errors, report.summary.warnings, report.summary.infos, report.manifest_dir
    );

    for cat in &report.categories {
        let errs = cat
            .diagnostics
            .iter()
            .filter(|d| d.severity == ValidationSeverity::Error)
            .count();
        let warns = cat
            .diagnostics
            .iter()
            .filter(|d| d.severity == ValidationSeverity::Warning)
            .count();
        if errs == 0 && warns == 0 {
            println!("  {}: ok", cat.name);
        } else {
            println!("  {}: {} error(s), {} warning(s)", cat.name, errs, warns);
        }
    }

    if args.coverage {
        if let Err(e) = write_lcars_coverage(&manifest_dir, write_json, write_md) {
            eprintln!("validate_data: coverage report failed: {e}");
            std::process::exit(2);
        }
    }

    if report.has_errors() {
        eprintln!(
            "validate_data: failing due to one or more errors (see JSON/Markdown or stdout above)."
        );
        std::process::exit(1);
    }
}

fn write_lcars_coverage(
    manifest_dir: &Path,
    write_json: bool,
    write_md: bool,
) -> Result<(), String> {
    let yaml_path = manifest_dir.join("data/officers/officers.lcars.yaml");
    let file = kobayashi::lcars::load_lcars_file(&yaml_path)
        .map_err(|e| format!("load {}: {e}", yaml_path.display()))?;
    let officer_count = file.officers.len();
    let drops = collect_lcars_drops(&file.officers);

    println!(
        "LCARS coverage: {} drop(s) across {} officer(s) ({} categories)",
        drops.len(),
        drops.officers_by_count().len(),
        drops.category_counts().len(),
    );

    if write_json {
        let path = manifest_dir.join("coverage_report.json");
        let payload = serde_json::json!({
            "summary": {
                "officers_scanned": officer_count,
                "total_drops": drops.len(),
                "officers_with_drops": drops.officers_by_count().len(),
                "categories": drops.category_counts().len(),
            },
            "by_category": drops.category_counts().into_iter().map(|(cat, count, officers)| {
                serde_json::json!({"category": cat, "count": count, "distinct_officers": officers})
            }).collect::<Vec<_>>(),
            "by_reason": drops.reasons_with_officer_samples().into_iter().map(|(reason, count, distinct, samples)| {
                serde_json::json!({
                    "reason": reason,
                    "count": count,
                    "distinct_officers": distinct,
                    "sample_officer_ids": samples,
                })
            }).collect::<Vec<_>>(),
            "by_officer": drops.officers_by_count().into_iter().map(|(officer, count, top)| {
                serde_json::json!({"officer_id": officer, "count": count, "top_reason": top})
            }).collect::<Vec<_>>(),
            "drops": drops.drops,
        });
        let json = serde_json::to_string_pretty(&payload).map_err(|e| format!("serialize: {e}"))?;
        fs::write(&path, json.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
        eprintln!("Wrote {}", path.display());
    }

    if write_md {
        let path = manifest_dir.join("coverage_report.md");
        let md = format_coverage_markdown(officer_count, &drops);
        fs::write(&path, md.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
        eprintln!("Wrote {}", path.display());
    }
    Ok(())
}

fn format_coverage_markdown(officer_count: usize, drops: &LcarsDropReport) -> String {
    let mut out = String::new();
    out.push_str("# LCARS Coverage Report\n\n");
    out.push_str(&format!("- Officers scanned: **{officer_count}**\n"));
    out.push_str(&format!("- Total drops: **{}**\n", drops.len()));
    out.push_str(&format!(
        "- Officers with drops: **{}**\n",
        drops.officers_by_count().len()
    ));
    out.push_str(&format!(
        "- Drop categories: **{}**\n\n",
        drops.category_counts().len()
    ));
    out.push_str("Drops are LCARS effects the YAML→IR adapter silently skipped at load time. ");
    out.push_str("Regenerate via `cargo run --bin validate_data -- --coverage`.\n\n");

    out.push_str("## By category\n\n");
    out.push_str("| Category | Count | Distinct officers |\n");
    out.push_str("|---|---:|---:|\n");
    for (cat, count, officers) in drops.category_counts() {
        out.push_str(&format!("| `{cat}` | {count} | {officers} |\n"));
    }
    out.push('\n');

    out.push_str("## Top reasons\n\n");
    out.push_str("| # | Reason | Count | Distinct officers | Sample officer ids |\n");
    out.push_str("|---:|---|---:|---:|---|\n");
    for (i, (reason, count, distinct, samples)) in drops
        .reasons_with_officer_samples()
        .iter()
        .take(30)
        .enumerate()
    {
        let sample_cells = samples
            .iter()
            .map(|s| format!("`{s}`"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "| {} | `{reason}` | {count} | {distinct} | {sample_cells} |\n",
            i + 1
        ));
    }
    out.push('\n');

    out.push_str("## Top officers by drop count\n\n");
    out.push_str("| # | Officer | Drops | Top reason |\n");
    out.push_str("|---:|---|---:|---|\n");
    for (i, (officer, count, top)) in drops.officers_by_count().iter().take(30).enumerate() {
        out.push_str(&format!(
            "| {} | `{officer}` | {count} | `{top}` |\n",
            i + 1
        ));
    }
    out.push('\n');

    out
}
