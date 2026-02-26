pub mod analytical;
pub mod crew_generator;
pub mod genetic;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

use crate::optimizer::crew_generator::CrewGenerator;
use crate::optimizer::monte_carlo::{run_monte_carlo_parallel, SimulationResult};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::parallel::batch_ranges;

/// Number of progress-reporting batches for optimize-with-progress (UI jobs).
const OPTIMIZE_PROGRESS_BATCH_COUNT: usize = 40;

#[derive(Debug, Clone)]
pub struct OptimizationScenario<'a> {
    pub ship: &'a str,
    pub hostile: &'a str,
    pub simulation_count: usize,
    pub seed: u64,
}

pub fn optimize_scenario(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    let generator = CrewGenerator::new();
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

/// Like [optimize_scenario] but runs candidates in batches and invokes `on_progress(crews_done, total_crews)` after each batch.
/// Use for UI jobs so the server can report progress (e.g. for polling).
pub fn optimize_scenario_with_progress<F>(
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(u32, u32),
{
    let generator = CrewGenerator::new();
    let candidates = generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
    let total = candidates.len();
    if total == 0 {
        return Vec::new();
    }

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

pub fn optimize_crew(ship: &str, hostile: &str, sim_count: u32) -> Vec<RankedCrewResult> {
    optimize_scenario(&OptimizationScenario {
        ship,
        hostile,
        simulation_count: sim_count as usize,
        seed: 0,
    })
}
