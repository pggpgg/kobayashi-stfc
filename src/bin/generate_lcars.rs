//! Thin CLI around [`kobayashi::lcars::build_officer_model`]: writes the in-process officer model
//! to `<output>/officers.lcars.yaml` for **debugging / inspection only**. The monolith is no longer
//! a runtime artifact — the server, optimizer, and tests build the model in-process via
//! `build_officer_model` / `build_officer_model_default`.
//!
//! Run: `cargo run --bin generate_lcars [-- path/to/officers.canonical.json] [--output dir]`
//!   `[--summary …] [--translations …] [--officer-data-dir …] [--no-ability-names]`

use std::fs;
use std::path::{Path, PathBuf};

use kobayashi::lcars::{build_officer_model, LcarsFile};
use kobayashi::logging;

const DEFAULT_INPUT: &str = "data/officers/officers.canonical.json";
const DEFAULT_OUTPUT_DIR: &str = "data/officers";
const DEFAULT_SUMMARY: &str = "data/upstream/data-stfc-space/summary-officer.json";
const DEFAULT_TRANSLATIONS: &str = "data/upstream/data-stfc-space/translations-officer_buffs.json";
const DEFAULT_OFFICER_DATA_DIR: &str = "data/upstream/data-stfc-space/officers";

fn abs_or_base(arg: &str, base: &Path) -> PathBuf {
    let p = Path::new(arg);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    logging::init();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base = Path::new(&manifest_dir);

    let args: Vec<String> = std::env::args().collect();
    let mut input_path = base.join(DEFAULT_INPUT);
    let mut output_dir = base.join(DEFAULT_OUTPUT_DIR);
    let mut summary_path = base.join(DEFAULT_SUMMARY);
    let mut translations_path = base.join(DEFAULT_TRANSLATIONS);
    let mut officer_data_dir = base.join(DEFAULT_OFFICER_DATA_DIR);
    let mut skip_names = false;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--output" && i + 1 < args.len() {
            output_dir = base.join(&args[i + 1]);
            i += 2;
        } else if args[i] == "--summary" && i + 1 < args.len() {
            summary_path = abs_or_base(&args[i + 1], base);
            i += 2;
        } else if args[i] == "--translations" && i + 1 < args.len() {
            translations_path = abs_or_base(&args[i + 1], base);
            i += 2;
        } else if args[i] == "--officer-data-dir" && i + 1 < args.len() {
            officer_data_dir = abs_or_base(&args[i + 1], base);
            i += 2;
        } else if args[i] == "--no-ability-names" {
            skip_names = true;
            i += 1;
        } else if !args[i].starts_with("--") {
            input_path = abs_or_base(&args[i], base);
            i += 1;
        } else {
            i += 1;
        }
    }

    let officers = build_officer_model(
        &input_path,
        &summary_path,
        &translations_path,
        &officer_data_dir,
        skip_names,
    )?;
    let count = officers.len();
    fs::create_dir_all(&output_dir)?;
    let out_path = output_dir.join("officers.lcars.yaml");
    let yaml = serde_yaml::to_string(&LcarsFile { officers })?;
    fs::write(&out_path, &yaml)?;
    println!("Wrote {} ({count} officers)", out_path.display());
    Ok(())
}
