//! Deterministic tests for research catalog → profile merge (no dependency on `data/research_catalog.json`).

use kobayashi::data::import::ResearchEntry;
use kobayashi::data::profile::merge_research_bonuses_into_profile;
use kobayashi::data::profile::PlayerProfile;
use kobayashi::data::research::ResearchCatalog;

#[test]
fn merge_research_applies_fixture_catalog_weapon_damage() {
    let catalog: ResearchCatalog =
        serde_json::from_str(include_str!("fixtures/research/research_catalog_fixture.json"))
            .expect("parse fixture research catalog");

    let imported = vec![ResearchEntry {
        rid: 99000001,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);

    let w = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
    assert!(
        (w - 0.12).abs() < 1e-9,
        "expected weapon_damage 0.12 from fixture rid, got {w}"
    );
}

#[test]
fn merge_research_skips_unknown_rid_in_fixture_catalog() {
    let catalog: ResearchCatalog =
        serde_json::from_str(include_str!("fixtures/research/research_catalog_fixture.json"))
            .expect("parse fixture research catalog");

    let imported = vec![ResearchEntry {
        rid: 99999999,
        level: 5,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);
    assert!(profile.bonuses.is_empty());
}
