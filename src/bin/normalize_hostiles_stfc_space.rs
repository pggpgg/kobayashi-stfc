//! Normalize data.stfc.space hostile detail JSON into `data/hostiles/`.
//! Reads `data/upstream/data-stfc-space/hostiles/*.json`.
//! Writes per-id JSON + `index.json`, merge-updates `data/registry.json` hostiles entry only.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use kobayashi::data::hostile::{
    hostile_hull_type_raw_to_ship_class, HostileFactionRef, HostileIndex, HostileIndexEntry,
    HostileRecord, HostileResourceDrop,
};
use kobayashi::data::hostile_loca::{
    load_hostile_loca_display_names, resolve_hostile_display_name,
};
use kobayashi::data::registry::merge_registry_entry;

const UPSTREAM_HOSTILES_SUFFIX: &str = "data/upstream/data-stfc-space/hostiles";
const OUT_HOSTILES_SUFFIX: &str = "data/hostiles";
const DEFAULT_SOURCE_NOTE: &str =
    "data.stfc.space hostile detail (cached under data/upstream/data-stfc-space/hostiles)";

fn repo_root() -> std::path::PathBuf {
    std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| Path::new(".").to_path_buf())
}

#[derive(Debug, Deserialize)]
struct RawUpstream {
    id: u64,
    #[serde(default)]
    loca_id: Option<Value>,
    #[serde(default)]
    faction: Option<HostileFactionRef>,
    level: Value,
    #[serde(default)]
    ship_type: u32,
    #[serde(default)]
    is_scout: bool,
    #[serde(default)]
    is_outpost: bool,
    #[serde(default)]
    hull_type: u32,
    #[serde(default)]
    rarity: u32,
    #[serde(default)]
    strength: Value,
    #[serde(default)]
    systems: Vec<Value>,
    #[serde(default)]
    xp_amount: u32,
    #[serde(default)]
    warp: u32,
    #[serde(default)]
    warp_with_superhighway: u32,
    #[serde(default)]
    components: Vec<Value>,
    #[serde(default)]
    resources: Vec<HostileResourceDrop>,
    #[serde(default, rename = "ability")]
    ability: Vec<Value>,
    #[serde(default)]
    stats: Option<RawStats>,
}

#[derive(Debug, Default, Deserialize)]
struct RawStats {
    #[serde(default)]
    health: f64,
    #[serde(default)]
    defense: f64,
    #[serde(default)]
    attack: f64,
    #[serde(default)]
    dpr: f64,
    #[serde(default)]
    strength: f64,
    #[serde(default)]
    hull_hp: f64,
    #[serde(default)]
    shield_hp: f64,
    #[serde(default)]
    armor: f64,
    #[serde(default)]
    absorption: f64,
    #[serde(default)]
    dodge: f64,
    #[serde(default)]
    accuracy: f64,
    #[serde(default)]
    armor_piercing: f64,
    #[serde(default)]
    shield_piercing: f64,
    #[serde(default)]
    critical_chance: f64,
    #[serde(default)]
    critical_damage: f64,
}

fn value_to_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().filter(|&i| i >= 0).map(|i| i as u64))
}

fn value_to_u32(v: &Value) -> u32 {
    value_to_u64(v)
        .and_then(|u| u32::try_from(u).ok())
        .unwrap_or(0)
}

fn shield_mitigation_from_components(components: &[Value]) -> Option<f64> {
    for c in components {
        let tag = c
            .get("data")
            .and_then(|d| d.get("tag"))
            .and_then(|t| t.as_str());
        if tag == Some("Shield") {
            if let Some(m) = c
                .get("data")
                .and_then(|d| d.get("mitigation"))
                .and_then(|x| x.as_f64())
            {
                return Some(m);
            }
        }
    }
    None
}

fn systems_from_values(vals: &[Value]) -> Vec<u64> {
    vals.iter().filter_map(value_to_u64).collect()
}

/// Upstream hostile JSON has no tag field; we merge [`HostileRecord::hostile_tags`] for known
/// Conqueror Borg Suppressor / Obliterator rows so Borg Sphere passives can gate in combat, and
/// for Update 89 Borg Sphere combat gating.
///
/// Conqueror Borg: names from `translations-navigation.json` (`marauder_name_only`):
/// **Conqueror Borg Suppressor** (`loca_id` 89050–89052), **Conqueror Borg Obliterator**
/// (`loca_id` 89053–89055). Extend if Scopely adds more `loca_id`s with the same combat family.
///
/// Sub-tags `conqueror_borg_suppressor` / `conqueror_borg_obliterator` drive combat resonance beams
/// (`crate::combat::conqueror_borg_beams`).
fn curated_hostile_tags_for_upstream(
    id: u64,
    loca_id: Option<u64>,
    faction_loca_id: Option<u64>,
) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let mut push_unique = |s: &str| {
        let t = s.to_string();
        if !tags.contains(&t) {
            tags.push(t);
        }
    };

    if faction_loca_id == Some(82001) {
        push_unique("aggregation_hostile");
    }

    // Gorn Hunters: Pteran (65100), Acrocanth (65101), Macronyx (65102) — `translations-navigation.json` marauder_name_only.
    if matches!(loca_id, Some(65100..=65102)) {
        push_unique("gorn_hunter");
    }

    const CONQUEROR_BORG_IDS: &[u64] = &[
        316662618, 467189343, 500305250, 622949343, 778146301, 80039078, 864202735, 1117012705,
        1137196075, 1361645982, 1361807236, 1379653470, 1458200336, 1472989983, 1851381248,
        1976922167, 2529890757, 2689858241, 2813867447, 3021977695, 3192494251, 3293454542,
        3385025265, 3644288642, 3767637067, 3820292784, 3907896092, 4008480588, 4082735163,
        4113208627, 4280642366,
    ];
    if CONQUEROR_BORG_IDS.contains(&id) {
        push_unique("conqueror_borg");
        if let Some(loca) = loca_id {
            if (89050..=89052).contains(&loca) {
                push_unique("conqueror_borg_suppressor");
            } else if (89053..=89055).contains(&loca) {
                push_unique("conqueror_borg_obliterator");
            }
        }
    }
    tags
}

fn raw_to_record(
    raw: RawUpstream,
    unknown_hull: &mut u32,
    loca_map: &HashMap<u64, String>,
) -> HostileRecord {
    let id = raw.id.to_string();
    let stats = raw.stats.unwrap_or_default();
    let loca_id = raw.loca_id.as_ref().and_then(value_to_u64);
    let level = value_to_u32(&raw.level);
    let strength = value_to_u64(&raw.strength).unwrap_or(0);
    let systems = systems_from_values(&raw.systems);

    let ship_class = match hostile_hull_type_raw_to_ship_class(raw.hull_type) {
        Some(c) => c.to_string(),
        None => {
            *unknown_hull += 1;
            eprintln!(
                "warning: unknown hull_type {} for hostile id {} — using battleship",
                raw.hull_type, id
            );
            "battleship".to_string()
        }
    };

    let shield_mitigation = shield_mitigation_from_components(&raw.components);
    let faction_loca_id = raw.faction.as_ref().and_then(|f| f.loca_id);
    let hostile_tags = curated_hostile_tags_for_upstream(raw.id, loca_id, faction_loca_id);

    HostileRecord {
        id: id.clone(),
        hostile_name: resolve_hostile_display_name(loca_map, loca_id, &format!("Hostile {}", id)),
        level,
        ship_class,
        armor: stats.armor,
        shield_deflection: stats.absorption,
        dodge: stats.dodge,
        hull_health: stats.hull_hp,
        shield_health: stats.shield_hp,
        shield_mitigation,
        apex_barrier: 0.0,
        isolytic_defense: 0.0,
        mitigation_floor: None,
        mitigation_ceiling: None,
        mystery_mitigation_factor: None,
        loca_id,
        faction: raw.faction,
        upstream_ship_type: raw.ship_type,
        hull_type_raw: raw.hull_type,
        rarity: raw.rarity,
        is_scout: raw.is_scout,
        is_outpost: raw.is_outpost,
        strength,
        systems,
        xp_amount: raw.xp_amount,
        warp: raw.warp,
        warp_with_superhighway: raw.warp_with_superhighway,
        stat_health: stats.health,
        stat_defense: stats.defense,
        stat_attack: stats.attack,
        dpr: stats.dpr,
        stat_strength: stats.strength,
        accuracy: stats.accuracy,
        armor_piercing: stats.armor_piercing,
        shield_piercing: stats.shield_piercing,
        crit_chance: stats.critical_chance,
        crit_damage: stats.critical_damage,
        hostile_tags,
        engagement_enemy_types: None,
        components: raw.components,
        ability: raw.ability,
        resources: raw.resources,
    }
}

/// Kobayashi-maintained hostile fixtures (e.g. theoretical damage sponge for tests) live under
/// `data/hostiles/kobayashi_*.json` and are not in the stfc.space upstream cache. Preserve them
/// in `index.json` across normalizer runs.
fn merge_kobayashi_fixture_index_entries(
    out_dir: &Path,
    index_entries: &mut Vec<HostileIndexEntry>,
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut merged = 0u32;
    for entry in fs::read_dir(out_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !stem.starts_with("kobayashi_") {
            continue;
        }
        if index_entries.iter().any(|e| e.id == stem) {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let rec: HostileRecord = serde_json::from_str(&content)?;
        index_entries.push(HostileIndexEntry {
            id: rec.id.clone(),
            hostile_name: rec.hostile_name.clone(),
            level: rec.level,
            ship_class: rec.ship_class.clone(),
            rarity: Some(rec.rarity),
            upstream_ship_type: Some(rec.upstream_ship_type),
            loca_id: rec.loca_id,
        });
        merged += 1;
    }
    index_entries.sort_by(|a, b| {
        let na: u64 = a.id.parse().unwrap_or(0);
        let nb: u64 = b.id.parse().unwrap_or(0);
        na.cmp(&nb).then_with(|| a.id.cmp(&b.id))
    });
    Ok(merged)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = repo_root();
    let upstream = repo.join(UPSTREAM_HOSTILES_SUFFIX);
    let out_dir = repo.join(OUT_HOSTILES_SUFFIX);

    if !upstream.is_dir() {
        eprintln!(
            "error: upstream hostiles directory not found: {}",
            upstream.display()
        );
        std::process::exit(1);
    }

    let data_version = std::env::var("STFCSPACE_HOSTILES_VERSION").unwrap_or_else(|_| {
        format!(
            "stfcspace-hostiles-{}",
            chrono::Utc::now().format("%Y-%m-%d")
        )
    });
    let source_note = std::env::var("STFCSPACE_HOSTILES_SOURCE_NOTE")
        .unwrap_or_else(|_| DEFAULT_SOURCE_NOTE.to_string());

    fs::create_dir_all(&out_dir)?;

    let loca_map = load_hostile_loca_display_names(&repo);
    let mut unknown_hull = 0u32;
    let mut parse_errors = 0u32;
    let mut records: Vec<HostileRecord> = Vec::new();

    for entry in fs::read_dir(&upstream)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: read {}: {e}", path.display());
                parse_errors += 1;
                continue;
            }
        };
        let raw: RawUpstream = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("warning: parse {}: {e}", path.display());
                parse_errors += 1;
                continue;
            }
        };
        records.push(raw_to_record(raw, &mut unknown_hull, &loca_map));
    }

    if records.is_empty() {
        eprintln!("error: no hostile JSON parsed from {}", upstream.display());
        std::process::exit(1);
    }

    records.sort_by(|a, b| {
        let na: u64 = a.id.parse().unwrap_or(0);
        let nb: u64 = b.id.parse().unwrap_or(0);
        na.cmp(&nb)
    });

    let mut index_entries: Vec<HostileIndexEntry> = Vec::with_capacity(records.len());
    for rec in &records {
        index_entries.push(HostileIndexEntry {
            id: rec.id.clone(),
            hostile_name: rec.hostile_name.clone(),
            level: rec.level,
            ship_class: rec.ship_class.clone(),
            rarity: Some(rec.rarity),
            upstream_ship_type: Some(rec.upstream_ship_type),
            loca_id: rec.loca_id,
        });
        let out_path = out_dir.join(format!("{}.json", rec.id));
        fs::write(out_path, serde_json::to_string_pretty(rec)?)?;
    }

    let fixtures_merged = merge_kobayashi_fixture_index_entries(&out_dir, &mut index_entries)?;

    let index = HostileIndex {
        data_version: Some(data_version.clone()),
        source_note: Some(source_note.clone()),
        hostiles: index_entries,
    };
    fs::write(
        out_dir.join("index.json"),
        serde_json::to_string_pretty(&index)?,
    )?;

    merge_registry_entry(&repo, "hostiles", &data_version, "hostiles/index.json")?;

    // Re-load validation (same as normalize_stfc_data)
    let hostile_index_path = out_dir.join("index.json");
    let re_index =
        kobayashi::data::hostile::load_hostile_index(hostile_index_path.to_str().unwrap())
            .ok_or("Failed to re-load hostile index")?;
    if let Some(first) = re_index.hostiles.first() {
        kobayashi::data::hostile::load_hostile_record(&out_dir, &first.id)
            .ok_or("Failed to re-load a hostile record")?;
    }

    println!(
        "Normalized {} hostiles from data.stfc.space cache. kobayashi_fixtures_merged={} unknown_hull_types={} parse_errors={} data_version={:?}",
        records.len(),
        fixtures_merged,
        unknown_hull,
        parse_errors,
        data_version
    );
    println!("source_note={:?}", source_note);
    println!("Registry hostiles entry updated (merge).");
    Ok(())
}
