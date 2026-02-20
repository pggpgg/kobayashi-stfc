use kobayashi::combat::{
    aggregate_contributions, component_mitigation, mitigation, serialize_events_json,
    simulate_combat, Ability, AbilityClass, AbilityEffect, AttackerStats, CombatEvent, Combatant,
    CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats, EventSource, ShipType,
    SimulationConfig, StackContribution, StatStacking, TimingWindow, TraceCollector, TraceMode,
    EPSILON,
};
use serde_json::{Map, Value};

fn approx_eq(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "expected {b}, got {a}");
}

#[test]
fn component_mitigation_clamps_non_positive_piercing_to_epsilon() {
    let with_zero = component_mitigation(10.0, 0.0);
    let with_negative = component_mitigation(10.0, -5.0);
    let with_epsilon = component_mitigation(10.0, EPSILON);

    approx_eq(with_zero, with_epsilon, 1e-15);
    approx_eq(with_negative, with_epsilon, 1e-15);
}

#[test]
fn component_mitigation_clamps_negative_defense_to_zero() {
    let clamped = component_mitigation(-10.0, 5.0);
    let zero = component_mitigation(0.0, 5.0);

    approx_eq(clamped, zero, 1e-15);
}

#[test]
fn mitigation_output_is_bounded_for_extreme_inputs() {
    let low = mitigation(
        DefenderStats {
            armor: -1.0,
            shield_deflection: -5.0,
            dodge: -10.0,
        },
        AttackerStats {
            armor_piercing: 1e12,
            shield_piercing: 1e12,
            accuracy: 1e12,
        },
        ShipType::Survey,
    );
    let high = mitigation(
        DefenderStats {
            armor: 1e12,
            shield_deflection: 1e12,
            dodge: 1e12,
        },
        AttackerStats {
            armor_piercing: 0.0,
            shield_piercing: -1.0,
            accuracy: EPSILON / 2.0,
        },
        ShipType::Interceptor,
    );

    assert!(
        (0.0..=1.0).contains(&low),
        "low mitigation out of bounds: {low}"
    );
    assert!(
        (0.0..=1.0).contains(&high),
        "high mitigation out of bounds: {high}"
    );
}

#[test]
fn golden_values_match_python_reference_for_each_ship_type() {
    let defender = DefenderStats {
        armor: 100.0,
        shield_deflection: 80.0,
        dodge: 60.0,
    };
    let attacker = AttackerStats {
        armor_piercing: 50.0,
        shield_piercing: 40.0,
        accuracy: 30.0,
    };

    approx_eq(
        mitigation(defender, attacker, ShipType::Survey),
        0.5489034243492552,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Battleship),
        0.5914393181871193,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Explorer),
        0.5914393181871193,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Interceptor),
        0.5914393181871193,
        1e-12,
    );
}

#[test]
fn trace_collector_records_only_when_enabled() {
    let event = CombatEvent {
        event_type: "round_start".to_string(),
        round_index: 1,
        phase: "round".to_string(),
        source: EventSource {
            ship_ability_id: Some("baseline_round".to_string()),
            ..EventSource::default()
        },
        values: Map::new(),
    };

    let mut trace_on = TraceCollector::new(true);
    trace_on.record(event.clone());
    assert_eq!(trace_on.events().len(), 1);

    let mut trace_off = TraceCollector::new(false);
    trace_off.record(event);
    assert!(trace_off.events().is_empty());
}

#[test]
fn serialize_events_json_matches_python_shape() {
    let json = serialize_events_json(&[CombatEvent {
        event_type: "attack_roll".to_string(),
        round_index: 1,
        phase: "attack".to_string(),
        source: EventSource {
            officer_id: Some("nero".to_string()),
            ..EventSource::default()
        },
        values: Map::from_iter([("roll".to_string(), Value::from(0.617753))]),
    }])
    .expect("serialization should succeed");

    let parsed: Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(parsed[0]["event_type"], "attack_roll");
    assert_eq!(parsed[0]["round_index"], 1);
    assert_eq!(parsed[0]["phase"], "attack");
    assert_eq!(
        parsed[0]["source"],
        serde_json::json!({"officer_id": "nero"})
    );
    assert_eq!(parsed[0]["values"], serde_json::json!({"roll": 0.617753}));
}

#[test]
fn simulate_combat_uses_seed_and_emits_canonical_events() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 120.0,
        mitigation: 0.1,
        pierce: 0.15,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 10.0,
        mitigation: 0.35,
        pierce: 0.0,
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
        trace_mode: TraceMode::Events,
    };

    let crew = CrewConfiguration::default();
    let first = simulate_combat(&attacker, &defender, config, &crew);
    let second = simulate_combat(&attacker, &defender, config, &crew);

    assert_eq!(first.events, second.events);
    assert_eq!(first.total_damage, second.total_damage);
    assert_eq!(first.events.len(), 8);
    assert_eq!(first.events[0].event_type, "round_start");
    assert_eq!(first.events[1].event_type, "attack_roll");
    assert_eq!(first.events[2].event_type, "mitigation_calc");
    assert_eq!(first.events[3].event_type, "pierce_calc");
}

#[test]
fn stacking_additive_only_stacks() {
    let totals = aggregate_contributions(vec![
        StackContribution::base("attack", 100.0),
        StackContribution::base("attack", 50.0),
        StackContribution::base("attack", 25.0),
    ]);

    let attack = totals
        .get("attack")
        .expect("attack totals should be present");
    approx_eq(attack.base, 175.0, 1e-12);
    approx_eq(attack.modifier, 0.0, 1e-12);
    approx_eq(attack.flat, 0.0, 1e-12);
    approx_eq(attack.compose(), 175.0, 1e-12);
}

#[test]
fn stacking_modifier_only_stacks() {
    let totals = aggregate_contributions(vec![
        StackContribution::base("damage", 200.0),
        StackContribution::modifier("damage", 0.10),
        StackContribution::modifier("damage", 0.25),
    ]);

    let damage = totals
        .get("damage")
        .expect("damage totals should be present");
    approx_eq(damage.modifier, 0.35, 1e-12);
    approx_eq(damage.compose(), 270.0, 1e-12);
}

#[test]
fn stacking_mixed_category_stacks() {
    let totals = aggregate_contributions(vec![
        StackContribution::base("crit", 100.0),
        StackContribution::modifier("crit", 0.40),
        StackContribution::flat("crit", 35.0),
    ]);

    let crit = totals.get("crit").expect("crit totals should be present");
    approx_eq(crit.compose(), 175.0, 1e-12);
}

#[test]
fn stacking_is_order_independent_within_categories() {
    let contributions = vec![
        StackContribution::base("attack", 100.0),
        StackContribution::base("attack", 50.0),
        StackContribution::modifier("attack", 0.30),
        StackContribution::modifier("attack", 0.20),
        StackContribution::flat("attack", 10.0),
        StackContribution::flat("attack", 5.0),
    ];

    let ordered = aggregate_contributions(contributions.clone());
    let mut reversed_contribs = contributions;
    reversed_contribs.reverse();
    let reversed = aggregate_contributions(reversed_contribs);

    let ordered_totals = ordered
        .get("attack")
        .expect("ordered attack totals should exist");
    let reversed_totals = reversed
        .get("attack")
        .expect("reversed attack totals should exist");

    approx_eq(ordered_totals.base, reversed_totals.base, 1e-12);
    approx_eq(ordered_totals.modifier, reversed_totals.modifier, 1e-12);
    approx_eq(ordered_totals.flat, reversed_totals.flat, 1e-12);
    approx_eq(ordered_totals.compose(), reversed_totals.compose(), 1e-12);

    let mut stacking = StatStacking::new();
    stacking.add_many(vec![
        StackContribution::base("shield", 75.0),
        StackContribution::modifier("shield", 0.5),
        StackContribution::flat("shield", 8.0),
    ]);
    approx_eq(
        stacking
            .composed_for(&"shield")
            .expect("shield value should exist"),
        120.5,
        1e-12,
    );
}

#[test]
fn crew_slot_gating_matrix_controls_activation() {
    let captain_ability = Ability {
        name: "captain_strike".to_string(),
        class: AbilityClass::CaptainManeuver,
        timing: TimingWindow::AttackPhase,
        boostable: true,
        effect: AbilityEffect::AttackMultiplier(0.2),
    };
    let bridge_ability = Ability {
        name: "bridge_targeting".to_string(),
        class: AbilityClass::BridgeAbility,
        timing: TimingWindow::AttackPhase,
        boostable: true,
        effect: AbilityEffect::PierceBonus(0.1),
    };

    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.15,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.5,
        pierce: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 9,
        trace_mode: TraceMode::Events,
    };

    let valid_crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: captain_ability.clone(),
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: bridge_ability.clone(),
                boosted: false,
            },
        ],
    };
    let wrong_seat_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: captain_ability,
            boosted: false,
        }],
    };

    let valid = simulate_combat(&attacker, &defender, config, &valid_crew);
    let wrong = simulate_combat(&attacker, &defender, config, &wrong_seat_crew);

    assert!(valid.total_damage > wrong.total_damage);
    assert_eq!(
        valid
            .events
            .iter()
            .filter(|event| event.event_type == "ability_activation")
            .count(),
        2
    );
    assert!(wrong
        .events
        .iter()
        .all(|event| event.event_type != "ability_activation"));
}

#[test]
fn boosted_non_boostable_abilities_are_filtered_out() {
    let non_boostable = Ability {
        name: "steady_hands".to_string(),
        class: AbilityClass::BridgeAbility,
        timing: TimingWindow::AttackPhase,
        boostable: false,
        effect: AbilityEffect::AttackMultiplier(0.5),
    };

    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.1,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.2,
        pierce: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Events,
    };

    let boosted = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: non_boostable.clone(),
            boosted: true,
        }],
    };
    let unboosted = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: non_boostable,
            boosted: false,
        }],
    };

    let boosted_result = simulate_combat(&attacker, &defender, config, &boosted);
    let unboosted_result = simulate_combat(&attacker, &defender, config, &unboosted);

    assert!(unboosted_result.total_damage > boosted_result.total_damage);
    assert!(boosted_result
        .events
        .iter()
        .all(|event| event.event_type != "ability_activation"));
    assert_eq!(
        unboosted_result
            .events
            .iter()
            .filter(|event| event.event_type == "ability_activation")
            .count(),
        1
    );
}
