//! Compare tiered scout schedulers on work, elapsed time, winner, and top-K recall.
//!
//! Run from the repo root (so `data/` and `profiles/` resolve):
//! `cargo run --release --bin tiered_scout_budget_compare`
//!
//! Uses [`kobayashi::data::profile_index::DEMO_PROFILE_ID`] so `roster.imported.json` exists and
//! officer pools are non-empty (the default API profile may have no roster file on disk).
//!
//! Scenarios set `prior_reference_crews` empty; production runs may populate it from optimize history
//! for analytical rank only (see `kobayashi::data::optimize_history::prior_reference_crews_for_matchup_priors`).

use std::collections::HashSet;
use std::time::Instant;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::crew_generator::DEFAULT_BELOW_DECKS_SLOTS;
use kobayashi::optimizer::monte_carlo::DefenderOpponent;
use kobayashi::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizeRunOutcome,
    OptimizerStrategy,
};
use kobayashi::parallel::init_from_env;

struct BenchRow {
    label: &'static str,
    ship: &'static str,
    hostile: &'static str,
    seed: u64,
    max_candidates: usize,
    tiered_scout_sims: usize,
    tiered_top_k: usize,
    simulation_count: usize,
}

const ROWS: &[BenchRow] = &[
    BenchRow {
        label: "saladin_vs_numeric_hostile",
        ship: "saladin",
        hostile: "2918121098",
        seed: 11,
        max_candidates: 120,
        tiered_scout_sims: 500,
        tiered_top_k: 20,
        simulation_count: 2_000,
    },
    BenchRow {
        label: "uss_enterprise_d_vs_numeric_hostile",
        ship: "uss_enterprise_d",
        hostile: "2918121098",
        seed: 42,
        max_candidates: 128,
        tiered_scout_sims: 500,
        tiered_top_k: 20,
        simulation_count: 2_000,
    },
    BenchRow {
        label: "amalgam_vs_numeric_hostile_tiered_ship",
        ship: "amalgam",
        hostile: "2918121098",
        seed: 7,
        max_candidates: 100,
        tiered_scout_sims: 400,
        tiered_top_k: 16,
        simulation_count: 1_800,
    },
];

#[derive(Clone, Copy)]
enum ScoutScheduler {
    Uniform,
    Adaptive,
    Racing,
}

fn scenario(row: &BenchRow, scheduler: ScoutScheduler) -> OptimizationScenario<'static> {
    OptimizationScenario {
        ship: row.ship,
        hostile: row.hostile,
        ship_tier: None,
        ship_level: None,
        simulation_count: row.simulation_count,
        seed: row.seed,
        max_candidates: Some(row.max_candidates),
        strategy: OptimizerStrategy::Tiered,
        below_decks_pool_mode: kobayashi::data::heuristics::BelowDecksPoolMode::default(),
        seed_population: Vec::new(),
        profile_id: Some(DEMO_PROFILE_ID),
        tiered_scout_sims: Some(row.tiered_scout_sims),
        tiered_top_k: Some(row.tiered_top_k),
        tiered_scout_uniform: matches!(scheduler, ScoutScheduler::Uniform),
        tiered_confirm_budget_cap_mult: None,
        optimize_history_confirm_cap_mult: None,
        tiered_scout_priority_queue: matches!(scheduler, ScoutScheduler::Racing),
        tiered_pq_minimal_scout: None,
        tiered_pq_selection_mult: None,
        tiered_pq_abandon_margin: None,
        tiered_random_exploration_pct: None,
        exhaustive_scout_sims: None,
        exhaustive_scout_top_keep: None,
        analytical_prefilter_keep: None,
        prune_analytical_hull_fraction: None,
        prune_static_gate_max_fraction: None,
        below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
        constraints: None,
        support_buffs: Vec::new(),
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        player_defender_officer_crew: None,
        pvp: None,
        enemy_type: kobayashi::combat::EnemyType::RedMovingSpace,
        warm_start: Vec::new(),
        prior_reference_crews: Vec::new(),
        optimize_cache_key: None,
        reuse_fingerprint: None,
        enable_learned_pair_prior: true,
        learned_officer_scores: None,

        local_refinement: None,
    }
}

fn run(
    registry: &DataRegistry,
    row: &BenchRow,
    scheduler: ScoutScheduler,
) -> (OptimizeRunOutcome, f64) {
    let started = Instant::now();
    let outcome = optimize_scenario_with_progress_with_registry(
        registry,
        &scenario(row, scheduler),
        |_| true,
        || true,
    );
    (outcome, started.elapsed().as_secs_f64())
}

fn crew_hashes(outcome: &OptimizeRunOutcome) -> HashSet<u64> {
    outcome
        .ranked
        .iter()
        .map(|r| {
            kobayashi::optimizer::monte_carlo::crew_candidate_stable_hash(
                &kobayashi::optimizer::crew_generator::CrewCandidate {
                    captain: r.captain.clone(),
                    bridge: r.bridge.clone(),
                    below_decks: r.below_decks.clone(),
                },
            )
        })
        .collect()
}

fn print_scheduler(
    row: &BenchRow,
    label: &str,
    outcome: &OptimizeRunOutcome,
    elapsed: f64,
    reference_hashes: &HashSet<u64>,
    reference_winner: Option<u64>,
) {
    let budget = outcome.tiered_scout_budget.unwrap_or_default();
    let hashes = crew_hashes(outcome);
    let overlap = hashes.intersection(reference_hashes).count();
    let winner = outcome.ranked.first().map(|r| {
        kobayashi::optimizer::monte_carlo::crew_candidate_stable_hash(
            &kobayashi::optimizer::crew_generator::CrewCandidate {
                captain: r.captain.clone(),
                bridge: r.bridge.clone(),
                below_decks: r.below_decks.clone(),
            },
        )
    });
    println!(
        "{:<36} {:<8} {:>5} {:>12} {:>12} {:>8.3} {:>7}/{:<3} {:>7}",
        row.label,
        label,
        outcome.tiered_resolved.map(|(n, _, _)| n).unwrap_or(0),
        budget.scout_trials_final,
        budget.scout_trials_executed_total,
        elapsed,
        overlap,
        reference_hashes.len(),
        if winner == reference_winner {
            "same"
        } else {
            "changed"
        },
    );
}

fn main() {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        let _ = std::env::set_current_dir(m);
    }
    init_from_env();
    let registry = DataRegistry::load().expect("data registry");

    println!(
        "kobayashi tiered_scout_budget_compare (profile_id={DEMO_PROFILE_ID}) — {}",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "{:<36} {:<8} {:>5} {:>12} {:>12} {:>8} {:>11} {:>7}",
        "scenario", "mode", "n", "final_trials", "actual_trials", "secs", "top overlap", "winner"
    );

    for row in ROWS {
        let (uniform, uniform_secs) = run(&registry, row, ScoutScheduler::Uniform);
        let reference_hashes = crew_hashes(&uniform);
        let reference_winner = uniform.ranked.first().map(|r| {
            kobayashi::optimizer::monte_carlo::crew_candidate_stable_hash(
                &kobayashi::optimizer::crew_generator::CrewCandidate {
                    captain: r.captain.clone(),
                    bridge: r.bridge.clone(),
                    below_decks: r.below_decks.clone(),
                },
            )
        });
        let (adaptive, adaptive_secs) = run(&registry, row, ScoutScheduler::Adaptive);
        let (racing, racing_secs) = run(&registry, row, ScoutScheduler::Racing);
        print_scheduler(
            row,
            "uniform",
            &uniform,
            uniform_secs,
            &reference_hashes,
            reference_winner,
        );
        print_scheduler(
            row,
            "adaptive",
            &adaptive,
            adaptive_secs,
            &reference_hashes,
            reference_winner,
        );
        print_scheduler(
            row,
            "racing",
            &racing,
            racing_secs,
            &reference_hashes,
            reference_winner,
        );
    }
}
