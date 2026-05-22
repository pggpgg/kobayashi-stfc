//! Normalize STFCcommunity raw JSON into KOBAYASHI hostiles (and optionally buildings/factions).
//! Reads data/upstream/stfccommunity-data/ (hostiles/*.json, etc.).
//! Writes data/hostiles/ with index.json. Ship output is no longer written (use data/ships_extended
//! from normalize_data_stfc_space + build_ship_registry.py instead). Run after scripts/fetch_stfc_data.ps1.

use std::fs;

use serde::Deserialize;

const UPSTREAM_HOSTILES_SUFFIX: &str = "data/upstream/stfccommunity-data";
const UPSTREAM_BUILDINGS_SUFFIX: &str = "data/upstream/stfccommunity-data/buildings";
const UPSTREAM_FACTION_REP_SUFFIX: &str = "data/upstream/stfccommunity-data/faction_reputation";
const OUT_HOSTILES_SUFFIX: &str = "data/hostiles";
const OUT_BUILDINGS_SUFFIX: &str = "data/buildings";
const OUT_FACTION_REP_SUFFIX: &str = "data/faction_reputation";
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

// ----- Raw STFCcommunity building (partial) -----
#[derive(Debug, Deserialize)]
struct RawBuildingBonusMeta {
    #[serde(default)]
    name: String,
    #[serde(default)]
    percentage: bool,
}

#[derive(Debug, Deserialize)]
struct RawBuildingLevel {
    #[serde(default)]
    level: u32,
    #[serde(default)]
    bonuses: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct RawBuilding {
    #[serde(default)]
    building_name: String,
    #[serde(default)]
    bonuses: std::collections::HashMap<String, RawBuildingBonusMeta>,
    #[serde(default)]
    levels: Vec<RawBuildingLevel>,
}

// ----- Raw STFCcommunity faction_reputation -----
#[derive(Debug, Deserialize)]
struct RawReputationTier {
    #[serde(default)]
    points_min: i64,
    #[serde(default)]
    reputation_id: u32,
    #[serde(default)]
    reputation_name: String,
}

#[derive(Debug, Deserialize)]
struct RawFactionReputation {
    #[serde(default)]
    faction: String,
    #[serde(default)]
    reputation: Vec<RawReputationTier>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_version =
        std::env::var("STFC_DATA_VERSION").unwrap_or_else(|_| "stfccommunity-main".to_string());

    let hostiles_dir = repo_data_path(UPSTREAM_HOSTILES_SUFFIX);
    let buildings_dir = repo_data_path(UPSTREAM_BUILDINGS_SUFFIX);
    let faction_rep_dir = repo_data_path(UPSTREAM_FACTION_REP_SUFFIX);
    let out_hostiles = repo_data_path(OUT_HOSTILES_SUFFIX);
    let out_buildings = repo_data_path(OUT_BUILDINGS_SUFFIX);
    let out_faction_rep = repo_data_path(OUT_FACTION_REP_SUFFIX);

    if !hostiles_dir.is_dir() {
        eprintln!(
            "error: upstream hostiles directory not found: {}",
            hostiles_dir.display()
        );
        eprintln!("Run the fetch script first: powershell -ExecutionPolicy Bypass -File scripts/fetch_stfc_data.ps1");
        std::process::exit(1);
    }
    fs::create_dir_all(&out_hostiles)?;

    // ----- Hostiles -----
    let mut hostile_index_entries: Vec<kobayashi::data::hostile::HostileIndexEntry> = Vec::new();
    {
        for entry in fs::read_dir(&hostiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let content = fs::read_to_string(&path)?;
                let raw: RawHostile =
                    serde_json::from_str(&content).unwrap_or_else(|_| RawHostile {
                        hostile_name: id.clone(),
                        level: 0,
                        ship_class: String::new(),
                        stats: RawHostileStats {
                            defense: RawHostileStatsDefense {
                                armor: 0.0,
                                dodge: 0.0,
                                shield_deflect: 0.0,
                            },
                            health: RawHostileStatsHealth {
                                hull_health: 0.0,
                                shield_health: 0.0,
                            },
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
                    shield_mitigation: None,
                    apex_barrier: 0.0,
                    isolytic_defense: 0.0,
                    mitigation_floor: None,
                    mitigation_ceiling: None,
                    mystery_mitigation_factor: None,
                    loca_id: None,
                    faction: None,
                    upstream_ship_type: 0,
                    hull_type_raw: 0,
                    rarity: 0,
                    is_scout: false,
                    is_outpost: false,
                    strength: 0,
                    systems: Vec::new(),
                    xp_amount: 0,
                    warp: 0,
                    warp_with_superhighway: 0,
                    stat_health: 0.0,
                    stat_defense: 0.0,
                    stat_attack: 0.0,
                    dpr: 0.0,
                    stat_strength: 0.0,
                    accuracy: 0.0,
                    armor_piercing: 0.0,
                    shield_piercing: 0.0,
                    crit_chance: 0.0,
                    crit_damage: 0.0,
                    hostile_tags: Vec::new(),
                    engagement_enemy_types: None,
                    components: Vec::new(),
                    ability: Vec::new(),
                    resources: Vec::new(),
                };
                hostile_index_entries.push(kobayashi::data::hostile::HostileIndexEntry {
                    id: rec.id.clone(),
                    hostile_name: rec.hostile_name.clone(),
                    level: rec.level,
                    ship_class: rec.ship_class.clone(),
                    rarity: None,
                    upstream_ship_type: None,
                    loca_id: None,
                });
                let out_path = out_hostiles.join(format!("{}.json", rec.id));
                fs::write(out_path, serde_json::to_string_pretty(&rec)?)?;
            }
        }
    }

    if hostile_index_entries.is_empty() {
        eprintln!(
            "warning: no hostile JSON files found in {}",
            hostiles_dir.display()
        );
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

    // ----- Buildings (optional: upstream may not have been fetched) -----
    let mut building_index_entries: Vec<kobayashi::data::building::BuildingIndexEntry> = Vec::new();
    if buildings_dir.is_dir() {
        fs::create_dir_all(&out_buildings)?;
        for entry in fs::read_dir(&buildings_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let content = fs::read_to_string(&path)?;
                let raw: RawBuilding =
                    serde_json::from_str(&content).unwrap_or_else(|_| RawBuilding {
                        building_name: id.clone(),
                        bonuses: std::collections::HashMap::new(),
                        levels: Vec::new(),
                    });
                let rec = raw_to_building_record(&id, &raw);
                building_index_entries.push(kobayashi::data::building::BuildingIndexEntry {
                    id: rec.id.clone(),
                    building_name: rec.building_name.clone(),
                    file: None,
                    bid: kobayashi::data::building::infer_building_bid(&rec.id, None),
                });
                let out_path = out_buildings.join(format!("{}.json", rec.id));
                fs::write(out_path, serde_json::to_string_pretty(&rec)?)?;
            }
        }
        let building_index = kobayashi::data::building::BuildingIndex {
            data_version: Some(data_version.clone()),
            source_note: Some(SOURCE_NOTE.to_string()),
            buildings: building_index_entries.clone(),
        };
        fs::write(
            out_buildings.join("index.json"),
            serde_json::to_string_pretty(&building_index)?,
        )?;
    }

    // ----- Faction reputation (optional) -----
    let mut faction_list: Vec<String> = Vec::new();
    if faction_rep_dir.is_dir() {
        fs::create_dir_all(&out_faction_rep)?;
        for entry in fs::read_dir(&faction_rep_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let content = fs::read_to_string(&path)?;
                let raw: RawFactionReputation =
                    serde_json::from_str(&content).unwrap_or_else(|_| RawFactionReputation {
                        faction: String::new(),
                        reputation: Vec::new(),
                    });
                let rec = kobayashi::data::faction_reputation::FactionReputationRecord {
                    faction: raw.faction.clone(),
                    reputation: raw
                        .reputation
                        .iter()
                        .map(|t| kobayashi::data::faction_reputation::ReputationTier {
                            points_min: t.points_min,
                            reputation_id: t.reputation_id,
                            reputation_name: t.reputation_name.clone(),
                        })
                        .collect(),
                };
                faction_list.push(rec.faction.clone());
                let out_path = out_faction_rep.join(format!("{}.json", rec.faction));
                fs::write(out_path, serde_json::to_string_pretty(&rec)?)?;
            }
        }
        let faction_index = kobayashi::data::faction_reputation::FactionReputationIndex {
            data_version: Some(data_version.clone()),
            source_note: Some(SOURCE_NOTE.to_string()),
            factions: faction_list.clone(),
        };
        fs::write(
            out_faction_rep.join("index.json"),
            serde_json::to_string_pretty(&faction_index)?,
        )?;
    }

    // ----- Registry -----
    let last_updated = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut registry = kobayashi::data::registry::Registry::new();
    registry.insert(
        "hostiles".to_string(),
        kobayashi::data::registry::DataSetEntry {
            source: "stfccommunity".to_string(),
            data_version: hostile_index.data_version.clone(),
            last_updated: Some(last_updated.clone()),
            path: "hostiles/index.json".to_string(),
        },
    );
    if !building_index_entries.is_empty() {
        registry.insert(
            "buildings".to_string(),
            kobayashi::data::registry::DataSetEntry {
                source: "stfccommunity".to_string(),
                data_version: Some(data_version.clone()),
                last_updated: Some(last_updated.clone()),
                path: "buildings/index.json".to_string(),
            },
        );
    }
    if !faction_list.is_empty() {
        registry.insert(
            "faction_reputation".to_string(),
            kobayashi::data::registry::DataSetEntry {
                source: "stfccommunity".to_string(),
                data_version: Some(data_version.clone()),
                last_updated: Some(last_updated.clone()),
                path: "faction_reputation/index.json".to_string(),
            },
        );
    }
    let registry_path = repo_data_path("data/registry.json");
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(registry_path, serde_json::to_string_pretty(&registry)?)?;

    // Validation: re-load index and one record each to ensure schema is loadable (only if we have data)
    if !hostile_index.hostiles.is_empty() {
        let hostile_index_path = out_hostiles.join("index.json");
        let re_hostile_index =
            kobayashi::data::hostile::load_hostile_index(hostile_index_path.to_str().unwrap())
                .ok_or("Failed to re-load hostile index")?;
        if let Some(first) = re_hostile_index.hostiles.first() {
            kobayashi::data::hostile::load_hostile_record(&out_hostiles, &first.id)
                .ok_or("Failed to re-load a hostile record")?;
        }
    }
    println!(
        "Normalized {} hostiles, {} buildings, {} factions. (Ships use data/ships_extended.) data_version={:?} source_note={:?}",
        hostile_index.hostiles.len(),
        building_index_entries.len(),
        faction_list.len(),
        hostile_index.data_version,
        hostile_index.source_note
    );
    Ok(())
}

/// Maps a human-readable building bonus label from the upstream dataset to an
/// engine/LCARS stat key used by the simulator. This table should stay
/// consistent with combat stat keys used in ship/hostile records and
/// `syndicate_combat`.
fn bonus_name_to_stat(name: &str) -> String {
    let normalized = name.trim();
    if normalized.is_empty() {
        return String::new();
    }

    let key = normalized.to_lowercase();

    // Station / defense platform combat bonuses.
    if key.contains("defense platform damage") || key.contains("defense platform dmg") {
        return "defense_platform_damage".to_string();
    }
    if key.contains("station hull health") || key.contains("station hull hp") {
        return "hull_hp".to_string();
    }
    if key.contains("station shield health") || key.contains("station shield hp") {
        return "shield_hp".to_string();
    }

    // Generic damage / crit / isolytic hooks, aligned with existing engine keys.
    if key.contains("weapon damage") || key.contains("ship damage") || key.contains("damage output")
    {
        return "weapon_damage".to_string();
    }
    if key.contains("critical damage") || key.contains("crit damage") {
        return "crit_damage".to_string();
    }
    if key.contains("isolytic damage") {
        return "isolytic_damage".to_string();
    }
    if key.contains("isolytic mitigation") || key.contains("isolytic defense") {
        return "isolytic_defense".to_string();
    }

    // Fallback: normalize to a snake_case-ish identifier so unknown labels
    // still have deterministic keys.
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphabetic() || c.is_numeric() {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn raw_to_building_record(
    id: &str,
    raw: &RawBuilding,
) -> kobayashi::data::building::BuildingRecord {
    let levels: Vec<kobayashi::data::building::BuildingLevel> = raw
        .levels
        .iter()
        .map(|lvl| {
            let bonuses: Vec<kobayashi::data::building::BonusEntry> = lvl
                .bonuses
                .iter()
                .map(|(key, value)| {
                    let (stat_label, percentage) = raw
                        .bonuses
                        .get(key)
                        .map(|m| (m.name.as_str(), m.percentage))
                        .unwrap_or((key.as_str(), false));
                    let stat = bonus_name_to_stat(stat_label);
                    // Normalize percentage-style values to fractional bonuses where
                    // possible so downstream consumers can rely on consistent units.
                    let normalized_value = if percentage { *value / 100.0 } else { *value };
                    kobayashi::data::building::BonusEntry {
                        stat,
                        value: normalized_value,
                        operator: "add".to_string(),
                        conditions: Vec::new(),
                        notes: None,
                    }
                })
                .collect();
            kobayashi::data::building::BuildingLevel {
                level: lvl.level,
                ops_min: None,
                ops_max: None,
                bonuses,
            }
        })
        .collect();
    kobayashi::data::building::BuildingRecord {
        id: id.to_string(),
        building_name: raw.building_name.clone(),
        data_version: None,
        source_note: None,
        levels,
    }
}
