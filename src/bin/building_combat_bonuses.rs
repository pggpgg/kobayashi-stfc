//! Print combined combat stat bonuses if all buildings were at max level.
//! Usage: run from project root: cargo run --bin building_combat_bonuses
//! Output: combat stat keys and total additive bonus (e.g. weapon_damage: 0.80 = +80%).

use std::collections::HashMap;
use std::path::Path;

const COMBAT_KEYS: &[&str] = &[
    "weapon_damage",
    "hull_hp",
    "shield_hp",
    "crit_chance",
    "crit_damage",
    "pierce",
    "shield_mitigation",
];

/// Try loading a building record; use alternate filename for known prefixed files.
fn load_building_record_any(data_dir: &Path, id: &str) -> Option<kobayashi::data::building::BuildingRecord> {
    kobayashi::data::building::load_building_record(data_dir, id).or_else(|| {
        let alt = match id {
            "ops_center" => "00_ops_center",
            "parsteel_generator_a" => "01_parsteel_generator_a",
            _ => return None,
        };
        kobayashi::data::building::load_building_record(data_dir, alt)
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let data_dir = Path::new(&manifest_dir).join("data/buildings");
    let index_path = data_dir.join("index.json");
    let index = kobayashi::data::building::load_building_index(index_path.to_str().unwrap())
        .ok_or("Failed to load building index (data/buildings/index.json)")?;

    let mut records: Vec<kobayashi::data::building::BuildingRecord> = Vec::new();
    let mut levels_by_id: HashMap<String, u32> = HashMap::new();

    for entry in &index.buildings {
        let Some(rec) = load_building_record_any(&data_dir, &entry.id) else {
            continue;
        };
        let max_lvl = kobayashi::data::building::max_level(&rec);
        if max_lvl == 0 {
            continue;
        }
        levels_by_id.insert(rec.id.clone(), max_lvl);
        records.push(rec);
    }

    let bonuses = kobayashi::data::building::cumulative_building_bonuses(&records, &levels_by_id);

    // Restrict to combat keys and fold armor_pierce/shield_pierce into pierce
    let mut combat: HashMap<String, f64> = HashMap::new();
    for (stat, value) in bonuses {
        let key: String = if stat == "armor_pierce" || stat == "shield_pierce" {
            "pierce".to_string()
        } else if COMBAT_KEYS.contains(&stat.as_str()) {
            stat
        } else {
            continue;
        };
        *combat.entry(key).or_insert(0.0) += value;
    }

    println!(
        "Combat stat bonuses if all {} buildings at max level (additive; 0.35 = +35% in profile):",
        records.len()
    );
    println!("(Sums every building that grants each stat; in-game only some buildings may apply to combat.)");
    println!();
    for key in COMBAT_KEYS {
        let v = combat.get(*key).copied().unwrap_or(0.0);
        println!("  {}: {:.4}  ({:.2}%)", key, v, v * 100.0);
    }
    Ok(())
}
