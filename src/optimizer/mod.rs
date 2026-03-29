pub mod analytical;
pub mod constraints;
pub mod crew_generator;
pub mod genetic;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

use crate::data::data_registry::DataRegistry;
use crate::optimizer::analytical::expected_damage;
use crate::optimizer::constraints::{filter_candidates, CrewSearchConstraints};
use crate::optimizer::crew_generator::{
    CandidateStrategy, CrewCandidate, CrewGenerator, DEFAULT_BELOW_DECKS_SLOTS,
};
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

fn apply_crew_constraints(
    candidates: Vec<CrewCandidate>,
    scenario: &OptimizationScenario<'_>,
) -> Vec<CrewCandidate> {
    match &scenario.constraints {
        Some(c) => filter_candidates(candidates, c),
        None => candidates,
    }
}

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

/// After analytical ranking, optionally keep only the top `keep` crews (approximate proxy; full MC still determines win rate).
/// Returns `(candidates, Some((generated, kept)))` when truncation happened.
pub(crate) fn sort_and_analytical_prefilter(
    shared: &SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    keep: Option<usize>,
) -> (Vec<CrewCandidate>, Option<(usize, usize)>) {
    let generated = candidates.len();
    let mut sorted = sort_candidates_by_analytical_expected_damage(shared, candidates, seed);
    let Some(k) = keep.filter(|n| *n > 0) else {
        return (sorted, None);
    };
    if sorted.len() > k {
        sorted.truncate(k);
        (sorted, Some((generated, k)))
    } else {
        (sorted, None)
    }
}

/// Result of [`optimize_scenario_with_progress_with_registry`] including optional analytical pre-filter stats.
#[derive(Debug, Clone)]
pub struct OptimizeRunOutcome {
    pub ranked: Vec<RankedCrewResult>,
    /// `Some((generated, kept))` when crews were truncated after analytical ranking before Monte Carlo.
    pub analytical_prefilter: Option<(usize, usize)>,
}

/// Progress update for async optimize jobs (SSE / polling): phase label, counts, optional partial top crews.
#[derive(Debug, Clone)]
pub struct OptimizeProgressTick {
    pub crews_done: u32,
    pub total_crews: u32,
    /// Stable labels: `heuristics`, `monte_carlo`, `genetic`, `tiered_scout`, `tiered_confirm`.
    pub phase: &'static str,
    pub partial_top: Option<Vec<RankedCrewResult>>,
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
    /// When set, keep only this many crews after analytical expected-hull-damage ranking before Monte Carlo. Genetic ignores this.
    pub analytical_prefilter_keep: Option<usize>,
    /// Below-decks slot count for candidate generation (resolved from API / tier defaults upstream).
    pub below_decks_slots: usize,
    /// Optional filters on candidate crews (must-include, exclude, groups, seating).
    pub constraints: Option<CrewSearchConstraints>,
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
            analytical_prefilter_keep: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
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
        below_decks_slots: scenario.below_decks_slots,
        ..CandidateStrategy::default()
    });
    let candidates = generator.generate_candidates_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.seed,
        scenario.profile_id,
    );
    let candidates = apply_crew_constraints(candidates, scenario);
    let shared_tiered = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        None,
        None,
        scenario.profile_id,
    );
    let (candidates, _) = sort_and_analytical_prefilter(
        &shared_tiered,
        candidates,
        scenario.seed,
        scenario.analytical_prefilter_keep,
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
        |_| true,
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
        below_decks_slots: scenario.below_decks_slots,
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
    let (candidates, _) = sort_and_analytical_prefilter(
        &shared_ex,
        candidates,
        scenario.seed,
        scenario.analytical_prefilter_keep,
    );
    let (simulation_results, _) = run_monte_carlo_parallel_with_registry(
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
        below_decks_slots: scenario.below_decks_slots,
        ..crate::optimizer::crew_generator::CandidateStrategy::default()
    });
    let candidates = generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
    let candidates = apply_crew_constraints(candidates, scenario);
    let shared = build_shared_scenario_data_standalone(
        scenario.ship,
        scenario.hostile,
    );
    let (candidates, _) = sort_and_analytical_prefilter(
        &shared,
        candidates,
        scenario.seed,
        scenario.analytical_prefilter_keep,
    );
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
    let filtered_seeds: Vec<CrewCandidate> = apply_crew_constraints(
        scenario.seed_population.clone(),
        scenario,
    );

    let config = if filtered_seeds.is_empty() {
        GeneticConfig {
            only_below_decks_with_ability: scenario.only_below_decks_with_ability,
            below_decks_slots: scenario.below_decks_slots,
            constraints: scenario.constraints.clone(),
            ..GeneticConfig::default()
        }
    } else {
        let mut cfg = GeneticConfig::seeded(filtered_seeds);
        cfg.only_below_decks_with_ability = scenario.only_below_decks_with_ability;
        cfg.below_decks_slots = scenario.below_decks_slots;
        cfg.constraints = scenario.constraints.clone();
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

/// Like [optimize_scenario] but runs in batches and invokes `on_progress` with phase and optional partial top-N.
/// For exhaustive: done/total = crews. For genetic: done/total = generations. Tiered requires registry.
/// Returning `false` from `on_progress` aborts between batches (sync callers typically always return `true`).
pub fn optimize_scenario_with_progress<F>(
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(OptimizeProgressTick) -> bool,
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
                analytical_prefilter_keep: scenario.analytical_prefilter_keep,
                below_decks_slots: scenario.below_decks_slots,
                constraints: scenario.constraints.clone(),
            };
            optimize_scenario_with_progress(&scenario_ex, on_progress)
        }
        OptimizerStrategy::Exhaustive => {
            let generator = CrewGenerator::with_strategy(
                crate::optimizer::crew_generator::CandidateStrategy {
                    max_candidates: scenario.max_candidates,
                    only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                    below_decks_slots: scenario.below_decks_slots,
                    ..crate::optimizer::crew_generator::CandidateStrategy::default()
                },
            );
            let candidates =
                generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
            let candidates = apply_crew_constraints(candidates, scenario);
            let shared = build_shared_scenario_data_standalone(
                scenario.ship,
                scenario.hostile,
            );
            let (candidates, _) = sort_and_analytical_prefilter(
                &shared,
                candidates,
                scenario.seed,
                scenario.analytical_prefilter_keep,
            );
            let total = candidates.len();
            if total == 0 {
                return Vec::new();
            }
            if !on_progress(OptimizeProgressTick {
                crews_done: 0,
                total_crews: total as u32,
                phase: "monte_carlo",
                partial_top: None,
            }) {
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
                let partial_top = rank_results(all_results.clone())
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>();
                if !on_progress(OptimizeProgressTick {
                    crews_done: end as u32,
                    total_crews: total as u32,
                    phase: "monte_carlo",
                    partial_top: Some(partial_top),
                }) {
                    break;
                }
            }

            rank_results(all_results)
        }
        OptimizerStrategy::Genetic => {
            optimize_scenario_genetic(scenario, |gen, max_gen, _| {
                on_progress(OptimizeProgressTick {
                    crews_done: gen as u32,
                    total_crews: max_gen.max(1) as u32,
                    phase: "genetic",
                    partial_top: None,
                })
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
) -> OptimizeRunOutcome
where
    F: FnMut(OptimizeProgressTick) -> bool,
{
    match scenario.strategy {
        OptimizerStrategy::Tiered => {
            let generator = CrewGenerator::with_strategy(CandidateStrategy {
                max_candidates: scenario.max_candidates,
                only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                below_decks_slots: scenario.below_decks_slots,
                ..CandidateStrategy::default()
            });
            let candidates = generator.generate_candidates_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.seed,
                scenario.profile_id,
            );
            let candidates = apply_crew_constraints(candidates, scenario);
            let shared = build_shared_scenario_data_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.ship_tier,
                scenario.ship_level,
                scenario.profile_id,
            );
            let (candidates, analytical_prefilter) = sort_and_analytical_prefilter(
                &shared,
                candidates,
                scenario.seed,
                scenario.analytical_prefilter_keep,
            );
            let scout_sims = scenario.tiered_scout_sims.unwrap_or(DEFAULT_SCOUT_SIMS);
            let top_k = scenario.tiered_top_k.unwrap_or(DEFAULT_TOP_K);
            let ranked = run_tiered_with_registry_with_progress(
                registry,
                scenario.ship,
                scenario.hostile,
                candidates,
                scout_sims,
                scenario.simulation_count.max(1),
                top_k,
                scenario.seed,
                scenario.profile_id,
                |tick| on_progress(tick),
            );
            OptimizeRunOutcome {
                ranked,
                analytical_prefilter,
            }
        }
        OptimizerStrategy::Exhaustive => {
            let generator = CrewGenerator::with_strategy(
                crate::optimizer::crew_generator::CandidateStrategy {
                    max_candidates: scenario.max_candidates,
                    only_below_decks_with_ability: scenario.only_below_decks_with_ability,
                    below_decks_slots: scenario.below_decks_slots,
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
            let candidates = apply_crew_constraints(candidates, scenario);
            let shared_ex = build_shared_scenario_data_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.ship_tier,
                scenario.ship_level,
                scenario.profile_id,
            );
            let (candidates, analytical_prefilter) = sort_and_analytical_prefilter(
                &shared_ex,
                candidates,
                scenario.seed,
                scenario.analytical_prefilter_keep,
            );
            let total = candidates.len();
            if total == 0 {
                return OptimizeRunOutcome {
                    ranked: Vec::new(),
                    analytical_prefilter,
                };
            }
            if !on_progress(OptimizeProgressTick {
                crews_done: 0,
                total_crews: total as u32,
                phase: "monte_carlo",
                partial_top: None,
            }) {
                return OptimizeRunOutcome {
                    ranked: Vec::new(),
                    analytical_prefilter,
                };
            }

            let num_batches = OPTIMIZE_PROGRESS_BATCH_COUNT.min(total);
            let ranges = batch_ranges(total, num_batches);
            let mut all_results: Vec<SimulationResult> = Vec::with_capacity(total);
            let sim_count = scenario.simulation_count.max(1);

            for (start, end) in ranges {
                let batch = &candidates[start..end];
                let (batch_results, _) = run_monte_carlo_parallel_with_registry(
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
                let partial_top = rank_results(all_results.clone())
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>();
                if !on_progress(OptimizeProgressTick {
                    crews_done: end as u32,
                    total_crews: total as u32,
                    phase: "monte_carlo",
                    partial_top: Some(partial_top),
                }) {
                    break;
                }
            }

            OptimizeRunOutcome {
                ranked: rank_results(all_results),
                analytical_prefilter,
            }
        }
        OptimizerStrategy::Genetic => OptimizeRunOutcome {
            ranked: optimize_scenario_genetic(scenario, |gen, max_gen, _| {
                on_progress(OptimizeProgressTick {
                    crews_done: gen as u32,
                    total_crews: max_gen.max(1) as u32,
                    phase: "genetic",
                    partial_top: None,
                })
            }),
            analytical_prefilter: None,
        },
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
        analytical_prefilter_keep: None,
        below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
        constraints: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizerStrategy,
    };
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::crew_generator::DEFAULT_BELOW_DECKS_SLOTS;

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
            analytical_prefilter_keep: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
        };
        let results = super::optimize_scenario(&scenario);
        for r in &results {
            assert_eq!(r.bridge.len(), 2, "each result must have 2 bridge");
            assert_eq!(
                r.below_decks.len(),
                DEFAULT_BELOW_DECKS_SLOTS,
                "each result must match scenario below_decks_slots"
            );
        }
    }

    #[test]
    fn analytical_prefilter_truncates_before_monte_carlo() {
        let registry = DataRegistry::load().expect("data registry");
        let scenario = OptimizationScenario {
            ship: "saladin",
            hostile: "2918121098",
            ship_tier: None,
            ship_level: None,
            simulation_count: 15,
            seed: 11,
            max_candidates: Some(80),
            strategy: OptimizerStrategy::Exhaustive,
            only_below_decks_with_ability: false,
            seed_population: Vec::new(),
            profile_id: None,
            tiered_scout_sims: None,
            tiered_top_k: None,
            analytical_prefilter_keep: Some(4),
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
        };
        let out = optimize_scenario_with_progress_with_registry(&registry, &scenario, |_| true);
        assert!(
            out.ranked.len() <= 4,
            "expected at most 4 ranked crews, got {}",
            out.ranked.len()
        );
        let (g, k) = out.analytical_prefilter.expect("truncation should be recorded");
        assert!(g > k, "generated {g} should exceed kept {k}");
        assert_eq!(k, 4);
    }
}
