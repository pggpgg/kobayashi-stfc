//! Report stfc.cc cheat-sheet → [`kobayashi::data::combat_effect_spec::CombatEffectSpec`] mapping coverage.
//!
//! Usage: `stfc_cc_cheat_sheet_report [--json] [path/to/raw-officers-m88-17rc.csv]`
//! Default path: `data/upstream/cheat-sheet/raw-officers-m88-17rc.csv` (run from repo root).

use std::env;
use std::fs::File;
use std::path::Path;

use kobayashi::data::stfc_cc_effect_spec_adapter::scan_stfc_cc_cheat_sheet_csv;
use serde::Serialize;

#[derive(Serialize)]
struct ReportJson<'a> {
    file: &'a str,
    rows_total: usize,
    rows_full_convert: usize,
    full_coverage: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    top_diagnostics: Vec<DiagnosticJson>,
}

#[derive(Serialize)]
struct DiagnosticJson {
    count: usize,
    message: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut json_out = false;
    let mut path: Option<String> = None;
    for a in env::args().skip(1) {
        if a == "--json" {
            json_out = true;
        } else if !a.starts_with('-') && path.is_none() {
            path = Some(a);
        }
    }
    let path = path.unwrap_or_else(|| "data/upstream/cheat-sheet/raw-officers-m88-17rc.csv".to_string());
    let f = File::open(&path)?;
    let summary = scan_stfc_cc_cheat_sheet_csv(f)?;
    let full_coverage = summary.rows_full_convert == summary.rows_total;
    let top: Vec<DiagnosticJson> = summary
        .top_diagnostics(40)
        .into_iter()
        .map(|(message, count)| DiagnosticJson { count, message })
        .collect();

    if json_out {
        let report = ReportJson {
            file: &path,
            rows_total: summary.rows_total,
            rows_full_convert: summary.rows_full_convert,
            full_coverage,
            top_diagnostics: top,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("file: {}", Path::new(&path).display());
    println!("rows_total: {}", summary.rows_total);
    println!("rows_full_convert: {}", summary.rows_full_convert);
    if full_coverage {
        println!("coverage: full (all rows converted to CombatEffectSpec)");
    } else {
        println!(
            "coverage: partial ({}/{})",
            summary.rows_full_convert, summary.rows_total
        );
    }
    println!("top_diagnostics:");
    if top.is_empty() {
        println!("  (none)");
    } else {
        for d in &top {
            println!("  {:5}  {}", d.count, d.message);
        }
    }
    Ok(())
}
