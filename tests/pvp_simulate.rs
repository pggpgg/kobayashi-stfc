//! PvP ship-vs-ship simulate API and scenario builder.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::{load_profile_index, DEMO_PROFILE_ID};
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    pvp_scenario_params_from_api_fields, run_monte_carlo_with_registry, DefenderOpponent,
};
use kobayashi::server::api::{validate_scenario_target, ScenarioTarget, ScenarioTargetFields};

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
        .expect("enterprise");
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
        None,
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
