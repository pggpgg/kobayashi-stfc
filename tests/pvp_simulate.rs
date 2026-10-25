//! PvP ship-vs-ship simulate API and scenario builder.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::{load_profile_index, DEMO_PROFILE_ID};
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    pvp_scenario_params_from_api_fields, run_monte_carlo_with_registry, DefenderOpponent,
};
use kobayashi::server::api::{validate_scenario_target, ScenarioTarget, ScenarioTargetFields};

#[test]
fn simulate_validates_defender_seats_using_the_defender_ship_schedule() {
    use kobayashi::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;
    use kobayashi::server::api::simulate_payload;
    let registry = DataRegistry::load().unwrap();
    let request = serde_json::json!({
        "ship":"realta", "ship_tier":4, "ship_level":20, "hostile":"",
        "below_decks_slots":0,
        "crew":{"captain":"718-0-2509d7","bridge":["kirk-1323b6","spock-c04738"]},
        "defender_ship":"stella", "defender_ship_tier":4,"defender_ship_level":25,
        "defender_profile_id":"demo",
        "defender_crew":{"captain":"718-0-2509d7","bridge":["kirk-1323b6","spock-c04738"],
            "below_deck":["scotty-a83cb5","uhura-ea117c","unresolved-defender-officer"]},
        "num_sims":1,"seed":42
    });
    let mut control = request.clone();
    control["defender_crew"]["below_deck"][2] = serde_json::json!("kira-nerys-a5253a");
    simulate_payload(
        &registry,
        &control.to_string(),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .expect("otherwise-valid PvP request must reach simulation");
    let error = simulate_payload(
        &registry,
        &request.to_string(),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .expect_err("third defender seat must be validated even when the attacker has zero BD slots");
    assert!(
        error.to_string().contains("defender_crew"),
        "wrong validation failure: {error}"
    );
}

fn second_profile_id() -> Option<String> {
    let index = load_profile_index();
    index
        .profiles
        .iter()
        .map(|p| p.id.clone())
        .find(|id| id != DEMO_PROFILE_ID)
}

#[test]
fn validate_scenario_target_pvp_and_pve() {
    let pve = validate_scenario_target(&ScenarioTargetFields {
        hostile: Some("2918121098".into()),
        ..Default::default()
    })
    .expect("pve");
    assert!(matches!(pve, ScenarioTarget::Pve { .. }));

    let pvp = validate_scenario_target(&ScenarioTargetFields {
        defender_ship: Some("rotarran".into()),
        defender_profile_id: Some("opponent".into()),
        ..Default::default()
    })
    .expect("pvp");
    assert!(matches!(pvp, ScenarioTarget::Pvp { .. }));
}

#[test]
fn mitigation_player_vs_player_is_deterministic() {
    use kobayashi::data::profile::PlayerProfile;
    use kobayashi::optimizer::monte_carlo::mitigation_and_pierce_for_player_vs_player;

    let registry = DataRegistry::load().expect("registry");
    let ent = registry
        .resolve_ship_with_tier_level("uss_enterprise_d", Some(5), Some(50))
        .expect("uss_enterprise");
    let rot = registry
        .resolve_ship_with_tier_level("rotarran", Some(5), Some(50))
        .expect("rotarran");
    let profile = PlayerProfile::default();
    let buffs = std::collections::HashMap::new();
    let (m1, p1) = mitigation_and_pierce_for_player_vs_player(&ent, &rot, &profile, &buffs);
    let (m2, p2) = mitigation_and_pierce_for_player_vs_player(&ent, &rot, &profile, &buffs);
    assert_eq!(m1, m2);
    assert_eq!(p1, p2);
    assert!(m1 > 0.0 && m1 < 1.0);
}

#[test]
fn pvp_simulate_resolves_ships_not_placeholder() {
    let opponent = match second_profile_id() {
        Some(id) => id,
        None => {
            eprintln!("skip pvp_simulate_resolves_ships_not_placeholder: need a second profile");
            return;
        }
    };
    let registry = DataRegistry::load().expect("registry");
    let pvp =
        pvp_scenario_params_from_api_fields(Some("rotarran"), Some(5), Some(50), Some(&opponent))
            .expect("pvp params");
    let candidate = CrewCandidate {
        captain: "ent-e-picard-556227".to_string(),
        bridge: vec![
            "ent-e-data-871245".to_string(),
            "five-of-eleven-d9aa11".to_string(),
        ],
        below_decks: vec![],
    };
    let defender_key = pvp.defender_ship.clone();
    let (_, placeholder) = run_monte_carlo_with_registry(
        &registry,
        "uss_enterprise_d",
        &defender_key,
        Some(5),
        Some(50),
        std::slice::from_ref(&candidate),
        64,
        42,
        Some(DEMO_PROFILE_ID),
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Player,
        None,
        Some(pvp),
    );
    assert!(
        !placeholder,
        "PvP should resolve both ships from ships_extended"
    );
}

#[test]
fn pvp_defender_support_buffs_sidecar_resolves() {
    use kobayashi::data::support_buffs;

    let registry = DataRegistry::load().expect("registry");
    let cat = registry.support_buffs_catalog().expect("catalog");
    let resolution = support_buffs::resolve_pvp_support_sidecars(
        cat,
        Some(&["titan_a_fortification".to_string()]),
        None,
    );
    assert_eq!(
        resolution.resolved_defender_support,
        vec!["titan_a_fortification"]
    );
    assert!(
        resolution
            .defender_static
            .get("crit_damage")
            .copied()
            .unwrap_or(1.0)
            > 1.0,
        "titan_a_fortification should raise defender crit_damage static bonus"
    );
}
