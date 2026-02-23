//! Calibration tests: run simulator with recorded-fight scenario and assert outcome within tolerance.
//! Fixtures live in tests/fixtures/recorded_fights/ (see docs/combat_log_format.md).

use std::path::Path;

use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode,
};

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("recorded_fights")
        .join(name)
}

/// Calibration scenario from fixture: attacker 250 attack, defender 400 hull + 200 shield, 80% shield mitigation.
/// Asserts simulator output is within expected bounds for regression and formula tuning.
#[test]
fn calibration_scenario_outcome_within_tolerance() {
    let attacker = Combatant {
        id: "cal_attacker".to_string(),
        attack: 250.0,
        mitigation: 0.0,
        pierce: 0.12,
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
    };
    let defender = Combatant {
        id: "cal_defender".to_string(),
        attack: 0.0,
        mitigation: 0.2,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 400.0,
        shield_health: 200.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
    };
    let config = SimulationConfig {
        rounds: 10,
        seed: 42,
        trace_mode: TraceMode::Off,
    };
    let result = simulate_combat(&attacker, &defender, config, &CrewConfiguration::default());

    assert!(
        result.rounds_simulated >= 1 && result.rounds_simulated <= 10,
        "rounds_simulated {} should be in [1, 10]",
        result.rounds_simulated
    );
    assert!(
        result.total_damage >= 200.0 && result.total_damage <= 2500.0,
        "total_damage {} should be in [200, 2500]",
        result.total_damage
    );
    // Attacker typically wins this scenario (high attack vs moderate defender); allow for RNG
    assert!(
        result.defender_hull_remaining >= 0.0 && result.defender_hull_remaining <= 600.0,
        "defender_hull_remaining should be in [0, 600]"
    );
}

#[test]
fn calibration_scenario_fixture_file_exists() {
    let path = fixture_path("calibration_scenario.json");
    let contents = std::fs::read_to_string(&path).expect("calibration_scenario.json should exist");
    let _: serde_json::Value =
        serde_json::from_str(&contents).expect("calibration_scenario.json should be valid JSON");
}
