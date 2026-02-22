//! Validate data registry: check that each referenced path exists and is loadable.
//! Run: cargo run --bin validate_data

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let data_root = Path::new(&manifest_dir).join("data");
    let registry_path = data_root.join("registry.json");

    if !registry_path.exists() {
        eprintln!("Registry not found: {}", registry_path.display());
        eprintln!("Run the normalizer first: cargo run --bin normalize_stfc_data");
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&registry_path)?;
    let registry: kobayashi::data::registry::Registry = serde_json::from_str(&content)?;

    let mut ok = 0;
    let mut err = 0;
    for (name, entry) in &registry {
        let path = data_root.join(&entry.path);
        if !path.exists() {
            eprintln!("[{}] path missing: {}", name, path.display());
            err += 1;
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[{}] read failed: {} - {}", name, path.display(), e);
                err += 1;
                continue;
            }
        };
        let _: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[{}] invalid JSON: {} - {}", name, path.display(), e);
                err += 1;
                continue;
            }
        };
        ok += 1;
    }

    println!("Validated {} datasets, {} ok, {} errors", registry.len(), ok, err);
    if err > 0 {
        std::process::exit(1);
    }
    Ok(())
}
