//! Parity between upstream hostile `ability[]` ids and hostile_ability_catalog.json.

use std::collections::HashSet;
use std::path::PathBuf;

use kobayashi::data::hostile_ability_resolve::{
    collect_upstream_hostile_ability_ids, load_hostile_ability_catalog,
    DEFAULT_HOSTILE_ABILITY_CATALOG_PATH,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn hostile_ability_catalog_covers_all_upstream_ability_ids() {
    let upstream_dir = repo_root().join("data/upstream/data-stfc-space/hostiles");
    let upstream = collect_upstream_hostile_ability_ids(&upstream_dir);
    assert!(
        !upstream.is_empty(),
        "expected upstream hostile ability ids under {}",
        upstream_dir.display()
    );

    let catalog_path = repo_root().join(DEFAULT_HOSTILE_ABILITY_CATALOG_PATH);
    let catalog = load_hostile_ability_catalog(catalog_path.to_str().unwrap())
        .unwrap_or_else(|| panic!("failed to load {}", catalog_path.display()));

    let catalog_ids: HashSet<&str> = catalog.entries.keys().map(String::as_str).collect();
    let missing: Vec<&str> = upstream
        .keys()
        .map(String::as_str)
        .filter(|id| !catalog_ids.contains(id))
        .collect();

    assert!(
        missing.is_empty(),
        "catalog missing {} upstream ability ids (first 10): {:?}",
        missing.len(),
        &missing[..missing.len().min(10)]
    );
}

#[test]
fn hostile_ability_catalog_has_modeled_isolytic_and_apex_rows() {
    let catalog_path = repo_root().join(DEFAULT_HOSTILE_ABILITY_CATALOG_PATH);
    let catalog = load_hostile_ability_catalog(catalog_path.to_str().unwrap())
        .expect("hostile ability catalog should load");

    let modeled_isolytic = catalog
        .entries
        .values()
        .filter(|e| {
            matches!(
                e.effect_type.as_str(),
                "isolytic_damage" | "isolytic_defense"
            )
        })
        .count();
    let modeled_apex = catalog
        .entries
        .values()
        .filter(|e| matches!(e.effect_type.as_str(), "apex_barrier" | "apex_shred"))
        .count();

    assert!(
        modeled_isolytic >= 100,
        "expected substantial isolytic catalog coverage, got {modeled_isolytic}"
    );
    assert!(
        modeled_apex >= 50,
        "expected substantial apex catalog coverage, got {modeled_apex}"
    );
}
