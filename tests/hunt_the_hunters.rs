//! Gorn Eviscerator Hunt the Hunters: level-scaled isolytic vs gorn_hunter-tagged hostiles only.

use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode,
    HOSTILE_TAG_MASK_GORN_HUNTER,
};
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;

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
    assert!(hunt
        .condition_opponent_hostile_tags
        .as_ref()
        .is_some_and(|t| t == &["gorn_hunter".to_string()]));
}

#[test]
fn hunt_the_hunters_isolytic_applies_only_vs_gorn_hunter_hostiles() {
    let ship_rec =
        resolve_ship_with_tier_level("gorn_eviscerator", Some(10), Some(50)).expect("ship record");
    let hunt = ship_rec
        .abilities
        .as_ref()
        .and_then(|abilities| abilities.iter().find(|ability| ability.id == "1273528452"))
        .expect("Hunt the Hunters ability");
    let crew = CrewConfiguration {
        seats: ship_abilities_to_crew_seat_contexts(std::slice::from_ref(hunt)),
    };
    assert_eq!(crew.seats.len(), 1, "Hunt the Hunters must compile");

    // A fixed, noncritical single shot isolates the compiled ability's gate.
    // Different real enemies also change survival, defense, and shot counts, so
    // their full-fight isolytic totals cannot establish whether this gate works.
    let attacker = Combatant {
        id: "controlled_attacker".into(),
        attack: 1_000.0,
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    let defender = Combatant {
        id: "controlled_defender".into(),
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 0,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let untagged = simulate_combat(&attacker, &defender, &config, &crew);
    let tagged = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            defender_hostile_tag_mask: HOSTILE_TAG_MASK_GORN_HUNTER,
            ..config
        },
        &crew,
    );

    for (result, expected_activation, expected_isolytic) in
        [(&untagged, false, 0.0), (&tagged, true, 60_000.0)]
    {
        let activated = result.events.iter().any(|event| {
            event.event_type == "ability_activation"
                && event.source.ship_ability_id.as_deref() == Some("1273528452")
        });
        assert_eq!(activated, expected_activation);
        let shots: Vec<_> = result
            .events
            .iter()
            .filter(|event| {
                event.event_type == "damage_application"
                    && event.source.officer_id.as_deref() == Some("controlled_attacker")
            })
            .collect();
        assert_eq!(shots.len(), 1, "both controls must fire exactly one shot");
        assert_eq!(
            shots[0].values["isolytic_component"].as_f64(),
            Some(expected_isolytic),
            "L50 Hunt the Hunters adds 60 times the fixed 1,000-damage shot only when tagged"
        );
        assert_eq!(result.total_isolytic_damage, expected_isolytic);
        assert_eq!(result.total_damage, 1_000.0 + expected_isolytic);
        assert_eq!(
            result.defender_hull_remaining,
            defender.hull_health - result.total_damage
        );
    }
}
