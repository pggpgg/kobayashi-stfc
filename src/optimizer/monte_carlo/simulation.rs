//! Simulation orchestration: run_monte_carlo* and SimulationResult.

use rayon::prelude::*;

use crate::combat::{simulate_combat, SimulationConfig, TraceMode};
use crate::data::data_registry::DataRegistry;
use crate::optimizer::crew_generator::CrewCandidate;

use super::crew_resolution::seeded_variance;
use super::scenario::{
    build_shared_scenario_data_from_registry, build_shared_scenario_data_standalone,
    scenario_to_combat_input_from_shared, SharedScenarioData,
};

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub candidate: CrewCandidate,
    pub win_rate: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub avg_hull_remaining: f64,
}

pub fn run_monte_carlo(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    allow_duplicate_officers: bool,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(
        ship,
        hostile,
        candidates,
        iterations,
        seed,
        false,
        allow_duplicate_officers,
    )
}

/// Like [run_monte_carlo] but distributes candidates across all CPU cores via Rayon.
/// Use for large candidate lists (e.g. optimizer sweeps). Results order matches input order.
pub fn run_monte_carlo_parallel(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    allow_duplicate_officers: bool,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(
        ship,
        hostile,
        candidates,
        iterations,
        seed,
        true,
        allow_duplicate_officers,
    )
}

/// Like [run_monte_carlo_parallel] but uses [DataRegistry] for officers and ship/hostile resolution (no reload).
/// When ship_tier or ship_level is set, uses data/ships_extended for accurate stats.
pub fn run_monte_carlo_parallel_with_registry(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    profile_id: Option<&str>,
    allow_duplicate_officers: bool,
) -> Vec<SimulationResult> {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
        allow_duplicate_officers,
    );
    run_monte_carlo_with_shared(shared, candidates, iterations, seed, true)
}

/// Like [run_monte_carlo] but uses [DataRegistry] for officers and ship/hostile resolution (no reload).
/// When ship_tier or ship_level is set, uses data/ships_extended for accurate stats.
pub fn run_monte_carlo_with_registry(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    profile_id: Option<&str>,
    allow_duplicate_officers: bool,
) -> Vec<SimulationResult> {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
        allow_duplicate_officers,
    );
    run_monte_carlo_with_shared(shared, candidates, iterations, seed, false)
}

fn run_monte_carlo_with_parallelism(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    allow_duplicate_officers: bool,
) -> Vec<SimulationResult> {
    let shared = build_shared_scenario_data_standalone(ship, hostile, allow_duplicate_officers);
    run_monte_carlo_with_shared(shared, candidates, iterations, seed, parallel)
}

/// Run Monte Carlo using pre-built SharedScenarioData (used by both legacy and registry paths).
pub(crate) fn run_monte_carlo_with_shared(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
) -> Vec<SimulationResult> {
    let run_one = |candidate: &CrewCandidate| {
        let input = scenario_to_combat_input_from_shared(&shared, candidate, seed);
        let mut wins = 0usize;
        let mut stalls = 0usize;
        let mut losses = 0usize;
        let mut surviving_hull_sum = 0.0;

        for iteration in 0..iterations {
            let iteration_seed = input.base_seed.wrapping_add(iteration as u64);
            let result = simulate_combat(
                &input.attacker,
                &input.defender,
                SimulationConfig {
                    rounds: input.rounds,
                    seed: iteration_seed,
                    trace_mode: TraceMode::Off,
                    allow_duplicate_officers: input.allow_duplicate_officers,
                },
                &input.crew,
            );
            let effective_hull = input.defender_hull * seeded_variance(iteration_seed);

            if result.winner_by_round_limit {
                stalls += 1;
            } else if result.attacker_won {
                wins += 1;
            } else {
                losses += 1;
            }

            if result.attacker_won {
                let remaining = if result.winner_by_round_limit {
                    (result.attacker_hull_remaining / input.attacker.hull_health.max(1.0))
                        .clamp(0.0, 1.0)
                } else {
                    ((result.total_damage - effective_hull) / effective_hull).clamp(0.0, 1.0)
                };
                surviving_hull_sum += remaining;
            }
        }

        let n = iterations as f64;
        let win_rate = if iterations == 0 { 0.0 } else { wins as f64 / n };
        let stall_rate = if iterations == 0 { 0.0 } else { stalls as f64 / n };
        let loss_rate = if iterations == 0 { 0.0 } else { losses as f64 / n };
        let avg_hull_remaining = if iterations == 0 {
            0.0
        } else {
            surviving_hull_sum / n
        };

        SimulationResult {
            candidate: candidate.clone(),
            win_rate,
            stall_rate,
            loss_rate,
            avg_hull_remaining,
        }
    };

    if parallel {
        candidates.par_iter().map(run_one).collect()
    } else {
        candidates.iter().map(run_one).collect()
    }
}
