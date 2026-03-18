//! Normalize data.stfc.space ship JSON into KOBAYASHI extended ship format.
//! Reads data/upstream/data-stfc-space/ships/*.json and ship_id_registry.json,
//! outputs data/ships_extended/<id>.json (one file per ship with tiers + levels).

use std::fs;
use std::path::Path;

use serde_json::Value;

#[derive(Debug)]
struct AbilityCatalogEntry {
    timing: String,
    effect_type: String,
    value_is_percentage: bool,
}

use kobayashi::data::ship::{
    ExtendedShipRecord, LevelBonus, ShipAbility, ShipIdRegistry, ShipIdRegistryEntry, TierStats,
    WeaponRecord, DEFAULT_SHIP_ID_REGISTRY_PATH,
};

const SHIP_ABILITY_CATALOG_PATH: &str = "data/upstream/data-stfc-space/ship_ability_catalog.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(".");
    let upstream_ships = repo_root.join("data/upstream/data-stfc-space/ships");
    let registry_path = repo_root.join(DEFAULT_SHIP_ID_REGISTRY_PATH);
    let catalog_path = repo_root.join(SHIP_ABILITY_CATALOG_PATH);
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

    let ability_catalog: Option<std::collections::HashMap<String, AbilityCatalogEntry>> =
        fs::read_to_string(&catalog_path)
            .ok()
            .and_then(|s| {
                let root: Value = serde_json::from_str(&s).ok()?;
                let entries = root.get("entries")?.as_object()?;
                let mut map = std::collections::HashMap::new();
                for (k, v) in entries {
                    let timing = v.get("timing")?.as_str()?.to_string();
                    let effect_type = v.get("effect_type")?.as_str()?.to_string();
                    let value_is_percentage = v.get("value_is_percentage").and_then(Value::as_bool).unwrap_or(false);
                    map.insert(k.clone(), AbilityCatalogEntry { timing, effect_type, value_is_percentage });
                }
                Some(map)
            });

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
        let extended = raw_to_extended(&raw, &reg.id, &reg.ship_name, &reg.ship_class, ability_catalog.as_ref())?;
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
    ability_catalog: Option<&std::collections::HashMap<String, AbilityCatalogEntry>>,
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

    let abilities = ability_catalog.and_then(|catalog| {
        let arr = raw.get("ability")?.as_array()?;
        let mut out = Vec::new();
        for ab in arr {
            let id_num = ab.get("id")?.as_u64()?;
            let id_str = id_num.to_string();
            let entry = catalog.get(&id_str)?;
            let value_is_percentage = ab.get("value_is_percentage").and_then(Value::as_bool).unwrap_or(entry.value_is_percentage);
            let raw_value = ab.get("values")?.as_array()?.first().and_then(|v| v.get("value").and_then(Value::as_f64)).unwrap_or(0.0);
            let value = if value_is_percentage { raw_value * 0.01 } else { raw_value };
            out.push(ShipAbility {
                id: id_str,
                timing: entry.timing.clone(),
                effect_type: entry.effect_type.clone(),
                value,
            });
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    });

    Ok(ExtendedShipRecord {
        id: canonical_id.to_string(),
        ship_name: ship_name.to_string(),
        ship_class: ship_class.to_string(),
        tiers,
        levels,
        abilities,
    })
}

/// Order value used when component has no order or order is -1 (sort after valid weapons).
const WEAPON_ORDER_LAST: i64 = 999;

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
    let mut hull_health = 0.0;
    let mut shield_health = 0.0;
    let mut shield_mitigation = 0.8;

    // Collect weapon components with their order for deterministic sorting (primary first).
    let mut weapon_components: Vec<(i64, &Value)> = Vec::new();

    for comp in components {
        let data = match comp.get("data") {
            Some(d) => d,
            None => continue,
        };
        let tag = data.get("tag").and_then(Value::as_str).unwrap_or("");
        match tag {
            "Weapon" => {
                let order = comp
                    .get("order")
                    .and_then(Value::as_i64)
                    .filter(|&o| o >= 0)
                    .unwrap_or(WEAPON_ORDER_LAST);
                weapon_components.push((order, data));
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

    // Sort by order so primary weapon (order 1) is first; same order fires in same sequence.
    weapon_components.sort_by_key(|(order, _)| *order);

    let mut armor_piercing_sum = 0.0;
    let mut shield_piercing_sum = 0.0;
    let mut accuracy_sum = 0.0;
    let mut attack_total = 0.0;
    let mut crit_chance = 0.1;
    let mut crit_damage = 1.5;
    let mut weapons_out: Vec<WeaponRecord> = Vec::new();
    let mut first_weapon = true;

    for (_, data) in weapon_components {
        let penetration = data.get("penetration").and_then(Value::as_f64).unwrap_or(0.0);
        let modulation = data.get("modulation").and_then(Value::as_f64).unwrap_or(0.0);
        let accuracy = data.get("accuracy").and_then(Value::as_f64).unwrap_or(0.0);
        let min_d = data.get("minimum_damage").and_then(Value::as_f64).unwrap_or(0.0);
        let max_d = data.get("maximum_damage").and_then(Value::as_f64).unwrap_or(0.0);
        let shots_u = data.get("shots").and_then(Value::as_u64).unwrap_or(1);
        let shots = shots_u.max(1) as u32;

        armor_piercing_sum += penetration;
        shield_piercing_sum += modulation;
        accuracy_sum += accuracy;

        let avg_damage = (min_d + max_d) * 0.5;
        attack_total += avg_damage * (shots as f64);

        // Crit from primary weapon only (first by order).
        if first_weapon {
            first_weapon = false;
            if let Some(c) = data.get("crit_chance").and_then(Value::as_f64) {
                crit_chance = c;
            }
            if let Some(c) = data.get("crit_modifier").and_then(Value::as_f64) {
                crit_damage = c;
            }
        }

        weapons_out.push(WeaponRecord {
            attack: avg_damage,
            shots: Some(shots),
        });
    }

    let weapon_count = weapons_out.len().max(1);
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

    let weapons = if weapons_out.is_empty() {
        None
    } else {
        Some(weapons_out)
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
