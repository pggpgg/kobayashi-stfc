//! One-off: merge all *.lcars.yaml in data/officers into a single officers.lcars.yaml.
//! Run from project root: cargo run --bin merge_lcars [-- data/officers]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use kobayashi::lcars::{load_lcars_dir, LcarsFile, LcarsOfficer};

const DEFAULT_OFFICERS_DIR: &str = "data/officers";
const OUTPUT_FILE: &str = "officers.lcars.yaml";

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_OFFICERS_DIR.to_string());
    let dir = Path::new(&dir);

    let officers = load_lcars_dir(dir)?;
    // Deduplicate by id: first occurrence wins (stable order).
    let mut by_id: HashMap<String, LcarsOfficer> = HashMap::new();
    for o in officers {
        by_id.entry(o.id.clone()).or_insert(o);
    }
    let merged: Vec<LcarsOfficer> = by_id.into_values().collect();

    let out_path = dir.join(OUTPUT_FILE);
    let file = LcarsFile { officers: merged };
    let yaml = serde_yaml::to_string(&file)?;
    fs::write(&out_path, yaml)?;
    println!("Wrote {} ({} officers)", out_path.display(), file.officers.len());

    // Remove other .yaml/.yml files in the directory (keep only officers.lcars.yaml).
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension();
            if ext.map_or(false, |e| e == "yaml" || e == "yml") {
                if path.file_name().map_or(true, |n| n != OUTPUT_FILE) {
                    fs::remove_file(&path)?;
                    println!("Removed {}", path.display());
                }
            }
        }
    }

    Ok(())
}
