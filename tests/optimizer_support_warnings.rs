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
        "tiered_top_k":2, "below_decks_slots":0, "novelty_lambda":0.5,
        "novelty_diverse_top":2, "novelty_pool":2,
        "warm_start_crews":[
            {"captain":"Spock", "bridge":["Kirk","Phlox"], "below_decks":[]},
            {"captain":"Spock", "bridge":["Kirk","Ro Mudd"], "below_decks":[]}
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
            row["bridge"].as_array().unwrap().iter().any(|name| {
                name.as_str()
                    .is_some_and(|s| s.to_lowercase().contains("ro mudd") || s.contains("ro-mudd"))
            })
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
    let simulate_request = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "num_sims":1, "seed":11,
        "crew":{"captain":"Spock", "bridge":["Kirk","Tasha Yar"], "below_deck":[]}
    });
    let body = simulate_payload(
        &registry,
        &simulate_request.to_string(),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .expect("valid simulation request");
    let response: serde_json::Value = serde_json::from_str(&body).unwrap();
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
        "max_candidates":1, "strategy":"tiered", "tiered_scout_sims":5,
        "tiered_top_k":1, "below_decks_slots":0,
        "warm_start_crews":[
            {"captain":"Spock", "bridge":["Kirk","Tasha Yar"], "below_decks":[]}
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
        .expect("Tasha crew was not returned")
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
