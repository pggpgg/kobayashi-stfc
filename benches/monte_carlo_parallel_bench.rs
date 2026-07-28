//! Compare sequential vs parallel Monte Carlo run times.
//!
//! Run with: `cargo bench --bench monte_carlo_parallel`
//! Or quick comparison: `cargo run --bin benchmark_parallel_speedup` (see src/bin)
//! Uses `seed: 42`, ship `uss_saladin`, hostile `2918121098` (see `docs/PERFORMANCE.md` regression
//! gate).
//!
//! Both ids must **resolve**. An unresolvable id does not fail: the scenario builder falls back to
//! a fight synthesized from hashing the id strings (`src/optimizer/monte_carlo/scenario.rs`), so
//! the bench would measure a ~260–540 hull toy defender instead of real combat. This bench used
//! `saladin` — not a ship id — until 2026-07-28 and did exactly that.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kobayashi::optimizer::crew_generator::{CrewCandidate, CrewGenerator};
use kobayashi::optimizer::monte_carlo::{
    run_monte_carlo, run_monte_carlo_parallel, DefenderOpponent,
};
use kobayashi::parallel::init_from_env;

/// Build a candidate list: from CrewGenerator if data exists, else synthetic list so bench still runs.
fn candidates(ship: &str, hostile: &str, seed: u64, min_count: usize) -> Vec<CrewCandidate> {
    let gen = CrewGenerator::new();
    let mut from_gen = gen.generate_candidates(ship, hostile, seed);
    if from_gen.len() >= min_count {
        from_gen.truncate(128);
        return from_gen;
    }
    // Synthetic candidates so we can measure parallel vs sequential even without full officer data
    (0..min_count)
        .map(|i| CrewCandidate {
            captain: format!("Captain_{i}"),
            bridge: vec!["Bridge_A".to_string(), "Bridge_B".to_string()],
            below_decks: vec![
                "Below_1".to_string(),
                "Below_2".to_string(),
                "Below_3".to_string(),
            ],
        })
        .collect()
}

fn bench_monte_carlo_sequential_vs_parallel(c: &mut Criterion) {
    init_from_env();
    let ship = "uss_saladin";
    let hostile = "2918121098";
    let seed = 42u64;
    let iterations = 500;
    let candidate_list = candidates(ship, hostile, seed, 32);

    let mut group = c.benchmark_group("monte_carlo");
    if std::env::var_os("CI").is_some() {
        group.sample_size(10);
        group.measurement_time(std::time::Duration::from_secs(2));
    } else {
        group.sample_size(20);
        group.measurement_time(std::time::Duration::from_secs(10));
    }

    group.bench_function("sequential", |b| {
        b.iter(|| {
            black_box(run_monte_carlo(
                ship,
                hostile,
                &candidate_list,
                iterations,
                seed,
                None,
                None,
                DefenderOpponent::Hostile,
            ))
        });
    });

    group.bench_function("parallel", |b| {
        b.iter(|| {
            black_box(run_monte_carlo_parallel(
                ship,
                hostile,
                &candidate_list,
                iterations,
                seed,
                None,
                None,
                DefenderOpponent::Hostile,
            ))
        });
    });

    group.finish();
}

criterion_group!(benches, bench_monte_carlo_sequential_vs_parallel);
criterion_main!(benches);
