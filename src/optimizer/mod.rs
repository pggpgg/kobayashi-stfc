pub mod analytical;
pub mod crew_generator;
pub mod genetic;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

use crate::optimizer::crew_generator::CrewGenerator;
use crate::optimizer::genetic::{run_genetic_optimizer_ranked, GeneticConfig};
use crate::optimizer::monte_carlo::{run_monte_carlo_parallel, SimulationResult};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::parallel::batch_ranges;

/// Number of progress-reporting batches for optimize-with-progress (UI jobs).
const OPTIMIZE_PROGRESS_BATCH_COUNT: usize = 40;

/// Optimizer strategy: exhaustive/sampled (candidate generation) or genetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerStrategy {
    /// Current path: CrewGenerator then Monte Carlo then rank.
    Exhaustive,
    /// Genetic algorithm for large search spaces.
    Genetic,
}

impl Default for OptimizerStrategy {
    fn default() -> Self {
        Self::Exhaustive
    }
}

#[derive(Debug, Clone)]
pub struct OptimizationScenario<'a> {
    pub ship: &'a str,
    pub hostile: &'a str,
    pub simulation_count: usize,
    pub seed: u64,
    /// When None, all crew combinations are explored. When Some(n), generation stops after n candidates.
    pub max_candidates: Option<usize>,
    /// Which optimizer to use. When Genetic, max_candidates is ignored and GA config is used.
    pub strategy: OptimizerStrategy,
    /// When true, below-decks pool only includes officers that have a below-decks ability.
    pub only_below_decks_with_ability: bool,
}

impl Default for OptimizationScenario<'_> {
    fn default() -> Self {
        Self {
            ship: "",
            hostile: "",
            simulation_count: 5000,
            seed: 0,
            max_candidates: Some(128),
            strategy: OptimizerStrategy::Exhaustive,
            only_below_decks_with_ability: false,
        }
    }
}

pub fn optimize_scenario(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    match scenario.strategy {
        OptimizerStrategy::Exhaustive => optimize_scenario_exhaustive(scenario),
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |_, _, _| {}),
    }
}

/// Exhaustive/sampled path: generator → Monte Carlo → rank.
fn optimize_scenario_exhaustive(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    let generator = CrewGenerator::with_strategy(crate::optimizer::crew_generator::CandidateStrategy {
        max_candidates: scenario.max_candidates,
        only_below_decks_with_ability: scenario.only_below_decks_with_ability,
        ..crate::optimizer::crew_generator::CandidateStrategy::default()
    });
    let candidates = generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
    let simulation_results = run_monte_carlo_parallel(
        scenario.ship,
        scenario.hostile,
        &candidates,
        scenario.simulation_count.max(1),
        scenario.seed,
    );
    rank_results(simulation_results)
}

/// Genetic path: GA with progress callback, then final MC on top candidates, then rank.
pub fn optimize_scenario_genetic<F>(
    scenario: &OptimizationScenario<'_>,
    on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(usize, usize, f32),
{
    let config = GeneticConfig {
        only_below_decks_with_ability: scenario.only_below_decks_with_ability,
        ..GeneticConfig::default()
    };
    run_genetic_optimizer_ranked(
        scenario.ship,
        scenario.hostile,
        &config,
        scenario.seed,
        scenario.simulation_count.max(1),
        on_progress,
    )
}

/// Like [optimize_scenario] but runs in batches and invokes `on_progress(done, total)`.
/// For exhaustive: done/total = crews. For genetic: done/total = generations.
pub fn optimize_scenario_with_progress<F>(
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(u32, u32),
{
    match scenario.strategy {
        OptimizerStrategy::Exhaustive => {
            let generator = CrewGenerator::with_strategy(
                crate::optimizer::crew_generator::CandidateStrategy {
                    max_candidates: scenario.max_candidates,
                    only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                    ..crate::optimizer::crew_generator::CandidateStrategy::default()
                },
            );
            let candidates =
                generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
            let total = candidates.len();
            if total == 0 {
                return Vec::new();
            }
            // Report total immediately so UI shows "0 / total" while first batch runs.
            on_progress(0, total as u32);

            let num_batches = OPTIMIZE_PROGRESS_BATCH_COUNT.min(total);
            let ranges = batch_ranges(total, num_batches);
            let mut all_results: Vec<SimulationResult> = Vec::with_capacity(total);
            let sim_count = scenario.simulation_count.max(1);

            for (start, end) in ranges {
                let batch = &candidates[start..end];
                let batch_results = run_monte_carlo_parallel(
                    scenario.ship,
                    scenario.hostile,
                    batch,
                    sim_count,
                    scenario.seed,
                );
                all_results.extend(batch_results);
                on_progress(end as u32, total as u32);
            }

            rank_results(all_results)
        }
        OptimizerStrategy::Genetic => {
            let config = GeneticConfig {
                only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                ..GeneticConfig::default()
            };
            run_genetic_optimizer_ranked(
                scenario.ship,
                scenario.hostile,
                &config,
                scenario.seed,
                scenario.simulation_count.max(1),
                |gen, max_gen, _| on_progress(gen as u32, max_gen as u32),
            )
        }
    }
}

pub fn optimize_crew(ship: &str, hostile: &str, sim_count: u32) -> Vec<RankedCrewResult> {
    optimize_scenario(&OptimizationScenario {
        ship,
        hostile,
        simulation_count: sim_count as usize,
        seed: 0,
        max_candidates: Some(128),
        strategy: OptimizerStrategy::Exhaustive,
        only_below_decks_with_ability: false,
    })
}

#[cfg(test)]
mod tests {
    use super::{OptimizationScenario, OptimizerStrategy};

    #[test]
    fn genetic_strategy_returns_ranked_results_shape() {
        let scenario = OptimizationScenario {
            ship: "enterprise",
            hostile: "swarm",
            simulation_count: 100,
            seed: 42,
            max_candidates: None,
            strategy: OptimizerStrategy::Genetic,
            only_below_decks_with_ability: false,
        };
        let results = super::optimize_scenario(&scenario);
        for r in &results {
            assert_eq!(r.bridge.len(), 2, "each result must have 2 bridge");
            assert_eq!(r.below_decks.len(), 3, "each result must have 3 below_decks");
        }
    }
}
