use kobayashi::combat::{
    component_mitigation, mitigation, serialize_events_json, simulate_combat, AttackerStats,
    CombatEvent, Combatant, DefenderStats, EventSource, ShipType, SimulationConfig, TraceCollector,
    TraceMode, EPSILON,
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

    let first = simulate_combat(&attacker, &defender, config);
    let second = simulate_combat(&attacker, &defender, config);

    assert_eq!(first.events, second.events);
    assert_eq!(first.total_damage, second.total_damage);
    assert_eq!(first.events.len(), 8);
    assert_eq!(first.events[0].event_type, "round_start");
    assert_eq!(first.events[1].event_type, "attack_roll");
    assert_eq!(first.events[2].event_type, "mitigation_calc");
    assert_eq!(first.events[3].event_type, "pierce_calc");
}
