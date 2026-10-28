//! Quarantined native research must be visible to API callers.

use std::fs;
use std::path::PathBuf;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::server::api::{
    compare_crews_payload, optimize_payload, replay_optimize_seed_payload, simulate_payload,
};

struct SyntheticProfile {
    root: PathBuf,
    path: PathBuf,
}

impl Drop for SyntheticProfile {
    fn drop(&mut self) {
        if let (Ok(root), Ok(path)) = (self.root.canonicalize(), self.path.canonicalize()) {
            if path.parent() == Some(root.as_path())
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("research-warning-"))
            {
                fs::remove_dir_all(path).expect("remove synthetic test profile");
            }
        }
    }
}

fn profile(active_apex: bool) -> (SyntheticProfile, String) {
    fs::create_dir_all("profiles").unwrap();
    let root = fs::canonicalize("profiles").unwrap();
    let id = format!("research-warning-{}", uuid::Uuid::new_v4().simple());
    let path = root.join(&id);
    fs::create_dir(&path).unwrap();
    let dir = SyntheticProfile { root, path };
    let research = if active_apex {
        serde_json::json!({"research":[{"rid":2708500806_i64,"level":1}]})
    } else {
        serde_json::json!({"research":[]})
    };
    let buffs = if active_apex {
        serde_json::json!({"buffs":[{"bid":11214279,"level":1}]})
    } else {
        serde_json::json!({"buffs":[]})
    };
    fs::write(
        dir.path.join("research.imported.json"),
        research.to_string(),
    )
    .unwrap();
    fs::write(dir.path.join("buffs.imported.json"), buffs.to_string()).unwrap();
    (dir, id)
}

fn has_warning(response: &str, prefix: &str) -> bool {
    let json: serde_json::Value = serde_json::from_str(response).unwrap();
    json["warnings"].as_array().unwrap().iter().any(|warning| {
        warning.as_str().is_some_and(|warning| {
            warning.starts_with(prefix)
                && warning.contains("1 owned project(s)")
                && warning.contains("buff 11214279 rank 1: 300")
        })
    })
}

#[test]
fn research_omissions_reach_all_matching_api_responses_and_pvp_defender() {
    let (_attacker_dir, attacker) = profile(true);
    let (_empty_dir, empty) = profile(false);
    let (_defender_dir, defender) = profile(true);
    let registry = DataRegistry::load().expect("isolated registry");
    let crew = serde_json::json!({"captain":"Spock","bridge":["Kirk","Phlox"]});
    let simulate = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "crew":crew,
        "num_sims":1, "seed":11, "below_decks_slots":0
    });
    let response = simulate_payload(&registry, &simulate.to_string(), Some(&attacker)).unwrap();
    assert!(has_warning(&response, "Conditional Apex Barrier research"));

    let compare = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "crews":[crew, crew],
        "num_sims":1, "seed":11, "below_decks_slots":0
    });
    let response = compare_crews_payload(&registry, &compare.to_string(), Some(&attacker)).unwrap();
    assert!(has_warning(&response, "Conditional Apex Barrier research"));

    let replay = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "crew":crew,
        "sim_index":0, "seed":11
    });
    let response =
        replay_optimize_seed_payload(&registry, &replay.to_string(), Some(&attacker)).unwrap();
    assert!(has_warning(&response, "Conditional Apex Barrier research"));

    let optimize = serde_json::json!({
        "ship":"uss_saladin", "hostile":"1127817100", "sims":1, "seed":11,
        "max_candidates":1, "strategy":"tiered", "tiered_scout_sims":1,
        "tiered_top_k":1, "below_decks_slots":0,
        "warm_start_crews":[{"captain":"Spock","bridge":["Kirk","Phlox"],"below_decks":[]}]
    });
    let response = optimize_payload(&registry, &optimize.to_string(), Some(&attacker)).unwrap();
    assert!(has_warning(&response, "Conditional Apex Barrier research"));

    let pvp = serde_json::json!({
        "ship":"realta", "ship_tier":4, "ship_level":20, "hostile":"",
        "crew":{"captain":"718-0-2509d7","bridge":["kirk-1323b6","spock-c04738"]},
        "defender_ship":"stella", "defender_ship_tier":4, "defender_ship_level":25,
        "defender_profile_id":defender, "num_sims":1, "seed":42
    });
    let response = simulate_payload(&registry, &pvp.to_string(), Some(&empty)).unwrap();
    assert!(has_warning(
        &response,
        "Defender: Conditional Apex Barrier research"
    ));
    let mut pvp_compare = pvp.clone();
    pvp_compare["crews"] = serde_json::json!([pvp["crew"], pvp["crew"]]);
    let response =
        compare_crews_payload(&registry, &pvp_compare.to_string(), Some(&empty)).unwrap();
    assert!(has_warning(
        &response,
        "Defender: Conditional Apex Barrier research"
    ));

    let mut pvp_optimize = pvp.clone();
    pvp_optimize["sims"] = serde_json::json!(1);
    pvp_optimize["max_candidates"] = serde_json::json!(1);
    pvp_optimize["strategy"] = serde_json::json!("tiered");
    pvp_optimize["tiered_scout_sims"] = serde_json::json!(1);
    pvp_optimize["tiered_top_k"] = serde_json::json!(1);
    pvp_optimize["below_decks_slots"] = serde_json::json!(0);
    pvp_optimize["warm_start_crews"] = serde_json::json!([{
        "captain":"718-0-2509d7", "bridge":["kirk-1323b6","spock-c04738"], "below_decks":[]
    }]);
    let response = optimize_payload(&registry, &pvp_optimize.to_string(), Some(&empty)).unwrap();
    assert!(has_warning(
        &response,
        "Defender: Conditional Apex Barrier research"
    ));
}
