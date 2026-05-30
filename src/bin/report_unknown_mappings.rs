//! Maintainer report: canonical officer `conditions` tokens not resolved for the officer LCARS
//! pipeline (`kobayashi::lcars::is_canonical_officer_condition_resolved`), and
//! hostile `upstream_ship_type` values from `data/hostiles/index.json`.
//!
//! Run: `cargo run --bin report_unknown_mappings`
//!   [--canonical path/to/officers.canonical.json] [--hostile-index path/to/index.json] [--output path.md]

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use kobayashi::data::mapping_gap_report::{
    format_unknown_mappings_markdown, load_opaque_buff_allowlist, run_research_mapping_gaps_scan,
    scan_building_bonus_gaps, scan_canonical_officer_conditions, scan_forbidden_tech_bonus_gaps,
    scan_hostile_index_upstream_ship_types, DEFAULT_FORBIDDEN_CHAOS_CATALOG_PATH,
    DEFAULT_OPAQUE_BUFF_ALLOWLIST_PATH,
};

const DEFAULT_CANONICAL: &str = "data/officers/officers.canonical.json";
const DEFAULT_HOSTILE_INDEX: &str = "data/hostiles/index.json";
const DEFAULT_BUILDINGS_DIR: &str = "data/buildings";

struct Args {
    canonical: PathBuf,
    hostile_index: PathBuf,
    output: Option<PathBuf>,
}

fn parse_args(base: &Path, args: &[String]) -> Args {
    let mut canonical = base.join(DEFAULT_CANONICAL);
    let mut hostile_index = base.join(DEFAULT_HOSTILE_INDEX);
    let mut output: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--canonical" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    canonical = PathBuf::from(p);
                }
            }
            "--hostile-index" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    hostile_index = PathBuf::from(p);
                }
            }
            "--output" | "-o" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    output = Some(PathBuf::from(p));
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: report_unknown_mappings [--canonical PATH] [--hostile-index PATH] [--output PATH]"
                );
                eprintln!("Defaults (relative to crate root): {DEFAULT_CANONICAL}, {DEFAULT_HOSTILE_INDEX}");
                eprintln!("With no --output, Markdown is written to stdout.");
                std::process::exit(0);
            }
            other => {
                eprintln!("report_unknown_mappings: unknown argument {other:?} (try --help)");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    Args {
        canonical,
        hostile_index,
        output,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base = Path::new(&manifest_dir);
    let args: Vec<String> = std::env::args().collect();
    let cfg = parse_args(base, &args);

    let token_map = scan_canonical_officer_conditions(&cfg.canonical)
        .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;
    let ship_map = scan_hostile_index_upstream_ship_types(&cfg.hostile_index)
        .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;
    let research_gaps = run_research_mapping_gaps_scan(base).ok();
    if research_gaps.is_none() {
        eprintln!(
            "report_unknown_mappings: research mapping gaps scan skipped (node script failed or upstream cache missing)"
        );
    }

    let buildings_dir = base.join(DEFAULT_BUILDINGS_DIR);
    let building_allowlist_path = base.join(DEFAULT_OPAQUE_BUFF_ALLOWLIST_PATH);
    let building_allowlist = load_opaque_buff_allowlist(&building_allowlist_path);
    let building_gaps = scan_building_bonus_gaps(&buildings_dir).ok();
    if building_gaps.is_none() {
        eprintln!(
            "report_unknown_mappings: building bonus gaps scan skipped (missing or invalid {})",
            buildings_dir.display()
        );
    }

    let forbidden_catalog = base.join(DEFAULT_FORBIDDEN_CHAOS_CATALOG_PATH);
    let forbidden_tech_gaps = scan_forbidden_tech_bonus_gaps(&forbidden_catalog).ok();
    if forbidden_tech_gaps.is_none() {
        eprintln!(
            "report_unknown_mappings: forbidden-tech bonus gaps scan skipped (missing {})",
            forbidden_catalog.display()
        );
    }

    let md = format_unknown_mappings_markdown(
        &cfg.canonical,
        &cfg.hostile_index,
        &token_map,
        &ship_map,
        research_gaps.as_ref(),
        building_gaps.as_ref(),
        Some(buildings_dir.as_path()),
        Some(&building_allowlist),
        forbidden_tech_gaps.as_ref(),
    );

    if let Some(out_path) = cfg.output {
        let mut f = fs::File::create(&out_path)?;
        f.write_all(md.as_bytes())?;
        eprintln!("Wrote {}", out_path.display());
    } else {
        print!("{md}");
    }

    Ok(())
}
