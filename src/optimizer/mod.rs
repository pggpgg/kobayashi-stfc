pub mod analytical;
pub mod chain;
pub mod constraints;
pub mod crew_generator;
pub mod genetic;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

pub use chain::{ChainGrindParams, ChainSecondaryObjective, ChainSimulationSummary};

use std::collections::HashSet;

use crate::data::data_registry::DataRegistry;
use crate::optimizer::analytical::expected_damage;
use crate::optimizer::constraints::{filter_candidates, CrewSearchConstraints};
use crate::optimizer::crew_generator::{
    CandidateStrategy, CrewCandidate, CrewGenerator, DEFAULT_BELOW_DECKS_SLOTS,
};
use crate::optimizer::genetic::{run_genetic_optimizer_ranked, GeneticConfig};
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, build_shared_scenario_data_standalone,
    scenario_to_combat_input_from_shared, DefenderOpponent, SharedScenarioData,
};
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_parallel,
    run_monte_carlo_parallel_with_registry, SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::tiered::{
    run_tiered_with_registry_with_progress, DEFAULT_SCOUT_SIMS, DEFAULT_TOP_K,
};

fn scenario_support_slice<'a>(scenario: &'a OptimizationScenario<'_>) -> Option<&'a [String]> {
    if scenario.support_buffs.is_empty() {
        None
    } else {
        Some(scenario.support_buffs.as_slice())
    }
}
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

fn analytical_prefilter_unless_chain(
    shared: &SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    keep: Option<usize>,
    chain_grind: &Option<ChainGrindParams>,
) -> (Vec<CrewCandidate>, Option<(usize, usize)>) {
    if chain_grind.is_some() {
        (candidates, None)
    } else {
        sort_and_analytical_prefilter(shared, candidates, seed, keep)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizerStrategy {
    /// Current path: CrewGenerator then Monte Carlo then rank.
    #[default]
    Exhaustive,
    /// Genetic algorithm for large search spaces.
    Genetic,
    /// Two-pass: cheap scouting sims then full MC on top K.
    Tiered,
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
    /// Support buff ids (lower decks) applied when building scenario profile; empty = none.
    pub support_buffs: Vec<String>,
    /// Sequential chain grind (HHP carry-over, full SHP each link). When set, analytical prefilter is skipped.
    pub chain_grind: Option<ChainGrindParams>,
    /// Defender is NPC hostile vs player ship for canonical opponent-category conditions.
    pub defender_opponent: DefenderOpponent,
    /// Optional crews prepended before generated candidates (deduped by stable hash); e.g. warm-start from UI.
    pub warm_start: Vec<CrewCandidate>,
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
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
        }
    }
}

fn prepend_warm_start_dedupe(warm: &[CrewCandidate], generated: Vec<CrewCandidate>) -> Vec<CrewCandidate> {
    if warm.is_empty() {
        return generated;
    }
    let mut seen: HashSet<u64> = HashSet::new();
    let mut out = Vec::with_capacity(warm.len() + generated.len());
    for c in warm {
        if seen.insert(crew_candidate_stable_hash(c)) {
            out.push(c.clone());
        }
    }
    for c in generated {
        if seen.insert(crew_candidate_stable_hash(&c)) {
            out.push(c);
        }
    }
    out
}

fn candidate_strategy_from_scenario(scenario: &OptimizationScenario<'_>) -> CandidateStrategy {
    CandidateStrategy {
        max_candidates: scenario.max_candidates,
        only_below_decks_with_ability: scenario.only_below_decks_with_ability,
        below_decks_slots: scenario.below_decks_slots,
        constraints: scenario.constraints.clone(),
        ..CandidateStrategy::default()
    }
}

/// Counts candidates the optimizer will run after generation, warm-start prepend, and constraint filter.
/// Used by the API when `strategy` is omitted to auto-pick tiered vs exhaustive.
pub fn count_effective_optimize_candidates(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    seed: u64,
    profile_id: Option<&str>,
    strategy: CandidateStrategy,
    warm_start: &[CrewCandidate],
) -> usize {
    let constraints = strategy.constraints.clone();
    let generator = CrewGenerator::with_strategy(strategy);
    let candidates = generator.generate_candidates_from_registry(
        registry,
        ship,
        hostile,
        seed,
        profile_id,
    );
    let candidates = prepend_warm_start_dedupe(warm_start, candidates);
    match constraints.as_ref() {
        Some(c) if !c.is_empty() => filter_candidates(candidates, c).len(),
        _ => candidates.len(),
    }
}

/// Automatic analytical prefilter cap when the client omitted `analytical_prefilter_keep`.
/// Uses post-merge candidate count `n` and optional `max_candidates` for budget shaping.
pub(crate) fn analytical_prefilter_keep_auto(
    n: usize,
    max_candidates: Option<usize>,
    top_k: usize,
) -> Option<usize> {
    const MIN_N: usize = 400;
    if n <= MIN_N {
        return None;
    }
    let top_ref = top_k.max(1);
    let mut keep = 25usize.saturating_mul(top_ref);
    if let Some(mc) = max_candidates {
        if mc > 0 {
            let mc = mc.min(n);
            keep = keep.min(mc.saturating_mul(4).max(top_ref.saturating_mul(10)));
        }
    }
    keep = keep.clamp(256, 6000);
    keep = keep.min(n);
    if keep >= n {
        None
    } else {
        Some(keep)
    }
}

/// Request `analytical_prefilter_keep`, or an automatic cap when omitted (not for chain / genetic).
fn resolved_analytical_prefilter_keep(
    scenario: &OptimizationScenario<'_>,
    n_after_merge: usize,
) -> Option<usize> {
    if scenario.chain_grind.is_some() {
        return scenario.analytical_prefilter_keep;
    }
    if matches!(scenario.strategy, OptimizerStrategy::Genetic) {
        return scenario.analytical_prefilter_keep;
    }
    if let Some(k) = scenario.analytical_prefilter_keep {
        return Some(k);
    }
    analytical_prefilter_keep_auto(
        n_after_merge,
        scenario.max_candidates,
        scenario.tiered_top_k.unwrap_or(DEFAULT_TOP_K).max(1),
    )
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
    let shared_tiered = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        scenario.profile_id,
        scenario_support_slice(scenario),
        scenario.defender_opponent,
    );
    let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
    let (candidates, _) = analytical_prefilter_unless_chain(
        &shared_tiered,
        candidates,
        scenario.seed,
        keep,
        &scenario.chain_grind,
    );
    let scout_sims = scenario.tiered_scout_sims.unwrap_or(DEFAULT_SCOUT_SIMS);
    let top_k = scenario.tiered_top_k.unwrap_or(DEFAULT_TOP_K);
    run_tiered_with_registry_with_progress(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        candidates,
        scout_sims,
        scenario.simulation_count.max(1),
        top_k,
        scenario.seed,
        scenario.profile_id,
        scenario_support_slice(scenario),
        scenario.chain_grind.clone(),
        scenario.defender_opponent,
        |_| true,
    )
}

/// Like [optimize_scenario] but uses [DataRegistry] for officers and ship/hostile (no reload).
pub fn optimize_scenario_with_registry(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
) -> Vec<RankedCrewResult> {
    match scenario.strategy {
        OptimizerStrategy::Exhaustive => {
            optimize_scenario_exhaustive_with_registry(registry, scenario)
        }
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |_, _, _| true),
        OptimizerStrategy::Tiered => optimize_scenario_tiered_with_registry(registry, scenario),
    }
}

/// Exhaustive path using registry (no officer/ship/hostile reload).
fn optimize_scenario_exhaustive_with_registry(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
) -> Vec<RankedCrewResult> {
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
    let shared_ex = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        scenario.profile_id,
        scenario_support_slice(scenario),
        scenario.defender_opponent,
    );
    let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
    let (candidates, _) = analytical_prefilter_unless_chain(
        &shared_ex,
        candidates,
        scenario.seed,
        keep,
        &scenario.chain_grind,
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
        scenario_support_slice(scenario),
        scenario.chain_grind.clone(),
        scenario.defender_opponent,
    );
    rank_results(simulation_results)
}

/// Exhaustive/sampled path: generator → Monte Carlo → rank.
fn optimize_scenario_exhaustive(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    let generator = CrewGenerator::with_strategy(candidate_strategy_from_scenario(scenario));
    let candidates = generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
    let candidates = prepend_warm_start_dedupe(&scenario.warm_start, candidates);
    let candidates = apply_crew_constraints(candidates, scenario);
    let shared = build_shared_scenario_data_standalone(
        scenario.ship,
        scenario.hostile,
        scenario_support_slice(scenario),
        scenario.defender_opponent,
    );
    let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
    let (candidates, _) = analytical_prefilter_unless_chain(
        &shared,
        candidates,
        scenario.seed,
        keep,
        &scenario.chain_grind,
    );
    let simulation_results = run_monte_carlo_parallel(
        scenario.ship,
        scenario.hostile,
        &candidates,
        scenario.simulation_count.max(1),
        scenario.seed,
        scenario_support_slice(scenario),
        scenario.chain_grind.clone(),
        scenario.defender_opponent,
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
    let filtered_seeds: Vec<CrewCandidate> =
        apply_crew_constraints(scenario.seed_population.clone(), scenario);

    let config = if filtered_seeds.is_empty() {
        GeneticConfig {
            only_below_decks_with_ability: scenario.only_below_decks_with_ability,
            below_decks_slots: scenario.below_decks_slots,
            constraints: scenario.constraints.clone(),
            support_buffs: scenario.support_buffs.clone(),
            chain_grind: scenario.chain_grind.clone(),
            defender_opponent: scenario.defender_opponent,
            ..GeneticConfig::default()
        }
    } else {
        let mut cfg = GeneticConfig::seeded(filtered_seeds);
        cfg.only_below_decks_with_ability = scenario.only_below_decks_with_ability;
        cfg.below_decks_slots = scenario.below_decks_slots;
        cfg.constraints = scenario.constraints.clone();
        cfg.support_buffs = scenario.support_buffs.clone();
        cfg.chain_grind = scenario.chain_grind.clone();
        cfg.defender_opponent = scenario.defender_opponent;
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
                support_buffs: scenario.support_buffs.clone(),
                chain_grind: scenario.chain_grind.clone(),
                defender_opponent: scenario.defender_opponent,
                warm_start: scenario.warm_start.clone(),
            };
            optimize_scenario_with_progress(&scenario_ex, on_progress)
        }
        OptimizerStrategy::Exhaustive => {
            let generator = CrewGenerator::with_strategy(candidate_strategy_from_scenario(scenario));
            let candidates =
                generator.generate_candidates(scenario.ship, scenario.hostile, scenario.seed);
            let candidates = prepend_warm_start_dedupe(&scenario.warm_start, candidates);
            let candidates = apply_crew_constraints(candidates, scenario);
            let shared = build_shared_scenario_data_standalone(
                scenario.ship,
                scenario.hostile,
                scenario_support_slice(scenario),
                scenario.defender_opponent,
            );
            let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
            let (candidates, _) = analytical_prefilter_unless_chain(
                &shared,
                candidates,
                scenario.seed,
                keep,
                &scenario.chain_grind,
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
                    scenario_support_slice(scenario),
                    scenario.chain_grind.clone(),
                    scenario.defender_opponent,
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
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |gen, max_gen, _| {
            on_progress(OptimizeProgressTick {
                crews_done: gen as u32,
                total_crews: max_gen.max(1) as u32,
                phase: "genetic",
                partial_top: None,
            })
        }),
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
                scenario_support_slice(scenario),
                scenario.defender_opponent,
            );
            let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
            let (candidates, analytical_prefilter) = analytical_prefilter_unless_chain(
                &shared,
                candidates,
                scenario.seed,
                keep,
                &scenario.chain_grind,
            );
            let scout_sims = scenario.tiered_scout_sims.unwrap_or(DEFAULT_SCOUT_SIMS);
            let top_k = scenario.tiered_top_k.unwrap_or(DEFAULT_TOP_K);
            let ranked = run_tiered_with_registry_with_progress(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.ship_tier,
                scenario.ship_level,
                candidates,
                scout_sims,
                scenario.simulation_count.max(1),
                top_k,
                scenario.seed,
                scenario.profile_id,
                scenario_support_slice(scenario),
                scenario.chain_grind.clone(),
                scenario.defender_opponent,
                &mut on_progress,
            );
            OptimizeRunOutcome {
                ranked,
                analytical_prefilter,
            }
        }
        OptimizerStrategy::Exhaustive => {
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
            let shared_ex = build_shared_scenario_data_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.ship_tier,
                scenario.ship_level,
                scenario.profile_id,
                scenario_support_slice(scenario),
                scenario.defender_opponent,
            );
            let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
            let (candidates, analytical_prefilter) = analytical_prefilter_unless_chain(
                &shared_ex,
                candidates,
                scenario.seed,
                keep,
                &scenario.chain_grind,
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
                    scenario_support_slice(scenario),
                    scenario.chain_grind.clone(),
                    scenario.defender_opponent,
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
        support_buffs: Vec::new(),
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        warm_start: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        analytical_prefilter_keep_auto, count_effective_optimize_candidates,
        optimize_scenario_with_progress_with_registry, CandidateStrategy, OptimizationScenario,
        OptimizerStrategy,
    };
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::constraints::CrewSearchConstraints;
    use crate::optimizer::crew_generator::{CrewCandidate, DEFAULT_BELOW_DECKS_SLOTS};
    use crate::optimizer::monte_carlo::scenario::{
        build_shared_scenario_data_from_registry, scenario_to_combat_input_from_shared,
        DefenderOpponent,
    };

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
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
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
    fn analytical_prefilter_keep_auto_skips_small_n() {
        assert_eq!(analytical_prefilter_keep_auto(200, None, 20), None);
    }

    #[test]
    fn analytical_prefilter_keep_auto_max_candidates_tightens_vs_unbounded() {
        let unbounded = analytical_prefilter_keep_auto(20_000, None, 40).expect("keep");
        let capped = analytical_prefilter_keep_auto(20_000, Some(150), 40).expect("keep");
        assert!(capped < unbounded, "capped={capped} unbounded={unbounded}");
    }

    #[test]
    fn count_effective_candidates_respects_must_include_filter() {
        let registry = DataRegistry::load().expect("registry");
        let strat = CandidateStrategy {
            max_candidates: Some(2000),
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: Some(CrewSearchConstraints {
                must_include: vec!["___kobayashi_nonexistent_officer___".into()],
                ..Default::default()
            }),
            ..CandidateStrategy::default()
        };
        let n = count_effective_optimize_candidates(
            &registry,
            "saladin",
            "2918121098",
            1,
            None,
            strat,
            &[],
        );
        assert_eq!(n, 0);
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
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
        };
        let out = optimize_scenario_with_progress_with_registry(&registry, &scenario, |_| true);
        assert!(
            out.ranked.len() <= 4,
            "expected at most 4 ranked crews, got {}",
            out.ranked.len()
        );
        let (g, k) = out
            .analytical_prefilter
            .expect("truncation should be recorded");
        assert!(g > k, "generated {g} should exceed kept {k}");
        assert_eq!(k, 4);
    }

    /// Regression: tiered Monte Carlo must use the same resolved ship row as exhaustive when tier/level are set.
    #[test]
    fn tiered_shared_scenario_respects_ship_tier() {
        let registry = DataRegistry::load().expect("data registry");
        let hostile = "2918121098";
        let candidate = CrewCandidate {
            captain: "James T. Kirk".to_string(),
            bridge: vec!["Spock".to_string(), "Leonard McCoy".to_string()],
            below_decks: vec![
                "Montgomery Scott".to_string(),
                "Hikaru Sulu".to_string(),
                "Nyota Uhura".to_string(),
            ],
        };
        let shared_low = build_shared_scenario_data_from_registry(
            &registry,
            "amalgam",
            hostile,
            Some(1),
            Some(1),
            None,
            None,
            DefenderOpponent::Hostile,
        );
        let shared_high = build_shared_scenario_data_from_registry(
            &registry,
            "amalgam",
            hostile,
            Some(5),
            Some(1),
            None,
            None,
            DefenderOpponent::Hostile,
        );
        assert!(
            !shared_low.using_placeholder_combatants && !shared_high.using_placeholder_combatants,
            "expected real ship rows for amalgam"
        );
        let atk_low =
            scenario_to_combat_input_from_shared(&shared_low, &candidate, 1).attacker.attack;
        let atk_high =
            scenario_to_combat_input_from_shared(&shared_high, &candidate, 1).attacker.attack;
        assert!(
            (atk_high - atk_low).abs() > 1.0,
            "tier 1 vs 5 amalgam attacker attack should differ materially: {atk_low} vs {atk_high}"
        );
    }
}
