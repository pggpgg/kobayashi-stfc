//! Officer-resolution sanity checks. Officer abilities resolve **solely** through LCARS
//! (`resolve_crew_to_buff_set`) — there is no placeholder/stub path (`KOBAYASHI_OFFICER_SOURCE` is
//! no longer consulted). These pin that the default registry loads LCARS, that the reference crew
//! resolves end to end, and that resolution is **per seat** (an unresolved captain doesn't drop the
//! rest of the crew).

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry, DefenderOpponent,
};
use std::sync::Arc;

/// The Enterprise-D reference crew (also used by the calibration trace).
fn reference_candidate() -> CrewCandidate {
    CrewCandidate {
        captain: "ent-e-picard-556227".to_string(),
        bridge: vec![
            "ent-e-data-871245".to_string(),
            "five-of-eleven-d9aa11".to_string(),
        ],
        below_decks: vec!["harry-kim-a79fdf (T5)".to_string()],
    }
}

/// Replay one fixed scenario for `candidate`; returns (lcars_loaded, total_damage).
fn replay_damage_for(candidate: &CrewCandidate) -> (bool, f64) {
    let registry = Arc::new(DataRegistry::load().expect("DataRegistry::load"));
    let lcars_loaded = registry.lcars_officers.is_some();
    let replay = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        "uss_enterprise_d",
        "kobayashi_theoretical_damage_sponge",
        Some(5),
        Some(7),
        candidate,
        42,
        0,
        Some(kobayashi::data::profile_index::DEMO_PROFILE_ID),
        0, // no trace events needed; we only compare totals
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        !replay.using_placeholder_combatants,
        "ship/hostile must resolve from data"
    );
    (lcars_loaded, replay.total_damage)
}

#[test]
fn default_loads_lcars_officers() {
    let registry = Arc::new(DataRegistry::load().expect("DataRegistry::load"));
    let officers = registry
        .lcars_officers
        .as_ref()
        .expect("LCARS officers must load (the sole ability source)");
    assert!(
        officers.len() > 100,
        "expected the full LCARS officer set, got {}",
        officers.len()
    );
}

#[test]
fn reference_crew_resolves_via_lcars() {
    let (lcars_loaded, dmg) = replay_damage_for(&reference_candidate());
    assert!(lcars_loaded, "registry must load LCARS officers");
    assert!(dmg > 0.0, "reference crew should deal damage; got {dmg}");
}

/// Per-seat resolution: an unresolved **captain** must not drop the rest of the crew. A crew with a
/// bogus captain but a real bridge/below resolves those seats via LCARS, so its damage differs
/// materially from an all-bogus crew (which resolves no officers at all).
#[test]
fn unresolved_captain_still_resolves_remaining_crew() {
    let bogus = "totally-bogus-officer-xyz-000000".to_string();
    let real_bridge = CrewCandidate {
        captain: bogus.clone(),
        bridge: vec![
            "ent-e-data-871245".to_string(),
            "five-of-eleven-d9aa11".to_string(),
        ],
        below_decks: vec!["harry-kim-a79fdf (T5)".to_string()],
    };
    let all_bogus = CrewCandidate {
        captain: bogus.clone(),
        bridge: vec![format!("{bogus}-a"), format!("{bogus}-b")],
        below_decks: vec![format!("{bogus}-c")],
    };

    let (_, with_bridge) = replay_damage_for(&real_bridge);
    let (_, no_officers) = replay_damage_for(&all_bogus);
    let rel = (with_bridge - no_officers).abs() / with_bridge.abs().max(1.0);
    assert!(
        rel > 1e-3,
        "a real bridge/below must resolve via LCARS even when the captain doesn't \
         (rel diff {rel:e}): with_bridge={with_bridge} no_officers={no_officers}"
    );
}
