//! Normalize data.stfc.space ship JSON into KOBAYASHI extended ship format.
//! Reads data/upstream/data-stfc-space/ships/*.json and ship_id_registry.json,
//! outputs data/ships_extended/<id>.json (one file per ship with tiers + levels).

use std::fs;
use std::path::Path;

use serde_json::Value;

use kobayashi::data::ship::{
    ExtendedShipRecord, LevelBonus, ShipIdRegistry, ShipIdRegistryEntry, TierStats, WeaponRecord,
    DEFAULT_SHIP_ID_REGISTRY_PATH,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(".");
    let upstream_ships = repo_root.join("data/upstream/data-stfc-space/ships");
    let registry_path = repo_root.join(DEFAULT_SHIP_ID_REGISTRY_PATH);
    let out_dir = repo_root.join("data/ships_extended");

    if !upstream_ships.is_dir() {
        eprintln!("error: upstream ships directory not found: {}", upstream_ships.display());
        std::process::exit(1);
    }

    let registry: ShipIdRegistry = {
        let data = fs::read_to_string(&registry_path)
            .map_err(|e| format!("read ship_id_registry: {}", e))?;
        serde_json::from_str(&data).map_err(|e| format!("parse ship_id_registry: {}", e))?
    };

    let id_by_numeric: std::collections::HashMap<u64, &ShipIdRegistryEntry> =
        registry.ships.iter().map(|e| (e.numeric_id, e)).collect();

    fs::create_dir_all(&out_dir)?;
    let mut count = 0u32;
    let mut index_entries: Vec<kobayashi::data::ship::ExtendedShipIndexEntry> = Vec::new();

    for entry in fs::read_dir(&upstream_ships)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "json") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let numeric_id: u64 = match stem.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let reg = match id_by_numeric.get(&numeric_id) {
            Some(r) => r,
            None => {
                eprintln!("skip {}: no registry entry for numeric_id {}", path.display(), numeric_id);
                continue;
            }
        };

        let content = fs::read_to_string(&path)?;
        let raw: Value = serde_json::from_str(&content)?;
        let extended = raw_to_extended(&raw, &reg.id, &reg.ship_name, &reg.ship_class)?;
        index_entries.push(kobayashi::data::ship::ExtendedShipIndexEntry {
            id: extended.id.clone(),
            ship_name: extended.ship_name.clone(),
            ship_class: extended.ship_class.clone(),
        });
        let out_path = out_dir.join(format!("{}.json", extended.id));
        fs::write(&out_path, serde_json::to_string_pretty(&extended)?)?;
        count += 1;
    }

    // Write extended index for resolver (id, ship_name, ship_class per normalized ship).
    let extended_index = kobayashi::data::ship::ExtendedShipIndex {
        data_version: Some("data-stfc-space".to_string()),
        source_note: Some("From normalize_data_stfc_space".to_string()),
        ships: index_entries,
    };
    fs::write(
        out_dir.join("index.json"),
        serde_json::to_string_pretty(&extended_index)?,
    )?;

    println!("Normalized {} ships from data-stfc.space -> {}", count, out_dir.display());
    Ok(())
}

fn raw_to_extended(
    raw: &Value,
    canonical_id: &str,
    ship_name: &str,
    ship_class: &str,
) -> Result<ExtendedShipRecord, Box<dyn std::error::Error>> {
    let tiers_arr = raw
        .get("tiers")
        .and_then(Value::as_array)
        .ok_or("missing tiers")?;
    let levels_arr = raw
        .get("levels")
        .and_then(Value::as_array)
        .ok_or("missing levels")?;

    let mut tiers: Vec<TierStats> = Vec::new();
    for t in tiers_arr {
        let tier_num = t.get("tier").and_then(Value::as_u64).unwrap_or(0) as u32;
        let components: &[Value] = t
            .get("components")
            .and_then(Value::as_array)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let (armor_piercing, shield_piercing, accuracy, attack, crit_chance, crit_damage, hull_health, shield_health, shield_mitigation, weapons) =
            extract_tier_combat(components)?;
        tiers.push(TierStats {
            tier: tier_num,
            armor_piercing,
            shield_piercing,
            accuracy,
            attack,
            crit_chance,
            crit_damage,
            hull_health,
            shield_health,
            shield_mitigation: Some(shield_mitigation),
            weapons,
        });
    }

    let mut levels: Vec<LevelBonus> = Vec::new();
    for l in levels_arr {
        let level = l.get("level").and_then(Value::as_u64).unwrap_or(0) as u32;
        let shield = l.get("shield").and_then(Value::as_f64).unwrap_or(0.0);
        let health = l.get("health").and_then(Value::as_f64).unwrap_or(0.0);
        levels.push(LevelBonus { level, shield, health });
    }

    Ok(ExtendedShipRecord {
        id: canonical_id.to_string(),
        ship_name: ship_name.to_string(),
        ship_class: ship_class.to_string(),
        tiers,
        levels,
    })
}

fn extract_tier_combat(
    components: &[Value],
) -> Result<
    (
        f64,
        f64,
        f64,
        f64,
        f64,
        f64,
        f64,
        f64,
        f64,
        Option<Vec<WeaponRecord>>,
    ),
    Box<dyn std::error::Error>,
> {
    let mut armor_piercing_sum = 0.0;
    let mut shield_piercing_sum = 0.0;
    let mut accuracy_sum = 0.0;
    let mut attack_total = 0.0;
    let mut crit_chance = 0.1;
    let mut crit_damage = 1.5;
    let mut hull_health = 0.0;
    let mut shield_health = 0.0;
    let mut shield_mitigation = 0.8;
    let mut weapon_attacks: Vec<f64> = Vec::new();

    for comp in components {
        let data = match comp.get("data") {
            Some(d) => d,
            None => continue,
        };
        let tag = data.get("tag").and_then(Value::as_str).unwrap_or("");
        match tag {
            "Weapon" => {
                let penetration = data.get("penetration").and_then(Value::as_f64).unwrap_or(0.0);
                let modulation = data.get("modulation").and_then(Value::as_f64).unwrap_or(0.0);
                let accuracy = data.get("accuracy").and_then(Value::as_f64).unwrap_or(0.0);
                let min_d = data.get("minimum_damage").and_then(Value::as_f64).unwrap_or(0.0);
                let max_d = data.get("maximum_damage").and_then(Value::as_f64).unwrap_or(0.0);
                let shots = data.get("shots").and_then(Value::as_u64).unwrap_or(1) as f64;
                armor_piercing_sum += penetration;
                shield_piercing_sum += modulation;
                accuracy_sum += accuracy;
                let avg_damage = (min_d + max_d) * 0.5;
                let per_weapon_attack = avg_damage * shots;
                attack_total += per_weapon_attack;
                weapon_attacks.push(per_weapon_attack);
                if let Some(c) = data.get("crit_chance").and_then(Value::as_f64) {
                    crit_chance = c;
                }
                if let Some(c) = data.get("crit_modifier").and_then(Value::as_f64) {
                    crit_damage = c;
                }
            }
            "Shield" => {
                shield_health = data.get("hp").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(m) = data.get("mitigation").and_then(Value::as_f64) {
                    shield_mitigation = m;
                }
            }
            "Armor" => {
                hull_health = data.get("hp").and_then(Value::as_f64).unwrap_or(0.0);
            }
            _ => {}
        }
    }

    let weapon_count = weapon_attacks.len().max(1);
    let armor_piercing = armor_piercing_sum / weapon_count as f64;
    let shield_piercing = shield_piercing_sum / weapon_count as f64;
    let accuracy = accuracy_sum / weapon_count as f64;
    let attack = if attack_total <= 0.0 { 100.0 } else { attack_total };
    if shield_health <= 0.0 {
        shield_health = 1000.0;
    }
    if hull_health <= 0.0 {
        hull_health = shield_health * 2.0;
    }

    let weapons = if weapon_attacks.is_empty() {
        None
    } else {
        Some(
            weapon_attacks
                .into_iter()
                .map(|a| WeaponRecord { attack: a, shots: None })
                .collect(),
        )
    };

    Ok((
        armor_piercing,
        shield_piercing,
        accuracy,
        attack,
        crit_chance,
        crit_damage,
        hull_health,
        shield_health,
        shield_mitigation,
        weapons,
    ))
}
