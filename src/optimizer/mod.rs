pub mod analytical;
pub mod crew_generator;
pub mod genetic;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

use crate::optimizer::crew_generator::CrewGenerator;
use crate::optimizer::monte_carlo::run_monte_carlo;
use crate::optimizer::ranking::{rank_results, RankedCrewResult};

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
    let simulation_results = run_monte_carlo(
        scenario.ship,
        scenario.hostile,
        &candidates,
        scenario.simulation_count,
        scenario.seed,
    );
    rank_results(simulation_results)
}

pub fn optimize_crew(ship: &str, hostile: &str, sim_count: u32) -> Vec<RankedCrewResult> {
    optimize_scenario(&OptimizationScenario {
        ship,
        hostile,
        simulation_count: sim_count as usize,
        seed: 0,
    })
}
