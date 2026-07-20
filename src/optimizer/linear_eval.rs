//! Fast crew ranking by closed-form expected hull damage (no Monte Carlo).
//!
//! See [`crate::optimizer::analytical::expected_damage`] and docs/DESIGN.md §6.

use rayon::prelude::*;

use crate::data::data_registry::DataRegistry;
use crate::optimizer::analytical::expected_damage;
use crate::optimizer::crew_generator::{CrewCandidate, CrewGenerator};
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, scenario_to_combat_input_from_shared,
};
use crate::optimizer::ranking::{rank_results_by_expected_damage, RankedCrewResult};
use crate::optimizer::{
    apply_crew_constraints, candidate_strategy_from_scenario, prepend_warm_start_dedupe,
    scenario_support_buff_request, OptimizationScenario, OptimizeProgressTick,
};
use crate::parallel::batch_ranges;

const PROGRESS_BATCH_COUNT: usize = 40;
const PREVIEW_TOP: usize = 5;

/// Evaluate all generated candidates by pure expected hull damage and return ranked results.
pub fn run_linear_eval_with_registry<F, G>(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
    mut should_continue: G,
) -> Vec<RankedCrewResult>
where
    F: FnMut(OptimizeProgressTick) -> bool,
    G: FnMut() -> bool,
{
    let generator = CrewGenerator::with_strategy(candidate_strategy_from_scenario(scenario));
    let candidates = generator.generate_candidates_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.seed,
        scenario.profile_id,
    );
    let candidates = prepend_warm_start_dedupe(&scenario.warm_start, candidates);
    let candidates = apply_crew_constraints(candidates, scenario);
    let shared = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        scenario.profile_id,
        scenario_support_buff_request(scenario),
        scenario.defender_opponent,
        scenario.player_defender_officer_crew.clone(),
        scenario.pvp.clone(),
    );

    let total = candidates.len();
    if total == 0 {
        let _ = on_progress(OptimizeProgressTick {
            crews_done: 0,
            total_crews: 0,
            phase: "linear_eval",
            partial_top: None,
        });
        return Vec::new();
    }

    let seed = scenario.seed;
    let num_batches = batch_ranges(total, PROGRESS_BATCH_COUNT).len().max(1);
    let ranges = batch_ranges(total, num_batches);
    let mut scored: Vec<(CrewCandidate, f64)> = Vec::with_capacity(total);

    for (start, end) in ranges {
        if !should_continue() {
            break;
        }
        let batch_scored: Vec<(CrewCandidate, f64)> = candidates[start..end]
            .par_iter()
            .map(|candidate| {
                let input = scenario_to_combat_input_from_shared(&shared, candidate, seed);
                let damage = f64::from(expected_damage(&input));
                (candidate.clone(), damage)
            })
            .collect();
        scored.extend(batch_scored);

        let partial_top = preview_top_by_damage(&scored);
        if !on_progress(OptimizeProgressTick {
            crews_done: end as u32,
            total_crews: total as u32,
            phase: "linear_eval",
            partial_top: Some(partial_top),
        }) {
            break;
        }
    }

    if !should_continue() {
        return Vec::new();
    }

    let mut ranked = rank_results_by_expected_damage(scored);
    if let Some(cap) = scenario.max_candidates {
        ranked.truncate(cap);
    }

    let _ = on_progress(OptimizeProgressTick {
        crews_done: total as u32,
        total_crews: total as u32,
        phase: "linear_eval",
        partial_top: Some(ranked.iter().take(PREVIEW_TOP).cloned().collect()),
    });

    ranked
}

fn preview_top_by_damage(scored: &[(CrewCandidate, f64)]) -> Vec<RankedCrewResult> {
    let mut slice: Vec<(CrewCandidate, f64)> = scored.to_vec();
    slice.sort_by(|a, b| b.1.total_cmp(&a.1));
    slice.truncate(PREVIEW_TOP);
    rank_results_by_expected_damage(slice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data_registry::DataRegistry;
    use crate::data::heuristics::BelowDecksPoolMode;
    use crate::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;
    use crate::optimizer::monte_carlo::scenario::DefenderOpponent;
    use crate::optimizer::OptimizerStrategy;

    fn minimal_scenario<'a>(
        ship: &'a str,
        hostile: &'a str,
        max_candidates: Option<usize>,
    ) -> OptimizationScenario<'a> {
        OptimizationScenario {
            ship,
            hostile,
            ship_tier: None,
            ship_level: None,
            simulation_count: 100,
            seed: 42,
            max_candidates,
            strategy: OptimizerStrategy::LinearEval,
            below_decks_pool_mode: BelowDecksPoolMode::Strict,
            seed_population: Vec::new(),
            // Keep catalog-ranking tests independent of the user's mutable
            // default roster in profiles/index.json.
            profile_id: Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            tiered_scout_sims: None,
            tiered_top_k: None,
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            tiered_scout_priority_queue: false,
            tiered_pq_minimal_scout: None,
            tiered_pq_selection_mult: None,
            tiered_pq_abandon_margin: None,
            tiered_random_exploration_pct: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: None,
            prune_analytical_hull_fraction: None,
            prune_static_gate_max_fraction: None,
            below_decks_slots: 3,
            constraints: None,
            support_buffs: Vec::new(),
            defender_support_buffs: None,
            defender_alliance_debuffs: None,
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            player_defender_officer_crew: None,
            pvp: None,
            enemy_type: crate::combat::EnemyType::RedMovingSpace,
            warm_start: Vec::new(),
            prior_reference_crews: Vec::new(),
            optimize_cache_key: None,
            enable_learned_pair_prior: false,
            learned_officer_scores: None,
        }
    }

    #[test]
    fn linear_eval_ranks_crews_by_expected_damage_descending() {
        let registry = DataRegistry::load().expect("registry");
        let scenario = minimal_scenario("saladin", "2918121098", Some(8));
        let ranked = run_linear_eval_with_registry(&registry, &scenario, |_| true, || true);
        assert!(!ranked.is_empty());
        for r in &ranked {
            assert_eq!(r.trials_run, 0);
            assert!(r.expected_hull_damage.is_some());
            assert_eq!(r.win_rate, 0.0);
        }
        for w in ranked.windows(2) {
            let a = w[0].expected_hull_damage.unwrap();
            let b = w[1].expected_hull_damage.unwrap();
            assert!(a >= b, "expected descending damage order");
        }
    }

    #[test]
    fn linear_eval_respects_max_candidates_output_cap() {
        let registry = DataRegistry::load().expect("registry");
        let scenario = minimal_scenario("saladin", "2918121098", Some(3));
        let ranked = run_linear_eval_with_registry(&registry, &scenario, |_| true, || true);
        assert!(ranked.len() <= 3);
    }
}
