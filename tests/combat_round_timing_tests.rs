//! Regression tests for combat phase ordering (round-end vs weapon sub-rounds).

use kobayashi::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, SimulationConfig, TimingWindow, TraceMode, NO_EXPLICIT_CONTRIBUTION_BATCH,
};

fn approx_eq(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "expected {b}, got {a}");
}

/// RoundEnd apex shred must not enter the weapon damage pipeline for the same round.
/// CombatBegin shred does (see `combat_tests::officer_apex_shred_bonus_at_combat_begin_increases_damage_through_barrier`).
#[test]
fn round_end_apex_shred_does_not_affect_same_round_weapon_damage() {
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
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
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
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
    };

    let baseline = simulate_combat(&attacker, &defender, config, &CrewConfiguration::default());

    let round_end_shred = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "RoundEnd Apex Shred".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundEnd,
                boostable: false,
                effect: AbilityEffect::ApexShredBonus(0.15),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let combat_begin_shred = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "CombatBegin Apex Shred".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ApexShredBonus(0.15),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let with_round_end = simulate_combat(&attacker, &defender, config, &round_end_shred);
    let with_combat_begin = simulate_combat(&attacker, &defender, config, &combat_begin_shred);

    approx_eq(with_round_end.total_damage, baseline.total_damage, 1e-9);
    assert!(
        with_combat_begin.total_damage > baseline.total_damage,
        "CombatBegin apex shred should increase weapon-phase damage; baseline={}, cb={}",
        baseline.total_damage,
        with_combat_begin.total_damage
    );
}
