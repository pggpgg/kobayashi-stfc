//! These bindings are verified against native OfficerSpec slot references and the
//! game's English LocaleDB, not the formerly swapped community name labels.
use kobayashi::combat::{AbilityEffect, CrewSeat};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

#[test]
fn client_identity_manifest_agrees_with_canonical_ids_registry_and_slot_abilities() {
    let proof: serde_json::Value = serde_json::from_str(include_str!(
        "../data/officers/client_identity_bindings_2026-10-23.json"
    ))
    .unwrap();
    let canonical: serde_json::Value =
        serde_json::from_str(include_str!("../data/officers/officers.canonical.json")).unwrap();
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../data/officers/id_registry.json")).unwrap();
    for binding in proof["officers"].as_array().unwrap() {
        let id = binding["canonical_id"].as_str().unwrap();
        let source_id = binding["source_officer_id"].as_str().unwrap();
        let row = canonical["officers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"].as_str() == Some(id))
            .unwrap();
        assert_eq!(row["source_officer_id"], source_id);
        assert_eq!(row["name"], binding["client_english_name"]);
        assert_eq!(registry[source_id], id);
        let below = row["abilities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["slot"] == "below_decks")
            .unwrap();
        assert_eq!(below["ability_id"], binding["below_decks_ability_id"]);
    }
}

#[test]
fn native_harry_kim_burns_while_native_neelix_supplies_morale() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let options = ResolveOptions {
        tier: Some(5),
        officer_tiers: None,
        officer_levels: None,
    };
    let resolve = |id: &str| {
        resolve_crew_to_buff_set("unused_captain", &[], &[id.into()], &officers, &options).crew
    };
    let kim = resolve("harry-kim-a79fdf");
    assert!(kim.seats.iter().any(|seat| seat.seat == CrewSeat::BelowDeck
        && matches!(
            seat.ability.effect,
            AbilityEffect::Burning {
                chance: 1.0,
                duration_rounds: 1
            }
        )));
    assert!(!kim
        .seats
        .iter()
        .any(|seat| matches!(seat.ability.effect, AbilityEffect::Morale(_))));
    let neelix = resolve("neelix-c8a380");
    assert!(neelix
        .seats
        .iter()
        .any(|seat| seat.seat == CrewSeat::BelowDeck
            && seat.ability.effect == AbilityEffect::Morale(1.0)));
    assert!(!neelix
        .seats
        .iter()
        .any(|seat| matches!(seat.ability.effect, AbilityEffect::Burning { .. })));
}
