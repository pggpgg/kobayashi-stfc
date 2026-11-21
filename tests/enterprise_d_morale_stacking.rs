//! Enterprise-D's native round-start weapon bonus is earned only with Morale.
use kobayashi::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, SimulationConfig, TraceMode, WeaponStats, WeaponType, WeaponTypeScope,
};
use kobayashi::data::{
    loader::resolve_ship_with_tier_level,
    ship_ability_resolve::ship_abilities_to_crew_seat_contexts,
};

fn enterprise_crew(morale_chance: f64) -> CrewConfiguration {
    let ship = resolve_ship_with_tier_level("uss_enterprise_d", Some(1), Some(1))
        .expect("bundled Enterprise-D");
    let mut seats = ship_abilities_to_crew_seat_contexts(ship.abilities.as_deref().unwrap());
    seats.push(CrewSeatContext::legacy(
        CrewSeat::Bridge,
        Ability {
            name: "morale".into(),
            class: AbilityClass::BridgeAbility,
            timing: kobayashi::combat::TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::Morale(morale_chance),
            condition: None,
            weapon_scope: Default::default(),
        },
        false,
    ));
    CrewConfiguration { seats }
}

#[test]
fn growth_counts_only_morale_rounds_and_survives_a_morale_gap() {
    let attacker = Combatant {
        id: "enterprise-d".into(),
        attack: 300.0, // already includes a +200% profile weapon bonus
        hull_health: 1e9,
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 1e9,
        ..Default::default()
    };
    for pool in [None, Some(2.0)] {
        let config = SimulationConfig {
            rounds: 12,
            seed: 7,
            trace_mode: TraceMode::Events,
            profile_weapon_damage_fraction: 2.0,
            weapon_damage_profile_additive_pool: pool,
            ..Default::default()
        };
        let result = simulate_combat(&attacker, &defender, &config, &enterprise_crew(0.5));
        let mut successful_rounds = 0;
        let mut saw_gap_after_success = false;
        let mut saw_success_after_gap = false;
        for round in 1..=12 {
            let morale = result
                .events
                .iter()
                .find(|event| event.round_index == round && event.event_type == "morale_activation")
                .expect("round morale trace")
                .values["triggered"]
                .as_bool()
                .unwrap();
            if morale {
                successful_rounds += 1;
                saw_success_after_gap |= saw_gap_after_success;
            } else if successful_rounds > 0 {
                saw_gap_after_success = true;
            }
            let actual = result
                .events
                .iter()
                .find(|event| {
                    event.round_index == round
                        && event.event_type == "attack_roll"
                        && event.phase == "attack"
                })
                .expect("round attack trace")
                .values["effective_attack"]
                .as_f64()
                .unwrap();
            let expected = 100.0 * (3.0 + f64::from(successful_rounds) * 0.85);
            assert!((actual - expected).abs() < 1e-6,
                "round {round}, morale={morale}, earned={successful_rounds}, actual={actual}, expected={expected}");
        }
        assert!(
            saw_gap_after_success && saw_success_after_gap,
            "seed must exercise both a morale gap and a later successful round"
        );
        let mut without_events = result;
        without_events.events.clear();
        let untraced = simulate_combat(
            &attacker,
            &defender,
            &SimulationConfig {
                trace_mode: TraceMode::Off,
                ..config
            },
            &enterprise_crew(0.5),
        );
        assert_eq!(without_events, untraced);
    }
}

#[test]
fn always_and_never_morale_have_expected_growth() {
    let attacker = Combatant {
        attack: 100.0,
        hull_health: 1e9,
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e9,
        ..Default::default()
    };
    for chance in [0.0, 1.0] {
        let result = simulate_combat(
            &attacker,
            &defender,
            &SimulationConfig {
                rounds: 4,
                seed: 7,
                trace_mode: TraceMode::Events,
                ..Default::default()
            },
            &enterprise_crew(chance),
        );
        let attacks: Vec<_> = result
            .events
            .iter()
            .filter(|event| event.event_type == "attack_roll" && event.phase == "attack")
            .collect();
        assert_eq!(attacks.len(), 4);
        for (index, attack) in attacks.iter().enumerate() {
            let expected = 100.0 * (1.0 + chance * (index + 1) as f64 * 0.85);
            let actual = attack.values["effective_attack"].as_f64().unwrap();
            assert!(
                (actual - expected).abs() < 1e-6,
                "chance={chance}, actual={actual}"
            );
        }
    }
}

#[test]
fn earned_growth_remains_scoped_to_matching_weapons() {
    let mut crew = enterprise_crew(0.5);
    crew.seats
        .iter_mut()
        .find(|seat| seat.ability.name == "448699234")
        .unwrap()
        .ability
        .weapon_scope = WeaponTypeScope::EnergyOnly;
    let attacker = Combatant {
        attack: 100.0,
        hull_health: 1e9,
        weapons: [WeaponType::Energy, WeaponType::Kinetic]
            .into_iter()
            .map(|weapon_type| WeaponStats {
                attack: 100.0,
                weapon_type,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e9,
        ..Default::default()
    };
    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 12,
            seed: 7,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &crew,
    );
    let mut earned = 0;
    let mut saw_gap = false;
    for round in 1..=12 {
        let morale = result
            .events
            .iter()
            .find(|event| event.round_index == round && event.event_type == "morale_activation")
            .unwrap()
            .values["triggered"]
            .as_bool()
            .unwrap();
        if morale {
            earned += 1;
        } else if earned > 0 {
            saw_gap = true;
        }
        for (weapon, expected) in [(0, 100.0 * (1.0 + f64::from(earned) * 0.85)), (1, 100.0)] {
            let attack = result
                .events
                .iter()
                .find(|event| {
                    event.round_index == round
                        && event.event_type == "attack_roll"
                        && event.phase == "attack"
                        && event.weapon_index == Some(weapon)
                })
                .unwrap();
            let actual = attack.values["effective_attack"].as_f64().unwrap();
            assert!(
                (actual - expected).abs() < 1e-6,
                "round={round} weapon={weapon} actual={actual} expected={expected}"
            );
        }
    }
    assert!(
        saw_gap,
        "seed must exercise an inactive round after earned growth"
    );
}
