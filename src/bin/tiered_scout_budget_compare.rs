//! Print tiered scout trial totals: legacy uniform scout vs adaptive coarse→refine.
//!
//! Run from the repo root (so `data/` resolves): `cargo run --release --bin tiered_scout_budget_compare`
//!
//! Compare `scout_trials_final` from [`kobayashi::optimizer::OptimizeRunOutcome::tiered_scout_budget`].
//! If `ranked.len()` is zero (no generated candidates for the ship/hostile pair), scout totals stay at zero.
//! With a non-empty candidate pool, adaptive scout typically lowers total scout trials versus uniform scout.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::DEFAULT_BELOW_DECKS_SLOTS;
use kobayashi::optimizer::monte_carlo::DefenderOpponent;
use kobayashi::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizerStrategy,
};
use kobayashi::parallel::init_from_env;

fn scenario(uniform: bool) -> OptimizationScenario<'static> {
    OptimizationScenario {
        ship: "enterprise",
        hostile: "swarm",
        ship_tier: None,
        ship_level: None,
        simulation_count: 1_200,
        seed: 42,
        max_candidates: Some(256),
        strategy: OptimizerStrategy::Tiered,
        only_below_decks_with_ability: false,
        seed_population: Vec::new(),
        profile_id: None,
        tiered_scout_sims: Some(400),
        tiered_top_k: Some(12),
        tiered_scout_uniform: uniform,
        analytical_prefilter_keep: None,
        below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
        constraints: None,
        support_buffs: Vec::new(),
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        warm_start: Vec::new(),
        optimize_cache_key: None,
    }
}

fn main() {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        let _ = std::env::set_current_dir(m);
    }
    init_from_env();
    let registry = DataRegistry::load().expect("data registry");
    let uniform_out =
        optimize_scenario_with_progress_with_registry(&registry, &scenario(true), |_| true);
    let adaptive_out =
        optimize_scenario_with_progress_with_registry(&registry, &scenario(false), |_| true);
    let u = uniform_out
        .tiered_scout_budget
        .expect("uniform tiered scout budget");
    let a = adaptive_out
        .tiered_scout_budget
        .expect("adaptive tiered scout budget");
    println!(
        "uniform  scout_trials_final={} coarse_pass={} refine_pass={}",
        u.scout_trials_final, u.coarse_pass_trials, u.refine_pass_trials
    );
    println!(
        "adaptive scout_trials_final={} coarse_pass={} refine_pass={}",
        a.scout_trials_final, a.coarse_pass_trials, a.refine_pass_trials
    );
    if u.scout_trials_final > 0 {
        let ratio = a.scout_trials_final as f64 / u.scout_trials_final as f64;
        println!(
            "scout trial reduction vs uniform: {:.1}% (adaptive/uniform={ratio:.3})",
            100.0 * (1.0 - ratio)
        );
    }
}
