//! Data provenance and validation: index has data_version/source_note, and a subset of stats can be checked.
//! See data/README.md for provenance documentation.

use std::path::Path;

use kobayashi::data::hostile::{load_hostile_index, DEFAULT_HOSTILES_INDEX_PATH};
use kobayashi::data::ship::{
    load_extended_ship_index, load_extended_ship_record, DEFAULT_SHIPS_EXTENDED_DIR,
};

/// Regression guard: pin the `ship_class` of four iconic ships from each hull-type bucket.
///
/// Background: `scripts/build_ship_registry.py` maps upstream `hull_type` (0..3) to STFC ship
/// classes. An earlier mapping had 0/2/3 wrong — only `1 → survey` was correct. This was caught
/// during the officer-A/D/H runtime work because the Cerritos was claiming `interceptor` while
/// its in-game Defense tooltip clearly routes to Shield (Explorer behavior). The corrected
/// mapping is `0 → interceptor, 1 → survey, 2 → explorer, 3 → battleship`.
///
/// One representative ship per class, picked because the maintainer cross-checked each against
/// the in-game ship browser. If this test fails after a registry refresh, either the upstream
/// `hull_type` values changed (possible but unlikely) or the `HULL_TO_CLASS` mapping in
/// `scripts/build_ship_registry.py` regressed — investigate before "fixing" the test.
#[test]
fn iconic_ships_have_expected_class() {
    let ext_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    if !ext_dir.is_dir() {
        eprintln!("Skipping: {} not found", ext_dir.display());
        return;
    }
    let cases: &[(&str, &str)] = &[
        ("uss_cerritos", "explorer"),
        ("uss_crozier", "battleship"),
        ("ss_revenant", "interceptor"),
        ("nova", "survey"),
    ];
    for (id, expected_class) in cases {
        let rec = load_extended_ship_record(ext_dir, id)
            .unwrap_or_else(|| panic!("ship `{}` not found in {}", id, ext_dir.display()));
        assert_eq!(
            rec.ship_class, *expected_class,
            "ship `{}` should be classified as `{}`; got `{}`. \
             If upstream `hull_type` changed, verify against the in-game ship browser before \
             updating this assertion or `scripts/build_ship_registry.py`.",
            id, expected_class, rec.ship_class
        );
    }
}

#[test]
fn ship_index_loads_and_has_provenance_fields() {
    let ext_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    if !ext_dir.is_dir() {
        eprintln!("Skipping: {} not found", ext_dir.display());
        return;
    }
    let index = match load_extended_ship_index(ext_dir) {
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
    assert!(
        !index.hostiles.is_empty(),
        "hostile index should have entries"
    );
    let _ = &index.data_version;
    let _ = &index.source_note;
}

#[test]
fn resolve_one_ship_and_validate_stats_bounds() {
    let ext_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    let index = match load_extended_ship_index(ext_dir) {
        Some(i) => i,
        None => return,
    };
    let entry = index.ships.first().unwrap();
    let extended = match load_extended_ship_record(ext_dir, &entry.id) {
        Some(r) => r,
        None => return,
    };
    let rec = match extended.to_ship_record(Some(1), Some(1)) {
        Some(r) => r,
        None => return,
    };
    assert!(rec.attack >= 0.0, "attack should be non-negative");
    assert!(rec.hull_health > 0.0, "hull_health should be positive");
    assert!(
        rec.armor_piercing >= 0.0,
        "armor_piercing should be non-negative"
    );
}

/// U.S.S. Cerritos tier 12 Shield Deflection: in-game-verified anchor for the officer-defense
/// channel (docs/OFFICER_STAT_FORMULA.md §2c).
///
/// The engine's `shield_deflection` is the in-game *Shield Deflection* stat, sourced by
/// `normalize_data_stfc_space` from upstream's legacy `Shield.absorption` field. (The upstream
/// `Deflector.deflection` field — a constant 120 on every ship — maps to no in-game concept and
/// is ignored.) The value 13,338 was verified in-game from the Cerritos Defense tooltip via the
/// Sesha L15 / Ghrush L30 experiments; it is the explorer defense-channel constant, so a
/// regression here silently breaks officer Defense routing for every explorer.
#[test]
fn cerritos_tier12_shield_deflection_matches_in_game_observation() {
    let ext_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    let extended = match load_extended_ship_record(ext_dir, "uss_cerritos") {
        Some(r) => r,
        None => {
            eprintln!("Skipping: uss_cerritos not in {}", ext_dir.display());
            return;
        }
    };
    let tier12 = extended
        .tiers
        .iter()
        .find(|t| t.tier == 12)
        .expect("Cerritos tier 12");
    assert_eq!(
        tier12.shield_deflection, 13338.0,
        "Cerritos T12 Shield Deflection is 13,338 (in-game verified, OFFICER_STAT_FORMULA.md §2c); \
         got {}. If a data refresh moved this, re-verify against the in-game Defense tooltip \
         before updating this anchor.",
        tier12.shield_deflection
    );
    // Real Shield Deflection grows with tier; the retired stale constant (120) did not.
    let tier1 = extended
        .tiers
        .iter()
        .find(|t| t.tier == 1)
        .expect("Cerritos tier 1");
    assert!(
        tier1.shield_deflection > 0.0 && tier1.shield_deflection < tier12.shield_deflection,
        "Shield Deflection should grow with tier; got t1 {} vs t12 {}",
        tier1.shield_deflection,
        tier12.shield_deflection
    );
}

/// Shieldless hulls stay shieldless: the Sarcophagus and Enterprise NX-01 have no shields
/// in-game (upstream `Shield: {hp: 0, absorption: 0}` on every tier). An earlier normalizer
/// fallback gave any zero value a phantom 1000 shield HP; this pins the faithful zero so the
/// fallback is not reintroduced. The engine routes all damage to hull when shields are 0
/// (`apply_shield_hull_split` overflow path).
#[test]
fn shieldless_ships_normalize_to_zero_shield_health() {
    let ext_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    for id in ["sarcophagus", "enterprise_nx_01"] {
        let extended = match load_extended_ship_record(ext_dir, id) {
            Some(r) => r,
            None => {
                eprintln!("Skipping: {} not in {}", id, ext_dir.display());
                continue;
            }
        };
        for tier in &extended.tiers {
            assert_eq!(
                tier.shield_health, 0.0,
                "{} tier {} should have zero shield_health (shieldless in-game); got {}",
                id, tier.tier, tier.shield_health
            );
        }
        let rec = extended
            .to_ship_record(Some(1), Some(1))
            .expect("tier 1 level 1 record");
        assert_eq!(
            rec.shield_health, 0.0,
            "{} resolved record should keep zero shield_health (no per-level shield bonuses)",
            id
        );
        assert!(
            rec.hull_health > 0.0,
            "{} must still have positive hull",
            id
        );
    }
}

/// USS Crozier tier 1: per-shot attack and shots from normalize_data_stfc_space (data-stfc.space).
#[test]
fn crozier_tier1_has_per_shot_attack_and_shots() {
    let ext_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    let extended = match load_extended_ship_record(ext_dir, "uss_crozier") {
        Some(r) => r,
        None => {
            eprintln!("Skipping: uss_crozier not in {}", ext_dir.display());
            return;
        }
    };
    let tier1 = extended.tiers.iter().find(|t| t.tier == 1).expect("tier 1");
    let weapons = match &tier1.weapons {
        Some(w) => w,
        None => {
            eprintln!("Skipping: Crozier tier 1 has no weapons array");
            return;
        }
    };
    assert!(weapons.len() >= 3, "Crozier has at least 3 weapons");
    assert_eq!(weapons[0].shots, Some(3), "primary weapon has 3 shots");
    assert_eq!(weapons[1].shots, Some(2), "second weapon has 2 shots");
    assert_eq!(weapons[2].shots, Some(2), "third weapon has 2 shots");
    // Per-shot damage (not total): primary ~192475, secondary ~258662
    assert!(
        (190_000.0..200_000.0).contains(&weapons[0].attack),
        "primary per-shot attack ~192475, got {}",
        weapons[0].attack
    );
    assert!(
        (255_000.0..265_000.0).contains(&weapons[1].attack),
        "secondary per-shot attack ~258662, got {}",
        weapons[1].attack
    );

    let abilities = extended
        .abilities
        .as_ref()
        .expect("ship_ability_catalog should map U.S.S. Crozier ability id 47269853");
    assert_eq!(abilities.len(), 1, "one hull ability row upstream");
    assert_eq!(abilities[0].id, "47269853");
    assert_eq!(abilities[0].effect_type, "hostile_crit_damage_reduction");
    assert!(
        (abilities[0].value - 0.02).abs() < 1e-12,
        "tier-1 value 0.02 as fractional reduction (2%), got {}",
        abilities[0].value
    );
    assert_eq!(abilities[0].duration_rounds, Some(5));
}
