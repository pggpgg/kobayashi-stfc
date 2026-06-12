//! Property-based combat-engine invariants (roadmap task 8).
//!
//! Generates random-but-valid combatant pairs (field domains per [`Combatant`] docs) and asserts
//! the invariants that must hold for ANY fight: non-negative finite damage, hull/shield remaining
//! within [0, max], rounds within the configured cap, and same-seed determinism (the SplitMix64
//! contract: identical inputs + seed → identical [`SimulationResult`]).
//!
//! Case count is capped at 64 per property: each fight is microseconds, so the whole crate adds
//! ~1s to `cargo test`. Failures persist minimized counterexamples under `proptest-regressions/`
//! — commit those files.

use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, SimulationResult, TraceMode,
};
use proptest::prelude::*;

/// Random-but-valid combatant per the documented field domains: hull > 0, fractions in [0, 1],
/// multipliers ≥ 1, raw stats ≥ 0, scalar attack (empty `weapons`), no crew effects.
fn combatant(tag: &'static str) -> impl Strategy<Value = Combatant> {
    let offense = (0.0..1e5f64, 0.0..=1.0f64, 0.0..=1.0f64, 1.0..5.0f64); // attack, pierce, crit_chance, crit_multiplier
    let defense = (
        0.0..=1.0f64,
        0.0..1e5f64,
        0.0..1e5f64,
        0.0..1e5f64,
        0.0..1e3f64,
    ); // mitigation, armor, shield_deflection, dodge, damage_reduction
    let procs = (0.0..=1.0f64, 1.0..3.0f64, 0.0..1e3f64); // proc_chance, proc_multiplier, end_of_round_damage
    let pools = (1.0..1e6f64, 0.0..1e5f64, 0.0..=1.0f64); // hull_health, shield_health, shield_mitigation
    (offense, defense, procs, pools).prop_map(
        move |(
            (attack, pierce, crit_chance, crit_multiplier),
            (mitigation, armor, shield_deflection, dodge, damage_reduction),
            (proc_chance, proc_multiplier, end_of_round_damage),
            (hull_health, shield_health, shield_mitigation),
        )| Combatant {
            id: tag.to_string(),
            attack,
            mitigation,
            armor,
            shield_deflection,
            dodge,
            damage_reduction,
            pierce,
            crit_chance,
            crit_multiplier,
            proc_chance,
            proc_multiplier,
            end_of_round_damage,
            hull_health,
            shield_health,
            shield_mitigation,
            ..Combatant::default()
        },
    )
}

fn config(rounds: u32, seed: u64) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
        trace_mode: TraceMode::Off,
        ..SimulationConfig::default()
    }
}

fn run(attacker: &Combatant, defender: &Combatant, rounds: u32, seed: u64) -> SimulationResult {
    let crew = CrewConfiguration { seats: Vec::new() };
    simulate_combat(attacker, defender, &config(rounds, seed), &crew)
}

fn assert_result_invariants(
    result: &SimulationResult,
    attacker: &Combatant,
    defender: &Combatant,
    rounds: u32,
) -> Result<(), TestCaseError> {
    prop_assert!(
        result.total_damage.is_finite() && result.total_damage >= 0.0,
        "total_damage = {}",
        result.total_damage
    );
    prop_assert!(
        result.rounds_simulated >= 1 && result.rounds_simulated <= rounds,
        "rounds_simulated = {} (cap {rounds})",
        result.rounds_simulated
    );
    for (label, remaining, max) in [
        (
            "attacker hull",
            result.attacker_hull_remaining,
            attacker.hull_health,
        ),
        (
            "defender hull",
            result.defender_hull_remaining,
            defender.hull_health,
        ),
        (
            "attacker shield",
            result.attacker_shield_remaining,
            attacker.shield_health,
        ),
        (
            "defender shield",
            result.defender_shield_remaining,
            defender.shield_health,
        ),
    ] {
        prop_assert!(
            remaining.is_finite() && remaining >= 0.0 && remaining <= max + 1e-9,
            "{label} remaining = {remaining}, max = {max}"
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Damage, hull, shield, and round-count invariants hold for arbitrary valid fights.
    #[test]
    fn fight_outputs_stay_within_physical_bounds(
        attacker in combatant("attacker"),
        defender in combatant("defender"),
        rounds in 1u32..20,
        seed in any::<u64>(),
    ) {
        let result = run(&attacker, &defender, rounds, seed);
        assert_result_invariants(&result, &attacker, &defender, rounds)?;
    }

    /// SplitMix64 contract: the same combatants, config, and seed produce an identical
    /// [`SimulationResult`] — the foundation of replay-seed, CRN sensitivity, and drift fixtures.
    #[test]
    fn same_seed_produces_identical_results(
        attacker in combatant("attacker"),
        defender in combatant("defender"),
        rounds in 1u32..20,
        seed in any::<u64>(),
    ) {
        let first = run(&attacker, &defender, rounds, seed);
        let second = run(&attacker, &defender, rounds, seed);
        prop_assert_eq!(first, second);
    }
}
