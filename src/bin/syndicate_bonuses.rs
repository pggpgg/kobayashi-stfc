//! Print cumulative Syndicate bonuses up to a given Syndicate level, optionally filtered by operations level band.
//! Usage: cargo run --bin syndicate_bonuses -- <syndicate_level> [ops_level] [--combat-only]
//! Example: cargo run --bin syndicate_bonuses -- 42 51
//!   → bonuses at Syndicate 42 for the 51-60 ops band.
//! Example: cargo run --bin syndicate_bonuses -- 42 51 --combat-only
//!   → combat stat bonuses only (engine keys: weapon_damage, hull_hp, etc.).

use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let syndicate_level: u32 = args
        .first()
        .and_then(|s| s.parse().ok())
        .ok_or("Usage: syndicate_bonuses <syndicate_level> [ops_level] [--combat-only]. E.g. 42 51 --combat-only")?;
    let ops_level: Option<u32> = args.get(1).and_then(|s| s.parse().ok());
    let combat_only = args.iter().any(|a| a == "--combat-only");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let path = std::path::Path::new(&manifest_dir).join("data/syndicate_reputation.json");
    let path_str = path.to_string_lossy();
    let data = kobayashi::data::syndicate_reputation::load_syndicate_reputation(&path_str)
        .ok_or_else(|| format!("Load {} (run import_syndicate_reputation first)", path.display()))?;

    if combat_only {
        let ops = ops_level.ok_or("--combat-only requires ops_level (e.g. 42 51 --combat-only)")?;
        let combat = kobayashi::data::syndicate_combat::cumulative_combat_bonuses(
            &data,
            syndicate_level,
            ops,
        );
        let sorted: BTreeMap<_, _> = combat.into_iter().collect();
        println!(
            "Cumulative combat bonuses at Syndicate {} for ops level {} (band {})",
            syndicate_level,
            ops,
            kobayashi::data::syndicate_combat::ops_level_to_band(ops)
        );
        println!();
        for (key, value) in &sorted {
            let pct = if *value >= 0.0 && *value <= 2.0 && value.fract() != 0.0 {
                format!("{:.2}%", value * 100.0)
            } else {
                format!("{}", value)
            };
            println!("  {}: {}", key, pct);
        }
        println!("\nTotal combat entries: {}", sorted.len());
        return Ok(());
    }

    let band = ops_level.map(|o| kobayashi::data::syndicate_combat::ops_level_to_band(o).to_string());

    // Cumulative: sum all bonuses from level 1..=syndicate_level
    let mut cumulative: BTreeMap<String, f64> = BTreeMap::new();
    for entry in data.levels.iter().filter(|e| e.level >= 1 && e.level <= syndicate_level) {
        for b in &entry.bonuses {
            let stat = b.stat.as_str();
            if let Some(ref band_str) = band {
                if !stat.contains(band_str) {
                    continue;
                }
            }
            let v = cumulative.get(stat).copied().unwrap_or(0.0);
            let new_v = if b.operator.eq_ignore_ascii_case("multiply") {
                (1.0 + v) * (1.0 + b.value) - 1.0
            } else {
                v + b.value
            };
            cumulative.insert(b.stat.clone(), new_v);
        }
    }

    println!(
        "Cumulative Syndicate bonuses at level {} (levels 1..={})",
        syndicate_level, syndicate_level
    );
    if let Some(ops) = ops_level {
        let band_str = kobayashi::data::syndicate_combat::ops_level_to_band(ops);
        println!(
            "Filtered to ops band '{}' (ops level {}). Stats without a band (unlocks, shards, etc.) apply to all.",
            band_str, ops
        );
    }
    println!();

    for (stat, value) in &cumulative {
        let pct = if *value >= 0.0 && *value <= 2.0 && value.fract() != 0.0 {
            format!("{:.2}%", value * 100.0)
        } else {
            format!("{}", value)
        };
        println!("  {}: {}", stat, pct);
    }
    println!("\nTotal entries: {}", cumulative.len());
    Ok(())
}
