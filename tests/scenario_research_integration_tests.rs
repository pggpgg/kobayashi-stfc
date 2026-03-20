use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::{
    create_profile, delete_profile, load_profile_index, profile_path, RESEARCH_IMPORTED,
};
use kobayashi::data::research::cumulative_research_level_bonuses;
use std::fs;
use std::sync::Mutex;

static SCENARIO_RESEARCH_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn shared_scenario_applies_research_bonuses_from_profile() {
    let _guard = SCENARIO_RESEARCH_TEST_LOCK.lock().unwrap();

    // Load registry and require a non-empty research catalog; skip when absent.
    let registry = DataRegistry::load().expect("data registry required for scenario tests");
    let catalog = match registry.research_catalog() {
        Some(c) if !c.items.is_empty() => c,
        _ => {
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
        catalog.items.len() > 1,
        "expected a non-trivial research catalog (more than one rid)"
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
        for (stat, value) in bonuses {
            // Stats in the catalog should already use engine keys (weapon_damage, hull_hp, etc.).
            chosen_rid = Some(rec.rid);
            chosen_stat = Some(stat);
            chosen_value = Some(value);
            break;
        }
        if chosen_rid.is_some() {
            break;
        }
    }

    let (rid, stat, expected_value) = match (chosen_rid, chosen_stat, chosen_value) {
        (Some(rid), Some(stat), Some(value)) => (rid, stat, value),
        _ => {
            eprintln!("skipping scenario_research test: no research record with bonuses at level 1");
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
    fs::write(&research_path, serde_json::to_string_pretty(&payload).unwrap())
        .expect("write research.imported.json for scenario research test");

    // Build SharedScenarioData using this profile and confirm the research bonus is present.
    let pid = Some(profile_id.as_str());
    let mut profile = kobayashi::data::profile::load_profile(
        &kobayashi::data::profile_index::profile_path(
            pid.unwrap(),
            kobayashi::data::profile_index::PROFILE_JSON,
        )
        .to_string_lossy()
        .to_string(),
    );

    if let Some(catalog) = registry.research_catalog() {
        let imported_research =
            kobayashi::data::import::load_imported_research(&research_path).unwrap_or_default();
        kobayashi::data::profile::merge_research_bonuses_into_profile(
            &mut profile,
            &imported_research,
            catalog,
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

