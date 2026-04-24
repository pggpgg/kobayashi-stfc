pub mod analytical;
pub mod budget_hints;
pub mod chain;
pub mod constraints;
pub mod crew_generator;
pub(crate) mod exhaustive_adaptive;
pub mod genetic;
pub mod matchup_priors;
pub mod monte_carlo;
pub mod ranking;
pub mod tiered;

pub use chain::{ChainGrindParams, ChainSecondaryObjective, ChainSimulationSummary};

use std::collections::{HashMap, HashSet};
use tracing::info;

use crate::data::data_registry::DataRegistry;
use crate::optimizer::constraints::{filter_candidates, CrewSearchConstraints};
use crate::optimizer::crew_generator::{
    CandidateStrategy, CrewCandidate, CrewGenerator, DEFAULT_BELOW_DECKS_SLOTS,
};
use crate::optimizer::genetic::{run_genetic_optimizer_ranked, GeneticConfig};
use crate::optimizer::matchup_priors::analytical_prefilter_rank_score;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, build_shared_scenario_data_standalone,
    scenario_to_combat_input_from_shared, DefenderOpponent, SharedScenarioData,
};
use crate::optimizer::exhaustive_adaptive::run_exhaustive_scout_then_full_mc;
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_parallel, run_monte_carlo_parallel_with_registry,
    SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::tiered::{
    run_tiered_with_registry_with_progress, tiered_scout_sims_for_workload,
    tiered_top_k_for_workload, TieredScoutBudgetStats, DEFAULT_SCOUT_SIMS, DEFAULT_TOP_K,
};

/// Reference full Monte Carlo count for auto prefilter scaling (matches [`OptimizationScenario`] default).
const ANALYTICAL_PREFILTER_REF_FULL_SIMS: usize = 5000;

/// Which per-crew simulation cost should shape automatic analytical prefilter keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnalyticalPrefilterWorkload {
    /// Tiered strategy: scouting pass uses this many simulations per crew.
    Tiered { scout_sims_per_crew: usize },
    /// Exhaustive (or tiered fallback without registry): full MC uses this many per crew.
    Exhaustive { full_sims_per_crew: usize },
}

impl AnalyticalPrefilterWorkload {
    fn ref_and_actual_sims(self) -> (usize, usize) {
        match self {
            AnalyticalPrefilterWorkload::Tiered {
                scout_sims_per_crew,
            } => (DEFAULT_SCOUT_SIMS, scout_sims_per_crew.max(1)),
            AnalyticalPrefilterWorkload::Exhaustive { full_sims_per_crew } => (
                ANALYTICAL_PREFILTER_REF_FULL_SIMS,
                full_sims_per_crew.max(1),
            ),
        }
    }
}

/// Scale `keep` by `ref_sims / actual_sims`, clamping the ratio to \[0.5, 2\] so extreme API values
/// do not explode or collapse the search space.
fn scale_keep_for_per_crew_sims(keep: usize, ref_sims: usize, actual_sims: usize) -> usize {
    let denom = actual_sims.max(1);
    let mut numer = ref_sims;
    if numer > 2 * denom {
        numer = 2 * denom;
    }
    if numer * 2 < denom {
        numer = (denom / 2).max(1);
    }
    keep.saturating_mul(numer) / denom
}

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
    warm_start: &[CrewCandidate],
) -> (Vec<CrewCandidate>, Option<(usize, usize)>) {
    if chain_grind.is_some() {
        (candidates, None)
    } else {
        sort_and_analytical_prefilter(shared, candidates, seed, keep, warm_start)
    }
}

/// Order candidates by closed-form expected hull damage (high first) so limited `max_candidates`
/// slices and progress batches prioritize analytically stronger crews. See [crate::optimizer::analytical].
fn sort_candidates_by_analytical_expected_damage(
    shared: &SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    warm_start: &[CrewCandidate],
) -> Vec<CrewCandidate> {
    let mut indexed: Vec<(usize, CrewCandidate)> = candidates.into_iter().enumerate().collect();
    indexed.sort_by(|(ia, ca), (ib, cb)| {
        let input_a = scenario_to_combat_input_from_shared(shared, ca, seed);
        let input_b = scenario_to_combat_input_from_shared(shared, cb, seed);
        let sa = analytical_prefilter_rank_score(shared, &input_a, ca, warm_start);
        let sb = analytical_prefilter_rank_score(shared, &input_b, cb, warm_start);
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
    warm_start: &[CrewCandidate],
) -> (Vec<CrewCandidate>, Option<(usize, usize)>) {
    let generated = candidates.len();
    let mut sorted =
        sort_candidates_by_analytical_expected_damage(shared, candidates, seed, warm_start);
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
    /// When tiered ran: `(n_candidates, resolved_scout_sims, resolved_top_k)` for cross-session cache metadata.
    pub tiered_resolved: Option<(usize, usize, usize)>,
    /// Scout-phase trial totals when tiered adaptive scout ran (see [`TieredScoutBudgetStats`]).
    pub tiered_scout_budget: Option<TieredScoutBudgetStats>,
    /// When exhaustive two-phase Monte Carlo ran (`exhaustive_scout_sims` + `exhaustive_scout_top_keep`): trial accounting (reuses [`TieredScoutBudgetStats`] shape).
    pub exhaustive_adaptive_budget: Option<TieredScoutBudgetStats>,
    /// Crews in the tiered candidate list that reused [`crate::data::optimize_history`] confirmation rows.
    pub optimize_history_confirm_hits: u32,
}

/// Progress update for async optimize jobs (SSE / polling): phase label, counts, optional partial top crews.
#[derive(Debug, Clone)]
pub struct OptimizeProgressTick {
    pub crews_done: u32,
    pub total_crews: u32,
    /// Stable labels: `heuristics`, `monte_carlo`, `genetic`, `tiered_scout`, `tiered_scout_refine`, `tiered_confirm`, `exhaustive_scout`, `exhaustive_confirm`.
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
    /// Tiered only: sims per crew in scouting pass. None = workload-aware default (see [`crate::optimizer::tiered::tiered_scout_sims_for_workload`]).
    pub tiered_scout_sims: Option<usize>,
    /// Tiered only: number of top crews to run full confirmation. None = workload-aware default (see [`crate::optimizer::tiered::tiered_top_k_for_workload`]).
    pub tiered_top_k: Option<usize>,
    /// Tiered only: when true, use a single uniform scout pass at the resolved scout cap (legacy). When false (default), use adaptive coarse→refine scout.
    pub tiered_scout_uniform: bool,
    /// Tiered: when set, shrink per-top-K confirmation totals so the sum does not exceed `floor(mult * K * simulation_count)`.
    /// Exhaustive two-phase: same cap applies to the width-based confirmation pass on the top-`keep` crews.
    pub tiered_confirm_budget_cap_mult: Option<f64>,
    /// Exhaustive only: when **both** this and [`Self::exhaustive_scout_top_keep`] are `Some`, run scout Monte Carlo at this many trials per crew on the full candidate list, then run full [`Self::simulation_count`] only on the top `exhaustive_scout_top_keep` crews by scout rank (others keep scout statistics).
    pub exhaustive_scout_sims: Option<usize>,
    /// Exhaustive only: paired with [`Self::exhaustive_scout_sims`] (see there).
    pub exhaustive_scout_top_keep: Option<usize>,
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
    /// Opaque client fingerprint for [`crate::data::optimize_history`] when `profile_id` is set.
    pub optimize_cache_key: Option<String>,
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
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
            optimize_cache_key: None,
        }
    }
}

fn tiered_scout_allocator_id(scenario: &OptimizationScenario<'_>) -> u8 {
    if scenario.tiered_scout_uniform {
        0
    } else {
        1
    }
}

fn tiered_preconfirmed_map(
    scenario: &OptimizationScenario<'_>,
    n_tiered: usize,
    scout_sims: usize,
    top_k: usize,
    candidates: &[CrewCandidate],
) -> (HashMap<u64, SimulationResult>, u32) {
    match (scenario.profile_id, scenario.optimize_cache_key.as_ref()) {
        (Some(pid), Some(key)) => {
            let t = key.trim();
            if t.is_empty() {
                return (HashMap::new(), 0);
            }
            let sims_u32 = scenario.simulation_count.min(u32::MAX as usize) as u32;
            crate::data::optimize_history::preconfirmed_for_candidates(
                pid,
                t,
                sims_u32,
                scenario.seed,
                scout_sims,
                top_k,
                n_tiered,
                tiered_scout_allocator_id(scenario),
                &scenario.chain_grind,
                crate::data::optimize_history::TIERED_BUDGET_POLICY_V2,
                scenario.tiered_confirm_budget_cap_mult.map(|x| x as f32),
                candidates,
            )
        }
        _ => (HashMap::new(), 0),
    }
}

fn exhaustive_two_phase_preconfirmed_map(
    scenario: &OptimizationScenario<'_>,
    n_candidates: usize,
    exhaustive_scout_sims: usize,
    exhaustive_top_keep: usize,
    candidates: &[CrewCandidate],
) -> (HashMap<u64, SimulationResult>, u32) {
    match (scenario.profile_id, scenario.optimize_cache_key.as_ref()) {
        (Some(pid), Some(key)) => {
            let t = key.trim();
            if t.is_empty() {
                return (HashMap::new(), 0);
            }
            let sims_u32 = scenario.simulation_count.min(u32::MAX as usize) as u32;
            crate::data::optimize_history::preconfirmed_for_exhaustive_two_phase(
                pid,
                t,
                sims_u32,
                scenario.seed,
                n_candidates,
                exhaustive_scout_sims,
                exhaustive_top_keep,
                &scenario.chain_grind,
                crate::data::optimize_history::EXHAUSTIVE_CONFIRM_POLICY_WIDTH_V1,
                scenario.tiered_confirm_budget_cap_mult.map(|x| x as f32),
                candidates,
            )
        }
        _ => (HashMap::new(), 0),
    }
}

fn prepend_warm_start_dedupe(
    warm: &[CrewCandidate],
    generated: Vec<CrewCandidate>,
) -> Vec<CrewCandidate> {
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
        roster_profile_id: scenario.profile_id.map(String::from),
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
    let candidates =
        generator.generate_candidates_from_registry(registry, ship, hostile, seed, profile_id);
    let candidates = prepend_warm_start_dedupe(warm_start, candidates);
    match constraints.as_ref() {
        Some(c) if !c.is_empty() => filter_candidates(candidates, c).len(),
        _ => candidates.len(),
    }
}

/// Automatic analytical prefilter cap when the client omitted `analytical_prefilter_keep`.
/// Uses post-merge candidate count `n`, optional `max_candidates`, and per-crew sim workload
/// (tiered scout sims vs exhaustive full sims) so expensive runs keep fewer crews before MC.
pub(crate) fn analytical_prefilter_keep_auto(
    n: usize,
    max_candidates: Option<usize>,
    top_k: usize,
    workload: AnalyticalPrefilterWorkload,
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
    let (ref_sims, actual_sims) = workload.ref_and_actual_sims();
    keep = scale_keep_for_per_crew_sims(keep, ref_sims, actual_sims);
    let min_keep = top_ref.saturating_mul(6).max(128).min(n);
    keep = keep.clamp(min_keep, 6000);
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
    let workload = if matches!(scenario.strategy, OptimizerStrategy::Tiered) {
        AnalyticalPrefilterWorkload::Tiered {
            scout_sims_per_crew: scenario
                .tiered_scout_sims
                .unwrap_or_else(|| tiered_scout_sims_for_workload(n_after_merge))
                .max(1),
        }
    } else {
        AnalyticalPrefilterWorkload::Exhaustive {
            full_sims_per_crew: scenario.simulation_count.max(1),
        }
    };
    let top_ref = scenario
        .tiered_top_k
        .unwrap_or_else(|| {
            if matches!(scenario.strategy, OptimizerStrategy::Tiered) {
                tiered_top_k_for_workload(n_after_merge)
            } else {
                DEFAULT_TOP_K
            }
        })
        .max(1);
    analytical_prefilter_keep_auto(n_after_merge, scenario.max_candidates, top_ref, workload)
}

pub fn optimize_scenario(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    match scenario.strategy {
        OptimizerStrategy::Exhaustive => optimize_scenario_exhaustive(scenario),
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |_, _, _| true, || true),
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
        &scenario.warm_start,
    );
    let n_tiered = candidates.len();
    let scout_sims = scenario
        .tiered_scout_sims
        .unwrap_or_else(|| tiered_scout_sims_for_workload(n_tiered))
        .max(1);
    let top_k = scenario
        .tiered_top_k
        .unwrap_or_else(|| tiered_top_k_for_workload(n_tiered))
        .max(1);
    let (pre_map, _) = tiered_preconfirmed_map(scenario, n_tiered, scout_sims, top_k, &candidates);
    let pre_ref = if pre_map.is_empty() {
        None
    } else {
        Some(&pre_map)
    };
    let budget_hints_storage = scenario
        .profile_id
        .and_then(|pid| crate::optimizer::budget_hints::load_for_profile(pid));
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
        pre_ref,
        !scenario.tiered_scout_uniform,
        scenario.tiered_confirm_budget_cap_mult,
        budget_hints_storage.as_ref(),
        |_| true,
    )
    .0
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
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |_, _, _| true, || true),
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
        &scenario.warm_start,
    );
    if let Some((scout_s, top_keep)) =
        scenario.exhaustive_scout_sims.zip(scenario.exhaustive_scout_top_keep)
    {
        let (pre_map, _) = exhaustive_two_phase_preconfirmed_map(
            scenario,
            candidates.len(),
            scout_s,
            top_keep,
            &candidates,
        );
        let pre_ref = if pre_map.is_empty() {
            None
        } else {
            Some(&pre_map)
        };
        if let Some((merged, _)) = run_exhaustive_scout_then_full_mc(
            shared_ex,
            &candidates,
            scout_s,
            scenario.simulation_count.max(1),
            top_keep,
            scenario.seed,
            scenario.chain_grind.clone(),
            pre_ref,
            scenario.tiered_confirm_budget_cap_mult,
            |_| true,
            || true,
        ) {
            return rank_results(merged);
        }
        return Vec::new();
    }
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
        &scenario.warm_start,
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
/// `eval_should_continue` is polled between deduplicated Monte Carlo chunks within each generation
/// (for async cancel); sync callers should pass `|| true`.
pub fn optimize_scenario_genetic<F, G>(
    scenario: &OptimizationScenario<'_>,
    on_progress: F,
    eval_should_continue: G,
) -> Vec<RankedCrewResult>
where
    F: FnMut(usize, usize, f32) -> bool,
    G: FnMut() -> bool,
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
            roster_profile_id: scenario.profile_id.map(String::from),
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
        cfg.roster_profile_id = scenario.profile_id.map(String::from);
        cfg
    };
    run_genetic_optimizer_ranked(
        scenario.ship,
        scenario.hostile,
        &config,
        scenario.seed,
        scenario.simulation_count.max(1),
        on_progress,
        eval_should_continue,
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
                tiered_scout_uniform: scenario.tiered_scout_uniform,
                tiered_confirm_budget_cap_mult: scenario.tiered_confirm_budget_cap_mult,
                exhaustive_scout_sims: scenario.exhaustive_scout_sims,
                exhaustive_scout_top_keep: scenario.exhaustive_scout_top_keep,
                analytical_prefilter_keep: scenario.analytical_prefilter_keep,
                below_decks_slots: scenario.below_decks_slots,
                constraints: scenario.constraints.clone(),
                support_buffs: scenario.support_buffs.clone(),
                chain_grind: scenario.chain_grind.clone(),
                defender_opponent: scenario.defender_opponent,
                warm_start: scenario.warm_start.clone(),
                optimize_cache_key: scenario.optimize_cache_key.clone(),
            };
            optimize_scenario_with_progress(&scenario_ex, on_progress)
        }
        OptimizerStrategy::Exhaustive => {
            let generator =
                CrewGenerator::with_strategy(candidate_strategy_from_scenario(scenario));
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
                &scenario.warm_start,
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
            let total_batches = ranges.len();
            let mut all_results: Vec<SimulationResult> = Vec::with_capacity(total);
            let sim_count = scenario.simulation_count.max(1);

            for (batch_index, (start, end)) in ranges.into_iter().enumerate() {
                info!(
                    phase = "monte_carlo",
                    strategy = "exhaustive",
                    seed = scenario.seed,
                    batch_index = (batch_index + 1) as u64,
                    batch_total = total_batches as u64,
                    batch_start = start as u64,
                    batch_end = end as u64,
                    batch_candidates = (end - start) as u64,
                    total_candidates = total as u64,
                    sims_per_candidate = sim_count as u64,
                    "optimize_sim_batch_started"
                );
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
                info!(
                    phase = "monte_carlo",
                    strategy = "exhaustive",
                    seed = scenario.seed,
                    batch_index = (batch_index + 1) as u64,
                    batch_total = total_batches as u64,
                    crews_done = end as u64,
                    total_candidates = total as u64,
                    "optimize_sim_batch_completed"
                );
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
        OptimizerStrategy::Genetic => optimize_scenario_genetic(
            scenario,
            |gen, max_gen, _| {
                on_progress(OptimizeProgressTick {
                    crews_done: gen as u32,
                    total_crews: max_gen.max(1) as u32,
                    phase: "genetic",
                    partial_top: None,
                })
            },
            || true,
        ),
    }
}

/// Like [optimize_scenario_with_progress] but uses [DataRegistry] for exhaustive path (no reload).
/// Progress callback returns true to continue, false to abort (e.g. user cancelled).
/// `eval_should_continue` is polled between genetic dedupe Monte Carlo chunks and at the start of
/// each exhaustive batch; use `|| true` when no cooperative cancel is needed.
pub fn optimize_scenario_with_progress_with_registry<F, G>(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
    mut eval_should_continue: G,
) -> OptimizeRunOutcome
where
    F: FnMut(OptimizeProgressTick) -> bool,
    G: FnMut() -> bool,
{
    match scenario.strategy {
        OptimizerStrategy::Tiered => {
            let generator =
                CrewGenerator::with_strategy(candidate_strategy_from_scenario(scenario));
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
                &scenario.warm_start,
            );
            let n_tiered = candidates.len();
            let scout_sims = scenario
                .tiered_scout_sims
                .unwrap_or_else(|| tiered_scout_sims_for_workload(n_tiered))
                .max(1);
            let top_k = scenario
                .tiered_top_k
                .unwrap_or_else(|| tiered_top_k_for_workload(n_tiered))
                .max(1);
            let (pre_map, hits) =
                tiered_preconfirmed_map(scenario, n_tiered, scout_sims, top_k, &candidates);
            let pre_ref = if pre_map.is_empty() {
                None
            } else {
                Some(&pre_map)
            };
            let scout_adaptive = !scenario.tiered_scout_uniform;
            let budget_hints_storage = scenario
                .profile_id
                .and_then(|pid| crate::optimizer::budget_hints::load_for_profile(pid));
            let (ranked, scout_budget) = run_tiered_with_registry_with_progress(
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
                pre_ref,
                scout_adaptive,
                scenario.tiered_confirm_budget_cap_mult,
                budget_hints_storage.as_ref(),
                &mut on_progress,
            );
            OptimizeRunOutcome {
                ranked,
                analytical_prefilter,
                tiered_resolved: Some((n_tiered, scout_sims, top_k)),
                tiered_scout_budget: Some(scout_budget),
                exhaustive_adaptive_budget: None,
                optimize_history_confirm_hits: hits,
            }
        }
        OptimizerStrategy::Exhaustive => {
            let generator =
                CrewGenerator::with_strategy(candidate_strategy_from_scenario(scenario));
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
                &scenario.warm_start,
            );
            let total = candidates.len();
            if total == 0 {
                return OptimizeRunOutcome {
                    ranked: Vec::new(),
                    analytical_prefilter,
                    tiered_resolved: None,
                    tiered_scout_budget: None,
                    exhaustive_adaptive_budget: None,
                    optimize_history_confirm_hits: 0,
                };
            }

            if let Some((scout_s, top_keep)) =
                scenario.exhaustive_scout_sims.zip(scenario.exhaustive_scout_top_keep)
            {
                let (pre_map, hits) = exhaustive_two_phase_preconfirmed_map(
                    scenario,
                    total,
                    scout_s,
                    top_keep,
                    &candidates,
                );
                let pre_ref = if pre_map.is_empty() {
                    None
                } else {
                    Some(&pre_map)
                };
                if let Some((merged, budget)) = run_exhaustive_scout_then_full_mc(
                    shared_ex,
                    &candidates,
                    scout_s,
                    scenario.simulation_count.max(1),
                    top_keep,
                    scenario.seed,
                    scenario.chain_grind.clone(),
                    pre_ref,
                    scenario.tiered_confirm_budget_cap_mult,
                    &mut on_progress,
                    &mut eval_should_continue,
                ) {
                    return OptimizeRunOutcome {
                        ranked: rank_results(merged),
                        analytical_prefilter,
                        tiered_resolved: None,
                        tiered_scout_budget: None,
                        exhaustive_adaptive_budget: Some(budget),
                        optimize_history_confirm_hits: hits,
                    };
                }
                return OptimizeRunOutcome {
                    ranked: Vec::new(),
                    analytical_prefilter,
                    tiered_resolved: None,
                    tiered_scout_budget: None,
                    exhaustive_adaptive_budget: None,
                    optimize_history_confirm_hits: 0,
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
                    tiered_resolved: None,
                    tiered_scout_budget: None,
                    exhaustive_adaptive_budget: None,
                    optimize_history_confirm_hits: 0,
                };
            }

            let num_batches = OPTIMIZE_PROGRESS_BATCH_COUNT.min(total);
            let ranges = batch_ranges(total, num_batches);
            let total_batches = ranges.len();
            let mut all_results: Vec<SimulationResult> = Vec::with_capacity(total);
            let sim_count = scenario.simulation_count.max(1);

            for (batch_index, (start, end)) in ranges.into_iter().enumerate() {
                if !eval_should_continue() {
                    break;
                }
                info!(
                    phase = "monte_carlo",
                    strategy = "exhaustive",
                    seed = scenario.seed,
                    batch_index = (batch_index + 1) as u64,
                    batch_total = total_batches as u64,
                    batch_start = start as u64,
                    batch_end = end as u64,
                    batch_candidates = (end - start) as u64,
                    total_candidates = total as u64,
                    sims_per_candidate = sim_count as u64,
                    "optimize_sim_batch_started"
                );
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
                info!(
                    phase = "monte_carlo",
                    strategy = "exhaustive",
                    seed = scenario.seed,
                    batch_index = (batch_index + 1) as u64,
                    batch_total = total_batches as u64,
                    crews_done = end as u64,
                    total_candidates = total as u64,
                    "optimize_sim_batch_completed"
                );
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
                tiered_resolved: None,
                tiered_scout_budget: None,
                exhaustive_adaptive_budget: None,
                optimize_history_confirm_hits: 0,
            }
        }
        OptimizerStrategy::Genetic => OptimizeRunOutcome {
            ranked: optimize_scenario_genetic(
                scenario,
                |gen, max_gen, _| {
                    on_progress(OptimizeProgressTick {
                        crews_done: gen as u32,
                        total_crews: max_gen.max(1) as u32,
                        phase: "genetic",
                        partial_top: None,
                    })
                },
                || eval_should_continue(),
            ),
            analytical_prefilter: None,
            tiered_resolved: None,
            tiered_scout_budget: None,
            exhaustive_adaptive_budget: None,
            optimize_history_confirm_hits: 0,
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
        tiered_scout_uniform: false,
        tiered_confirm_budget_cap_mult: None,
        exhaustive_scout_sims: None,
        exhaustive_scout_top_keep: None,
        analytical_prefilter_keep: None,
        below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
        constraints: None,
        support_buffs: Vec::new(),
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        warm_start: Vec::new(),
        optimize_cache_key: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        analytical_prefilter_keep_auto, count_effective_optimize_candidates,
        optimize_scenario_with_progress_with_registry, AnalyticalPrefilterWorkload,
        CandidateStrategy, OptimizationScenario, OptimizerStrategy,
    };
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::constraints::CrewSearchConstraints;
    use crate::optimizer::crew_generator::{
        CrewCandidate, DEFAULT_BELOW_DECKS_SLOTS, NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS,
    };
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
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
            optimize_cache_key: None,
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
        assert_eq!(
            analytical_prefilter_keep_auto(
                200,
                None,
                20,
                AnalyticalPrefilterWorkload::Exhaustive {
                    full_sims_per_crew: 5000
                }
            ),
            None
        );
    }

    #[test]
    fn analytical_prefilter_keep_auto_max_candidates_tightens_vs_unbounded() {
        let wl = AnalyticalPrefilterWorkload::Exhaustive {
            full_sims_per_crew: 5000,
        };
        let unbounded = analytical_prefilter_keep_auto(20_000, None, 40, wl).expect("keep");
        let capped = analytical_prefilter_keep_auto(20_000, Some(150), 40, wl).expect("keep");
        assert!(capped < unbounded, "capped={capped} unbounded={unbounded}");
    }

    #[test]
    fn analytical_prefilter_keep_auto_tiered_heavy_scout_tightens_keep() {
        let light = analytical_prefilter_keep_auto(
            20_000,
            None,
            40,
            AnalyticalPrefilterWorkload::Tiered {
                scout_sims_per_crew: 500,
            },
        )
        .expect("keep");
        let heavy = analytical_prefilter_keep_auto(
            20_000,
            None,
            40,
            AnalyticalPrefilterWorkload::Tiered {
                scout_sims_per_crew: 2000,
            },
        )
        .expect("keep");
        assert!(
            heavy < light,
            "heavy_scout_keep={heavy} light_scout_keep={light}"
        );
    }

    #[test]
    fn analytical_prefilter_keep_auto_exhaustive_high_sims_tightens_keep() {
        let baseline = analytical_prefilter_keep_auto(
            20_000,
            None,
            40,
            AnalyticalPrefilterWorkload::Exhaustive {
                full_sims_per_crew: 5000,
            },
        )
        .expect("keep");
        let expensive = analytical_prefilter_keep_auto(
            20_000,
            None,
            40,
            AnalyticalPrefilterWorkload::Exhaustive {
                full_sims_per_crew: 10_000,
            },
        )
        .expect("keep");
        assert!(
            expensive < baseline,
            "expensive_sims_keep={expensive} baseline_keep={baseline}"
        );
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
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: Some(4),
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
            optimize_cache_key: None,
        };
        let strat = super::candidate_strategy_from_scenario(&scenario);
        let n = count_effective_optimize_candidates(
            &registry,
            scenario.ship,
            scenario.hostile,
            scenario.seed,
            scenario.profile_id,
            strat,
            &scenario.warm_start,
        );
        let out =
            optimize_scenario_with_progress_with_registry(&registry, &scenario, |_| true, || true);
        assert!(
            out.ranked.len() <= n.min(4),
            "ranked crews {} exceed min(n, keep) with n={n}",
            out.ranked.len()
        );
        if n > 4 {
            let (g, k) = out
                .analytical_prefilter
                .expect("truncation should be recorded when n > keep");
            assert!(g > k, "generated {g} should exceed kept {k}");
            assert_eq!(k, 4);
        } else {
            assert!(
                out.analytical_prefilter.is_none(),
                "no truncation when candidate count n={n} does not exceed keep=4"
            );
        }
    }

    #[test]
    fn exhaustive_two_phase_scout_then_confirm_budgets() {
        let registry = DataRegistry::load().expect("data registry");
        let scenario = OptimizationScenario {
            ship: "saladin",
            hostile: "2918121098",
            ship_tier: None,
            ship_level: None,
            simulation_count: 40,
            seed: 11,
            max_candidates: Some(80),
            strategy: OptimizerStrategy::Exhaustive,
            only_below_decks_with_ability: false,
            seed_population: Vec::new(),
            profile_id: Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            tiered_scout_sims: None,
            tiered_top_k: None,
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            exhaustive_scout_sims: Some(12),
            exhaustive_scout_top_keep: Some(4),
            analytical_prefilter_keep: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
            optimize_cache_key: None,
        };
        let out =
            optimize_scenario_with_progress_with_registry(&registry, &scenario, |_| true, || true);
        let ranked_n = out.ranked.len();
        assert!(
            ranked_n > 0 && ranked_n <= 80,
            "unexpected ranked crew count ranked_n={ranked_n}"
        );
        let bud = out
            .exhaustive_adaptive_budget
            .expect("exhaustive two-phase should populate budget stats");
        assert!(
            bud.scout_trials_final <= (ranked_n as u64) * 12,
            "scout trials should not exceed n×scout cap: {} vs n={ranked_n}",
            bud.scout_trials_final
        );
        assert!(
            bud.confirm_trials_total >= 4 && bud.confirm_trials_total <= 4 * 40,
            "adaptive confirm totals should stay within [4, 4×sims], got {}",
            bud.confirm_trials_total
        );
        assert!(bud.confirm_sims_alloc_min >= 1);
        assert!(bud.confirm_sims_alloc_max <= 40);
        assert!(bud.confirm_sims_alloc_min <= bud.confirm_sims_alloc_max);
    }

    #[test]
    fn tiered_adaptive_scout_trials_not_above_uniform_small_pool() {
        let registry = DataRegistry::load().expect("data registry");
        let uniform = OptimizationScenario {
            ship: "defiant",
            hostile: "romulan",
            ship_tier: None,
            ship_level: None,
            simulation_count: 800,
            seed: 19,
            max_candidates: Some(48),
            strategy: OptimizerStrategy::Tiered,
            only_below_decks_with_ability: false,
            seed_population: Vec::new(),
            profile_id: Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            tiered_scout_sims: Some(320),
            tiered_top_k: Some(10),
            tiered_scout_uniform: true,
            tiered_confirm_budget_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            warm_start: Vec::new(),
            optimize_cache_key: None,
        };
        let mut adaptive = uniform.clone();
        adaptive.tiered_scout_uniform = false;
        let out_u = optimize_scenario_with_progress_with_registry(
            &registry,
            &uniform,
            |_| true,
            || true,
        );
        let out_a = optimize_scenario_with_progress_with_registry(
            &registry,
            &adaptive,
            |_| true,
            || true,
        );
        let trials_u = out_u
            .tiered_scout_budget
            .expect("tiered budget")
            .scout_trials_final;
        let trials_a = out_a
            .tiered_scout_budget
            .expect("tiered budget")
            .scout_trials_final;
        assert!(
            trials_a <= trials_u,
            "adaptive scout trials {trials_a} should not exceed uniform {trials_u}"
        );
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
        let atk_low = scenario_to_combat_input_from_shared(&shared_low, &candidate, 1)
            .attacker
            .attack;
        let atk_high = scenario_to_combat_input_from_shared(&shared_high, &candidate, 1)
            .attacker
            .attack;
        assert!(
            (atk_high - atk_low).abs() > 1.0,
            "tier 1 vs 5 amalgam attacker attack should differ materially: {atk_low} vs {atk_high}"
        );
    }
}
