use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;

#[test]
fn imperial_might_uses_one_level_scaled_stack_per_round() {
    let attacker = Combatant {
        id: "iss_jellyfish".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 1_000.0,
            shots: Some(2),
            ..WeaponStats::default()
        }],
        ..Combatant::default()
    };
    let defender = Combatant {
        id: "synthetic_hostile".into(),
        hull_health: 50_000.0,
        ..Combatant::default()
    };
    let config = SimulationConfig {
        rounds: 3,
        trace_mode: TraceMode::Events,
        ..SimulationConfig::default()
    };

    for (tier, level, expected) in [
        (1, 1, [2_300.0, 2_600.0, 2_900.0]),
        (6, 30, [2_880.0, 3_760.0, 4_640.0]),
        (12, 60, [3_480.0, 4_960.0, 6_440.0]),
    ] {
        let ship = resolve_ship_with_tier_level("iss_jellyfish", Some(tier), Some(level))
            .expect("resolve the catalog ship at the requested tier and level");
        let crew = CrewConfiguration {
            seats: ship_abilities_to_crew_seat_contexts(
                ship.abilities.as_deref().expect("Imperial Might row"),
            ),
        };
        let mut traced = simulate_combat(&attacker, &defender, &config, &crew);
        assert_eq!(traced.rounds_simulated, 3);

        let mut damage_by_round = [0.0; 3];
        for event in traced
            .events
            .iter()
            .filter(|event| event.event_type == "damage_application")
        {
            damage_by_round[event.round_index as usize - 1] += event.values["hull_damage"]
                .as_f64()
                .expect("traced hull damage");
        }
        assert_eq!(damage_by_round, expected, "Imperial Might at level {level}");

        let untraced = simulate_combat(
            &attacker,
            &defender,
            &SimulationConfig {
                trace_mode: TraceMode::Off,
                ..config.clone()
            },
            &crew,
        );
        traced.events.clear();
        assert_eq!(traced, untraced, "trace parity at level {level}");
    }
}
