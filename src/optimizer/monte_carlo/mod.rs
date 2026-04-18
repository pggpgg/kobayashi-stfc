//! Monte Carlo combat simulation and crew resolution.
//!
//! - [crew_resolution]: build crew from officer names, seats, and ability contexts.
//! - [scenario]: shared scenario data and candidate → combat input.
//! - [simulation]: run_monte_carlo* and SimulationResult.

mod compare_crews;
mod crew_resolution;
pub(crate) mod scenario;
mod simulation;

pub use compare_crews::{
    compare_crews_monte_carlo_with_registry, CompareCrewDistribution, CompareCrewsOutcome,
};
pub use crew_resolution::crew_from_officer_names;
pub use scenario::DefenderOpponent;
pub use simulation::{
    crew_candidate_stable_hash, replay_optimize_iteration_with_registry, run_monte_carlo,
    run_monte_carlo_parallel, run_monte_carlo_parallel_deduped,
    run_monte_carlo_parallel_with_registry, run_monte_carlo_with_registry, MonteCarloSeedReplay,
    SimulationResult,
};
pub(crate) use simulation::{run_monte_carlo_scout_phase_with_shared, run_monte_carlo_with_shared};
