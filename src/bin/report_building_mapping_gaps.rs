//! Maintainer report: distinct opaque `buff_*` building bonus stats and conditions not in the
//! documented allowlist (`is_known_building_condition`).
//!
//! Run: `cargo run --bin report_building_mapping_gaps` [--buildings-dir path]
//!
//! Output is Markdown on stdout (also emitted as per-row diagnostics by `validate_data`; see
//! [`kobayashi::data::mapping_gap_report::scan_building_bonus_gaps`] for the shared scan).

use std::path::{Path, PathBuf};

use kobayashi::data::mapping_gap_report::{
    format_building_bonus_gaps_markdown, load_opaque_buff_allowlist, scan_building_bonus_gaps,
    DEFAULT_OPAQUE_BUFF_ALLOWLIST_PATH,
};

fn usage() -> ! {
    eprintln!(
        "Usage: report_building_mapping_gaps [--buildings-dir PATH]\n\
         Default PATH: data/buildings (relative to crate root)."
    );
    std::process::exit(2);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let mut dir = Path::new(&manifest_dir).join("data/buildings");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => usage(),
            "--buildings-dir" => {
                i += 1;
                let Some(p) = args.get(i) else {
                    usage();
                };
                dir = PathBuf::from(p);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                usage();
            }
        }
        i += 1;
    }

    let report = scan_building_bonus_gaps(&dir)
        .map_err(|e| format!("scan {} (is --buildings-dir correct?): {e}", dir.display()))?;
    let allowlist_path = Path::new(&manifest_dir).join(DEFAULT_OPAQUE_BUFF_ALLOWLIST_PATH);
    let allowlist = load_opaque_buff_allowlist(&allowlist_path);
    print!(
        "{}",
        format_building_bonus_gaps_markdown(&report, &dir, Some(&allowlist))
    );

    Ok(())
}
