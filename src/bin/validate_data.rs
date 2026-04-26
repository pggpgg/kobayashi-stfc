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
use std::path::PathBuf;

use clap::Parser;
use kobayashi::data::validate::{
    full_validation_report_to_json, full_validation_report_to_markdown,
    validate_all_data_for_report, ValidationSeverity,
};

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

    if report.has_errors() {
        eprintln!(
            "validate_data: failing due to one or more errors (see JSON/Markdown or stdout above)."
        );
        std::process::exit(1);
    }
}
