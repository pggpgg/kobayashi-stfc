//! Gorn Eviscerator Hunt the Hunters: level-scaled isolytic vs gorn_hunter-tagged hostiles only.

use kobayashi::combat::HOSTILE_TAG_MASK_GORN_HUNTER;
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;
use kobayashi::optimizer::monte_carlo::{replay_optimize_iteration_with_registry, DefenderOpponent};
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::data::data_registry::DataRegistry;
use std::sync::Arc;

#[test]
fn acrocanth_carries_gorn_hunter_tag() {
    let rec = resolve_hostile("447012258").expect("acrocanth");
    assert_eq!(rec.loca_id, Some(65101));
    assert!(
        rec.hostile_tags.iter().any(|t| t == "gorn_hunter"),
        "447012258 (Acrocanth) should have gorn_hunter tag, got {:?}",
        rec.hostile_tags
    );
    assert_ne!(rec.hostile_tag_mask() & HOSTILE_TAG_MASK_GORN_HUNTER, 0);
}

#[test]
fn gorn_eviscerator_hunt_the_hunters_scales_with_ship_level() {
    let rec = resolve_ship_with_tier_level("gorn_eviscerator", Some(10), Some(50))
        .expect("gorn ship T10 L50");
    let hunt = rec
        .abilities
        .as_ref()
        .and_then(|abs| abs.iter().find(|a| a.id == "1273528452"))
        .expect("Hunt the Hunters ability");
    assert!(
        (hunt.value - 60.0).abs() < 1e-6,
        "L50 Hunt the Hunters isolytic bonus should be 60 (6000% in UI), got {}",
        hunt.value
    );
    assert!(
        hunt
            .condition_opponent_hostile_tags
            .as_ref()
            .is_some_and(|t| t == &["gorn_hunter".to_string()])
    );
}

#[test]
fn hunt_the_hunters_isolytic_applies_only_vs_gorn_hunter_hostiles() {
    let ship_rec =
        resolve_ship_with_tier_level("gorn_eviscerator", Some(10), Some(50)).expect("ship record");
    let seats = ship_abilities_to_crew_seat_contexts(ship_rec.abilities.as_deref().unwrap_or(&[]));
    assert!(
        seats.iter().any(|s| s.ability.name == "1273528452"),
        "Hunt the Hunters should compile to a crew seat"
    );

    let registry = Arc::new(DataRegistry::load().expect("registry"));
    let candidate = CrewCandidate {
        captain: String::new(),
        bridge: vec![],
        below_decks: vec![],
    };

    let vs_acro = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        "gorn_eviscerator",
        "447012258",
        Some(10),
        Some(50),
        &candidate,
        0,
        0,
        None,
        500_000,
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        !vs_acro.using_placeholder_combatants,
        "gorn vs acrocanth should resolve"
    );
    assert!(
        vs_acro.total_isolytic_damage > 0.0,
        "isolytic damage expected vs gorn hunter (got {})",
        vs_acro.total_isolytic_damage
    );

    // Non-gorn-hunter hostile — Hunt the Hunters should not apply.
    let vs_other = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        "gorn_eviscerator",
        "35645340",
        Some(10),
        Some(50),
        &candidate,
        0,
        0,
        None,
        500_000,
        None,
        DefenderOpponent::Hostile,
    );
    if vs_other.using_placeholder_combatants {
        return;
    }
    assert!(
        vs_acro.total_isolytic_damage > vs_other.total_isolytic_damage,
        "Hunt the Hunters should boost isolytic only vs gorn hunters (acro iso={} other iso={})",
        vs_acro.total_isolytic_damage,
        vs_other.total_isolytic_damage
    );
}
