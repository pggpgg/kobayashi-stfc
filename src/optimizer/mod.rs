pub mod analytical;
pub mod crew_generator;
pub mod genetic;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

use crate::data::data_registry::DataRegistry;
use crate::optimizer::analytical::expected_damage;
use crate::optimizer::crew_generator::{CandidateStrategy, CrewCandidate, CrewGenerator};
use crate::optimizer::genetic::{run_genetic_optimizer_ranked, GeneticConfig};
use crate::optimizer::monte_carlo::{
    run_monte_carlo_parallel, run_monte_carlo_parallel_with_registry, SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::tiered::{
    run_tiered_with_registry_with_progress, DEFAULT_SCOUT_SIMS, DEFAULT_TOP_K,
};
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, build_shared_scenario_data_standalone,
    scenario_to_combat_input_from_shared, SharedScenarioData,
};
use crate::parallel::batch_ranges;

/// Number of progress-reporting batches for optimize-with-progress (UI jobs).
const OPTIMIZE_PROGRESS_BATCH_COUNT: usize = 40;

/// Order candidates by closed-form expected hull damage (high first) so limited `max_candidates`
/// slices and progress batches prioritize analytically stronger crews. See [crate::optimizer::analytical].
fn sort_candidates_by_analytical_expected_damage(
    shared: &SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
) -> Vec<CrewCandidate> {
    let mut indexed: Vec<(usize, CrewCandidate)> = candidates.into_iter().enumerate().collect();
    indexed.sort_by(|(ia, ca), (ib, cb)| {
        let sa = expected_damage(&scenario_to_combat_input_from_shared(shared, ca, seed));
        let sb = expected_damage(&scenario_to_combat_input_from_shared(shared, cb, seed));
        sb.total_cmp(&sa).then_with(|| ia.cmp(ib))
    });
    indexed.into_iter().map(|(_, c)| c).collect()
}

/// Optimizer strategy: exhaustive/sampled (candidate generation), genetic, or tiered (scout → confirm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerStrategy {
    /// Current path: CrewGenerator then Monte Carlo then rank.
    Exhaustive,
    /// Genetic algorithm for large search spaces.
    Genetic,
    /// Two-pass: cheap scouting sims then full MC on top K.
    Tiered,
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
    /// Ship tier (1-based). When set, uses data/ships_extended if present for accurate stats.
    pub ship_tier: Option<u32>,
    /// Ship level (1-based). When set with tier, applies level bonuses from extended data.
    pub ship_level: Option<u32>,
    pub simulation_count: usize,
    pub seed: u64,
    /// When None, all crew combinations are explored. When Some(n), generation stops after n candidates.
    pub max_candidates: Option<usize>,
    /// Which optimizer to use. When Genetic, max_candidates is ignored and GA config is used.
    pub strategy: OptimizerStrategy,
    /// When true, below-decks pool only includes officers that have a below-decks ability.
    pub only_below_decks_with_ability: bool,
    /// When non-empty, seeds the genetic algorithm's initial population with these crews.
    /// Only used when strategy is Genetic; ignored for Exhaustive.
    pub seed_population: Vec<CrewCandidate>,
    /// Profile id for roster/profile/forbidden-tech paths. None = use default profile.
    pub profile_id: Option<&'a str>,
    /// Tiered only: sims per crew in scouting pass. None = use default (500).
    pub tiered_scout_sims: Option<usize>,
    /// Tiered only: number of top crews to run full confirmation. None = use default (20).
    pub tiered_top_k: Option<usize>,
}

impl Default for OptimizationScenario<'_> {
    fn default() -> Self {
        Self {
            ship: "",
            hostile: "",
            ship_tier: None,
            ship_level: None,
            simulation_count: 5000,
            seed: 0,
            max_candidates: Some(128),
            strategy: OptimizerStrategy::Exhaustive,
            only_below_decks_with_ability: false,
            seed_population: Vec::new(),
            profile_id: None,
            tiered_scout_sims: None,
            tiered_top_k: None,
        }
    }
}

pub fn optimize_scenario(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    match scenario.strategy {
        OptimizerStrategy::Exhaustive => optimize_scenario_exhaustive(scenario),
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |_, _, _| true),
        OptimizerStrategy::Tiered => optimize_scenario_exhaustive(scenario), // Tiered requires registry; fallback when none
    }
}

/// Tiered path with registry: generate candidates, then scouting → top K → full MC.
fn optimize_scenario_tiered_with_registry(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
) -> Vec<RankedCrewResult> {
    let generator = CrewGenerator::with_strategy(CandidateStrategy {
        max_candidates: scenario.max_candidates,
        only_below_decks_with_ability: scenario.only_below_decks_with_ability,
        ..CandidateStrategy::default()
    });
    let candidates = generator.generate_candidates_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.seed,
        scenario.profile_id,
    );
    let shared_tiered = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        None,
        None,
        scenario.profile_id,
    );
    let candidates =
        sort_candidates_by_analytical_expected_damage(&shared_tiered, candidates, scenario.seed);
    let scout_sims = scenario.tiered_scout_sims.unwrap_or(DEFAULT_SCOUT_SIMS);
    let top_k = scenario.tiered_top_k.unwrap_or(DEFAULT_TOP_K);
    run_tiered_with_registry_with_progress(
        registry,
        scenario.ship,
        scenario.hostile,
        candidates,
        scout_sims,
        scenario.simulation_count.max(1),
        top_k,
        scenario.seed,
        scenario.profile_id,
        |_, _| true,
    )
}

/// Like [optimize_scenario] but uses [DataRegistry] for officers and ship/hostile (no reload).
pub fn optimize_scenario_with_registry(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
) -> Vec<RankedCrewResult> {
    match scenario.strategy {
        OptimizerStrategy::Exhaustive => optimize_scenario_exhaustive_with_registry(registry, scenario),
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |_, _, _| true),
        OptimizerStrategy::Tiered => optimize_scenario_tiered_with_registry(registry, scenario),
    }
}

/// Exhaustive path using registry (no officer/ship/hostile reload).
fn optimize_scenario_exhaustive_with_registry(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
) -> Vec<RankedCrewResult> {
    let generator = CrewGenerator::with_strategy(crate::optimizer::crew_generator::CandidateStrategy {
        max_candidates: scenario.max_candidates,
        only_below_decks_with_ability: scenario.only_below_decks_with_ability,
        ..crate::optimizer::crew_generator::CandidateStrategy::default()
    });
    let candidates = generator.generate_candidates_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.seed,
        scenario.profile_id,
    );
    let shared_ex = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        scenario.profile_id,
    );
    let candidates =
        sort_candidates_by_analytical_expected_damage(&shared_ex, candidates, scenario.seed);
    let simulation_results = run_monte_carlo_parallel_with_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        &candidates,
        scenario.simulation_count.max(1),
        scenario.seed,
        scenario.profile_id,
    );
    rank_results(simulation_results)
}

/// Exhaustive/sampled path: generator → Monte Carlo → rank.
fn optimize_scenario_exhaustive(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    let generator = CrewGenerator::with_strategy(crate::optimizer::crew_generator::CandidateStrategy {
        max_candidates: scenario.max_candidates,
        only_below_decks_with_ability: scenario.only_below_decks_with_ability,
        ..crate::optimizer::crew_generator::CandidateStrategy::default()
    });
    let candidates = generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
    let shared = build_shared_scenario_data_standalone(
        scenario.ship,
        scenario.hostile,
    );
    let candidates =
        sort_candidates_by_analytical_expected_damage(&shared, candidates, scenario.seed);
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
/// When `scenario.seed_population` is non-empty, uses seeded config (larger pop, adaptive mutation).
/// Progress callback returns true to continue, false to abort.
pub fn optimize_scenario_genetic<F>(
    scenario: &OptimizationScenario<'_>,
    on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(usize, usize, f32) -> bool,
{
    let config = if scenario.seed_population.is_empty() {
        GeneticConfig {
            only_below_decks_with_ability: scenario.only_below_decks_with_ability,
            ..GeneticConfig::default()
        }
    } else {
        let mut cfg = GeneticConfig::seeded(scenario.seed_population.clone());
        cfg.only_below_decks_with_ability = scenario.only_below_decks_with_ability;
        cfg
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
/// For exhaustive: done/total = crews. For genetic: done/total = generations. Tiered requires registry.
pub fn optimize_scenario_with_progress<F>(
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(u32, u32),
{
    match scenario.strategy {
        OptimizerStrategy::Tiered => {
            // No registry; fall back to exhaustive with progress
            let scenario_ex = OptimizationScenario {
                ship: scenario.ship,
                hostile: scenario.hostile,
                ship_tier: scenario.ship_tier,
                ship_level: scenario.ship_level,
                simulation_count: scenario.simulation_count,
                seed: scenario.seed,
                max_candidates: scenario.max_candidates,
                strategy: OptimizerStrategy::Exhaustive,
                only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                seed_population: scenario.seed_population.clone(),
                profile_id: scenario.profile_id,
                tiered_scout_sims: scenario.tiered_scout_sims,
                tiered_top_k: scenario.tiered_top_k,
            };
            optimize_scenario_with_progress(&scenario_ex, on_progress)
        }
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
            let shared = build_shared_scenario_data_standalone(
                scenario.ship,
                scenario.hostile,
            );
            let candidates =
                sort_candidates_by_analytical_expected_damage(&shared, candidates, scenario.seed);
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
            optimize_scenario_genetic(scenario, |gen, max_gen, _| {
                on_progress(gen as u32, max_gen as u32);
                true
            })
        }
    }
}

/// Like [optimize_scenario_with_progress] but uses [DataRegistry] for exhaustive path (no reload).
/// Progress callback returns true to continue, false to abort (e.g. user cancelled).
pub fn optimize_scenario_with_progress_with_registry<F>(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(u32, u32) -> bool,
{
    match scenario.strategy {
        OptimizerStrategy::Tiered => {
            let generator = CrewGenerator::with_strategy(CandidateStrategy {
                max_candidates: scenario.max_candidates,
                only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                ..CandidateStrategy::default()
            });
            let candidates = generator.generate_candidates_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.seed,
                scenario.profile_id,
            );
            let scout_sims = scenario.tiered_scout_sims.unwrap_or(DEFAULT_SCOUT_SIMS);
            let top_k = scenario.tiered_top_k.unwrap_or(DEFAULT_TOP_K);
            run_tiered_with_registry_with_progress(
                registry,
                scenario.ship,
                scenario.hostile,
                candidates,
                scout_sims,
                scenario.simulation_count.max(1),
                top_k,
                scenario.seed,
                scenario.profile_id,
                &mut on_progress,
            )
        }
        OptimizerStrategy::Exhaustive => {
            let generator = CrewGenerator::with_strategy(
                crate::optimizer::crew_generator::CandidateStrategy {
                    max_candidates: scenario.max_candidates,
                    only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                    ..crate::optimizer::crew_generator::CandidateStrategy::default()
                },
            );
            let candidates = generator.generate_candidates_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.seed,
                scenario.profile_id,
            );
            let shared_ex = build_shared_scenario_data_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.ship_tier,
                scenario.ship_level,
                scenario.profile_id,
            );
            let candidates =
                sort_candidates_by_analytical_expected_damage(&shared_ex, candidates, scenario.seed);
            let total = candidates.len();
            if total == 0 {
                return Vec::new();
            }
            if !on_progress(0, total as u32) {
                return Vec::new();
            }

            let num_batches = OPTIMIZE_PROGRESS_BATCH_COUNT.min(total);
            let ranges = batch_ranges(total, num_batches);
            let mut all_results: Vec<SimulationResult> = Vec::with_capacity(total);
            let sim_count = scenario.simulation_count.max(1);

            for (start, end) in ranges {
                let batch = &candidates[start..end];
                let batch_results = run_monte_carlo_parallel_with_registry(
                    registry,
                    scenario.ship,
                    scenario.hostile,
                    scenario.ship_tier,
                    scenario.ship_level,
                    batch,
                    sim_count,
                    scenario.seed,
                    scenario.profile_id,
                );
                all_results.extend(batch_results);
                if !on_progress(end as u32, total as u32) {
                    break;
                }
            }

            rank_results(all_results)
        }
        OptimizerStrategy::Genetic => {
            optimize_scenario_genetic(scenario, |gen, max_gen, _| {
                on_progress(gen as u32, max_gen as u32);
                true
            })
        }
    }
}

pub fn optimize_crew(
    ship: &str,
    hostile: &str,
    sim_count: u32,
    profile_id: Option<&str>,
) -> Vec<RankedCrewResult> {
    optimize_scenario(&OptimizationScenario {
        ship,
        hostile,
        ship_tier: None,
        ship_level: None,
        simulation_count: sim_count as usize,
        seed: 0,
        max_candidates: Some(128),
        strategy: OptimizerStrategy::Exhaustive,
        only_below_decks_with_ability: false,
        seed_population: Vec::new(),
        profile_id,
        tiered_scout_sims: None,
        tiered_top_k: None,
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
            ship_tier: None,
            ship_level: None,
            simulation_count: 100,
            seed: 42,
            max_candidates: None,
            strategy: OptimizerStrategy::Genetic,
            only_below_decks_with_ability: false,
            seed_population: Vec::new(),
            profile_id: None,
            tiered_scout_sims: None,
            tiered_top_k: None,
        };
        let results = super::optimize_scenario(&scenario);
        for r in &results {
            assert_eq!(r.bridge.len(), 2, "each result must have 2 bridge");
            assert_eq!(r.below_decks.len(), 3, "each result must have 3 below_decks");
        }
    }
}
