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

#[test]
fn mudd_names_match_stfc_translations_and_catalog_has_no_cross_officer_name_swaps() {
    let canonical: serde_json::Value =
        serde_json::from_str(include_str!("../data/officers/officers.canonical.json")).unwrap();
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../data/officers/id_registry.json")).unwrap();
    let summary: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../data/upstream/data-stfc-space/summary-officer.json"
    ))
    .unwrap();
    let names: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../data/upstream/data-stfc-space/translations-officer_names.json"
    ))
    .unwrap();

    let officers = canonical["officers"].as_array().unwrap();
    let harcourt = officers
        .iter()
        .find(|o| o["id"].as_str() == Some("harry-mudd-374d5f"))
        .unwrap();
    assert_eq!(harcourt["source_officer_id"], "677303054");
    assert_eq!(harcourt["name"], "Harcourt Fenton Mudd");
    assert_eq!(registry["677303054"], "harry-mudd-374d5f");

    let harry = officers
        .iter()
        .find(|o| o["id"].as_str() == Some("mudd-32546a"))
        .unwrap();
    assert_eq!(harry["source_officer_id"], "634330627");
    assert_eq!(harry["name"], "Harry Mudd");
    assert_eq!(registry["634330627"], "mudd-32546a");

    let loca_to_game_name: std::collections::HashMap<i64, &str> = names
        .iter()
        .filter(|row| row["key"].as_str() == Some("officer_name"))
        .filter_map(|row| Some((row["id"].as_i64()?, row["text"].as_str()?)))
        .collect();
    let oid_to_loca: std::collections::HashMap<String, i64> = summary
        .iter()
        .filter_map(|row| Some((row["id"].as_u64()?.to_string(), row["loca_id"].as_i64()?)))
        .collect();
    let canonical_by_lower_name: std::collections::HashMap<String, &str> = officers
        .iter()
        .filter_map(|o| Some((o["name"].as_str()?.to_ascii_lowercase(), o["id"].as_str()?)))
        .collect();

    let mut collisions = Vec::new();
    for officer in officers {
        let Some(oid) = officer["source_officer_id"].as_str() else {
            continue;
        };
        let Some(id) = officer["id"].as_str() else {
            continue;
        };
        let Some(canon_name) = officer["name"].as_str() else {
            continue;
        };
        let Some(game_name) = oid_to_loca
            .get(oid)
            .and_then(|loca| loca_to_game_name.get(loca))
        else {
            continue;
        };
        if !game_name.eq_ignore_ascii_case(canon_name) {
            if let Some(&other_id) = canonical_by_lower_name.get(&game_name.to_ascii_lowercase()) {
                if other_id != id {
                    collisions.push(format!(
                        "{id} (oid={oid}, canon={canon_name}, game={game_name}) collides with {other_id}"
                    ));
                }
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "catalog must not assign officer A's STFC English name to officer B: {collisions:?}"
    );
}
