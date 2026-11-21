//! Scope guards for hull abilities assembled from the shipped ship catalog.
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Combatant, CrewConfiguration,
    EnemyType, OpponentFactionTag, ShipType, SimulationConfig, TraceMode,
};
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;

fn crew_for_ship(ship: &str, tier: u32, level: u32) -> CrewConfiguration {
    let record = resolve_ship_with_tier_level(ship, Some(tier), Some(level)).unwrap();
    CrewConfiguration {
        seats: ship_abilities_to_crew_seat_contexts(record.abilities.as_deref().unwrap()),
    }
}

fn one_shot(
    crew: &CrewConfiguration,
    defender: &Combatant,
    defender_type: ShipType,
    npc: bool,
    wave: bool,
    trace_mode: TraceMode,
) -> kobayashi::combat::SimulationResult {
    let attacker = Combatant {
        id: "scope_attacker".into(),
        attack: 1_000.0,
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    let mut config = SimulationConfig {
        rounds: 1,
        seed: 17,
        trace_mode,
        ..Default::default()
    };
    if wave {
        config.engagement_enemy_types = EnemyType::Waves.into();
    }
    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        defender,
        &config,
        crew,
        OpponentFactionTag::Unknown,
        defender_type,
        ShipType::Explorer,
        npc,
        !npc,
        &CrewConfiguration::default(),
    )
}

#[test]
fn gladius_secret_weapon_applies_only_against_npc_hostiles() {
    let defender = Combatant {
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    for (tier, level, expected_npc_damage) in [(1, 1, 1_350.0), (9, 45, 1_800.0)] {
        let crew = crew_for_ship("gladius", tier, level);
        for (npc, expected) in [(true, expected_npc_damage), (false, 1_000.0)] {
            let result = one_shot(
                &crew,
                &defender,
                ShipType::Explorer,
                npc,
                false,
                TraceMode::Events,
            );
            let plain = one_shot(
                &crew,
                &defender,
                ShipType::Explorer,
                npc,
                false,
                TraceMode::Off,
            );
            assert_eq!(result.total_damage, expected, "Gladius L{level}, NPC={npc}");
            assert_eq!(result.total_damage, plain.total_damage);
            assert_eq!(
                result.defender_hull_remaining,
                plain.defender_hull_remaining
            );
        }
    }
}

#[test]
fn gs_31_wave_only_shred_does_not_weaken_barrier_in_other_fights() {
    let crew = crew_for_ship("gs_31", 15, 75);
    let defender = Combatant {
        hull_health: 1_000_000.0,
        apex_barrier: 10_000.0,
        ..Default::default()
    };
    let raider = resolve_hostile("80126960").expect("Apex Raiders Orion Master");
    assert_eq!(raider.faction.as_ref().unwrap().id, 3_505_432_050);
    for (defender_type, npc, wave) in [
        (ShipType::Battleship, true, false),
        (ShipType::Armada, true, false),
        (ShipType::Battleship, false, false),
        // The generic Waves flag cannot establish Solo Wave Defense.
        (raider.ship_type(), true, true),
    ] {
        let result = one_shot(
            &crew,
            &defender,
            defender_type,
            npc,
            wave,
            TraceMode::Events,
        );
        let plain = one_shot(&crew, &defender, defender_type, npc, wave, TraceMode::Off);
        assert_eq!(
            result.total_damage, 500.0,
            "{defender_type:?}, NPC={npc}, Waves={wave}"
        );
        assert_eq!(result.total_damage, plain.total_damage);
        assert_eq!(
            result.defender_hull_remaining,
            plain.defender_hull_remaining
        );
        assert!(!result.events.iter().any(|event| {
            event.event_type == "ability_activation"
                && event.source.ship_ability_id.as_deref() == Some("1773930736")
        }));
    }
}
