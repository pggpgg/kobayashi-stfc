//! Print tiered scout trial totals: legacy uniform scout vs adaptive coarse→refine.
//!
//! Run from the repo root (so `data/` and `profiles/` resolve):
//! `cargo run --release --bin tiered_scout_budget_compare`
//!
//! Uses [`kobayashi::data::profile_index::DEMO_PROFILE_ID`] so `roster.imported.json` exists and
//! officer pools are non-empty (the default API profile may have no roster file on disk).
//!
//! Compare `scout_trials_final` from [`kobayashi::optimizer::OptimizeRunOutcome::tiered_scout_budget`].
//!
//! Scenarios set `prior_reference_crews` empty; production runs may populate it from optimize history
//! for analytical rank only (see `kobayashi::data::optimize_history::prior_reference_crews_for_matchup_priors`).

use std::time::Instant;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::crew_generator::DEFAULT_BELOW_DECKS_SLOTS;
use kobayashi::optimizer::monte_carlo::DefenderOpponent;
use kobayashi::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizerStrategy,
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

fn scenario(row: &BenchRow, uniform: bool) -> OptimizationScenario<'static> {
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
        tiered_scout_uniform: uniform,
        tiered_confirm_budget_cap_mult: None,
        tiered_scout_priority_queue: false,
        tiered_pq_minimal_scout: None,
        tiered_pq_selection_mult: None,
        tiered_pq_abandon_margin: None,
        exhaustive_scout_sims: None,
        exhaustive_scout_top_keep: None,
        analytical_prefilter_keep: None,
        prune_analytical_hull_fraction: None,
        prune_static_gate_max_fraction: None,
        below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
        constraints: None,
        support_buffs: Vec::new(),
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        player_defender_officer_crew: None,
        pvp: None,
        warm_start: Vec::new(),
        prior_reference_crews: Vec::new(),
        optimize_cache_key: None,
        enable_learned_pair_prior: true,
        learned_officer_scores: None,
    }
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
        "{:<36} {:>5} {:>14} {:>14} {:>10} {:>8}",
        "scenario", "n", "uniform_trials", "adaptive_trials", "reduction%", "secs"
    );

    for row in ROWS {
        let t0 = Instant::now();
        let uniform_out = optimize_scenario_with_progress_with_registry(
            &registry,
            &scenario(row, true),
            |_| true,
            || true,
        );
        let t1 = Instant::now();
        let adaptive_out = optimize_scenario_with_progress_with_registry(
            &registry,
            &scenario(row, false),
            |_| true,
            || true,
        );
        let elapsed = t0.elapsed().as_secs_f64() + t1.elapsed().as_secs_f64();

        let n = adaptive_out.tiered_resolved.map(|(n, _, _)| n).unwrap_or(0);
        let u = uniform_out
            .tiered_scout_budget
            .map(|b| b.scout_trials_final)
            .unwrap_or(0);
        let a = adaptive_out
            .tiered_scout_budget
            .map(|b| b.scout_trials_final)
            .unwrap_or(0);
        let pct = if u > 0 {
            100.0 * (1.0 - (a as f64 / u as f64))
        } else {
            0.0
        };
        println!(
            "{:<36} {:>5} {:>14} {:>14} {:>9.1}% {:>8.2}",
            row.label, n, u, a, pct, elapsed
        );
    }
}
