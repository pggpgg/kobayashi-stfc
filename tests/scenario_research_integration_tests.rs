use kobayashi::combat::Combatant;
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::import::ResearchEntry;
use kobayashi::data::profile::{
    apply_profile_to_attacker, merge_research_bonuses_into_profile, PlayerProfile,
};
use kobayashi::data::profile_index::{
    create_profile, delete_profile, load_profile_index, profile_path, RESEARCH_IMPORTED,
};
use kobayashi::data::research::cumulative_research_level_bonuses;
use std::fs;
use std::sync::Mutex;

static SCENARIO_RESEARCH_TEST_LOCK: Mutex<()> = Mutex::new(());

/// When true, missing/empty catalog fails the test (used in CI). Locally, the test skips instead.
fn strict_research_catalog_required() -> bool {
    matches!(std::env::var("CI").ok().as_deref(), Some("true"))
        || matches!(
            std::env::var("KOBAYASHI_REQUIRE_RESEARCH_CATALOG")
                .ok()
                .as_deref(),
            Some("1") | Some("true") | Some("yes")
        )
}

const RESEARCH_CATALOG_HELP: &str = "Populate data/research_catalog.json (tracked in git). \
Regenerate from cached upstream research JSON: \
node scripts/import_stfcspace_research.mjs --from-upstream --limit 0 \
(prerequisites: data/upstream/data-stfc-space/research/*.json — see data/README.md § Research and scripts/README.md).";

#[test]
fn shared_scenario_applies_research_bonuses_from_profile() {
    let _guard = SCENARIO_RESEARCH_TEST_LOCK.lock().unwrap();

    // Load registry and require a non-empty research catalog; in CI, fail with a clear message.
    let registry = DataRegistry::load().expect("data registry required for scenario tests");
    let catalog = match registry.research_catalog() {
        Some(c) if !c.items.is_empty() => c,
        _ => {
            if strict_research_catalog_required() {
                panic!(
                    "research catalog missing or empty (data/research_catalog.json). {RESEARCH_CATALOG_HELP}"
                );
            }
            eprintln!("skipping scenario_research test: research catalog missing or empty");
            return;
        }
    };

    // Ensure we are validating the real import pipeline (and not the legacy stub catalog).
    assert_ne!(
        catalog.source.as_deref(),
        Some("kobayashi_stub"),
        "expected regenerated research_catalog.json (not kobayashi_stub)"
    );
    assert!(
        catalog.items.len() >= 50,
        "expected a broad research catalog (regenerate via scripts/import_stfcspace_research.mjs)"
    );

    // Find a research record with at least one combat-relevant bonus at level 1.
    let mut chosen_rid: Option<i64> = None;
    let mut chosen_stat: Option<String> = None;
    let mut chosen_value: Option<f64> = None;

    for rec in &catalog.items {
        let bonuses = cumulative_research_level_bonuses(rec, 1);
        if bonuses.is_empty() {
            continue;
        }
        if let Some((stat, value)) = bonuses.into_iter().next() {
            // Stats in the catalog should already use engine keys (weapon_damage, hull_hp, etc.).
            chosen_rid = Some(rec.rid);
            chosen_stat = Some(stat);
            chosen_value = Some(value);
        }
        if chosen_rid.is_some() {
            break;
        }
    }

    let (rid, stat, expected_value) = match (chosen_rid, chosen_stat, chosen_value) {
        (Some(rid), Some(stat), Some(value)) => (rid, stat, value),
        _ => {
            eprintln!(
                "skipping scenario_research test: no research record with bonuses at level 1"
            );
            return;
        }
    };

    // Create a dedicated test profile and write research.imported.json with the chosen rid.
    let mut index = load_profile_index();
    let entry = create_profile(&mut index, None, "Scenario Research Test")
        .expect("create test profile for scenario research");
    let profile_id = entry.id.clone();

    let research_path = profile_path(&profile_id, RESEARCH_IMPORTED)
        .to_string_lossy()
        .to_string();

    let payload = serde_json::json!({
        "source_path": "scenario_research_integration_test",
        "research": [
            { "rid": rid, "level": 1 }
        ]
    });
    if let Some(parent) = std::path::Path::new(&research_path).parent() {
        fs::create_dir_all(parent).expect("create research.imported.json parent dir");
    }
    fs::write(
        &research_path,
        serde_json::to_string_pretty(&payload).unwrap(),
    )
    .expect("write research.imported.json for scenario research test");

    // Build SharedScenarioData using this profile and confirm the research bonus is present.
    let profile_json = kobayashi::data::profile_index::profile_path(
        &profile_id,
        kobayashi::data::profile_index::PROFILE_JSON,
    );
    let mut profile =
        kobayashi::data::profile::load_profile(profile_json.to_string_lossy().as_ref());

    if let Some(catalog) = registry.research_catalog() {
        let imported_research =
            kobayashi::data::import::load_imported_research(&research_path).unwrap_or_default();
        kobayashi::data::profile::merge_research_bonuses_into_profile(
            &mut profile,
            &imported_research,
            catalog,
            None,
        );
    }

    let actual = profile.bonuses.get(&stat).copied().unwrap_or(0.0);
    let diff = (actual - expected_value).abs();
    assert!(
        diff < 1e-9,
        "expected research bonus {}={} from rid {}, got {} (diff {})",
        stat,
        expected_value,
        rid,
        actual,
        diff
    );

    // Cleanup: remove the test profile so we don't leave clutter in profiles/.
    let mut index = load_profile_index();
    let _ = delete_profile(&mut index, &profile_id);
}

/// Picks a catalog `rid` with `apex_shred` at level 1 and checks merge + `apply_profile_to_attacker`.
#[test]
fn catalog_apex_research_round_trips_through_profile_combatant() {
    let _guard = SCENARIO_RESEARCH_TEST_LOCK.lock().unwrap();

    let registry = DataRegistry::load().expect("data registry required for scenario tests");
    let catalog = match registry.research_catalog() {
        Some(c) if !c.items.is_empty() => c,
        _ => {
            if strict_research_catalog_required() {
                panic!(
                    "research catalog missing or empty (data/research_catalog.json). {RESEARCH_CATALOG_HELP}"
                );
            }
            eprintln!("skipping catalog_apex test: research catalog missing or empty");
            return;
        }
    };

    let mut chosen_rid: Option<i64> = None;
    let mut expected_apex: Option<f64> = None;
    for rec in &catalog.items {
        let bonuses = cumulative_research_level_bonuses(rec, 1);
        if let Some(v) = bonuses.get("apex_shred").copied() {
            if v > 0.0 {
                chosen_rid = Some(rec.rid);
                expected_apex = Some(v);
                break;
            }
        }
    }

    let (rid, apex_val) = match (chosen_rid, expected_apex) {
        (Some(r), Some(v)) => (r, v),
        _ => {
            eprintln!("skipping catalog_apex test: no rid with apex_shred at level 1");
            return;
        }
    };

    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(
        &mut profile,
        &[ResearchEntry { rid, level: 1 }],
        catalog,
        None,
    );

    let merged = profile.bonuses.get("apex_shred").copied().unwrap_or(0.0);
    assert!(
        (merged - apex_val).abs() < 1e-6,
        "expected profile apex_shred {} from rid {}, got {}",
        apex_val,
        rid,
        merged
    );

    let attacker = Combatant {
        id: "test".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.01,
        weapons: vec![],
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        hostile_mitigation_params: None,
    };
    let out = apply_profile_to_attacker(attacker, &profile, None);
    assert!(
        (out.apex_shred - (0.01 + apex_val)).abs() < 1e-6,
        "expected combatant apex_shred base 0.01 + research {}",
        apex_val
    );
}
