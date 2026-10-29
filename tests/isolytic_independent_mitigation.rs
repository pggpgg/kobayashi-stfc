//! Isolytic bypasses conventional mitigation but is subject to isolytic
//! defense and final Apex mitigation, for both sides of a fight.
use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode, WeaponStats,
};

fn fight(
    attacker: &Combatant,
    defender: &Combatant,
    trace_mode: TraceMode,
) -> kobayashi::combat::SimulationResult {
    simulate_combat(
        attacker,
        defender,
        &SimulationConfig {
            rounds: 1,
            trace_mode,
            ..Default::default()
        },
        &CrewConfiguration::default(),
    )
}

#[test]
fn player_isolytic_ignores_conventional_mitigation_and_respects_iso_defense_and_apex() {
    let attacker = Combatant {
        id: "player".into(),
        attack: 100.0,
        hull_health: 10000.0,
        isolytic_damage: 1.0,
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 10000.0,
        mitigation: 0.5,
        ..Default::default()
    };
    // 50 standard + 100 isolytic; conventional mitigation affects only standard.
    for mode in [TraceMode::Off, TraceMode::Events] {
        let result = fight(&attacker, &defender, mode);
        assert!((result.defender_hull_remaining - 9850.0).abs() < 1e-9);
        assert!((result.total_isolytic_damage - 100.0).abs() < 1e-9);
    }
    let multi_shot = Combatant {
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(4),
            ..Default::default()
        }],
        ..attacker.clone()
    };
    let scalar = fight(&multi_shot, &defender, TraceMode::Events);
    let batch = fight(&multi_shot, &defender, TraceMode::Off);
    assert!((scalar.defender_hull_remaining - 9400.0).abs() < 1e-9);
    assert_eq!(
        scalar.defender_hull_remaining,
        batch.defender_hull_remaining
    );
    assert_eq!(scalar.total_isolytic_damage, batch.total_isolytic_damage);
    let protected = Combatant {
        isolytic_defense: 1.0,
        apex_barrier: 10000.0,
        ..defender
    };
    // Iso defense halves the isolytic leg, then Apex halves both legs.
    let result = fight(&attacker, &protected, TraceMode::Off);
    assert!((result.defender_hull_remaining - 9950.0).abs() < 1e-9);
    assert!((result.total_isolytic_damage - 25.0).abs() < 1e-9);
    let no_iso = Combatant {
        isolytic_damage: 0.0,
        ..attacker
    };
    let result = fight(
        &no_iso,
        &Combatant {
            apex_barrier: 0.0,
            ..protected
        },
        TraceMode::Off,
    );
    assert!((result.defender_hull_remaining - 9950.0).abs() < 1e-9);
}

#[test]
fn hostile_counter_isolytic_also_ignores_conventional_mitigation() {
    let attacker = Combatant {
        id: "player".into(),
        hull_health: 10000.0,
        mitigation: 0.5,
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        attack: 100.0,
        hull_health: 10000.0,
        isolytic_damage: 1.0,
        ..Default::default()
    };
    for mode in [TraceMode::Off, TraceMode::Events] {
        let result = fight(&attacker, &defender, mode);
        assert!((result.attacker_hull_remaining - 9850.0).abs() < 1e-9);
    }
    let protected = Combatant {
        isolytic_defense: 1.0,
        apex_barrier: 10000.0,
        ..attacker
    };
    let result = fight(&protected, &defender, TraceMode::Off);
    assert!((result.attacker_hull_remaining - 9950.0).abs() < 1e-9);
}
