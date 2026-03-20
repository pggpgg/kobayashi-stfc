//! Compatibility toggle: duplicate officer id across slots (see SimulationConfig::allow_duplicate_officers).

use kobayashi::combat::{
    apply_duplicate_officer_policy, simulate_combat, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, SimulationConfig, TimingWindow,
    TraceMode,
};

fn sample_combatant(id: &str, attack: f64, hull: f64) -> Combatant {
    Combatant {
        id: id.to_string(),
        attack,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: hull,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
    }
}

fn dup_officer_seat(batch: u32, mult: f64) -> CrewSeatContext {
    CrewSeatContext {
        seat: CrewSeat::Bridge,
        ability: Ability {
            name: "dup_test".to_string(),
            class: AbilityClass::BridgeAbility,
            timing: TimingWindow::RoundStart,
            boostable: true,
            effect: AbilityEffect::AttackMultiplier(mult),
            condition: None,
        },
        boosted: false,
        officer_id: Some("same_officer".to_string()),
        contribution_batch: batch,
    }
}

#[test]
fn apply_duplicate_officer_policy_keeps_first_batch_per_officer_when_disabled() {
    let crew = CrewConfiguration {
        seats: vec![dup_officer_seat(0, 0.1), dup_officer_seat(1, 0.5)],
    };
    let out = apply_duplicate_officer_policy(&crew, false);
    assert_eq!(out.seats.len(), 1);
    assert_eq!(out.seats[0].contribution_batch, 0);

    let all = apply_duplicate_officer_policy(&crew, true);
    assert_eq!(all.seats.len(), 2);
}

#[test]
fn simulate_combat_duplicate_toggle_changes_stacking_when_metadata_present() {
    let attacker = sample_combatant("a", 100.0, 500.0);
    let defender = sample_combatant("d", 0.0, 500.0);
    let crew = CrewConfiguration {
        seats: vec![dup_officer_seat(0, 0.1), dup_officer_seat(1, 0.5)],
    };

    let base = SimulationConfig {
        rounds: 1,
        seed: 1,
        trace_mode: TraceMode::Off,
        allow_duplicate_officers: false,
    };
    let dup_on = SimulationConfig {
        allow_duplicate_officers: true,
        ..base
    };

    let r_canonical = simulate_combat(&attacker, &defender, base, &crew);
    let r_dup = simulate_combat(&attacker, &defender, dup_on, &crew);
    assert!(
        r_dup.total_damage > r_canonical.total_damage,
        "second duplicate bridge buff should apply only when toggle is on"
    );
}
