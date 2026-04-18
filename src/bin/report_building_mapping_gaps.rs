//! Maintainer report: distinct opaque `buff_*` building bonus stats and conditions not in the
//! documented allowlist (`is_known_building_condition`).
//!
//! Run: `cargo run --bin report_building_mapping_gaps` [--buildings-dir path]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kobayashi::data::validate::is_known_building_condition;
use serde_json::Value;

#[derive(Default)]
struct Agg {
    count: usize,
    samples: Vec<String>,
}

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

    let index_path = dir.join("index.json");
    let raw = std::fs::read_to_string(&index_path).map_err(|e| {
        format!(
            "read {}: {e} (is --buildings-dir correct?)",
            index_path.display()
        )
    })?;
    let payload: Value = serde_json::from_str(&raw)?;
    let buildings = payload
        .get("buildings")
        .and_then(Value::as_array)
        .ok_or("index.json: missing 'buildings' array")?;

    let mut buffs: BTreeMap<String, Agg> = BTreeMap::new();
    let mut bad_conds: BTreeMap<String, Agg> = BTreeMap::new();

    for entry in buildings {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let Some(id) = obj
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let file_stem = obj
            .get("file")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(id);
        let path = dir.join(format!("{file_stem}.json"));
        let rec_raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skip id={id}: read {}: {e}", path.display());
                continue;
            }
        };
        let Ok(rec) = serde_json::from_str::<Value>(&rec_raw) else {
            eprintln!("skip id={id}: invalid JSON {}", path.display());
            continue;
        };
        let Some(levels) = rec.get("levels").and_then(Value::as_array) else {
            continue;
        };
        for level in levels {
            let bonuses = level
                .get("bonuses")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for bonus in bonuses {
                let Some(bo) = bonus.as_object() else {
                    continue;
                };
                if let Some(stat) = bo.get("stat").and_then(Value::as_str) {
                    if stat.starts_with("buff_") {
                        let e = buffs.entry(stat.to_string()).or_default();
                        e.count += 1;
                        if e.samples.len() < 4 && !e.samples.contains(&id.to_string()) {
                            e.samples.push(id.to_string());
                        }
                    }
                }
                if let Some(conds) = bo.get("conditions").and_then(Value::as_array) {
                    for c in conds {
                        if let Some(s) = c.as_str() {
                            if !is_known_building_condition(s) {
                                let e = bad_conds.entry(s.to_string()).or_default();
                                e.count += 1;
                                if e.samples.len() < 4 && !e.samples.contains(&id.to_string()) {
                                    e.samples.push(id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!("# Building bonus mapping gaps\n");
    println!("Directory: `{}`\n", dir.display());
    println!("## Opaque `buff_*` stats\n");
    println!(
        "These keys are not merged into the player combat profile (see `merge_building_bonuses_into_profile` / `normalize_profile_combat_stat` in `src/data/profile.rs`).\n"
    );
    if buffs.is_empty() {
        println!("None.\n");
    } else {
        println!("| Stat | Bonus rows | Sample building ids |");
        println!("| --- | ---: | --- |");
        for (k, v) in &buffs {
            let samples = v.samples.join(", ");
            println!("| `{k}` | {} | {samples} |", v.count);
        }
        println!();
    }

    println!("## Conditions not in `is_known_building_condition`\n");
    if bad_conds.is_empty() {
        println!("None.\n");
    } else {
        println!("| Condition | Occurrences | Sample building ids |");
        println!("| --- | ---: | --- |");
        for (k, v) in &bad_conds {
            let samples = v.samples.join(", ");
            println!("| `{k}` | {} | {samples} |", v.count);
        }
        println!();
    }

    Ok(())
}
