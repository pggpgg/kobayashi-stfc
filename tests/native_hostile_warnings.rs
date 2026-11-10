//! Native source gaps stay visible at each simulation result surface.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::server::api::{
    compare_crews_payload, optimize_payload, replay_optimize_seed_payload, simulate_payload,
};
use serde_json::{json, Value};

fn gaps(response: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(response).unwrap();
    value["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| s.starts_with("V'ger Gorn Hunter combat model is partial:"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn native_hostile_gaps_reach_simulate_compare_replay_and_optimize() {
    let registry = DataRegistry::load().expect("public fixture registry");
    let profile = format!("native-hostile-unowned-{}", uuid::Uuid::new_v4().simple());
    let crew = json!({"captain":"Spock","bridge":["Kirk","Phlox"]});
    let request = json!({
        "ship":"uss_saladin", "hostile":"2978535446", "crew":crew,
        "num_sims":1, "seed":11, "below_decks_slots":0
    });
    let simulate = simulate_payload(&registry, &request.to_string(), Some(&profile)).unwrap();
    let expected = gaps(&simulate);
    assert!(!expected.is_empty());
    assert!(expected
        .iter()
        .any(|s| s.contains("crew-seat requirements")));
    assert!(expected.iter().any(|s| s.contains("Isolytic Defense")));
    let mut compare = request.clone();
    compare["crews"] = json!([crew, crew]);
    assert_eq!(
        gaps(&compare_crews_payload(&registry, &compare.to_string(), Some(&profile)).unwrap()),
        expected
    );
    let mut replay = request.clone();
    replay["sim_index"] = json!(0);
    assert_eq!(
        gaps(
            &replay_optimize_seed_payload(&registry, &replay.to_string(), Some(&profile)).unwrap()
        ),
        expected
    );
    let optimize = json!({
        "ship":"uss_saladin", "hostile":"2978535446", "sims":1, "seed":11,
        "max_candidates":1, "strategy":"tiered", "tiered_scout_sims":1,
        "tiered_top_k":1, "below_decks_slots":0,
        "warm_start_crews":[{"captain":"Spock","bridge":["Kirk","Phlox"],"below_decks":[]}]
    });
    assert_eq!(
        gaps(&optimize_payload(&registry, &optimize.to_string(), Some(&profile)).unwrap()),
        expected
    );
    let mut ordinary = request;
    ordinary["hostile"] = json!("1127817100");
    assert!(
        gaps(&simulate_payload(&registry, &ordinary.to_string(), Some(&profile)).unwrap())
            .is_empty()
    );
    ordinary["hostile"] = json!("");
    ordinary["defender_ship"] = json!("stella");
    ordinary["defender_profile_id"] = json!(profile);
    assert!(
        gaps(&simulate_payload(&registry, &ordinary.to_string(), Some(&profile)).unwrap())
            .is_empty()
    );
}
