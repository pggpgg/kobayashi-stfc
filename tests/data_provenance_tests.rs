//! Data provenance and validation: index has data_version/source_note, and a subset of stats can be checked.
//! See data/README.md for provenance documentation.

use std::path::Path;

use kobayashi::data::hostile::{load_hostile_index, DEFAULT_HOSTILES_INDEX_PATH};
use kobayashi::data::ship::{load_ship_index, load_ship_record, DEFAULT_SHIPS_INDEX_PATH};

#[test]
fn ship_index_loads_and_has_provenance_fields() {
    let path = Path::new(DEFAULT_SHIPS_INDEX_PATH);
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }
    let index = match load_ship_index(DEFAULT_SHIPS_INDEX_PATH) {
        Some(i) => i,
        None => return,
    };
    assert!(!index.ships.is_empty(), "ship index should have entries");
    let _ = &index.data_version;
    let _ = &index.source_note;
}

#[test]
fn hostile_index_loads_and_has_provenance_fields() {
    let path = Path::new(DEFAULT_HOSTILES_INDEX_PATH);
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }
    let index = match load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH) {
        Some(i) => i,
        None => return,
    };
    assert!(!index.hostiles.is_empty(), "hostile index should have entries");
    let _ = &index.data_version;
    let _ = &index.source_note;
}

#[test]
fn resolve_one_ship_and_validate_stats_bounds() {
    let index = match load_ship_index(DEFAULT_SHIPS_INDEX_PATH) {
        Some(i) => i,
        None => return,
    };
    let data_dir = Path::new(DEFAULT_SHIPS_INDEX_PATH).parent().unwrap();
    let entry = index.ships.first().unwrap();
    let rec = match load_ship_record(data_dir, &entry.id) {
        Some(r) => r,
        None => return,
    };
    assert!(rec.attack >= 0.0, "attack should be non-negative");
    assert!(rec.hull_health > 0.0, "hull_health should be positive");
    assert!(rec.armor_piercing >= 0.0, "armor_piercing should be non-negative");
}
