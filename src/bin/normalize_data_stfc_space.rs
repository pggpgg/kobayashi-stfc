//! Normalize data.stfc.space ship JSON into KOBAYASHI extended ship format.
//! Reads data/upstream/data-stfc-space/ships/*.json and ship_id_registry.json,
//! outputs data/ships_extended/<id>.json (one file per ship with tiers + levels).

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct AbilityCatalogEntry {
    timing: String,
    effect_type: String,
    #[serde(default)]
    value_is_percentage: bool,
    /// When true, use only catalog `value_is_percentage` (ignore upstream `value_is_percentage`).
    /// Needed when upstream marks small decimals as `%` but values are already fractional (e.g. 0.02 = 2%).
    #[serde(default)]
    ignore_upstream_value_is_percentage: bool,
    #[serde(default)]
    duration_rounds: Option<u32>,
    /// When set, use this value instead of reading `ability.values[0].value` (e.g. 0 for catalogued-but-unmodeled rows).
    #[serde(default)]
    value_override: Option<f64>,
    #[serde(default)]
    condition_morale: bool,
    #[serde(default)]
    condition_defender_burning: bool,
    #[serde(default)]
    condition_defender_hull_breach: bool,
    #[serde(default)]
    condition_opponent_faction: Option<String>,
    #[serde(default)]
    condition_opponent_ship_class: Option<String>,
    #[serde(default)]
    condition_opponent_hostile_tags: Option<Vec<String>>,
    #[serde(default)]
    round_cap: Option<u32>,
    /// When true, emit [`ShipAbility::level_scaled_values`] from every upstream `values[]` row (still one [`ShipAbility`]).
    #[serde(default)]
    values_scale_with_ship_level: bool,
    /// Multiply normalized numeric `value` / curve entries after the usual `value_is_percentage` scaling (e.g. Borg Omicron uses `0.001` on upstream “percent × 100” rows).
    #[serde(default)]
    post_scale: Option<f64>,
}

use kobayashi::data::ship::{
    CrewSlotUnlock, ExtendedShipRecord, LevelBonus, OfficerBonusBreakpoint, OfficerBonusTable,
    ShipAbility, ShipIdRegistry, ShipIdRegistryEntry, TierStats, WeaponRecord,
    DEFAULT_SHIP_ID_REGISTRY_PATH,
};

const SHIP_ABILITY_CATALOG_PATH: &str = "data/upstream/data-stfc-space/ship_ability_catalog.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(".");
    let upstream_ships = repo_root.join("data/upstream/data-stfc-space/ships");
    let registry_path = repo_root.join(DEFAULT_SHIP_ID_REGISTRY_PATH);
    let catalog_path = repo_root.join(SHIP_ABILITY_CATALOG_PATH);
    let out_dir = repo_root.join("data/ships_extended");

    if !upstream_ships.is_dir() {
        eprintln!(
            "error: upstream ships directory not found: {}",
            upstream_ships.display()
        );
        std::process::exit(1);
    }

    let registry: ShipIdRegistry = {
        let data = fs::read_to_string(&registry_path)
            .map_err(|e| format!("read ship_id_registry: {}", e))?;
        serde_json::from_str(&data).map_err(|e| format!("parse ship_id_registry: {}", e))?
    };

    let ability_catalog: Option<std::collections::HashMap<String, AbilityCatalogEntry>> =
        fs::read_to_string(&catalog_path).ok().and_then(|s| {
            let root: Value = serde_json::from_str(&s).ok()?;
            let entries = root.get("entries")?.as_object()?;
            let mut map = std::collections::HashMap::new();
            for (k, v) in entries {
                let entry: AbilityCatalogEntry = serde_json::from_value(v.clone()).ok()?;
                map.insert(k.clone(), entry);
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
        if path.extension().is_none_or(|e| e != "json") {
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
                eprintln!(
                    "skip {}: no registry entry for numeric_id {}",
                    path.display(),
                    numeric_id
                );
                continue;
            }
        };

        let content = fs::read_to_string(&path)?;
        let raw: Value = serde_json::from_str(&content)?;
        let extended = raw_to_extended(
            &raw,
            &reg.id,
            &reg.ship_name,
            &reg.ship_class,
            ability_catalog.as_ref(),
        )?;
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

    println!(
        "Normalized {} ships from data-stfc.space -> {}",
        count,
        out_dir.display()
    );
    Ok(())
}

fn raw_to_extended(
    raw: &Value,
    canonical_id: &str,
    ship_name: &str,
    ship_class: &str,
    ability_catalog: Option<&std::collections::HashMap<String, AbilityCatalogEntry>>,
) -> Result<ExtendedShipRecord, Box<dyn std::error::Error>> {
    let faction = raw
        .get("faction")
        .and_then(|v| v.get("loca_id"))
        .and_then(Value::as_u64)
        .and_then(|id| match id {
            1 => Some("federation".to_string()),
            2 => Some("klingon".to_string()),
            3 => Some("romulan".to_string()),
            _ => None,
        });
    let tiers_arr = raw
        .get("tiers")
        .and_then(Value::as_array)
        .ok_or("missing tiers")?;
    let levels_arr = raw
        .get("levels")
        .and_then(Value::as_array)
        .ok_or("missing levels")?;

    let mut parsed_tiers: Vec<TierStats> = Vec::new();
    for t in tiers_arr {
        let tier_num = t.get("tier").and_then(Value::as_u64).unwrap_or(0) as u32;
        let components: &[Value] = t
            .get("components")
            .and_then(Value::as_array)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let (
            armor_piercing,
            shield_piercing,
            accuracy,
            attack,
            crit_chance,
            crit_damage,
            hull_health,
            shield_health,
            shield_mitigation,
            armor,
            shield_deflection,
            dodge,
            weapons,
        ) = extract_tier_combat(components)?;
        parsed_tiers.push(TierStats {
            tier: tier_num,
            armor_piercing,
            shield_piercing,
            accuracy,
            armor,
            shield_deflection,
            dodge,
            attack,
            crit_chance,
            crit_damage,
            hull_health,
            shield_health,
            shield_mitigation: Some(shield_mitigation),
            weapons,
        });
    }
    parsed_tiers.sort_by_key(|t| t.tier);
    let mut tiers: Vec<TierStats> = Vec::with_capacity(parsed_tiers.len());
    let mut cumulative_armor_piercing = 0.0;
    let mut cumulative_shield_piercing = 0.0;
    let mut cumulative_accuracy = 0.0;
    for mut t in parsed_tiers {
        // STFC displays these offensive stats as cumulative tier upgrade contributions.
        cumulative_armor_piercing += t.armor_piercing;
        cumulative_shield_piercing += t.shield_piercing;
        cumulative_accuracy += t.accuracy;
        t.armor_piercing = cumulative_armor_piercing;
        t.shield_piercing = cumulative_shield_piercing;
        t.accuracy = cumulative_accuracy;
        tiers.push(t);
    }

    let mut levels: Vec<LevelBonus> = Vec::new();
    for l in levels_arr {
        let level = l.get("level").and_then(Value::as_u64).unwrap_or(0) as u32;
        let shield = l.get("shield").and_then(Value::as_f64).unwrap_or(0.0);
        let health = l.get("health").and_then(Value::as_f64).unwrap_or(0.0);
        levels.push(LevelBonus {
            level,
            shield,
            health,
        });
    }

    let crew_slots: Vec<CrewSlotUnlock> = raw
        .get("crew_slots")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|row| CrewSlotUnlock {
                    slots: row
                        .get("slots")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    unlock_level: row.get("unlock_level").and_then(Value::as_u64).unwrap_or(0)
                        as u32,
                })
                .collect()
        })
        .unwrap_or_default();

    let abilities = ability_catalog.and_then(|catalog| {
        let arr = raw.get("ability")?.as_array()?;
        let mut out = Vec::new();
        for ab in arr {
            let Some(id_num) = ab.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let id_str = id_num.to_string();
            let Some(entry) = catalog.get(&id_str) else {
                continue;
            };
            let value_is_percentage = if entry.ignore_upstream_value_is_percentage {
                entry.value_is_percentage
            } else {
                ab.get("value_is_percentage")
                    .and_then(Value::as_bool)
                    .unwrap_or(entry.value_is_percentage)
            };
            let Some(values_arr) = ab.get("values").and_then(Value::as_array) else {
                continue;
            };
            let Some(first_val) = values_arr.first() else {
                continue;
            };

            let scale_curve = entry.values_scale_with_ship_level;
            let post = entry.post_scale.unwrap_or(1.0);
            let level_scaled_values = if scale_curve {
                let mut curve: Vec<f64> = Vec::with_capacity(values_arr.len());
                for item in values_arr {
                    let raw_value = item.get("value").and_then(Value::as_f64).unwrap_or(0.0);
                    let v = if value_is_percentage {
                        raw_value * 0.01
                    } else {
                        raw_value
                    };
                    curve.push(v * post);
                }
                Some(curve)
            } else {
                None
            };

            let value = if let Some(v) = entry.value_override {
                v * post
            } else {
                let raw_value = first_val
                    .get("value")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let base = if value_is_percentage {
                    raw_value * 0.01
                } else {
                    raw_value
                };
                base * post
            };

            out.push(ShipAbility {
                id: id_str,
                timing: entry.timing.clone(),
                effect_type: entry.effect_type.clone(),
                value,
                duration_rounds: entry.duration_rounds,
                condition_morale: entry.condition_morale,
                condition_defender_burning: entry.condition_defender_burning,
                condition_defender_hull_breach: entry.condition_defender_hull_breach,
                condition_opponent_faction: entry.condition_opponent_faction.clone(),
                condition_opponent_ship_class: entry.condition_opponent_ship_class.clone(),
                condition_opponent_hostile_tags: entry.condition_opponent_hostile_tags.clone(),
                round_cap: entry.round_cap,
                level_scaled_values,
            });
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    });

    let officer_bonus = parse_officer_bonus(raw);

    Ok(ExtendedShipRecord {
        id: canonical_id.to_string(),
        ship_name: ship_name.to_string(),
        ship_class: ship_class.to_string(),
        faction,
        tiers,
        levels,
        crew_slots,
        abilities,
        officer_bonus,
    })
}

fn parse_officer_bonus(raw: &Value) -> OfficerBonusTable {
    let Some(node) = raw.get("officer_bonus") else {
        return OfficerBonusTable::default();
    };
    let parse_channel = |key: &str| -> Vec<OfficerBonusBreakpoint> {
        let Some(arr) = node.get(key).and_then(Value::as_array) else {
            return Vec::new();
        };
        let mut out: Vec<OfficerBonusBreakpoint> = arr
            .iter()
            .filter_map(|row| {
                let value = row.get("value").and_then(Value::as_f64)?;
                let bonus = row.get("bonus").and_then(Value::as_f64)?;
                Some(OfficerBonusBreakpoint { value, bonus })
            })
            .collect();
        out.sort_by(|a, b| {
            a.value
                .partial_cmp(&b.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    };
    OfficerBonusTable {
        attack: parse_channel("attack"),
        defense: parse_channel("defense"),
        health: parse_channel("health"),
    }
}

/// Order value used when component has no order or order is -1 (sort after valid weapons).
const WEAPON_ORDER_LAST: i64 = 999;

#[allow(clippy::type_complexity)]
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
    // Player defender stats for hostile→player counter-fire mitigation, and the per-ship channel
    // constants for officer-stat Defense routing (see `docs/OFFICER_STAT_FORMULA.md` §2c).
    // Sources on data.stfc.space tier components (`Shield.absorption` is upstream's legacy field
    // name for the in-game Shield Deflection stat):
    //   - `Armor.plating`           → `armor`              (battleship-primary channel)
    //   - `Shield.absorption`       → `shield_deflection`  (explorer-primary channel)
    //   - `Impulse.dodge`           → `dodge`              (interceptor-primary channel)
    // The upstream `Deflector.deflection` field is a stale `120` for every ship, maps to no
    // in-game concept, and is intentionally ignored; the hostile normalizer
    // (`normalize_hostiles_stfc_space.rs`) already sources `shield_deflection` the same way,
    // so this keeps player ships in alignment.
    let mut armor_stat = 0.0;
    let mut shield_deflection_stat = 0.0;
    let mut dodge_stat = 0.0;

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
                if let Some(a) = data.get("absorption").and_then(Value::as_f64) {
                    shield_deflection_stat = a;
                }
            }
            "Armor" => {
                hull_health = data.get("hp").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(p) = data.get("plating").and_then(Value::as_f64) {
                    armor_stat = p;
                }
            }
            "Deflector" => {
                // `Deflector.deflection` is a stale constant (120) across every upstream ship and
                // maps to no in-game concept. The in-game Shield Deflection stat is sourced from
                // upstream's legacy `Shield.absorption` field (handled above).
            }
            "Impulse" => {
                if let Some(d) = data.get("dodge").and_then(Value::as_f64) {
                    dodge_stat = d;
                }
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
        let penetration = data
            .get("penetration")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let modulation = data
            .get("modulation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let accuracy = data.get("accuracy").and_then(Value::as_f64).unwrap_or(0.0);
        let min_d = data
            .get("minimum_damage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let max_d = data
            .get("maximum_damage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let shots_u = data.get("shots").and_then(Value::as_u64).unwrap_or(1);
        let shots = shots_u.max(1) as u32;

        armor_piercing_sum += penetration;
        shield_piercing_sum += modulation;
        accuracy_sum += accuracy;

        let avg_damage = (min_d + max_d) * 0.5;
        attack_total += avg_damage * (shots as f64);

        let w_crit_chance = data.get("crit_chance").and_then(Value::as_f64);
        let w_crit_mult = data
            .get("crit_modifier")
            .or_else(|| data.get("crit_damage"))
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite() && *c > 0.0);
        let w_proc_chance = data.get("proc_chance").and_then(Value::as_f64);
        let w_proc_mult = data
            .get("proc_multiplier")
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite() && *c > 0.0);

        // Tier-level crit scalars: primary weapon (first by order) for backward compatibility.
        if first_weapon {
            first_weapon = false;
            if let Some(c) = w_crit_chance {
                crit_chance = c;
            }
            if let Some(c) = w_crit_mult {
                crit_damage = c;
            }
        }

        weapons_out.push(WeaponRecord {
            attack: avg_damage,
            shots: Some(shots),
            armor_piercing: Some(penetration),
            shield_piercing: Some(modulation),
            accuracy: Some(accuracy),
            crit_chance: w_crit_chance,
            crit_multiplier: w_crit_mult,
            proc_chance: w_proc_chance,
            proc_multiplier: w_proc_mult,
            ..Default::default()
        });
    }

    let armor_piercing = armor_piercing_sum;
    let shield_piercing = shield_piercing_sum;
    let accuracy = accuracy_sum;
    let attack = if attack_total <= 0.0 {
        100.0
    } else {
        attack_total
    };
    // No fallback for zero shield_health: every upstream ship carries a Shield component, and
    // hp 0 means genuinely shieldless in-game (Sarcophagus, Enterprise NX-01) — the engine
    // routes all damage to hull when shields are 0. A phantom default here would give those
    // hulls a shield layer they don't have.
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
        armor_stat,
        shield_deflection_stat,
        dodge_stat,
        weapons,
    ))
}
