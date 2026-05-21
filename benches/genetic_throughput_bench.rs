//! Genetic-mode crew-exploration throughput benchmark.
//!
//! Question this bench answers: **how many DISTINCT crew compositions can the genetic optimizer
//! score per second** against a single hostile, with the exact production setup:
//! - **Demo profile** (roster / research / buildings / forbidden-tech from `profiles/demo/`)
//! - **USS Enterprise-D, tier 12, level 60** (max tier + max level)
//! - Hostile **`563945657`** (Romulan epic group armada, level 60)
//!
//! Run:
//!   cargo bench --bench genetic_throughput
//!   cargo bench --bench genetic_throughput -- --quick
//!
//! Drives the GA at `sims_per_eval = 1` — one combat per crew — so wall-clock time is dominated
//! by **how fast the optimizer can churn through different crews**, not by per-crew statistical
//! refinement. Criterion's throughput line reports `unique_crews / sec`.
//!
//! The DataRegistry is loaded **once** before the criterion loop and reused on every iteration
//! (this mirrors the server, which keeps one registry per process). Without this, ~20 % of the
//! reported wall time would be re-parsing officer LCARS YAML on every iteration.

use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::genetic::{
    run_genetic_optimizer_ranked_with_stats, GeneticConfig, GeneticRunStats,
};
use kobayashi::parallel::init_from_env;

const SHIP_ID: &str = "uss_enterprise_d";
const SHIP_TIER: u32 = 12;
const SHIP_LEVEL: u32 = 60;
const HOSTILE_ID: &str = "563945657";
const PROFILE_ID: &str = "demo";
const SEED: u64 = 42;

/// Push exploration hard: one sim per crew, large population, many generations.
/// Stagnation early-stop is disabled so we keep churning new crews even after fitness plateaus —
/// the bench's job is to measure crew throughput, not optimization quality.
fn extreme_explore_config() -> GeneticConfig {
    GeneticConfig {
        population_size: 128,
        generations: 30,
        sims_per_eval: 1,
        // High mutation keeps offspring diverse so the population doesn't collapse onto duplicates.
        mutation_rate: 0.40,
        adaptive_mutation: false,
        // Disable elite-cache reuse — we want every generation to score new crews, not skip them.
        incremental_fitness: false,
        offspring_reduced_budget_mul: None,
        // Disable early stop: we want to run the full schedule and see steady-state throughput.
        stagnation_limit: None,
        roster_profile_id: Some(PROFILE_ID.to_string()),
        ship_tier: Some(SHIP_TIER),
        ship_level: Some(SHIP_LEVEL),
        ..GeneticConfig::default()
    }
}

fn run_once(config: &GeneticConfig, registry: &Arc<DataRegistry>) -> (usize, GeneticRunStats) {
    let (results, stats) = run_genetic_optimizer_ranked_with_stats(
        SHIP_ID,
        HOSTILE_ID,
        config,
        SEED,
        config.sims_per_eval,
        Some(registry.as_ref()),
        |_, _, _| true,
        || true,
    );
    (results.len(), stats)
}

fn probe_unique_crews(config: &GeneticConfig, registry: &Arc<DataRegistry>) -> u64 {
    let t0 = Instant::now();
    let (_results, stats) = run_once(config, registry);
    let elapsed = t0.elapsed();
    let crews_per_sec = stats.unique_crews_evaluated as f64 / elapsed.as_secs_f64();
    eprintln!(
        "[probe] unique_crews={} generations={} elapsed={:.3}s crews/sec={:.1}",
        stats.unique_crews_evaluated,
        stats.generations_completed,
        elapsed.as_secs_f64(),
        crews_per_sec,
    );
    stats.unique_crews_evaluated.max(1) as u64
}

fn bench_genetic_exploration(c: &mut Criterion) {
    init_from_env();

    // Load the registry ONCE (officers, ships, hostiles, LCARS, catalogs) — same pattern as
    // the server. Reused across every criterion iteration so the bench measures GA work,
    // not file-loading work.
    let registry = DataRegistry::load().expect("DataRegistry::load() failed — run from repo root");

    let config = extreme_explore_config();
    let unique_unit = probe_unique_crews(&config, &registry);

    let mut group = c.benchmark_group("genetic_exploration");
    group.throughput(Throughput::Elements(unique_unit));
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    group.bench_function("extreme/pop128_gen30_sims1", |b| {
        b.iter(|| {
            let (n, _stats) = run_once(&config, &registry);
            black_box(n)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_genetic_exploration);
criterion_main!(benches);
