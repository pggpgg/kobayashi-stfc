//! Simulator throughput benchmarks: combats per second and rounds per second.
//!
//! Run with: `cargo bench`
//! Results show mean time per combat and throughput (combats/s, rounds/s).

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use kobayashi::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration,
    CrewSeat, CrewSeatContext, SimulationConfig, TimingWindow, TraceMode,
};

fn default_attacker() -> Combatant {
    Combatant {
        id: "attacker".to_string(),
        attack: 500.0,
        mitigation: 0.0,
        pierce: 200.0,
        crit_chance: 0.1,
        crit_multiplier: 1.5,
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
    }
}

fn default_defender() -> Combatant {
    Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 300.0,
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
    }
}

/// Synthetic below-decks row exercising [`AbilityEffect::HullRegenPrevRoundFraction`] (bench-only).
fn synthetic_prev_round_hull_crew() -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::BelowDeck,
            Ability {
                name: "BenchPrevRoundHull".into(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullRegenPrevRoundFraction(0.35),
                condition: None,
            },
            false,
        )],
    }
}

fn bench_simulator(c: &mut Criterion) {
    let crew = CrewConfiguration::default();

    let mut group = c.benchmark_group("simulator");
    group.sample_size(100);

    // Short combat (3 rounds) – typical for quick sims
    let rounds_short = 3u32;
    group.bench_with_input("combat_3_rounds", &rounds_short, |b, &rounds| {
        let attacker = default_attacker();
        let defender = default_defender();
        let config = SimulationConfig {
            rounds,
            seed: 7,
            trace_mode: TraceMode::Off,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
        };
        b.iter_batched(
            || (attacker.clone(), defender.clone()),
            |(a, d)| black_box(simulate_combat(&a, &d, config, &crew)),
            BatchSize::SmallInput,
        );
    });
    group.throughput(Throughput::Elements(1));

    // Medium (20 rounds)
    let rounds_medium = 20u32;
    group.bench_with_input("combat_20_rounds", &rounds_medium, |b, &rounds| {
        let attacker = default_attacker();
        let defender = default_defender();
        let config = SimulationConfig {
            rounds,
            seed: 7,
            trace_mode: TraceMode::Off,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
        };
        b.iter_batched(
            || (attacker.clone(), defender.clone()),
            |(a, d)| black_box(simulate_combat(&a, &d, config, &crew)),
            BatchSize::SmallInput,
        );
    });
    group.throughput(Throughput::Elements(1));

    // Full combat (100 rounds max)
    let rounds_full = 100u32;
    group.bench_with_input("combat_100_rounds", &rounds_full, |b, &rounds| {
        let attacker = default_attacker();
        let defender = default_defender();
        let config = SimulationConfig {
            rounds,
            seed: 7,
            trace_mode: TraceMode::Off,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
        };
        b.iter_batched(
            || (attacker.clone(), defender.clone()),
            |(a, d)| black_box(simulate_combat(&a, &d, config, &crew)),
            BatchSize::SmallInput,
        );
    });
    group.throughput(Throughput::Elements(1));

    // Same combat shape as `combat_100_rounds`, with synthetic prev-round hull regen crew (A/B vs empty).
    let prev_round_crew = synthetic_prev_round_hull_crew();
    group.bench_with_input(
        "combat_100_rounds_prev_round_hull_crew",
        &rounds_full,
        |b, &rounds| {
            let attacker = default_attacker();
            let defender = default_defender();
            let config = SimulationConfig {
                rounds,
                seed: 7,
                trace_mode: TraceMode::Off,
                initial_attacker_hull_damage: 0.0,
                weapon_damage_profile_additive_pool: None,
                profile_weapon_damage_fraction: 0.0,
                defender_hull_faction_id: 0,
                defender_hostile_tag_mask: 0,
            };
            b.iter_batched(
                || (attacker.clone(), defender.clone()),
                |(a, d)| black_box(simulate_combat(&a, &d, config, &prev_round_crew)),
                BatchSize::SmallInput,
            );
        },
    );
    group.throughput(Throughput::Elements(1));

    group.finish();
}

criterion_group!(benches, bench_simulator);
criterion_main!(benches);
