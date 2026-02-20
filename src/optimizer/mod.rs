pub mod analytical;
pub mod crew_generator;
pub mod genetic;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

use crate::optimizer::crew_generator::CrewGenerator;
use crate::optimizer::monte_carlo::run_monte_carlo;
use crate::optimizer::ranking::{rank_results, RankedCrewResult};

pub fn optimize_crew(ship: &str, hostile: &str, sim_count: u32) -> Vec<RankedCrewResult> {
    let generator = CrewGenerator::new();
    let candidates = generator.generate_candidates(ship, hostile);
    let simulation_results = run_monte_carlo(ship, hostile, &candidates, sim_count as usize);
    rank_results(simulation_results)
}
