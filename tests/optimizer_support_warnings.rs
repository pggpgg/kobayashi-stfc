//! Ranked support warnings must describe the final displayed order.
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;
use kobayashi::server::api::{optimize_payload, simulate_payload};

fn listed_ranks(warning: &str) -> Vec<usize> {
    let ranks = warning
        .strip_prefix("Ranked result(s) ")
        .unwrap()
        .split_once(" may omit")
        .unwrap()
        .0;
    ranks
        .split(", ")
        .flat_map(|item| {
            if let Some((a, b)) = item.split_once('–') {
                (a.parse::<usize>().unwrap()..=b.parse::<usize>().unwrap()).collect::<Vec<_>>()
            } else {
                vec![item.parse::<usize>().unwrap()]
            }
        })
        .collect()
}

#[test]
fn optimizer_warnings_identify_the_final_rank_after_novelty_ordering() {
    let registry = DataRegistry::load().expect("registry");
    let request = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "sims":10, "seed":11,
        "max_candidates":2, "strategy":"tiered", "tiered_scout_sims":5,
        // Retain all 4 scout candidates (2 generated + 2 warm-start) through confirmation
        // and novelty re-ordering so catalog additions cannot evict the flagged warm-start crew.
        "tiered_top_k":4, "below_decks_slots":0, "novelty_lambda":0.5,
        "novelty_diverse_top":4, "novelty_pool":4,
        "warm_start_crews":[
            {"captain":"Spock", "bridge":["Kirk","Phlox"], "below_decks":[]},
            {"captain":"Beverly Crusher", "bridge":["TMP Hikaru Sulu","Shev"], "below_decks":[]}
        ]
    });
    let body = optimize_payload(
        &registry,
        &request.to_string(),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .expect("valid optimizer request");
    let response: serde_json::Value = serde_json::from_str(&body).unwrap();
    let recommendations = response["recommendations"]
        .as_array()
        .expect("recommendations");
    let unsupported_rank = recommendations
        .iter()
        .position(|row| {
            row["bridge"]
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name.as_str().is_some_and(|s| s == "Shev"))
        })
        .unwrap_or_else(|| {
            panic!("unsupported warm-start crew must remain in the ranked result: {response}")
        })
        + 1;
    assert!(
        unsupported_rank > 1,
        "fixture must exercise a non-top flagged crew: {response}"
    );
    let warnings = response["warnings"].as_array().unwrap();
    let warning = warnings
        .iter()
        .filter_map(|v| v.as_str())
        .find(|text| text.starts_with("Ranked result(s) "))
        .expect("missing ranked warning");
    assert!(
        listed_ranks(warning).contains(&unsupported_rank),
        "rank {unsupported_rank} is absent from {warning}"
    );
}

#[test]
fn unknown_battle_type_is_reported_in_simulation_and_final_optimizer_rank() {
    let registry = DataRegistry::load().expect("registry");

    // On a known battle type (standard red space hostile -> CombatBattleType 2), Tasha Yar's
    // CombatBattleType condition resolves cleanly with NO unknown-battle-type warning.
    // Use non-op-5 crewmates (Kirk + Uhura) so missing officer levels under
    // `NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS` do not emit an unrelated crew-stat warning.
    let known_simulate_request = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "num_sims":1, "seed":11,
        "crew":{"captain":"Kirk", "bridge":["Uhura","Tasha Yar"], "below_deck":[]}
    });
    let known_body = simulate_payload(
        &registry,
        &known_simulate_request.to_string(),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .expect("valid known battle-type simulation request");
    let known_response: serde_json::Value = serde_json::from_str(&known_body).unwrap();
    assert_eq!(
        known_response["resolved_context"]["combat_battle_type_id"],
        2
    );
    assert!(
        !known_response["warnings"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|v| v.as_str())
            .any(|s| s.contains("combat ability") && s.contains("cannot be evaluated")),
        "known battle-type scenario must not emit unknown-battle-type warning: {known_response}"
    );

    // On an unverified battle type (`assaults` -> combat_battle_type_id = None), Tasha Yar's
    // CombatBattleType gate fails closed with a diagnostic warning in both simulate and optimize.
    let simulate_request = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "num_sims":1, "seed":11,
        "enemy_type":"assaults",
        "crew":{"captain":"Kirk", "bridge":["Uhura","Tasha Yar"], "below_deck":[]}
    });
    let body = simulate_payload(
        &registry,
        &simulate_request.to_string(),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .expect("valid simulation request");
    let response: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        response["resolved_context"]["combat_battle_type_id"].is_null(),
        "assaults should keep unknown combat_battle_type_id: {response}"
    );
    assert!(
        response["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .any(|s| s.contains("combat ability") && s.contains("cannot be evaluated")),
        "missing unsupported battle-type warning: {response}"
    );

    let request = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "sims":10, "seed":11,
        "enemy_type":"assaults", "wave_defense":true,
        "max_candidates":1, "strategy":"tiered", "tiered_scout_sims":5,
        "tiered_top_k":1, "below_decks_slots":0,
        // Exercise warning propagation independently of combat scores. A warm
        // start alone does not guarantee that this crew wins the final ranking.
        "constraints":{"captain_must_be":"Kirk", "bridge_must_include":["Uhura","Tasha Yar"]},
        "warm_start_crews":[
            {"captain":"Kirk", "bridge":["Uhura","Tasha Yar"], "below_decks":[]}
        ]
    });
    let body = optimize_payload(
        &registry,
        &request.to_string(),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .expect("valid optimizer request");
    let response: serde_json::Value = serde_json::from_str(&body).unwrap();
    let rank = response["recommendations"]
        .as_array()
        .unwrap()
        .iter()
        .position(|row| {
            row["bridge"].as_array().unwrap().iter().any(|name| {
                name.as_str()
                    .is_some_and(|s| s.to_lowercase().contains("tasha") || s.contains("tasha-yar"))
            })
        })
        .unwrap_or_else(|| panic!("constrained Tasha crew was not returned: {response}"))
        + 1;
    let warning = response["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .find(|s| s.starts_with("Ranked result(s) "))
        .expect("missing ranked warning");
    assert!(
        listed_ranks(warning).contains(&rank),
        "battle-type warning omits rank {rank}: {warning}"
    );
}
