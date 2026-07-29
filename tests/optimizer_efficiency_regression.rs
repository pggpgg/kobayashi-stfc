use std::collections::HashSet;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::monte_carlo::crew_candidate_stable_hash;
use kobayashi::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizeRunOutcome,
    OptimizerStrategy,
};

fn scenario(priority_queue: bool, uniform: bool) -> OptimizationScenario<'static> {
    OptimizationScenario {
        ship: "uss_saladin",
        hostile: "1140710508",
        simulation_count: 600,
        seed: 11,
        max_candidates: Some(120),
        strategy: OptimizerStrategy::Tiered,
        profile_id: Some(DEMO_PROFILE_ID),
        tiered_scout_sims: Some(500),
        tiered_top_k: Some(20),
        tiered_scout_uniform: uniform,
        tiered_scout_priority_queue: priority_queue,
        ..OptimizationScenario::default()
    }
}

fn finalist_hashes(outcome: &OptimizeRunOutcome) -> HashSet<u64> {
    outcome
        .ranked
        .iter()
        .map(|row| {
            crew_candidate_stable_hash(&kobayashi::optimizer::crew_generator::CrewCandidate {
                captain: row.captain.clone(),
                bridge: row.bridge.clone(),
                below_decks: row.below_decks.clone(),
            })
        })
        .collect()
}

fn winner_hash(outcome: &OptimizeRunOutcome) -> Option<u64> {
    outcome.ranked.first().map(|row| {
        crew_candidate_stable_hash(&kobayashi::optimizer::crew_generator::CrewCandidate {
            captain: row.captain.clone(),
            bridge: row.bridge.clone(),
            below_decks: row.below_decks.clone(),
        })
    })
}

#[test]
fn confidence_racing_preserves_uniform_finalists_with_less_scout_work() {
    let registry = DataRegistry::load().expect("data registry");
    let uniform = optimize_scenario_with_progress_with_registry(
        &registry,
        &scenario(false, true),
        |_| true,
        || true,
    );
    let racing = optimize_scenario_with_progress_with_registry(
        &registry,
        &scenario(true, false),
        |_| true,
        || true,
    );

    let uniform_budget = uniform.tiered_scout_budget.expect("uniform budget");
    let racing_budget = racing.tiered_scout_budget.expect("racing budget");
    assert!(
        racing_budget.scout_trials_executed_total < uniform_budget.scout_trials_executed_total,
        "racing should execute fewer scout trials: racing={} uniform={}",
        racing_budget.scout_trials_executed_total,
        uniform_budget.scout_trials_executed_total
    );

    let uniform_hashes = finalist_hashes(&uniform);
    let racing_hashes = finalist_hashes(&racing);
    let overlap = uniform_hashes.intersection(&racing_hashes).count();
    assert!(
        overlap * 100 >= uniform_hashes.len() * 95,
        "racing finalist recall should remain >=95%: {overlap}/{}",
        uniform_hashes.len()
    );
    assert_eq!(winner_hash(&uniform), winner_hash(&racing));
}
