//! Normalize STFCcommunity raw JSON into KOBAYASHI hostiles/ships schema.
//! Reads data/upstream/stfccommunity-data/ (hostiles/*.json and ships/*.json),
//! writes data/hostiles/ and data/ships/ with index.json and data_version/source_note.
//! Run after scripts/fetch_stfc_data.ps1.

use std::fs;

use serde::Deserialize;

const UPSTREAM_HOSTILES_SUFFIX: &str = "data/upstream/stfccommunity-data";
const UPSTREAM_SHIPS_SUFFIX: &str = "data/upstream/stfccommunity-data/ships";
const OUT_HOSTILES_SUFFIX: &str = "data/hostiles";
const OUT_SHIPS_SUFFIX: &str = "data/ships";
const SOURCE_NOTE: &str = "STFCcommunity baseline (outdated ~3y)";

/// Resolve path relative to repo root (CARGO_MANIFEST_DIR when run via cargo).
fn repo_data_path(suffix: &str) -> std::path::PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return std::path::PathBuf::from(manifest_dir).join(suffix);
    }
    std::path::PathBuf::from(suffix)
}

// ----- Raw STFCcommunity hostile (partial) -----
#[derive(Debug, Default, Deserialize)]
struct RawHostileStatsDefense {
    #[serde(default)]
    armor: f64,
    #[serde(default)]
    dodge: f64,
    #[serde(default)]
    shield_deflect: f64,
}

#[derive(Debug, Default, Deserialize)]
struct RawHostileStatsHealth {
    #[serde(default)]
    hull_health: f64,
    #[serde(default)]
    shield_health: f64,
}

#[derive(Debug, Default, Deserialize)]
struct RawHostileStats {
    #[serde(default)]
    defense: RawHostileStatsDefense,
    #[serde(default)]
    health: RawHostileStatsHealth,
}

#[derive(Debug, Deserialize)]
struct RawHostile {
    #[serde(default)]
    hostile_name: String,
    #[serde(default)]
    level: u32,
    #[serde(default)]
    ship_class: String,
    #[serde(default)]
    stats: RawHostileStats,
}

// ----- Raw STFCcommunity ship (partial: first tier, components) -----
#[derive(Debug, Deserialize)]
struct RawWeaponsInfo {
    #[serde(default)]
    accuracy: f64,
    #[serde(default)]
    armor_pierce: f64,
    #[serde(default)]
    shield_pierce: f64,
    #[serde(default)]
    crit_chance: f64,
    #[serde(default)]
    crit_damage: f64,
    #[serde(default)]
    max_damage: f64,
    #[serde(default)]
    min_damage: f64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawShieldInfo {
    #[serde(default)]
    shield_deflection: f64,
    #[serde(default)]
    shield_health: f64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawImpulseInfo {
    #[serde(default)]
    dodge: f64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawComponentAdditionalInfo {
    #[serde(default)]
    weapons_info: Option<RawWeaponsInfo>,
    #[serde(default)]
    shield_info: Option<RawShieldInfo>,
    #[serde(default)]
    impulse_info: Option<RawImpulseInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawComponent {
    #[serde(default)]
    name: String,
    #[serde(default)]
    additional_info: Option<RawComponentAdditionalInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawTier {
    #[serde(default)]
    tier: u32,
    #[serde(default)]
    components: Vec<RawComponent>,
}

#[derive(Debug, Deserialize)]
struct RawShip {
    #[serde(default)]
    ship_name: String,
    #[serde(default)]
    ship_class: String,
    #[serde(default)]
    tiers: Vec<RawTier>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_version = std::env::var("STFC_DATA_VERSION").unwrap_or_else(|_| "stfccommunity-main".to_string());

    let hostiles_dir = repo_data_path(UPSTREAM_HOSTILES_SUFFIX);
    let ships_dir = repo_data_path(UPSTREAM_SHIPS_SUFFIX);
    let out_hostiles = repo_data_path(OUT_HOSTILES_SUFFIX);
    let out_ships = repo_data_path(OUT_SHIPS_SUFFIX);

    if !hostiles_dir.is_dir() {
        eprintln!(
            "error: upstream hostiles directory not found: {}",
            hostiles_dir.display()
        );
        eprintln!("Run the fetch script first: powershell -ExecutionPolicy Bypass -File scripts/fetch_stfc_data.ps1");
        std::process::exit(1);
    }
    if !ships_dir.is_dir() {
        eprintln!(
            "error: upstream ships directory not found: {}",
            ships_dir.display()
        );
        eprintln!("Run the fetch script first: powershell -ExecutionPolicy Bypass -File scripts/fetch_stfc_data.ps1");
        std::process::exit(1);
    }

    fs::create_dir_all(&out_hostiles)?;
    fs::create_dir_all(&out_ships)?;

    // ----- Hostiles -----
    let mut hostile_index_entries: Vec<kobayashi::data::hostile::HostileIndexEntry> = Vec::new();
    {
        for entry in fs::read_dir(&hostiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let content = fs::read_to_string(&path)?;
                let raw: RawHostile = serde_json::from_str(&content).unwrap_or_else(|_| RawHostile {
                    hostile_name: id.clone(),
                    level: 0,
                    ship_class: String::new(),
                    stats: RawHostileStats {
                        defense: RawHostileStatsDefense { armor: 0.0, dodge: 0.0, shield_deflect: 0.0 },
                        health: RawHostileStatsHealth { hull_health: 0.0, shield_health: 0.0 },
                    },
                });
                let rec = kobayashi::data::hostile::HostileRecord {
                    id: id.clone(),
                    hostile_name: raw.hostile_name.clone(),
                    level: raw.level,
                    ship_class: raw.ship_class.clone(),
                    armor: raw.stats.defense.armor,
                    shield_deflection: raw.stats.defense.shield_deflect,
                    dodge: raw.stats.defense.dodge,
                    hull_health: raw.stats.health.hull_health,
                    shield_health: raw.stats.health.shield_health,
                };
                hostile_index_entries.push(kobayashi::data::hostile::HostileIndexEntry {
                    id: rec.id.clone(),
                    hostile_name: rec.hostile_name.clone(),
                    level: rec.level,
                    ship_class: rec.ship_class.clone(),
                });
                let out_path = out_hostiles.join(format!("{}.json", rec.id));
                fs::write(out_path, serde_json::to_string_pretty(&rec)?)?;
            }
        }
    }

    if hostile_index_entries.is_empty() {
        eprintln!("warning: no hostile JSON files found in {}", hostiles_dir.display());
    }

    let hostile_index = kobayashi::data::hostile::HostileIndex {
        data_version: Some(data_version.clone()),
        source_note: Some(SOURCE_NOTE.to_string()),
        hostiles: hostile_index_entries,
    };
    fs::write(
        out_hostiles.join("index.json"),
        serde_json::to_string_pretty(&hostile_index)?,
    )?;

    // ----- Ships -----
    let mut ship_index_entries: Vec<kobayashi::data::ship::ShipIndexEntry> = Vec::new();
    {
        for entry in fs::read_dir(&ships_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let content = fs::read_to_string(&path)?;
                let raw: RawShip = serde_json::from_str(&content).unwrap_or_else(|_| RawShip {
                    ship_name: id.clone(),
                    ship_class: String::new(),
                    tiers: Vec::new(),
                });
                if let Some(rec) = raw_to_ship_record(&id, &raw) {
                    ship_index_entries.push(kobayashi::data::ship::ShipIndexEntry {
                        id: rec.id.clone(),
                        ship_name: rec.ship_name.clone(),
                        ship_class: rec.ship_class.clone(),
                    });
                    let out_path = out_ships.join(format!("{}.json", rec.id));
                    fs::write(out_path, serde_json::to_string_pretty(&rec)?)?;
                }
            }
        }
    }

    if ship_index_entries.is_empty() {
        eprintln!("warning: no ship JSON files found in {}", ships_dir.display());
    }

    let ship_index = kobayashi::data::ship::ShipIndex {
        data_version: Some(data_version),
        source_note: Some(SOURCE_NOTE.to_string()),
        ships: ship_index_entries,
    };
    fs::write(
        out_ships.join("index.json"),
        serde_json::to_string_pretty(&ship_index)?,
    )?;

    // Validation: re-load index and one record each to ensure schema is loadable (only if we have data)
    if !hostile_index.hostiles.is_empty() {
        let hostile_index_path = out_hostiles.join("index.json");
        let re_hostile_index = kobayashi::data::hostile::load_hostile_index(hostile_index_path.to_str().unwrap())
            .ok_or("Failed to re-load hostile index")?;
        if let Some(first) = re_hostile_index.hostiles.first() {
            kobayashi::data::hostile::load_hostile_record(&out_hostiles, &first.id)
                .ok_or("Failed to re-load a hostile record")?;
        }
    }
    if !ship_index.ships.is_empty() {
        let ship_index_path = out_ships.join("index.json");
        let re_ship_index =
            kobayashi::data::ship::load_ship_index(ship_index_path.to_str().unwrap()).ok_or("Failed to re-load ship index")?;
        if let Some(first) = re_ship_index.ships.first() {
            kobayashi::data::ship::load_ship_record(&out_ships, &first.id)
                .ok_or("Failed to re-load a ship record")?;
        }
    }

    println!(
        "Normalized {} hostiles and {} ships. data_version={:?} source_note={:?}",
        hostile_index.hostiles.len(),
        ship_index.ships.len(),
        hostile_index.data_version,
        hostile_index.source_note
    );
    Ok(())
}

fn raw_to_ship_record(id: &str, raw: &RawShip) -> Option<kobayashi::data::ship::ShipRecord> {
    let tier = raw.tiers.first()?;
    let mut armor_piercing = 0.0f64;
    let mut shield_piercing = 0.0f64;
    let mut accuracy = 0.0f64;
    let mut attack = 0.0f64;
    let mut crit_chance = 0.1f64;
    let mut crit_damage = 1.5f64;
    let mut shield_health = 0.0f64;
    let mut weapon_count = 0u32;

    for comp in &tier.components {
        if let Some(ref info) = comp.additional_info {
            if let Some(ref w) = info.weapons_info {
                weapon_count += 1;
                armor_piercing += w.armor_pierce;
                shield_piercing += w.shield_pierce;
                accuracy += w.accuracy;
                attack += (w.max_damage + w.min_damage) * 0.5;
                crit_chance = w.crit_chance;
                crit_damage = w.crit_damage;
            }
            if let Some(ref s) = info.shield_info {
                shield_health += s.shield_health;
            }
        }
    }
    if weapon_count > 0 {
        armor_piercing /= weapon_count as f64;
        shield_piercing /= weapon_count as f64;
        accuracy /= weapon_count as f64;
        attack *= weapon_count as f64;
    }
    if attack <= 0.0 {
        attack = 100.0;
    }
    if shield_health <= 0.0 {
        shield_health = 1000.0;
    }
    let hull_health = shield_health * 2.0;

    Some(kobayashi::data::ship::ShipRecord {
        id: id.to_string(),
        ship_name: raw.ship_name.clone(),
        ship_class: raw.ship_class.clone(),
        armor_piercing,
        shield_piercing,
        accuracy,
        attack,
        crit_chance,
        crit_damage,
        hull_health,
        shield_health,
    })
}
