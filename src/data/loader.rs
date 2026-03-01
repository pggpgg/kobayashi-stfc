//! Load and resolve hostiles and ships by name/id. Graceful fallback when data missing.
//! Uses data/hostiles/index.json and data/ships/index.json (from STFCcommunity normalizer).

use std::path::Path;

use crate::data::hostile::{
    load_hostile_index, load_hostile_record, HostileIndex, HostileRecord,
    DEFAULT_HOSTILES_INDEX_PATH,
};
use crate::data::ship::{
    load_ship_index, load_ship_record, ShipIndex, ShipRecord, DEFAULT_SHIPS_INDEX_PATH,
};

/// Normalize a string for lookup: lowercase, collapse spaces/underscores.
fn normalize_lookup(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() || c == '_' { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

/// Resolve a hostile using a pre-loaded index. Used by DataRegistry.
pub fn resolve_hostile_with_index(
    index: &HostileIndex,
    data_dir: &Path,
    name_or_id: &str,
) -> Option<HostileRecord> {
    let normalized = normalize_lookup(name_or_id);

    if let Some(entry) = index.hostiles.iter().find(|e| normalize_lookup(&e.id) == normalized) {
        return load_hostile_record(data_dir, &entry.id);
    }
    for entry in &index.hostiles {
        let name_level = format!("{}_{}", normalize_lookup(&entry.hostile_name), entry.level);
        if name_level == normalized {
            return load_hostile_record(data_dir, &entry.id);
        }
        let name_space_level = format!("{} {}", normalize_lookup(&entry.hostile_name), entry.level);
        if normalize_lookup(&name_space_level) == normalized {
            return load_hostile_record(data_dir, &entry.id);
        }
    }
    let by_name: Vec<_> = index
        .hostiles
        .iter()
        .filter(|e| normalize_lookup(&e.hostile_name) == normalized)
        .collect();
    if by_name.len() == 1 {
        return load_hostile_record(data_dir, &by_name[0].id);
    }
    None
}

/// Resolve a ship using a pre-loaded index. Used by DataRegistry.
pub fn resolve_ship_with_index(
    index: &ShipIndex,
    data_dir: &Path,
    name_or_id: &str,
) -> Option<ShipRecord> {
    let normalized = normalize_lookup(name_or_id);
    for entry in &index.ships {
        if normalize_lookup(&entry.id) == normalized || normalize_lookup(&entry.ship_name) == normalized {
            return load_ship_record(data_dir, &entry.id);
        }
    }
    None
}

/// Resolve a hostile by id or by "name level" / "name_level". Returns None if index missing or no match.
pub fn resolve_hostile(name_or_id: &str) -> Option<HostileRecord> {
    let index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH)?;
    let data_dir = Path::new(DEFAULT_HOSTILES_INDEX_PATH).parent()?;
    resolve_hostile_with_index(&index, data_dir, name_or_id)
}

/// Resolve a ship by id or ship_name. Returns None if index missing or no match.
pub fn resolve_ship(name_or_id: &str) -> Option<ShipRecord> {
    let index = load_ship_index(DEFAULT_SHIPS_INDEX_PATH)?;
    let data_dir = Path::new(DEFAULT_SHIPS_INDEX_PATH).parent()?;
    resolve_ship_with_index(&index, data_dir, name_or_id)
}
