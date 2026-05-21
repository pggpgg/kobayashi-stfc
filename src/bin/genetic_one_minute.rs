//! Run the extreme-exploration GA for roughly one minute against the same setup the bench uses
//! (Demo profile + USS Enterprise-D max tier/level + hostile 563945657) and print:
//!   - unique crew compositions evaluated
//!   - generations completed
//!   - elapsed wall time
//!   - crews / sec
//!
//! Usage:
//!   cargo run --release --bin genetic_one_minute
//!
//! Optional env vars:
//!   KOBAYASHI_ONE_MIN_TARGET_SECS=60     # target wall-clock budget (default 60)
//!   KOBAYASHI_ONE_MIN_POP=128            # GA population size  (default 128)
//!   KOBAYASHI_ONE_MIN_MUTATION_RATE=0.40 # mutation rate       (default 0.40)
//!
//! The binary scales `generations` based on the bench's measured throughput (~7,900 crews/sec at
//! pop=128) so the run takes roughly the target. Real wall time will vary with system load.

use std::time::Instant;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::genetic::{run_genetic_optimizer_ranked_with_stats, GeneticConfig};
use kobayashi::parallel::init_from_env;

const SHIP_ID: &str = "uss_enterprise_d";
const SHIP_TIER: u32 = 12;
const SHIP_LEVEL: u32 = 60;
const HOSTILE_ID: &str = "563945657";
const PROFILE_ID: &str = "demo";
const SEED: u64 = 42;

/// Reference throughput from `cargo bench --bench genetic_throughput` on this machine.
/// Used only to *estimate* the number of generations needed to fill the target wall budget —
/// the actual measurement at the end is real. Tracks the latest measured steady-state to keep
/// the default `target_secs=60` honest; bump after large speedup landings.
const REF_CREWS_PER_SEC: f64 = 10_300.0;

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    init_from_env();

    let target_secs = env_f64("KOBAYASHI_ONE_MIN_TARGET_SECS", 60.0);
    let pop = env_usize("KOBAYASHI_ONE_MIN_POP", 128);
    let mutation_rate = env_f64("KOBAYASHI_ONE_MIN_MUTATION_RATE", 0.40);

    // Estimate gens to fill the budget. The attempted-crew count is `pop × gens`; cap below
    // to avoid silly numbers if someone passes target_secs=3600.
    let estimated_attempts = target_secs * REF_CREWS_PER_SEC;
    let generations = ((estimated_attempts / pop as f64).round() as usize).clamp(2, 50_000);

    let config = GeneticConfig {
        population_size: pop,
        generations,
        sims_per_eval: 1,
        mutation_rate,
        adaptive_mutation: false,
        incremental_fitness: false,
        offspring_reduced_budget_mul: None,
        stagnation_limit: None,
        roster_profile_id: Some(PROFILE_ID.to_string()),
        ship_tier: Some(SHIP_TIER),
        ship_level: Some(SHIP_LEVEL),
        ..GeneticConfig::default()
    };

    let registry = DataRegistry::load().expect("DataRegistry::load() failed — run from repo root");

    eprintln!(
        "running GA: pop={pop} generations={generations} sims_per_eval=1 \
         mutation={mutation_rate} (target ~{target_secs:.0}s)"
    );

    let t0 = Instant::now();
    let (results, stats) = run_genetic_optimizer_ranked_with_stats(
        SHIP_ID,
        HOSTILE_ID,
        &config,
        SEED,
        config.sims_per_eval,
        Some(registry.as_ref()),
        |_, _, _| true,
        || true,
    );
    let elapsed = t0.elapsed();
    let secs = elapsed.as_secs_f64();
    let attempted = (pop * stats.generations_completed) as f64;
    let unique = stats.unique_crews_evaluated as f64;
    let dup_rate = if attempted > 0.0 {
        100.0 * (1.0 - unique / attempted)
    } else {
        0.0
    };

    println!();
    println!("─── results ──────────────────────────────────────────────");
    println!("setup           : Demo profile + USS Enterprise-D T{SHIP_TIER} L{SHIP_LEVEL} vs hostile {HOSTILE_ID}");
    println!("config          : pop={pop} gens={generations} sims=1 mutation={mutation_rate}");
    println!("elapsed         : {secs:.3} s");
    println!("generations done: {}", stats.generations_completed);
    println!(
        "attempted crews : {:.0}  (= pop × gens_completed)",
        attempted
    );
    println!(
        "unique crews    : {}  (dedup rate {:.1} %)",
        stats.unique_crews_evaluated, dup_rate
    );
    println!(
        "throughput      : {:.0} unique crews / sec   ({:.0} attempted / sec)",
        unique / secs,
        attempted / secs
    );
    println!("top crew kept   : {} entries returned", results.len());
    if let Some(top) = results.first() {
        println!(
            "best win_rate   : {:.4}  (captain={}, bridge={:?}, below={:?})",
            top.win_rate, top.captain, top.bridge, top.below_decks
        );
    }
}
