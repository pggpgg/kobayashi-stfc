//! Report stfc.cc cheat-sheet → [`kobayashi::data::combat_effect_spec::CombatEffectSpec`] mapping coverage.
//!
//! Usage: `stfc_cc_cheat_sheet_report [path/to/raw-officers-m88-17rc.csv]`
//! Default path: `data/upstream/cheat-sheet/raw-officers-m88-17rc.csv` (run from repo root).

use std::env;
use std::fs::File;
use std::path::Path;

use kobayashi::data::stfc_cc_effect_spec_adapter::scan_stfc_cc_cheat_sheet_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "data/upstream/cheat-sheet/raw-officers-m88-17rc.csv".to_string());
    let f = File::open(&path)?;
    let summary = scan_stfc_cc_cheat_sheet_csv(f)?;
    println!("file: {}", Path::new(&path).display());
    println!("rows_total: {}", summary.rows_total);
    println!("rows_full_convert: {}", summary.rows_full_convert);
    println!("top_diagnostics:");
    for (msg, count) in summary.top_diagnostics(40) {
        println!("  {:5}  {}", count, msg);
    }
    Ok(())
}
