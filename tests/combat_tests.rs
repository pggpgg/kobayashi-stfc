use kobayashi::combat::{
    aggregate_contributions, apply_morale_primary_piercing, component_mitigation, isolytic_damage,
    mitigation, mitigation_with_morale, pierce_damage_through_bonus, serialize_events_json,
    simulate_combat, Ability, AbilityClass, AbilityEffect, AttackerStats, CombatEvent, Combatant,
    CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats, EventSource, ShipType,
    SimulationConfig, StackContribution, StatStacking, TimingWindow, TraceCollector, TraceMode,
    EPSILON, PIERCE_CAP,
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
        mitigation(defender, attacker, ShipType::Armada),
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
fn pierce_damage_through_bonus_derived_from_mitigation() {
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
    for ship_type in [
        ShipType::Survey,
        ShipType::Battleship,
        ShipType::Explorer,
        ShipType::Interceptor,
    ] {
        let mit = mitigation(defender, attacker, ship_type);
        let pierce = pierce_damage_through_bonus(defender, attacker, ship_type);
        approx_eq(pierce, PIERCE_CAP * (1.0 - mit), 1e-12);
        assert!(pierce >= 0.0 && pierce <= PIERCE_CAP);
    }
}

#[test]
fn armada_mitigation_matches_survey_for_identical_stats() {
    let defender = DefenderStats {
        armor: 320.0,
        shield_deflection: 275.0,
        dodge: 145.0,
    };
    let attacker = AttackerStats {
        armor_piercing: 210.0,
        shield_piercing: 180.0,
        accuracy: 110.0,
    };

    approx_eq(
        mitigation(defender, attacker, ShipType::Armada),
        mitigation(defender, attacker, ShipType::Survey),
        1e-12,
    );
}

#[test]
fn morale_boosts_only_primary_piercing_per_ship_type() {
    let attacker = AttackerStats {
        armor_piercing: 100.0,
        shield_piercing: 80.0,
        accuracy: 60.0,
    };

    let battleship = apply_morale_primary_piercing(attacker, ShipType::Battleship);
    approx_eq(battleship.shield_piercing, 88.0, 1e-12);
    approx_eq(battleship.armor_piercing, 100.0, 1e-12);
    approx_eq(battleship.accuracy, 60.0, 1e-12);

    let interceptor = apply_morale_primary_piercing(attacker, ShipType::Interceptor);
    approx_eq(interceptor.armor_piercing, 110.0, 1e-12);
    approx_eq(interceptor.shield_piercing, 80.0, 1e-12);
    approx_eq(interceptor.accuracy, 60.0, 1e-12);

    let explorer = apply_morale_primary_piercing(attacker, ShipType::Explorer);
    approx_eq(explorer.accuracy, 66.0, 1e-12);
    approx_eq(explorer.armor_piercing, 100.0, 1e-12);
    approx_eq(explorer.shield_piercing, 80.0, 1e-12);

    let survey = apply_morale_primary_piercing(attacker, ShipType::Survey);
    approx_eq(survey.armor_piercing, 100.0, 1e-12);
    approx_eq(survey.shield_piercing, 80.0, 1e-12);
    approx_eq(survey.accuracy, 60.0, 1e-12);

    let armada = apply_morale_primary_piercing(attacker, ShipType::Armada);
    approx_eq(armada.armor_piercing, 100.0, 1e-12);
    approx_eq(armada.shield_piercing, 80.0, 1e-12);
    approx_eq(armada.accuracy, 60.0, 1e-12);
}

#[test]
fn mitigation_with_morale_applies_primary_piercing_bonus_when_active() {
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

    let baseline = mitigation_with_morale(defender, attacker, ShipType::Battleship, false);
    let morale = mitigation_with_morale(defender, attacker, ShipType::Battleship, true);

    approx_eq(
        baseline,
        mitigation(defender, attacker, ShipType::Battleship),
        1e-12,
    );
    assert!(
        morale < baseline,
        "morale should lower mitigation and increase final damage"
    );
    approx_eq(morale, 0.5869213146636679, 1e-12);
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
fn apex_barrier_reduces_damage_and_apex_shred_weakens_barrier() {
    // One round, no mitigation/pierce/crit/proc: damage = attack. Apex factor = 10000/(10000+effective_barrier).
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender_no_barrier = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender_10k_barrier = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 10_000.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
    };
    let crew = CrewConfiguration::default();

    let no_barrier = simulate_combat(&attacker, &defender_no_barrier, config, &crew);
    let with_10k_barrier = simulate_combat(&attacker, &defender_10k_barrier, config, &crew);
    // 10k barrier, 0 shred: factor = 10000/(10000+10000) = 0.5 → 50% damage gets through.
    approx_eq(no_barrier.total_damage, 200.0, 1e-12);
    approx_eq(with_10k_barrier.total_damage, 100.0, 1e-12);

    let attacker_100_pct_shred = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 1.0, // 100% shred
    };
    let with_shred = simulate_combat(&attacker_100_pct_shred, &defender_10k_barrier, config, &crew);
    // Effective barrier = 10000/(1+1) = 5000, factor = 10000/(10000+5000) = 2/3. Engine rounds total_damage.
    approx_eq(with_shred.total_damage, 200.0 * (10000.0 / 15000.0), 0.01);
}

/// Shield mitigation (STFC Toolbox game-mechanics): S * damage to shield, (1-S) * damage to hull.
/// When shields are depleted, all damage goes to hull.
#[test]
fn shield_mitigation_splits_damage_between_shield_and_hull() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    // Defender with 500 SHP, 80% shield mitigation → 80% of damage to shield, 20% to hull.
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 500.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
    };
    let result = simulate_combat(&attacker, &defender, config, &CrewConfiguration::default());
    // 200 damage: 80% = 160 to shield, 20% = 40 to hull.
    approx_eq(result.total_damage, 200.0, 1e-12);
    approx_eq(result.defender_shield_remaining, 500.0 - 160.0, 1e-12);
    approx_eq(result.defender_hull_remaining, 1000.0 - 40.0, 1e-12);
}

#[test]
fn shield_overflow_goes_to_hull_when_shields_depleted_mid_round() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 1000.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    // Defender has only 100 SHP; 80% of 1000 = 800 to shield → 100 absorbed, 700 overflow to hull. 20% = 200 to hull. Total hull = 900.
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 2000.0,
        shield_health: 100.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
    };
    let result = simulate_combat(&attacker, &defender, config, &CrewConfiguration::default());
    approx_eq(result.total_damage, 1000.0, 1e-12);
    approx_eq(result.defender_shield_remaining, 0.0, 1e-12);
    approx_eq(result.defender_hull_remaining, 2000.0 - 900.0, 1e-12); // 900 hull damage (200 + 700 overflow)
}

#[test]
fn when_shields_depleted_all_damage_goes_to_hull_next_rounds() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 500.0,
        shield_health: 50.0, // Round 1: 80% of 100 = 80 to shield → 50 absorbed, 30 overflow; 20% = 20 to hull. Shield gone. Hull takes 20+30 = 50.
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 7,
        trace_mode: TraceMode::Off,
    };
    let result = simulate_combat(&attacker, &defender, config, &CrewConfiguration::default());
    approx_eq(result.defender_shield_remaining, 0.0, 1e-12);
    // Round 1: 50 hull damage. Round 2 and 3: 100% to hull = 100 each. Total hull damage = 50 + 100 + 100 = 250.
    assert!(result.defender_hull_remaining <= (500.0 - 250.0) + 1.0);
    assert!(result.defender_hull_remaining >= (500.0 - 250.0) - 1.0);
}

#[test]
fn officer_apex_shred_bonus_at_combat_begin_increases_damage_through_barrier() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 10_000.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
    };
    let crew_no_apex = CrewConfiguration::default();
    let crew_with_apex_shred = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "Officer (Apex Shred)".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ApexShredBonus(0.15),
            },
            boosted: false,
        }],
    };

    let without = simulate_combat(&attacker, &defender, config, &crew_no_apex);
    let with_ability = simulate_combat(&attacker, &defender, config, &crew_with_apex_shred);
    // Without officer: factor = 10000/(10000+10000) = 0.5 → 100 damage.
    approx_eq(without.total_damage, 100.0, 1e-12);
    // With +15% Apex Shred: effective_barrier = 10000/1.15 ≈ 8695.65, factor ≈ 10000/18695.65 ≈ 0.535 → ~107 damage.
    assert!(
        with_ability.total_damage > without.total_damage,
        "officer Apex Shred should increase damage through barrier"
    );
    approx_eq(with_ability.total_damage, 200.0 * (10000.0 / (10000.0 + 10_000.0 / 1.15)), 0.5);
}

#[test]
fn officer_apex_barrier_bonus_at_combat_begin_reduces_damage_taken() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender_no_bonus = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 5_000.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
    };
    let crew_no_apex = CrewConfiguration::default();
    let crew_with_apex_barrier = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "Officer (Apex Barrier)".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ApexBarrierBonus(5000.0),
            },
            boosted: false,
        }],
    };

    let without = simulate_combat(&attacker, &defender_no_bonus, config, &crew_no_apex);
    let with_ability = simulate_combat(&attacker, &defender_no_bonus, config, &crew_with_apex_barrier);
    // Defender has 5k base barrier; officer adds 5k → effective 10k. Without officer: factor = 10000/15000 = 2/3 → 133.33. With officer: factor = 10000/20000 = 0.5 → 100.
    assert!(
        with_ability.total_damage < without.total_damage,
        "officer Apex Barrier bonus should reduce damage taken"
    );
    approx_eq(without.total_damage, 200.0 * (10000.0 / 15000.0), 0.5);
    approx_eq(with_ability.total_damage, 100.0, 0.5);
}

#[test]
fn below_deck_morale_effect_triggers_morale_and_increases_damage() {
    let attacker = Combatant {
        id: "enterprise".to_string(),
        attack: 120.0,
        mitigation: 0.1,
        pierce: 0.15,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 10.0,
        mitigation: 0.35,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let no_morale = CrewConfiguration::default();
    let morale_below_decks = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: Ability {
                name: "round_start_morale".to_string(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::Morale(1.0),
            },
            boosted: false,
        }],
    };

    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
        trace_mode: TraceMode::Events,
    };

    let baseline = simulate_combat(&attacker, &defender, config, &no_morale);
    let with_morale = simulate_combat(&attacker, &defender, config, &morale_below_decks);

    assert!(with_morale.total_damage > baseline.total_damage);

    let morale_events = with_morale
        .events
        .iter()
        .filter(|event| event.event_type == "morale_activation")
        .count();
    assert_eq!(morale_events, 2);
}

#[test]
fn assimilated_reduces_officer_effectiveness_by_twenty_five_percent() {
    let attacker = Combatant {
        id: "enterprise".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let baseline_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "damage_buff".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: true,
                effect: AbilityEffect::AttackMultiplier(1.0),
            },
            boosted: false,
        }],
    };

    let assimilated_crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::BelowDeck,
                ability: Ability {
                    name: "dezoc_like_assimilate".to_string(),
                    class: AbilityClass::BelowDeck,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::Assimilated {
                        chance: 1.0,
                        duration_rounds: 2,
                    },
                },
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "damage_buff".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::AttackPhase,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(1.0),
                },
                boosted: false,
            },
        ],
    };

    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Events,
    };

    let baseline = simulate_combat(&attacker, &defender, config, &baseline_crew);
    let with_assimilated = simulate_combat(&attacker, &defender, config, &assimilated_crew);

    approx_eq(baseline.total_damage, 200.0, 1e-12);
    approx_eq(with_assimilated.total_damage, 175.0, 1e-12);

    let attack_activation = with_assimilated
        .events
        .iter()
        .find(|event| {
            event.event_type == "ability_activation"
                && event.phase == "attack"
                && event.source.ship_ability_id.as_deref() == Some("damage_buff")
        })
        .expect("attack ability activation should be present");
    assert_eq!(attack_activation.values["assimilated"], Value::Bool(true));
    approx_eq(
        attack_activation.values["effectiveness_multiplier"]
            .as_f64()
            .expect("effectiveness multiplier as f64"),
        0.75,
        1e-12,
    );
}

#[test]
fn dezoc_style_assimilated_can_trigger_from_below_decks() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: Ability {
                name: "Dezoc".to_string(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::Assimilated {
                    chance: 1.0,
                    duration_rounds: 4,
                },
            },
            boosted: false,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 1,
            seed: 7,
            trace_mode: TraceMode::Events,
        },
        &crew,
    );

    let trigger_event = result
        .events
        .iter()
        .find(|event| event.event_type == "assimilated_trigger")
        .expect("assimilated trigger event should be emitted");
    assert_eq!(trigger_event.phase, "round_start");
    assert_eq!(trigger_event.values["triggered"], Value::Bool(true));
    assert_eq!(
        trigger_event.source.ship_ability_id.as_deref(),
        Some("Dezoc")
    );
}

#[test]
fn hull_breach_boosts_critical_damage_after_crit_multiplier() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 1.0,
        crit_multiplier: 2.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "Lorca".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::HullBreach {
                    chance: 1.0,
                    duration_rounds: 2,
                    requires_critical: false,
                },
            },
            boosted: false,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 1,
            seed: 7,
            trace_mode: TraceMode::Events,
        },
        &crew,
    );

    approx_eq(result.total_damage, 500.0, 1e-12);

    let crit_event = result
        .events
        .iter()
        .find(|event| event.event_type == "crit_resolution")
        .expect("crit event should be present");
    assert_eq!(crit_event.values["hull_breach_active"], Value::Bool(true));
    approx_eq(
        crit_event.values["multiplier"]
            .as_f64()
            .expect("multiplier as f64"),
        5.0,
        1e-12,
    );
}

#[test]
fn hull_breach_can_trigger_from_critical_hit_officer_ability() {
    let attacker = Combatant {
        id: "gorkon_ship".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 1.0,
        crit_multiplier: 1.5,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "target".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "Gorkon".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: true,
                effect: AbilityEffect::HullBreach {
                    chance: 1.0,
                    duration_rounds: 3,
                    requires_critical: true,
                },
            },
            boosted: false,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 1,
            seed: 7,
            trace_mode: TraceMode::Events,
        },
        &crew,
    );

    let hull_breach_event = result
        .events
        .iter()
        .find(|event| event.event_type == "hull_breach_trigger")
        .expect("hull breach trigger should be emitted");
    assert_eq!(hull_breach_event.phase, "attack");
    assert_eq!(hull_breach_event.values["triggered"], Value::Bool(true));
    assert_eq!(
        hull_breach_event.values["requires_critical"],
        Value::Bool(true)
    );
}
#[test]
fn simulate_combat_uses_seed_and_emits_canonical_events() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 120.0,
        mitigation: 0.1,
        pierce: 0.15,
        crit_chance: 0.5,
        crit_multiplier: 1.8,
        proc_chance: 0.4,
        proc_multiplier: 1.25,
        end_of_round_damage: 3.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 10.0,
        mitigation: 0.35,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
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
    approx_eq(first.total_damage, 318.0, 1e-12);

    assert_eq!(first.events.len(), 16);
    let expected_event_types = vec![
        "round_start",
        "attack_roll",
        "mitigation_calc",
        "pierce_calc",
        "crit_resolution",
        "proc_triggers",
        "damage_application",
        "end_of_round_effects",
    ];
    for (index, expected) in expected_event_types.iter().enumerate() {
        assert_eq!(first.events[index].event_type, *expected);
        assert_eq!(first.events[index + 8].event_type, *expected);
    }

    let round_one_crit = &first.events[4];
    let round_one_proc = &first.events[5];
    assert_eq!(round_one_crit.values["is_crit"], Value::Bool(true));
    assert_eq!(round_one_proc.values["triggered"], Value::Bool(true));
    approx_eq(
        round_one_crit.values["roll"]
            .as_f64()
            .expect("crit roll as f64"),
        0.198961,
        1e-12,
    );
    approx_eq(
        round_one_proc.values["roll"]
            .as_f64()
            .expect("proc roll as f64"),
        0.053962,
        1e-12,
    );

    let round_two_crit = &first.events[12];
    let round_two_proc = &first.events[13];
    assert_eq!(round_two_crit.values["is_crit"], Value::Bool(false));
    assert_eq!(round_two_proc.values["triggered"], Value::Bool(false));
    approx_eq(
        round_two_crit.values["roll"]
            .as_f64()
            .expect("crit roll as f64"),
        0.660146,
        1e-12,
    );
    approx_eq(
        round_two_proc.values["roll"]
            .as_f64()
            .expect("proc roll as f64"),
        0.766776,
        1e-12,
    );
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
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.5,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
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
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.2,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
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

#[test]
fn timing_windows_materially_change_damage_outcomes() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.5,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 17,
        trace_mode: TraceMode::Events,
    };

    let attack_phase_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "pierce_now".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::AttackPhase,
                boostable: true,
                effect: AbilityEffect::PierceBonus(0.2),
            },
            boosted: false,
        }],
    };
    let round_start_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "pierce_early".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::PierceBonus(0.2),
            },
            boosted: false,
        }],
    };
    let defense_phase_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "pierce_on_defense".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::DefensePhase,
                boostable: true,
                effect: AbilityEffect::PierceBonus(0.2),
            },
            boosted: false,
        }],
    };

    let attack_phase = simulate_combat(&attacker, &defender, config, &attack_phase_crew);
    let round_start = simulate_combat(&attacker, &defender, config, &round_start_crew);
    let defense_phase = simulate_combat(&attacker, &defender, config, &defense_phase_crew);

    assert!(round_start.total_damage > attack_phase.total_damage);
    assert!(defense_phase.total_damage > attack_phase.total_damage);
    approx_eq(attack_phase.total_damage, 60.0, 1e-12);
    approx_eq(round_start.total_damage, 70.0, 1e-12);
    approx_eq(defense_phase.total_damage, 70.0, 1e-12);
}

#[test]
fn burning_deals_one_percent_hull_per_round() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "target".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 500.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let burning_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "georgiou".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::Burning {
                    chance: 1.0,
                    duration_rounds: 2,
                },
            },
            boosted: false,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 3,
            seed: 1,
            trace_mode: TraceMode::Events,
        },
        &burning_crew,
    );

    approx_eq(result.total_damage, 15.0, 1e-12);
    let burning_ticks = result
        .events
        .iter()
        .filter(|event| event.event_type == "end_of_round_effects")
        .filter(|event| event.values["burning_damage"] == Value::from(5.0))
        .count();
    assert_eq!(burning_ticks, 3);
}

#[test]
fn emits_ability_activation_for_each_timing_window() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 120.0,
        mitigation: 0.0,
        pierce: 0.1,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 1.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.4,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    name: "combat_begin_alpha".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::CombatBegin,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                },
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "round_start_alpha".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                },
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::BelowDeck,
                ability: Ability {
                    name: "attack_alpha".to_string(),
                    class: AbilityClass::BelowDeck,
                    timing: TimingWindow::AttackPhase,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                },
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    name: "defense_alpha".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::DefensePhase,
                    boostable: true,
                    effect: AbilityEffect::PierceBonus(0.1),
                },
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "round_end_alpha".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundEnd,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.2),
                },
                boosted: false,
            },
        ],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 1,
            seed: 19,
            trace_mode: TraceMode::Events,
        },
        &crew,
    );

    let phases: Vec<_> = result
        .events
        .iter()
        .filter(|event| event.event_type == "ability_activation")
        .map(|event| event.phase.as_str())
        .collect();

    assert!(phases.contains(&"combat_begin"));
    assert!(phases.contains(&"round_start"));
    assert!(phases.contains(&"attack"));
    assert!(phases.contains(&"defense"));
    assert!(phases.contains(&"round_end"));
}

#[test]
fn additive_attack_modifiers_match_canonical_summed_behavior() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "target".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let two_ten_percent = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    name: "round_start_ten_alpha".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                },
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "round_start_ten_beta".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                },
                boosted: false,
            },
        ],
    };
    let single_twenty_percent = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "round_start_twenty".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::AttackMultiplier(0.2),
            },
            boosted: false,
        }],
    };

    let config = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Off,
    };

    let summed = simulate_combat(&attacker, &defender, config, &two_ten_percent);
    let canonical = simulate_combat(&attacker, &defender, config, &single_twenty_percent);

    approx_eq(summed.total_damage, 120.0, 1e-12);
    approx_eq(summed.total_damage, canonical.total_damage, 1e-12);
}

#[test]
fn combat_rounds_are_capped_at_100() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 1.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 150,
            seed: 9,
            trace_mode: TraceMode::Off,
        },
        &CrewConfiguration::default(),
    );

    assert_eq!(result.rounds_simulated, 100);
}

#[test]
fn round_end_regen_restores_shield_and_reduces_hull_damage() {
    use kobayashi::combat::CrewSeatContext;
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 150.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.3,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 600.0,
        shield_health: 200.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let crew_no_regen = CrewConfiguration::default();
    let crew_with_regen = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "ShieldRegen".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundEnd,
                    boostable: false,
                    effect: AbilityEffect::ShieldRegen(60.0),
                },
                boosted: false,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "HullRegen".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundEnd,
                    boostable: false,
                    effect: AbilityEffect::HullRegen(40.0),
                },
                boosted: false,
            },
        ],
    };
    let result_no_regen = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 2,
            seed: 99,
            trace_mode: TraceMode::Off,
        },
        &crew_no_regen,
    );
    let result_with_regen = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 2,
            seed: 99,
            trace_mode: TraceMode::Off,
        },
        &crew_with_regen,
    );
    assert!(
        result_with_regen.defender_shield_remaining >= result_no_regen.defender_shield_remaining,
        "regen should preserve or increase shield"
    );
    assert!(
        result_with_regen.defender_hull_remaining >= result_no_regen.defender_hull_remaining,
        "regen should preserve or increase hull"
    );
}

#[test]
fn round_limit_declares_winner_by_hull_without_destruction() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 1.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds: 100,
            seed: 3,
            trace_mode: TraceMode::Off,
        },
        &CrewConfiguration::default(),
    );

    assert!(result.winner_by_round_limit);
    assert!(result.attacker_won);
    assert!(result.attacker_hull_remaining > 0.0);
    assert!(result.defender_hull_remaining > 0.0);
}

#[test]
fn isolytic_damage_matches_reference_formula() {
    let damage = isolytic_damage(10_000.0, 0.3, 0.4);
    approx_eq(damage, 8_200.0, 1e-12);
}
