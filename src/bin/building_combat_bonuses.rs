//! Print combined combat stat bonuses: either all buildings at max level, or a profile's synced levels.
//! Usage (from project root):
//!   cargo run --bin building_combat_bonuses
//!   cargo run --bin building_combat_bonuses -- --profile default
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

/// Load a building record by id (uses index file field when present for bid_name naming).
fn load_building_record_any(
    data_dir: &Path,
    id: &str,
) -> Option<kobayashi::data::building::BuildingRecord> {
    kobayashi::data::building::load_building_record(data_dir, id)
}

fn infer_ops_level(
    imported: &[kobayashi::data::import::BuildingEntry],
    bid_to_id: &HashMap<i64, String>,
) -> Option<u32> {
    for entry in imported {
        let id = bid_to_id.get(&entry.bid)?;
        if id != "ops_center" {
            continue;
        }
        return Some(entry.level.clamp(0, i64::from(u32::MAX)) as u32);
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let args: Vec<String> = std::env::args().collect();
    let profile_id = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1).cloned());

    let data_dir = Path::new(&manifest_dir).join("data/buildings");
    let index_path = data_dir.join("index.json");
    let index = kobayashi::data::building::load_building_index(index_path.to_str().unwrap())
        .ok_or("Failed to load building index (data/buildings/index.json)")?;

    if let Some(pid) = &profile_id {
        let buildings_path = kobayashi::data::profile_index::profile_data_dir(pid)
            .join(kobayashi::data::profile_index::BUILDINGS_IMPORTED);
        let imported = kobayashi::data::import::load_imported_buildings(
            buildings_path.to_str().unwrap(),
        )
        .unwrap_or_default();
        if imported.is_empty() {
            eprintln!("No buildings in profile {} ({}); showing all at max.", pid, buildings_path.display());
            let mut recs = Vec::new();
            let mut levels = HashMap::new();
            for entry in &index.buildings {
                if let Some(rec) = load_building_record_any(&data_dir, &entry.id) {
                    let max_lvl = kobayashi::data::building::max_level(&rec);
                    if max_lvl > 0 {
                        levels.insert(rec.id.clone(), max_lvl);
                        recs.push(rec);
                    }
                }
            }
            let bonuses = kobayashi::data::building::cumulative_building_bonuses(&recs, &levels);
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
            println!("All {} buildings at max (profile has no buildings):", recs.len());
            println!();
            for key in COMBAT_KEYS {
                let v = combat.get(*key).copied().unwrap_or(0.0);
                println!("  {}: {:.4}  ({:.2}%)", key, v, v * 100.0);
            }
            return Ok(());
        } else {
            let bid_to_id = kobayashi::data::building_bid_resolver::load_bid_to_building_id(
                kobayashi::data::building_bid_resolver::DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
                &index,
            )
            .ok_or("Failed to load bid→id map (translations + index)")?;
            let mut levels_by_id: HashMap<String, u32> = HashMap::new();
            for entry in &imported {
                if let Some(id) = bid_to_id.get(&entry.bid) {
                    let lvl = entry.level.clamp(0, i64::from(u32::MAX)) as u32;
                    levels_by_id.insert(id.clone(), lvl);
                }
            }
            let mut records: Vec<kobayashi::data::building::BuildingRecord> = Vec::new();
            for id in levels_by_id.keys() {
                if let Some(rec) = load_building_record_any(&data_dir, id) {
                    records.push(rec);
                }
            }
            let ops_level = infer_ops_level(&imported, &bid_to_id);
            let context = kobayashi::data::building::BuildingBonusContext {
                ops_level,
                mode: kobayashi::data::building::BuildingMode::ShipCombat,
            };
            let bonuses = kobayashi::data::building::cumulative_building_bonuses_with_context(
                &records,
                &levels_by_id,
                &context,
            );
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
                "Combat stat bonuses for profile \"{}\" ({} buildings, ops_level={:?}):",
                pid,
                records.len(),
                ops_level
            );
            println!("(Additive; 0.35 = +35% in profile.)");
            println!();
            for key in COMBAT_KEYS {
                let v = combat.get(*key).copied().unwrap_or(0.0);
                println!("  {}: {:.4}  ({:.2}%)", key, v, v * 100.0);
            }
            return Ok(());
        }
    } else {
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
        println!("Use --profile <id> to show bonuses for a profile's synced building levels.");
        println!();
        for key in COMBAT_KEYS {
            let v = combat.get(*key).copied().unwrap_or(0.0);
            println!("  {}: {:.4}  ({:.2}%)", key, v, v * 100.0);
        }
        return Ok(());
    }
}
