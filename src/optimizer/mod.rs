pub mod analytical;
pub mod budget_hints;
pub mod chain;
pub mod constraints;
pub mod crew_generator;
pub(crate) mod exhaustive_adaptive;
pub mod genetic;
pub mod learning_signals;
pub mod linear_eval;
pub mod matchup_priors;
pub mod method_bench;
pub mod monte_carlo;
pub mod officer_learning;
pub mod pareto;
pub mod random_stratified;
pub mod ranking;
pub mod refinement;
pub mod sensitivity;
pub mod sensitivity_morris;
pub mod sensitivity_sobol;
pub mod tiered;

pub use chain::{ChainGrindParams, ChainSecondaryObjective, ChainSimulationSummary};

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use tracing::info;

use crate::combat::EnemyType;
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::BelowDecksPoolMode;
use crate::data::officer::{load_canonical_officers, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::officer_eligibility::{
    is_eligible_for_optimization, load_eligibility_matrix, EligibilityMatrix,
    DEFAULT_ELIGIBILITY_MATRIX_PATH,
};
use crate::optimizer::constraints::{
    filter_candidates, normalize_officer_name, CrewSearchConstraints,
};
use crate::optimizer::crew_generator::{
    build_officer_pools_from_registry, CandidateStrategy, CrewCandidate, CrewGenerator,
    BRIDGE_SLOTS, DEFAULT_BELOW_DECKS_SLOTS,
};
use crate::optimizer::exhaustive_adaptive::run_exhaustive_scout_then_full_mc;
use crate::optimizer::genetic::{run_genetic_optimizer_ranked, GeneticConfig};
use crate::optimizer::matchup_priors::analytical_prefilter_rank_score;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, build_shared_scenario_data_standalone,
    scenario_to_combat_input_from_shared, DefenderOpponent, PlayerDefenderOfficerCrewOverride,
    SharedScenarioData,
};
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, full_mc_early_stop_enabled,
    run_monte_carlo_confirm_topk_with_shared, run_monte_carlo_with_shared, SimulationResult,
};
use crate::optimizer::random_stratified::{
    sample_stratified_random_crews, StratifiedSampleParams, DEFAULT_RANDOM_STRATIFIED_CANDIDATES,
};
use crate::optimizer::ranking::{rank_results, sort_ranked_rows, RankedCrewResult};
use crate::optimizer::refinement::{refine_finalists, RefinementContext, RefinementProvenance};
use crate::optimizer::tiered::{
    run_tiered_with_registry_with_progress, tiered_scout_sims_for_workload,
    tiered_top_k_for_workload, TieredScoutBudgetStats, DEFAULT_SCOUT_SIMS, DEFAULT_TOP_K,
};
use crate::perf_log;

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

pub(crate) fn scenario_support_buff_request<'a>(
    scenario: &'a OptimizationScenario<'a>,
) -> crate::data::support_buffs::SupportBuffScenarioRequest<'a> {
    crate::data::support_buffs::SupportBuffScenarioRequest::from_optional_slices(
        if scenario.support_buffs.is_empty() {
            None
        } else {
            Some(scenario.support_buffs.as_slice())
        },
        scenario
            .defender_support_buffs
            .as_deref()
            .filter(|s| !s.is_empty()),
        scenario
            .defender_alliance_debuffs
            .as_deref()
            .filter(|s| !s.is_empty()),
    )
}
use crate::parallel::batch_ranges;

/// Number of progress-reporting batches for optimize-with-progress (UI jobs).
const OPTIMIZE_PROGRESS_BATCH_COUNT: usize = 40;

fn apply_crew_constraints(
    mut candidates: Vec<CrewCandidate>,
    scenario: &OptimizationScenario<'_>,
) -> Vec<CrewCandidate> {
    if let Ok(officers) = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH) {
        let officer_by_name: HashMap<String, _> = officers
            .iter()
            .map(|officer| (normalize_officer_name(&officer.name), officer))
            .collect();
        let pvp_mode = scenario.pvp.is_some();
        let enemy = scenario.enemy_type;
        // Eligibility matrix (per-ability scenario verdicts). Absent → `is_eligible_for_optimization`
        // falls back to the legacy below-decks heuristic, preserving prior behavior. Loaded once
        // per call, mirroring the canonical-officer load above.
        let matrix = load_optimization_eligibility_matrix();
        let matrix_ref = matrix.as_ref();
        let seat_eligible = |name: &str, slot: &str| -> bool {
            officer_by_name
                .get(&normalize_officer_name(name))
                .is_none_or(|officer| {
                    is_eligible_for_optimization(officer, slot, enemy, matrix_ref, pvp_mode)
                })
        };
        candidates.retain(|candidate| {
            seat_eligible(&candidate.captain, "captain")
                && candidate.bridge.iter().all(|n| seat_eligible(n, "officer"))
                && candidate
                    .below_decks
                    .iter()
                    .all(|n| seat_eligible(n, "below_decks"))
        });
    }

    match &scenario.constraints {
        Some(c) => filter_candidates(candidates, c),
        None => candidates,
    }
}

/// Load the eligibility matrix from the runtime path (then the compile-time default). Returns
/// `None` if the file is absent, in which case callers fall back to the legacy heuristics.
fn load_optimization_eligibility_matrix() -> Option<EligibilityMatrix> {
    load_eligibility_matrix(crate::runtime_paths::resolve(
        DEFAULT_ELIGIBILITY_MATRIX_PATH,
    ))
    .or_else(|| load_eligibility_matrix(std::path::Path::new(DEFAULT_ELIGIBILITY_MATRIX_PATH)))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CandidateLegalitySummary {
    pub dropped_wrong_shape: usize,
    pub dropped_duplicates: usize,
    pub dropped_seat_incompatible: usize,
}

pub fn enforce_candidate_legality_with_registry(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    candidates: Vec<CrewCandidate>,
) -> (Vec<CrewCandidate>, CandidateLegalitySummary) {
    enforce_candidate_legality_inner(registry, profile_id, below_decks_slots, None, candidates)
}

/// Enforce ordinary crew legality plus the hard scenario-specific below-decks optimizer rules.
pub fn enforce_candidate_optimization_eligibility_with_registry(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    enemy_type: EnemyType,
    candidates: Vec<CrewCandidate>,
) -> (Vec<CrewCandidate>, CandidateLegalitySummary) {
    enforce_candidate_legality_inner(
        registry,
        profile_id,
        below_decks_slots,
        Some(enemy_type),
        candidates,
    )
}

fn enforce_candidate_legality_inner(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    enemy_type: Option<EnemyType>,
    candidates: Vec<CrewCandidate>,
) -> (Vec<CrewCandidate>, CandidateLegalitySummary) {
    let mut summary = CandidateLegalitySummary::default();
    // The pool builder prunes ineligible officers across all seats for `enemy_type` (when set), so
    // the seat-membership check below also enforces scenario eligibility. `None` = legality only.
    let Some(pools) = build_officer_pools_from_registry(
        registry,
        BelowDecksPoolMode::Relaxed,
        enemy_type,
        profile_id,
        below_decks_slots,
        None,
    ) else {
        summary.dropped_wrong_shape = candidates.len();
        return (Vec::new(), summary);
    };

    let captain_pool: HashSet<String> = pools
        .captains
        .into_iter()
        .map(|name| normalize_officer_name(&name))
        .collect();
    let bridge_pool: HashSet<String> = pools
        .bridge
        .into_iter()
        .map(|name| normalize_officer_name(&name))
        .collect();
    let below_pool: HashSet<String> = pools
        .below_decks
        .into_iter()
        .map(|name| normalize_officer_name(&name))
        .collect();

    let mut accepted = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if candidate.bridge.len() != BRIDGE_SLOTS
            || candidate.below_decks.len() != below_decks_slots
        {
            summary.dropped_wrong_shape += 1;
            continue;
        }

        let captain_key = normalize_officer_name(&candidate.captain);
        let bridge_keys: Vec<String> = candidate
            .bridge
            .iter()
            .map(|name| normalize_officer_name(name))
            .collect();
        let below_keys: Vec<String> = candidate
            .below_decks
            .iter()
            .map(|name| normalize_officer_name(name))
            .collect();

        if captain_key.is_empty() {
            summary.dropped_wrong_shape += 1;
            continue;
        }

        // Empty string = unset slot (partial crew); only check duplicates among filled slots.
        let mut seen = HashSet::new();
        seen.insert(captain_key.clone());
        let mut unique = true;
        for key in bridge_keys.iter().chain(below_keys.iter()) {
            if key.is_empty() {
                continue;
            }
            if !seen.insert(key.clone()) {
                unique = false;
                break;
            }
        }
        if !unique {
            summary.dropped_duplicates += 1;
            continue;
        }

        // Empty slots are valid (no officer assigned); only validate filled slots.
        let seat_legal = captain_pool.contains(&captain_key)
            && bridge_pool.contains(&captain_key)
            && bridge_keys
                .iter()
                .filter(|k| !k.is_empty())
                .all(|k| bridge_pool.contains(k))
            && below_keys
                .iter()
                .filter(|k| !k.is_empty())
                .all(|k| below_pool.contains(k));
        if !seat_legal {
            summary.dropped_seat_incompatible += 1;
            continue;
        }

        accepted.push(candidate);
    }

    (accepted, summary)
}

#[allow(clippy::too_many_arguments)]
fn analytical_prefilter_unless_chain(
    shared: &SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    keep: Option<usize>,
    chain_grind: &Option<ChainGrindParams>,
    warm_start: &[CrewCandidate],
    prior_reference_crews: &[CrewCandidate],
    enable_learned_pair_prior: bool,
) -> (Vec<CrewCandidate>, Option<(usize, usize)>) {
    if chain_grind.is_some() {
        (candidates, None)
    } else {
        sort_and_analytical_prefilter(
            shared,
            candidates,
            seed,
            keep,
            warm_start,
            prior_reference_crews,
            enable_learned_pair_prior,
        )
    }
}

#[derive(Debug)]
struct AnalyticalScoredCandidate {
    original_index: usize,
    score: f64,
    candidate: CrewCandidate,
}

#[inline]
fn compare_analytical_scored_candidates(
    a: &AnalyticalScoredCandidate,
    b: &AnalyticalScoredCandidate,
) -> std::cmp::Ordering {
    b.score
        .total_cmp(&a.score)
        .then_with(|| a.original_index.cmp(&b.original_index))
}

/// Score candidates once, then retain them in descending analytical-score order.
///
/// When `keep` is smaller than the input, partition around the top-K boundary and sort only the
/// retained prefix. This avoids both a full O(N log N) sort and, more importantly, rebuilding a
/// full [`CombatSimulationInput`] twice for every comparator invocation.
fn score_and_select_candidates_by_analytical_expected_damage(
    shared: &SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    keep: Option<usize>,
    warm_start: &[CrewCandidate],
    prior_reference_crews: &[CrewCandidate],
    enable_learned_pair_prior: bool,
) -> Vec<CrewCandidate> {
    let rank_refs: Cow<'_, [CrewCandidate]> = if prior_reference_crews.is_empty() {
        Cow::Borrowed(warm_start)
    } else {
        let mut m = Vec::with_capacity(warm_start.len() + prior_reference_crews.len());
        m.extend_from_slice(warm_start);
        m.extend_from_slice(prior_reference_crews);
        Cow::Owned(m)
    };
    let refs = rank_refs.as_ref();
    let score_started = perf_log::perf_start();
    let mut scored: Vec<AnalyticalScoredCandidate> = candidates
        .into_iter()
        .enumerate()
        .map(|(original_index, candidate)| {
            let input = scenario_to_combat_input_from_shared(shared, &candidate, seed);
            let score = analytical_prefilter_rank_score(
                shared,
                &input,
                &candidate,
                refs,
                enable_learned_pair_prior,
            );
            AnalyticalScoredCandidate {
                original_index,
                score,
                candidate,
            }
        })
        .collect();
    perf_log::log_duration("analytical_prefilter.score_once", score_started);

    let select_started = perf_log::perf_start();
    if let Some(k) = keep.filter(|&k| k > 0 && k < scored.len()) {
        scored.select_nth_unstable_by(k, compare_analytical_scored_candidates);
        scored.truncate(k);
    }
    scored.sort_unstable_by(compare_analytical_scored_candidates);
    perf_log::log_duration("analytical_prefilter.select_topk", select_started);
    scored.into_iter().map(|row| row.candidate).collect()
}

/// After analytical ranking, optionally keep only the top `keep` crews (approximate proxy; full MC still determines win rate).
/// Returns `(candidates, Some((generated, kept)))` when truncation happened.
pub(crate) fn sort_and_analytical_prefilter(
    shared: &SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    keep: Option<usize>,
    warm_start: &[CrewCandidate],
    prior_reference_crews: &[CrewCandidate],
    enable_learned_pair_prior: bool,
) -> (Vec<CrewCandidate>, Option<(usize, usize)>) {
    let generated = candidates.len();
    let requested_keep = keep.filter(|n| *n > 0);
    let sorted = score_and_select_candidates_by_analytical_expected_damage(
        shared,
        candidates,
        seed,
        requested_keep,
        warm_start,
        prior_reference_crews,
        enable_learned_pair_prior,
    );
    let Some(k) = requested_keep else {
        return (sorted, None);
    };
    if generated > k {
        (sorted, Some((generated, k)))
    } else {
        (sorted, None)
    }
}

/// Candidate-count funnel for a registry-backed optimize run.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptimizeCandidateFunnel {
    /// Raw generated candidates before warm-start crews are prepended.
    pub generated_candidates: Option<usize>,
    /// Warm-start crews supplied to this optimizer scenario.
    pub warm_start_candidates: usize,
    /// Candidate count after warm-start prepending and stable-hash dedupe.
    pub after_warm_start_dedupe: Option<usize>,
    /// Candidate count after explicit optimize constraints are applied.
    pub after_constraints: Option<usize>,
    /// Analytical prefilter input count, when that filter truncated the set.
    pub analytical_prefilter_from: Option<usize>,
    /// Analytical prefilter output count, when that filter truncated the set.
    pub analytical_prefilter_kept: Option<usize>,
    /// Candidate count entering the scout / cheap-evaluation phase.
    pub scout_candidates: Option<usize>,
    /// Candidate count entering expensive confirmation or final ranking output.
    pub confirmed_candidates: Option<usize>,
    /// Stratified-random crews in the scout set (`random_stratified` lane or the
    /// tiered `tiered_random_exploration_pct` slice). `None` when the lane did not run.
    pub random_exploration_candidates: Option<usize>,
}

impl OptimizeCandidateFunnel {
    fn with_counts(
        generated_candidates: usize,
        warm_start_candidates: usize,
        after_warm_start_dedupe: usize,
        after_constraints: usize,
        analytical_prefilter: Option<(usize, usize)>,
        scout_candidates: usize,
        confirmed_candidates: usize,
    ) -> Self {
        Self {
            generated_candidates: Some(generated_candidates),
            warm_start_candidates,
            after_warm_start_dedupe: Some(after_warm_start_dedupe),
            after_constraints: Some(after_constraints),
            analytical_prefilter_from: analytical_prefilter.map(|(n, _)| n),
            analytical_prefilter_kept: analytical_prefilter.map(|(_, n)| n),
            scout_candidates: Some(scout_candidates),
            confirmed_candidates: Some(confirmed_candidates),
            random_exploration_candidates: None,
        }
    }
}

/// Result of [`optimize_scenario_with_progress_with_registry`] including optional analytical pre-filter stats.
#[derive(Debug, Clone)]
pub struct OptimizeRunOutcome {
    pub ranked: Vec<RankedCrewResult>,
    pub candidate_funnel: OptimizeCandidateFunnel,
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
    /// Stable hashes of stratified-random crews injected into the scout set
    /// (standalone `random_stratified` lane or the tiered exploration slice).
    /// Used downstream for per-row method provenance. Empty when the lane did not run.
    pub random_exploration_hashes: HashSet<u64>,
    /// Local-refinement provenance for rows added by an opt-in post-search hill-climb
    /// (tiered and genetic strategies; see [`OptimizationScenario::local_refinement`]), keyed by
    /// canonical crew hash. Empty when refinement did not run.
    pub refinement_provenance: HashMap<u64, RefinementProvenance>,
    /// Budget/effect accounting for the refinement pass. `Some` whenever the pass ran — including
    /// when it ran and accepted nothing, which is what separates "the finalists were already local
    /// optima at this depth" from "refinement never ran". `None` when it was off or skipped.
    pub refinement_stats: Option<crate::optimizer::refinement::LocalRefinementStats>,
}

/// Progress update for async optimize jobs (SSE / polling): phase label, counts, optional partial top crews.
#[derive(Debug, Clone)]
pub struct OptimizeProgressTick {
    pub crews_done: u32,
    pub total_crews: u32,
    /// Stable labels: `heuristics`, `monte_carlo`, `genetic`, `tiered_scout`, `tiered_scout_refine`, `tiered_confirm`, `exhaustive_scout`, `exhaustive_confirm`, `linear_eval`.
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
    /// Closed-form expected hull damage only; no Monte Carlo.
    LinearEval,
    /// Stratified random baseline (roadmap §1.1): sample legal crews stratified by
    /// captain faction/rarity and below-decks family, then scout → confirm.
    /// Benchmark control; ignores warm-start and skips the analytical prefilter.
    RandomStratified,
}

impl OptimizerStrategy {
    /// Stable label for structured logs. Matches the API's strategy strings so a log line and a
    /// response's `effective_strategy` can be correlated without a translation table.
    pub fn log_label(self) -> &'static str {
        match self {
            OptimizerStrategy::Exhaustive => "exhaustive",
            OptimizerStrategy::Genetic => "genetic",
            OptimizerStrategy::Tiered => "tiered",
            OptimizerStrategy::LinearEval => "linear_eval",
            OptimizerStrategy::RandomStratified => "random_stratified",
        }
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
    /// Below-decks pool sizing strategy. See [`BelowDecksPoolMode`].
    pub below_decks_pool_mode: BelowDecksPoolMode,
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
    ///
    /// This is the **effective** cap for *this* run, which the server may have auto-derived from
    /// learning signals when the caller did not ask for one. Use
    /// [`Self::optimize_history_confirm_cap_mult`] — not this — as the `optimize_history` cache key.
    pub tiered_confirm_budget_cap_mult: Option<f64>,
    /// Confirm cap the **caller asked for**, used only as the `optimize_history` compatibility key
    /// (`OptimizeHistoryEntry::tiered_confirm_cap_mult`, which stores the same value).
    ///
    /// Kept separate from [`Self::tiered_confirm_budget_cap_mult`] because the auto-tuner derives a
    /// cap *from* the stored entry: folding that derived value into cache identity made every entry
    /// reject itself on the next run, so the cache never hit on the default path. Cache identity has
    /// to depend on the request, not on something computed from the cached entry.
    ///
    /// Consequence, on purpose: rows reused from an entry written under a different auto-derived cap
    /// carry the confirmation depth they were measured at, which can differ from this run's fresh
    /// rows. Tiered results are already depth-heterogeneous (scout-only vs confirmed rows), and
    /// `recommendation_reason` discloses it (roadmap §1.2), so reuse stays honest — whereas a cache
    /// that never hits is simply wasted.
    pub optimize_history_confirm_cap_mult: Option<f64>,
    /// Tiered only: when true, use priority-queue-based scout scheduling instead of the default
    /// coarse→refine adaptive scout. Promising crews get more trials sooner; hopeless crews are
    /// abandoned early. Uses `tiered_pq_minimal_scout` (default 100), `tiered_pq_selection_mult`
    /// (default 4× top-K), and `tiered_pq_abandon_margin` (default 0.05).
    pub tiered_scout_priority_queue: bool,
    /// Tiered priority-queue only: trials for the quick initial pass on all candidates (min 64, max scout cap).
    /// When `None`, defaults to 100.
    pub tiered_pq_minimal_scout: Option<usize>,
    /// Tiered priority-queue only: keep top `K * mult` survivors (min 2×, max 16×). When `None`, defaults to 4.
    pub tiered_pq_selection_mult: Option<usize>,
    /// Tiered priority-queue only: abandon crews whose Wilson upper bound is below
    /// `(K-th lower - margin)`. When `None`, defaults to 0.05.
    pub tiered_pq_abandon_margin: Option<f64>,
    /// Tiered only (roadmap §1.1): replace this fraction (0, 0.5] of the scout candidate
    /// list — after warm-start, constraints, and analytical prefilter — with stratified-random
    /// crews that bypass the analytical proxy. Budget-neutral: the candidate count is unchanged;
    /// the analytically weakest tail is swapped out. Injected crews are provenance-labeled
    /// `random_stratified` when they reach the results. `None`/0 = off.
    pub tiered_random_exploration_pct: Option<f64>,
    /// Exhaustive only: when **both** this and [`Self::exhaustive_scout_top_keep`] are `Some`, run scout Monte Carlo at this many trials per crew on the full candidate list, then run full [`Self::simulation_count`] only on the top `exhaustive_scout_top_keep` crews by scout rank (others keep scout statistics).
    pub exhaustive_scout_sims: Option<usize>,
    /// Exhaustive only: paired with [`Self::exhaustive_scout_sims`] (see there).
    pub exhaustive_scout_top_keep: Option<usize>,
    /// When set, keep only this many crews after analytical expected-hull-damage ranking before Monte Carlo. Genetic ignores this.
    pub analytical_prefilter_keep: Option<usize>,
    /// When set, drop candidates whose `expected_damage()` is less than `prune_analytical_hull_fraction × defender.hull_health`.
    /// These crews deal negligible damage and cannot kill the hostile in any realistic number of rounds.
    /// Applied just before Monte Carlo simulation, after the analytical ranking/truncation step.
    /// A conservative value like 0.05 drops only truly hopeless crews while avoiding false negatives.
    pub prune_analytical_hull_fraction: Option<f64>,
    /// When set, drop candidates whose fraction of statically-failing ability conditions exceeds this
    /// (e.g. 0.95 drops crews where ≥95% of conditional abilities are gated on a mismatched defender faction,
    /// ship type, or engagement).  Conservative value like 0.95 only eliminates crews with near-total mismatch
    /// (Borg officers vs non-Borg hostile, etc.).  Applied alongside `prune_analytical_hull_fraction`.
    pub prune_static_gate_max_fraction: Option<f64>,
    /// Below-decks slot count for candidate generation (resolved from API / tier defaults upstream).
    pub below_decks_slots: usize,
    /// Optional filters on candidate crews (must-include, exclude, groups, seating).
    pub constraints: Option<CrewSearchConstraints>,
    /// Support buff ids (lower decks) applied when building scenario profile; empty = none.
    pub support_buffs: Vec<String>,
    /// PvP: defender alliance support buff ids (`defender_support_buffs` API field).
    pub defender_support_buffs: Option<Vec<String>>,
    /// PvP: alliance debuffs on the attacker (`defender_alliance_debuffs` API field).
    pub defender_alliance_debuffs: Option<Vec<String>>,
    /// Sequential chain grind (HHP carry-over, full SHP each link). When set, analytical prefilter is skipped.
    pub chain_grind: Option<ChainGrindParams>,
    /// Defender is NPC hostile vs player ship for canonical opponent-category conditions.
    pub defender_opponent: DefenderOpponent,
    /// Optional LCARS defender crew (API `defender_crew`) merged into shared scenario for registry tiered/exhaustive paths.
    pub player_defender_officer_crew: Option<PlayerDefenderOfficerCrewOverride>,
    /// Ship-vs-ship PvP: fixed defender ship + opponent profile (optimize searches attacker crews only).
    pub pvp: Option<crate::optimizer::monte_carlo::scenario::PvpScenarioParams>,
    /// Resolved combat scenario (explicit `enemy_type` request field, else inferred from the
    /// target). Drives the eligibility-matrix hard filter in [`apply_crew_constraints`].
    pub enemy_type: crate::combat::EnemyType,
    /// Optional crews prepended before generated candidates (deduped by stable hash); e.g. warm-start from UI.
    pub warm_start: Vec<CrewCandidate>,
    /// Crews used only for matchup priors in analytical ranking (not prepended to the candidate list).
    /// Typically populated from [`crate::data::optimize_history`] when `optimize_cache_key` matches.
    pub prior_reference_crews: Vec<CrewCandidate>,
    /// Opaque client fingerprint for [`crate::data::optimize_history`] when `profile_id` is set.
    pub optimize_cache_key: Option<String>,
    /// Server-computed reuse fingerprint ([`crate::data::optimize_fingerprint`]) covering the engine,
    /// catalogs, profile, and resolved matchup. Required to reuse **stored metrics** from
    /// `optimize_history`: `None` refuses reuse, so callers that never persist history (bench
    /// binaries, library users) fail closed without extra wiring.
    pub reuse_fingerprint: Option<String>,
    /// Analytical prefilter only: include learned pair co-occurrence prior from warm-start/history refs.
    pub enable_learned_pair_prior: bool,
    /// Optional per-officer performance scores loaded from optimize history.
    /// When set, candidate generation uses epsilon-greedy weighted below-decks
    /// officer sampling instead of stride-based sampling (closes the learning loop).
    pub learned_officer_scores:
        Option<crate::optimizer::officer_learning::OfficerPerformanceScores>,
    /// Opt-in local-refinement hill-climb for the tiered and genetic strategies: runs after the
    /// main search finishes, hill-climbing around its top finalists (single-slot swaps, captain
    /// swaps, and destroy-repair neighborhoods) to spend a small marginal sim budget looking
    /// for nearby improvements the main search happened not to sample. `None` = off (default);
    /// ignored for every other strategy.
    pub local_refinement: Option<crate::optimizer::refinement::LocalRefinementParams>,
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
            below_decks_pool_mode: BelowDecksPoolMode::default(),
            seed_population: Vec::new(),
            profile_id: None,
            tiered_scout_sims: None,
            tiered_top_k: None,
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            optimize_history_confirm_cap_mult: None,
            tiered_scout_priority_queue: true,
            tiered_pq_minimal_scout: None,
            tiered_pq_selection_mult: None,
            tiered_pq_abandon_margin: None,
            tiered_random_exploration_pct: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: None,
            prune_analytical_hull_fraction: None,
            prune_static_gate_max_fraction: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            defender_support_buffs: None,
            defender_alliance_debuffs: None,
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            player_defender_officer_crew: None,
            pvp: None,
            enemy_type: crate::combat::EnemyType::RedMovingSpace,
            warm_start: Vec::new(),
            prior_reference_crews: Vec::new(),
            optimize_cache_key: None,
            reuse_fingerprint: None,
            enable_learned_pair_prior: true,
            learned_officer_scores: None,
            local_refinement: None,
        }
    }
}

fn tiered_scout_allocator_id(scenario: &OptimizationScenario<'_>) -> u8 {
    if scenario.tiered_scout_priority_queue {
        2
    } else if scenario.tiered_scout_uniform {
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
                crate::data::optimize_history::TIERED_BUDGET_POLICY_V3,
                // The requested cap, not the effective one: the auto-tuner derives its cap from this
                // very entry, so keying on the derived value made entries reject themselves.
                scenario.optimize_history_confirm_cap_mult.map(|x| x as f32),
                scenario.reuse_fingerprint.as_deref(),
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
                // The requested cap, not the effective one: the auto-tuner derives its cap from this
                // very entry, so keying on the derived value made entries reject themselves.
                scenario.optimize_history_confirm_cap_mult.map(|x| x as f32),
                scenario.reuse_fingerprint.as_deref(),
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
        below_decks_pool_mode: scenario.below_decks_pool_mode,
        pvp_mode: scenario.pvp.is_some(),
        enemy_type: scenario.enemy_type,
        below_decks_slots: scenario.below_decks_slots,
        constraints: scenario.constraints.clone(),
        roster_profile_id: scenario.profile_id.map(String::from),
        learned_officer_scores: scenario.learned_officer_scores.clone(),
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

/// Sample the standalone `random_stratified` candidate set for a scenario.
/// Warm-start crews are deliberately ignored (the lane is a benchmark control);
/// constraints and eligibility filters still apply via the shared pool builder.
fn random_stratified_candidates_for_scenario(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
) -> Vec<CrewCandidate> {
    if !scenario.warm_start.is_empty() {
        info!(
            strategy = "random_stratified",
            ignored_warm_start = scenario.warm_start.len() as u64,
            "optimize_random_stratified_warm_start_ignored"
        );
    }
    sample_stratified_random_crews(
        registry,
        &StratifiedSampleParams {
            count: scenario
                .max_candidates
                .unwrap_or(DEFAULT_RANDOM_STRATIFIED_CANDIDATES)
                .max(1),
            seed: scenario.seed,
            below_decks_slots: scenario.below_decks_slots,
            below_decks_pool_mode: scenario.below_decks_pool_mode,
            enemy_type: scenario.enemy_type,
            profile_id: scenario.profile_id,
            constraints: scenario.constraints.as_ref(),
            exclude_hashes: None,
        },
    )
}

/// Tiered scout exploration slice (roadmap §1.1): when `tiered_random_exploration_pct`
/// is set, swap the tail `pct` of the post-prefilter scout candidate list for
/// stratified-random crews that never saw the analytical proxy. Budget-neutral —
/// the scout candidate count is unchanged (or shrinks slightly if the legal space
/// cannot supply enough distinct random crews). Returns the new list and the stable
/// hashes of the injected crews for downstream method provenance.
fn inject_random_exploration(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
    candidates: Vec<CrewCandidate>,
) -> (Vec<CrewCandidate>, HashSet<u64>) {
    let pct = scenario
        .tiered_random_exploration_pct
        .unwrap_or(0.0)
        .clamp(0.0, 0.5);
    let n = candidates.len();
    if pct <= 0.0 || n == 0 {
        return (candidates, HashSet::new());
    }
    let k = (((n as f64) * pct).round() as usize).min(n / 2);
    if k == 0 {
        return (candidates, HashSet::new());
    }
    let mut kept = candidates;
    kept.truncate(n - k);
    let exclude: HashSet<u64> = kept.iter().map(crew_candidate_stable_hash).collect();
    let sampled = sample_stratified_random_crews(
        registry,
        &StratifiedSampleParams {
            count: k,
            seed: scenario.seed,
            below_decks_slots: scenario.below_decks_slots,
            below_decks_pool_mode: scenario.below_decks_pool_mode,
            enemy_type: scenario.enemy_type,
            profile_id: scenario.profile_id,
            constraints: scenario.constraints.as_ref(),
            exclude_hashes: Some(&exclude),
        },
    );
    let hashes: HashSet<u64> = sampled.iter().map(crew_candidate_stable_hash).collect();
    info!(
        strategy = "tiered",
        seed = scenario.seed,
        random_exploration_pct = pct,
        random_exploration_requested = k as u64,
        random_exploration_injected = sampled.len() as u64,
        "optimize_random_exploration_injected"
    );
    kept.extend(sampled);
    (kept, hashes)
}

pub fn optimize_scenario(scenario: &OptimizationScenario<'_>) -> Vec<RankedCrewResult> {
    match scenario.strategy {
        OptimizerStrategy::Exhaustive => optimize_scenario_exhaustive(scenario),
        OptimizerStrategy::Genetic => optimize_scenario_genetic(scenario, |_, _, _| true, || true),
        OptimizerStrategy::Tiered => optimize_scenario_exhaustive(scenario), // Tiered requires registry; fallback when none
        OptimizerStrategy::LinearEval => optimize_scenario_exhaustive(scenario), // LinearEval requires registry; fallback when none
        OptimizerStrategy::RandomStratified => optimize_scenario_exhaustive(scenario), // Sampler requires registry; fallback when none
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
        scenario_support_buff_request(scenario),
        scenario.defender_opponent,
        scenario.player_defender_officer_crew.clone(),
        scenario.pvp.clone(),
    );
    let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
    let (candidates, _) = analytical_prefilter_unless_chain(
        &shared_tiered,
        candidates,
        scenario.seed,
        keep,
        &scenario.chain_grind,
        &scenario.warm_start,
        &scenario.prior_reference_crews,
        scenario.enable_learned_pair_prior,
    );
    let (candidates, _random_exploration_hashes) =
        inject_random_exploration(registry, scenario, candidates);
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
        .and_then(crate::optimizer::budget_hints::load_for_profile);
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
        scenario_support_buff_request(scenario),
        scenario.chain_grind.clone(),
        scenario.defender_opponent,
        scenario.player_defender_officer_crew.clone(),
        scenario.pvp.clone(),
        pre_ref,
        !scenario.tiered_scout_uniform,
        scenario.tiered_confirm_budget_cap_mult,
        budget_hints_storage.as_ref(),
        false, // scout_priority_queue
        None,  // pq_minimal_scout
        None,  // pq_selection_mult
        None,  // pq_abandon_margin
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
        OptimizerStrategy::Genetic => {
            optimize_scenario_genetic_inner(Some(registry), scenario, |_, _, _| true, || true)
        }
        OptimizerStrategy::Tiered => optimize_scenario_tiered_with_registry(registry, scenario),
        OptimizerStrategy::LinearEval => {
            linear_eval::run_linear_eval_with_registry(registry, scenario, |_| true, || true)
        }
        OptimizerStrategy::RandomStratified => {
            optimize_scenario_with_progress_with_registry(registry, scenario, |_| true, || true)
                .ranked
        }
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
        scenario_support_buff_request(scenario),
        scenario.defender_opponent,
        scenario.player_defender_officer_crew.clone(),
        scenario.pvp.clone(),
    );
    let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
    let (candidates, _) = analytical_prefilter_unless_chain(
        &shared_ex,
        candidates,
        scenario.seed,
        keep,
        &scenario.chain_grind,
        &scenario.warm_start,
        &scenario.prior_reference_crews,
        scenario.enable_learned_pair_prior,
    );
    if let Some((scout_s, top_keep)) = scenario
        .exhaustive_scout_sims
        .zip(scenario.exhaustive_scout_top_keep)
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
    let sims = scenario.simulation_count.max(1);
    // Top-K progressive abandonment is opt-in (KOBAYASHI_FULLMC_EARLY_STOP=1): on this codebase's
    // saturated win/loss matchups it rarely fires and adds leader-lock overhead, so the default is
    // the plain full pass. See docs/PERFORMANCE.md.
    let simulation_results = if full_mc_early_stop_enabled() {
        let abandon_k = exhaustive_abandon_top_k(scenario, candidates.len());
        run_monte_carlo_confirm_topk_with_shared(
            shared_ex,
            &candidates,
            sims,
            scenario.seed,
            true,
            scenario.chain_grind.clone(),
            abandon_k,
        )
    } else {
        run_monte_carlo_with_shared(
            shared_ex,
            &candidates,
            sims,
            scenario.seed,
            true,
            scenario.chain_grind.clone(),
        )
    };
    rank_results(simulation_results)
}

/// Conservative leaderboard size for full-MC progressive abandonment on the exhaustive path.
/// The top `k` crews are always simulated to full depth and ranked exactly; only the long tail
/// of provable losers is pruned. Floors well above the displayed result count so deep scrolling
/// still sees a faithful ranking.
fn exhaustive_abandon_top_k(scenario: &OptimizationScenario<'_>, n_candidates: usize) -> usize {
    const FULL_MC_ABANDON_MIN_K: usize = 64;
    scenario
        .tiered_top_k
        .unwrap_or_else(|| tiered_top_k_for_workload(n_candidates))
        .max(DEFAULT_TOP_K)
        .max(FULL_MC_ABANDON_MIN_K)
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
        scenario_support_buff_request(scenario),
        scenario.defender_opponent,
        scenario.player_defender_officer_crew.clone(),
    );
    let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
    let (candidates, _) = analytical_prefilter_unless_chain(
        &shared,
        candidates,
        scenario.seed,
        keep,
        &scenario.chain_grind,
        &scenario.warm_start,
        &scenario.prior_reference_crews,
        scenario.enable_learned_pair_prior,
    );
    let simulation_results = run_monte_carlo_with_shared(
        shared,
        &candidates,
        scenario.simulation_count.max(1),
        scenario.seed,
        true,
        scenario.chain_grind.clone(),
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
    optimize_scenario_genetic_inner(None, scenario, on_progress, eval_should_continue)
}

fn optimize_scenario_genetic_inner<F, G>(
    registry: Option<&DataRegistry>,
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
            below_decks_pool_mode: scenario.below_decks_pool_mode,
            pvp_mode: scenario.pvp.is_some(),
            below_decks_slots: scenario.below_decks_slots,
            constraints: scenario.constraints.clone(),
            support_buffs: scenario.support_buffs.clone(),
            defender_support_buffs: scenario.defender_support_buffs.clone(),
            defender_alliance_debuffs: scenario.defender_alliance_debuffs.clone(),
            chain_grind: scenario.chain_grind.clone(),
            defender_opponent: scenario.defender_opponent,
            roster_profile_id: scenario.profile_id.map(String::from),
            ship_tier: scenario.ship_tier,
            ship_level: scenario.ship_level,
            ..GeneticConfig::default()
        }
    } else {
        let mut cfg = GeneticConfig::seeded(filtered_seeds);
        cfg.below_decks_pool_mode = scenario.below_decks_pool_mode;
        cfg.pvp_mode = scenario.pvp.is_some();
        cfg.below_decks_slots = scenario.below_decks_slots;
        cfg.constraints = scenario.constraints.clone();
        cfg.support_buffs = scenario.support_buffs.clone();
        cfg.defender_support_buffs = scenario.defender_support_buffs.clone();
        cfg.defender_alliance_debuffs = scenario.defender_alliance_debuffs.clone();
        cfg.chain_grind = scenario.chain_grind.clone();
        cfg.defender_opponent = scenario.defender_opponent;
        cfg.roster_profile_id = scenario.profile_id.map(String::from);
        cfg.ship_tier = scenario.ship_tier;
        cfg.ship_level = scenario.ship_level;
        cfg
    };
    run_genetic_optimizer_ranked(
        scenario.ship,
        scenario.hostile,
        &config,
        scenario.seed,
        scenario.simulation_count.max(1),
        registry,
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
        // These strategies require a registry; without one fall back to exhaustive with progress.
        OptimizerStrategy::Tiered
        | OptimizerStrategy::LinearEval
        | OptimizerStrategy::RandomStratified => {
            let scenario_ex = OptimizationScenario {
                strategy: OptimizerStrategy::Exhaustive,
                ..scenario.clone()
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
                scenario_support_buff_request(scenario),
                scenario.defender_opponent,
                scenario.player_defender_officer_crew.clone(),
            );
            let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
            let (candidates, _) = analytical_prefilter_unless_chain(
                &shared,
                candidates,
                scenario.seed,
                keep,
                &scenario.chain_grind,
                &scenario.warm_start,
                &scenario.prior_reference_crews,
                scenario.enable_learned_pair_prior,
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
                let batch_results = run_monte_carlo_with_shared(
                    shared.clone(),
                    batch,
                    sim_count,
                    scenario.seed,
                    true,
                    scenario.chain_grind.clone(),
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

/// What a local-refinement pass did, for the caller to attach to its [`OptimizeRunOutcome`].
#[derive(Debug, Clone, Default)]
struct LocalRefinementPassReport {
    provenance: HashMap<u64, RefinementProvenance>,
    stats: Option<crate::optimizer::refinement::LocalRefinementStats>,
}

/// Run the opt-in local-refinement hill-climb around a finished search's top finalists
/// (roadmap §1.1), appending confirmed improvements to `ranked` and re-sorting it.
///
/// Shared by the tiered and genetic lanes. Refinement needs nothing from the search that produced
/// `ranked` beyond the rows themselves and their scores, and both lanes score through
/// [`rank_results`], so the same pass applies to either without knowing which ran.
///
/// Returns an empty report — leaving `ranked` untouched — when refinement is off
/// (`scenario.local_refinement` is `None`, the default), when the search found no finalists to
/// climb from, or when officer pools cannot be built for the scenario. That makes a run with
/// refinement disabled byte-identical to one from before the feature existed.
fn run_local_refinement_pass(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
    ranked: &mut Vec<RankedCrewResult>,
    should_continue: impl FnMut() -> bool,
) -> LocalRefinementPassReport {
    let Some(params) = scenario.local_refinement.as_ref() else {
        return LocalRefinementPassReport::default();
    };
    if ranked.is_empty() {
        return LocalRefinementPassReport::default();
    }
    let Some(pools) = build_officer_pools_from_registry(
        registry,
        scenario.below_decks_pool_mode,
        Some(scenario.enemy_type),
        scenario.profile_id,
        scenario.below_decks_slots,
        scenario.constraints.as_ref(),
    ) else {
        return LocalRefinementPassReport::default();
    };
    let shared = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        scenario.profile_id,
        scenario_support_buff_request(scenario),
        scenario.defender_opponent,
        scenario.player_defender_officer_crew.clone(),
        scenario.pvp.clone(),
    );
    let seeds: Vec<(CrewCandidate, f64)> = ranked
        .iter()
        .take(params.seed_crews)
        .map(|row| {
            (
                CrewCandidate {
                    captain: row.captain.clone(),
                    bridge: row.bridge.clone(),
                    below_decks: row.below_decks.clone(),
                },
                f64::from(row.score.value),
            )
        })
        .collect();
    let ctx = RefinementContext {
        shared: &shared,
        pools: &pools,
        constraints: scenario.constraints.as_ref(),
        chain_grind: scenario.chain_grind.clone(),
        seed: scenario.seed,
    };
    let outcome = refine_finalists(&ctx, &seeds, params, should_continue);
    // Logged unconditionally: a pass that generated neighbors and accepted none is the interesting
    // case (the finalists were already local optima at this depth), and it is indistinguishable
    // from a pass that never ran without these counts.
    info!(
        strategy = scenario.strategy.log_label(),
        seed = scenario.seed,
        seeds_refined = outcome.stats.seeds_refined as u64,
        rounds_run = outcome.stats.rounds_run as u64,
        neighbors_generated = outcome.stats.neighbors_generated as u64,
        neighbors_scouted = outcome.stats.neighbors_scouted as u64,
        neighbors_confirmed = outcome.stats.neighbors_confirmed as u64,
        improvements_accepted = outcome.stats.improvements_accepted as u64,
        "optimize_local_refinement_completed"
    );
    if !outcome.results.is_empty() {
        // Score each merged batch through the shared ranking independently, then re-sort the
        // combined list: per-row score does not depend on the other rows, so this is equivalent
        // to ranking them together.
        let mut refined_rows = rank_results(outcome.results);
        ranked.append(&mut refined_rows);
        sort_ranked_rows(ranked);
    }
    LocalRefinementPassReport {
        provenance: outcome.provenance,
        stats: Some(outcome.stats),
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
            let generated_candidates = candidates.len();
            let candidates = prepend_warm_start_dedupe(&scenario.warm_start, candidates);
            let after_warm_start_dedupe = candidates.len();
            let candidates = apply_crew_constraints(candidates, scenario);
            let after_constraints = candidates.len();
            let shared = build_shared_scenario_data_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.ship_tier,
                scenario.ship_level,
                scenario.profile_id,
                scenario_support_buff_request(scenario),
                scenario.defender_opponent,
                scenario.player_defender_officer_crew.clone(),
                scenario.pvp.clone(),
            );
            let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
            let (candidates, analytical_prefilter) = analytical_prefilter_unless_chain(
                &shared,
                candidates,
                scenario.seed,
                keep,
                &scenario.chain_grind,
                &scenario.warm_start,
                &scenario.prior_reference_crews,
                scenario.enable_learned_pair_prior,
            );
            // Prune crews whose analytical expected damage is below a configurable fraction
            // of the defender's hull — these crews cannot kill the hostile in any realistic
            // number of rounds. Skipped when chain grind is active (analytical prefilter is
            // also skipped for chain grind).
            let prune_frac = scenario
                .prune_analytical_hull_fraction
                .filter(|&f| f > 0.0 && scenario.chain_grind.is_none());
            let analytical_prune_dropped: usize;
            let candidates = if let Some(frac) = prune_frac {
                let (kept, dropped) =
                    crate::optimizer::analytical::prune_candidates_by_expected_hull_damage(
                        &shared,
                        candidates,
                        scenario.seed,
                        frac,
                    );
                analytical_prune_dropped = dropped;
                kept
            } else {
                analytical_prune_dropped = 0;
                candidates
            };
            if analytical_prune_dropped > 0 {
                info!(
                    strategy = "tiered",
                    seed = scenario.seed,
                    analytical_prune_dropped = analytical_prune_dropped as u64,
                    analytical_prune_remaining = candidates.len() as u64,
                    "optimize_hopeless_pruned"
                );
            }
            // Prune crews whose abilities are mostly gated on mismatched conditions
            // (e.g. Borg officers vs non-Borg hostile). Skipped for chain grind.
            let gate_frac = scenario
                .prune_static_gate_max_fraction
                .filter(|&f| f > 0.0 && f < 1.0 && scenario.chain_grind.is_none());
            let static_gate_prune_dropped: usize;
            let candidates = if let Some(frac) = gate_frac {
                let (kept, dropped) =
                    crate::optimizer::analytical::prune_candidates_by_static_gates(
                        &shared,
                        candidates,
                        scenario.seed,
                        frac,
                    );
                static_gate_prune_dropped = dropped;
                kept
            } else {
                static_gate_prune_dropped = 0;
                candidates
            };
            if static_gate_prune_dropped > 0 {
                info!(
                    strategy = "tiered",
                    seed = scenario.seed,
                    static_gate_prune_dropped = static_gate_prune_dropped as u64,
                    static_gate_prune_remaining = candidates.len() as u64,
                    "optimize_static_gate_pruned"
                );
            }
            let (candidates, random_exploration_hashes) =
                inject_random_exploration(registry, scenario, candidates);
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
                .and_then(crate::optimizer::budget_hints::load_for_profile);
            let (mut ranked, scout_budget) = run_tiered_with_registry_with_progress(
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
                scenario_support_buff_request(scenario),
                scenario.chain_grind.clone(),
                scenario.defender_opponent,
                scenario.player_defender_officer_crew.clone(),
                scenario.pvp.clone(),
                pre_ref,
                scout_adaptive,
                scenario.tiered_confirm_budget_cap_mult,
                budget_hints_storage.as_ref(),
                scenario.tiered_scout_priority_queue,
                scenario.tiered_pq_minimal_scout,
                scenario.tiered_pq_selection_mult,
                scenario.tiered_pq_abandon_margin,
                &mut on_progress,
            );

            // Counted before refinement on purpose: the funnel describes the *candidate pipeline*
            // (generated → warm-start → constraints → prefilter → tiered → confirmed), and refined
            // crews are synthesized after that pipeline has already run rather than drawn from it.
            // Counting them here would make the confirmed stage report crews that never entered
            // the funnel at all.
            let confirmed_candidates = ranked.len();

            let refinement = run_local_refinement_pass(
                registry,
                scenario,
                &mut ranked,
                &mut eval_should_continue,
            );

            let mut candidate_funnel = OptimizeCandidateFunnel::with_counts(
                generated_candidates,
                scenario.warm_start.len(),
                after_warm_start_dedupe,
                after_constraints,
                analytical_prefilter,
                n_tiered,
                confirmed_candidates,
            );
            if !random_exploration_hashes.is_empty() {
                candidate_funnel.random_exploration_candidates =
                    Some(random_exploration_hashes.len());
            }
            OptimizeRunOutcome {
                ranked,
                candidate_funnel,
                analytical_prefilter,
                tiered_resolved: Some((n_tiered, scout_sims, top_k)),
                tiered_scout_budget: Some(scout_budget),
                exhaustive_adaptive_budget: None,
                optimize_history_confirm_hits: hits,
                random_exploration_hashes,
                refinement_provenance: refinement.provenance,
                refinement_stats: refinement.stats,
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
            let generated_candidates = candidates.len();
            let candidates = prepend_warm_start_dedupe(&scenario.warm_start, candidates);
            let after_warm_start_dedupe = candidates.len();
            let candidates = apply_crew_constraints(candidates, scenario);
            let after_constraints = candidates.len();
            let shared_ex = build_shared_scenario_data_from_registry(
                registry,
                scenario.ship,
                scenario.hostile,
                scenario.ship_tier,
                scenario.ship_level,
                scenario.profile_id,
                scenario_support_buff_request(scenario),
                scenario.defender_opponent,
                scenario.player_defender_officer_crew.clone(),
                scenario.pvp.clone(),
            );
            let keep = resolved_analytical_prefilter_keep(scenario, candidates.len());
            let (candidates, analytical_prefilter) = analytical_prefilter_unless_chain(
                &shared_ex,
                candidates,
                scenario.seed,
                keep,
                &scenario.chain_grind,
                &scenario.warm_start,
                &scenario.prior_reference_crews,
                scenario.enable_learned_pair_prior,
            );
            // Prune hopeless crews (same logic as tiered path).
            let prune_frac = scenario
                .prune_analytical_hull_fraction
                .filter(|&f| f > 0.0 && scenario.chain_grind.is_none());
            let analytical_prune_dropped: usize;
            let candidates = if let Some(frac) = prune_frac {
                let (kept, dropped) =
                    crate::optimizer::analytical::prune_candidates_by_expected_hull_damage(
                        &shared_ex,
                        candidates,
                        scenario.seed,
                        frac,
                    );
                analytical_prune_dropped = dropped;
                kept
            } else {
                analytical_prune_dropped = 0;
                candidates
            };
            if analytical_prune_dropped > 0 {
                info!(
                    strategy = "exhaustive",
                    seed = scenario.seed,
                    analytical_prune_dropped = analytical_prune_dropped as u64,
                    analytical_prune_remaining = candidates.len() as u64,
                    "optimize_hopeless_pruned"
                );
            }
            let gate_frac = scenario
                .prune_static_gate_max_fraction
                .filter(|&f| f > 0.0 && f < 1.0 && scenario.chain_grind.is_none());
            let static_gate_prune_dropped: usize;
            let candidates = if let Some(frac) = gate_frac {
                let (kept, dropped) =
                    crate::optimizer::analytical::prune_candidates_by_static_gates(
                        &shared_ex,
                        candidates,
                        scenario.seed,
                        frac,
                    );
                static_gate_prune_dropped = dropped;
                kept
            } else {
                static_gate_prune_dropped = 0;
                candidates
            };
            if static_gate_prune_dropped > 0 {
                info!(
                    strategy = "exhaustive",
                    seed = scenario.seed,
                    static_gate_prune_dropped = static_gate_prune_dropped as u64,
                    static_gate_prune_remaining = candidates.len() as u64,
                    "optimize_static_gate_pruned"
                );
            }
            let total = candidates.len();
            let candidate_funnel = |confirmed_candidates: usize| {
                OptimizeCandidateFunnel::with_counts(
                    generated_candidates,
                    scenario.warm_start.len(),
                    after_warm_start_dedupe,
                    after_constraints,
                    analytical_prefilter,
                    total,
                    confirmed_candidates,
                )
            };
            if total == 0 {
                return OptimizeRunOutcome {
                    ranked: Vec::new(),
                    candidate_funnel: candidate_funnel(0),
                    analytical_prefilter,
                    tiered_resolved: None,
                    tiered_scout_budget: None,
                    exhaustive_adaptive_budget: None,
                    optimize_history_confirm_hits: 0,
                    random_exploration_hashes: HashSet::new(),
                    refinement_provenance: HashMap::new(),
                    refinement_stats: None,
                };
            }

            if let Some((scout_s, top_keep)) = scenario
                .exhaustive_scout_sims
                .zip(scenario.exhaustive_scout_top_keep)
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
                let confirmed_target = top_keep.max(1).min(total);
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
                        candidate_funnel: candidate_funnel(confirmed_target),
                        analytical_prefilter,
                        tiered_resolved: None,
                        tiered_scout_budget: None,
                        exhaustive_adaptive_budget: Some(budget),
                        optimize_history_confirm_hits: hits,
                        random_exploration_hashes: HashSet::new(),
                        refinement_provenance: HashMap::new(),
                        refinement_stats: None,
                    };
                }
                return OptimizeRunOutcome {
                    ranked: Vec::new(),
                    candidate_funnel: candidate_funnel(0),
                    analytical_prefilter,
                    tiered_resolved: None,
                    tiered_scout_budget: None,
                    exhaustive_adaptive_budget: None,
                    optimize_history_confirm_hits: 0,
                    random_exploration_hashes: HashSet::new(),
                    refinement_provenance: HashMap::new(),
                    refinement_stats: None,
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
                    candidate_funnel: candidate_funnel(0),
                    analytical_prefilter,
                    tiered_resolved: None,
                    tiered_scout_budget: None,
                    exhaustive_adaptive_budget: None,
                    optimize_history_confirm_hits: 0,
                    random_exploration_hashes: HashSet::new(),
                    refinement_provenance: HashMap::new(),
                    refinement_stats: None,
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
                let batch_results = run_monte_carlo_with_shared(
                    shared_ex.clone(),
                    batch,
                    sim_count,
                    scenario.seed,
                    true,
                    scenario.chain_grind.clone(),
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

            let ranked = rank_results(all_results);
            let confirmed_candidates = ranked.len();
            OptimizeRunOutcome {
                ranked,
                candidate_funnel: candidate_funnel(confirmed_candidates),
                analytical_prefilter,
                tiered_resolved: None,
                tiered_scout_budget: None,
                exhaustive_adaptive_budget: None,
                optimize_history_confirm_hits: 0,
                random_exploration_hashes: HashSet::new(),
                refinement_provenance: HashMap::new(),
                refinement_stats: None,
            }
        }
        OptimizerStrategy::Genetic => {
            let mut ranked = optimize_scenario_genetic(
                scenario,
                |gen, max_gen, _| {
                    on_progress(OptimizeProgressTick {
                        crews_done: gen as u32,
                        total_crews: max_gen.max(1) as u32,
                        phase: "genetic",
                        partial_top: None,
                    })
                },
                // Reborrowed rather than moved: the refinement pass below polls the same
                // cancellation check after the generations finish.
                &mut eval_should_continue,
            );
            // Counted before refinement, matching tiered: the funnel describes the candidate
            // pipeline, and refined crews are synthesized after it has already run.
            let confirmed_candidates = ranked.len();
            let refinement = run_local_refinement_pass(
                registry,
                scenario,
                &mut ranked,
                &mut eval_should_continue,
            );
            OptimizeRunOutcome {
                ranked,
                candidate_funnel: OptimizeCandidateFunnel {
                    warm_start_candidates: scenario.warm_start.len(),
                    scout_candidates: (!scenario.seed_population.is_empty())
                        .then_some(scenario.seed_population.len()),
                    confirmed_candidates: Some(confirmed_candidates),
                    ..OptimizeCandidateFunnel::default()
                },
                analytical_prefilter: None,
                tiered_resolved: None,
                tiered_scout_budget: None,
                exhaustive_adaptive_budget: None,
                optimize_history_confirm_hits: 0,
                random_exploration_hashes: HashSet::new(),
                refinement_provenance: refinement.provenance,
                refinement_stats: refinement.stats,
            }
        }
        OptimizerStrategy::LinearEval => {
            let ranked = linear_eval::run_linear_eval_with_registry(
                registry,
                scenario,
                &mut on_progress,
                &mut eval_should_continue,
            );
            let confirmed_candidates = ranked.len();
            OptimizeRunOutcome {
                ranked,
                candidate_funnel: OptimizeCandidateFunnel {
                    warm_start_candidates: scenario.warm_start.len(),
                    confirmed_candidates: Some(confirmed_candidates),
                    ..OptimizeCandidateFunnel::default()
                },
                analytical_prefilter: None,
                tiered_resolved: None,
                tiered_scout_budget: None,
                exhaustive_adaptive_budget: None,
                optimize_history_confirm_hits: 0,
                random_exploration_hashes: HashSet::new(),
                refinement_provenance: HashMap::new(),
                refinement_stats: None,
            }
        }
        OptimizerStrategy::RandomStratified => {
            let candidates = random_stratified_candidates_for_scenario(registry, scenario);
            let generated_candidates = candidates.len();
            let random_exploration_hashes: HashSet<u64> =
                candidates.iter().map(crew_candidate_stable_hash).collect();
            let scout_sims = scenario
                .tiered_scout_sims
                .unwrap_or_else(|| tiered_scout_sims_for_workload(generated_candidates))
                .max(1);
            let top_k = scenario
                .tiered_top_k
                .unwrap_or_else(|| tiered_top_k_for_workload(generated_candidates))
                .max(1);
            info!(
                strategy = "random_stratified",
                seed = scenario.seed,
                random_candidates = generated_candidates as u64,
                scout_sims = scout_sims as u64,
                top_k = top_k as u64,
                "optimize_random_stratified_sampled"
            );
            let budget_hints_storage = scenario
                .profile_id
                .and_then(crate::optimizer::budget_hints::load_for_profile);
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
                scenario_support_buff_request(scenario),
                scenario.chain_grind.clone(),
                scenario.defender_opponent,
                scenario.player_defender_officer_crew.clone(),
                scenario.pvp.clone(),
                // No optimize-history preconfirm reuse: the control lane stays
                // independent of previously confirmed crews.
                None,
                !scenario.tiered_scout_uniform,
                scenario.tiered_confirm_budget_cap_mult,
                budget_hints_storage.as_ref(),
                scenario.tiered_scout_priority_queue,
                scenario.tiered_pq_minimal_scout,
                scenario.tiered_pq_selection_mult,
                scenario.tiered_pq_abandon_margin,
                &mut on_progress,
            );
            let confirmed_candidates = ranked.len();
            let mut candidate_funnel = OptimizeCandidateFunnel::with_counts(
                generated_candidates,
                0,
                generated_candidates,
                generated_candidates,
                None,
                generated_candidates,
                confirmed_candidates,
            );
            candidate_funnel.random_exploration_candidates = Some(generated_candidates);
            OptimizeRunOutcome {
                ranked,
                candidate_funnel,
                analytical_prefilter: None,
                tiered_resolved: None,
                tiered_scout_budget: Some(scout_budget),
                exhaustive_adaptive_budget: None,
                optimize_history_confirm_hits: 0,
                random_exploration_hashes,
                refinement_provenance: HashMap::new(),
                refinement_stats: None,
            }
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
        below_decks_pool_mode: BelowDecksPoolMode::Strict,
        seed_population: Vec::new(),
        profile_id,
        tiered_scout_sims: None,
        tiered_top_k: None,
        tiered_scout_uniform: false,
        tiered_confirm_budget_cap_mult: None,
        optimize_history_confirm_cap_mult: None,
        tiered_scout_priority_queue: false,
        tiered_pq_minimal_scout: None,
        tiered_pq_selection_mult: None,
        tiered_pq_abandon_margin: None,
        tiered_random_exploration_pct: None,
        exhaustive_scout_sims: None,
        exhaustive_scout_top_keep: None,
        analytical_prefilter_keep: None,
        prune_analytical_hull_fraction: None,
        prune_static_gate_max_fraction: None,
        below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
        constraints: None,
        support_buffs: Vec::new(),
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        player_defender_officer_crew: None,
        pvp: None,
        enemy_type: crate::combat::EnemyType::RedMovingSpace,
        warm_start: Vec::new(),
        prior_reference_crews: Vec::new(),
        optimize_cache_key: None,
        reuse_fingerprint: None,
        enable_learned_pair_prior: true,
        learned_officer_scores: None,
        local_refinement: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        analytical_prefilter_keep_auto, count_effective_optimize_candidates,
        enforce_candidate_legality_with_registry,
        enforce_candidate_optimization_eligibility_with_registry,
        optimize_scenario_with_progress_with_registry, AnalyticalPrefilterWorkload,
        BelowDecksPoolMode, CandidateStrategy, OptimizationScenario, OptimizerStrategy,
    };
    use crate::combat::EnemyType;
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::constraints::CrewSearchConstraints;
    use crate::optimizer::crew_generator::CrewGenerator;
    use crate::optimizer::crew_generator::{
        build_officer_pools_from_registry, CrewCandidate, DEFAULT_BELOW_DECKS_SLOTS,
        NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS,
    };
    use crate::optimizer::matchup_priors::analytical_prefilter_rank_score;
    use crate::optimizer::monte_carlo::scenario::{
        build_shared_scenario_data_from_registry, scenario_to_combat_input_from_shared,
        DefenderOpponent,
    };
    use crate::optimizer::monte_carlo::{
        run_monte_carlo_confirm_topk_with_shared,
        run_monte_carlo_parallel_deduped_chunked_with_shared, run_monte_carlo_with_shared,
    };
    use crate::optimizer::ranking::rank_results;

    #[test]
    fn analytical_rank_score_prior_reference_boosts_identical_crew() {
        let registry = DataRegistry::load().expect("data registry");
        let shared = build_shared_scenario_data_from_registry(
            &registry,
            "saladin",
            "2918121098",
            None,
            None,
            None,
            crate::data::support_buffs::SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let seed = 11u64;
        let cand = CrewCandidate {
            captain: "James T. Kirk".into(),
            bridge: vec!["Spock".into(), "Leonard McCoy".into()],
            below_decks: vec![
                "Montgomery Scott".into(),
                "Hikaru Sulu".into(),
                "Nyota Uhura".into(),
            ],
        };
        let input = scenario_to_combat_input_from_shared(&shared, &cand, seed);
        let s0 = analytical_prefilter_rank_score(&shared, &input, &cand, &[], true);
        let prior = vec![cand.clone()];
        let s1 = analytical_prefilter_rank_score(&shared, &input, &cand, prior.as_slice(), true);
        assert!(
            s1 > s0,
            "history-shaped prior refs should raise composite rank score: s0={s0} s1={s1}"
        );
    }

    #[test]
    fn enforce_candidate_legality_rejects_duplicate_and_wrong_seat_candidates() {
        let registry = DataRegistry::load().expect("data registry");
        let candidates = vec![
            CrewCandidate {
                captain: "James T. Kirk".into(),
                bridge: vec!["Spock".into(), "Spock".into()],
                below_decks: vec![
                    "Montgomery Scott".into(),
                    "Hikaru Sulu".into(),
                    "Nyota Uhura".into(),
                ],
            },
            CrewCandidate {
                captain: "T'Laan".into(),
                bridge: vec!["Spock".into(), "Leonard McCoy".into()],
                below_decks: vec![
                    "Montgomery Scott".into(),
                    "Hikaru Sulu".into(),
                    "Nyota Uhura".into(),
                ],
            },
        ];

        let (kept, summary) = enforce_candidate_legality_with_registry(
            &registry,
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            3,
            candidates,
        );

        assert!(kept.is_empty(), "invalid candidates should be filtered");
        assert_eq!(summary.dropped_duplicates, 1);
        assert_eq!(summary.dropped_seat_incompatible, 1);
    }

    #[test]
    fn injected_optimizer_crews_obey_scenario_specific_below_decks_rules() {
        let registry = DataRegistry::load().expect("data registry");
        let profile = Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS);
        // Pick filler captain/bridge eligible for the PvE scenario so the kept-case assertion below
        // isolates the below-decks rule (all seats are now scenario-filtered, not just below-decks).
        let pools = build_officer_pools_from_registry(
            &registry,
            BelowDecksPoolMode::Relaxed,
            Some(EnemyType::RedMovingSpace),
            profile,
            1,
            None,
        )
        .expect("red-scenario pools");
        let captain = pools
            .captains
            .iter()
            .find(|captain| pools.bridge.contains(captain))
            .expect("captain that can occupy bridge")
            .clone();
        let candidate_with = |below_decks: &str| {
            let bridge: Vec<String> = pools
                .bridge
                .iter()
                .filter(|name| *name != &captain && !name.eq_ignore_ascii_case(below_decks))
                .take(2)
                .cloned()
                .collect();
            assert_eq!(bridge.len(), 2);
            CrewCandidate {
                captain: captain.clone(),
                bridge,
                below_decks: vec![below_decks.into()],
            }
        };

        let (manual, _) = enforce_candidate_legality_with_registry(
            &registry,
            profile,
            1,
            vec![candidate_with("Academy Doctor")],
        );
        assert_eq!(
            manual.len(),
            1,
            "manual simulation legality stays unrestricted"
        );

        let (pve_pvp_officer, _) = enforce_candidate_optimization_eligibility_with_registry(
            &registry,
            profile,
            1,
            EnemyType::RedMovingSpace,
            vec![candidate_with("Academy Doctor")],
        );
        assert!(pve_pvp_officer.is_empty());

        let (pve_loot_officer, _) = enforce_candidate_optimization_eligibility_with_registry(
            &registry,
            profile,
            1,
            EnemyType::RedMovingSpace,
            vec![candidate_with("The Doctor")],
        );
        assert_eq!(pve_loot_officer.len(), 1);

        let (pvp_loot_officer, _) = enforce_candidate_optimization_eligibility_with_registry(
            &registry,
            profile,
            1,
            EnemyType::PvpSpace,
            vec![candidate_with("The Doctor")],
        );
        assert!(pvp_loot_officer.is_empty());

        let (pvp_explicitly_banned_officer, _) =
            enforce_candidate_optimization_eligibility_with_registry(
                &registry,
                profile,
                1,
                EnemyType::PvpSpace,
                vec![candidate_with("SNW La'an")],
            );
        assert!(pvp_explicitly_banned_officer.is_empty());
    }

    /// Matchup-prior injection from `optimize_history` uses the same legality gate as warm-start
    /// and heuristics: crews referencing officers not in the roster-filtered pools are dropped.
    #[test]
    fn enforce_candidate_legality_drops_injected_crew_not_in_rostered_pools() {
        let registry = DataRegistry::load().expect("data registry");
        let not_in_demo_pools = CrewCandidate {
            captain: "Totally Fake Captain XYZ789".into(),
            bridge: vec!["Fake Bridge A".into(), "Fake Bridge B".into()],
            below_decks: vec![
                "Fake Below 1".into(),
                "Fake Below 2".into(),
                "Fake Below 3".into(),
            ],
        };
        let (kept, summary) = enforce_candidate_legality_with_registry(
            &registry,
            Some(crate::data::profile_index::DEMO_PROFILE_ID),
            3,
            vec![not_in_demo_pools],
        );
        assert!(
            kept.is_empty(),
            "expected fake officers absent from demo roster pools"
        );
        assert!(
            summary.dropped_wrong_shape > 0 || summary.dropped_seat_incompatible > 0,
            "expected illegality summary: {summary:?}"
        );
    }

    #[test]
    fn sort_and_analytical_prefilter_prior_reference_changes_keep_one_winner() {
        let registry = DataRegistry::load().expect("data registry");
        let shared = build_shared_scenario_data_from_registry(
            &registry,
            "saladin",
            "2918121098",
            None,
            None,
            None,
            crate::data::support_buffs::SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let seed = 11u64;
        let history_shape = CrewCandidate {
            captain: "James T. Kirk".into(),
            bridge: vec!["Spock".into(), "Leonard McCoy".into()],
            below_decks: vec![
                "Montgomery Scott".into(),
                "Hikaru Sulu".into(),
                "Nyota Uhura".into(),
            ],
        };
        let mut near_dup = history_shape.clone();
        near_dup.below_decks[2] = "Pavel Chekov".into();

        let candidates = vec![near_dup.clone(), history_shape.clone()];
        let (keep_empty, _) = super::sort_and_analytical_prefilter(
            &shared,
            candidates.clone(),
            seed,
            Some(1),
            &[],
            &[],
            true,
        );
        let (keep_prior, _) = super::sort_and_analytical_prefilter(
            &shared,
            candidates,
            seed,
            Some(1),
            &[],
            std::slice::from_ref(&history_shape),
            true,
        );
        assert_eq!(keep_empty.len(), 1);
        assert_eq!(keep_prior.len(), 1);
        assert_eq!(
            keep_empty[0].below_decks.last().map(String::as_str),
            Some("Pavel Chekov")
        );
        assert_eq!(
            keep_prior[0].below_decks.last().map(String::as_str),
            Some("Nyota Uhura")
        );
    }

    #[test]
    fn analytical_partial_topk_matches_full_ranking_prefix() {
        let registry = DataRegistry::load().expect("data registry");
        let ship = "saladin";
        let hostile = "2918121098";
        let seed = 19u64;
        let candidates = CrewGenerator::new()
            .generate_candidates_from_registry(&registry, ship, hostile, seed, None);
        assert!(
            candidates.len() >= 32,
            "fixture needs enough candidates to exercise partial selection"
        );
        let shared = build_shared_scenario_data_from_registry(
            &registry,
            ship,
            hostile,
            None,
            None,
            None,
            crate::data::support_buffs::SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let keep = 17;
        let (full, full_stats) = super::sort_and_analytical_prefilter(
            &shared,
            candidates.clone(),
            seed,
            None,
            &[],
            &[],
            true,
        );
        let (partial, partial_stats) = super::sort_and_analytical_prefilter(
            &shared,
            candidates,
            seed,
            Some(keep),
            &[],
            &[],
            true,
        );
        assert_eq!(full_stats, None);
        assert_eq!(partial_stats, Some((full.len(), keep)));
        assert_eq!(partial, full[..keep]);
    }

    #[test]
    fn confirm_topk_preserves_ranking_vs_full_pass() {
        // Full-MC top-K progressive abandonment must not change the answer: every crew that runs
        // to full depth is byte-identical to the no-abandonment baseline (same per-crew seeds), the
        // winner is preserved, and some losers really do get abandoned (otherwise the test is moot).
        //
        // botany_bay (a weak survey hull) at T2/L10 vs hostile 38048587 is a borderline matchup:
        // a few crews win meaningfully while most lose outright, giving the win-rate spread that
        // lets the leader cut prune the hopeless tail. (Most PvE matchups are all-win or all-lose,
        // where win-rate abandonment is a safe no-op and ranking is driven by hull remaining
        // instead.) Recalibrated from T1/L1 after the 2026-07-10 engine correctness fixes
        // (weapon_damage operator folding) collapsed that matchup to all-lose.
        let registry = DataRegistry::load().expect("data registry");
        let ship = "botany_bay";
        let hostile = "38048587";
        let candidates: Vec<CrewCandidate> = CrewGenerator::new()
            .generate_candidates_from_registry(&registry, ship, hostile, 7, None)
            .into_iter()
            .take(80)
            .collect();
        if candidates.len() < 20 {
            // Generation depends on local data; nothing to assert without a real population.
            return;
        }

        let sims = 2000usize;
        let seed = 7u64;
        let k = 2usize;

        let build_shared = || {
            build_shared_scenario_data_from_registry(
                &registry,
                ship,
                hostile,
                Some(2),
                Some(10),
                None,
                crate::data::support_buffs::SupportBuffScenarioRequest::default(),
                DefenderOpponent::Hostile,
                None,
                None,
            )
        };

        // Serial (parallel=false) so the leaderboard evolves deterministically.
        let baseline =
            run_monte_carlo_with_shared(build_shared(), &candidates, sims, seed, false, None);
        let treatment = run_monte_carlo_confirm_topk_with_shared(
            build_shared(),
            &candidates,
            sims,
            seed,
            false,
            None,
            k,
        );

        assert_eq!(baseline.len(), treatment.len());

        let mut abandoned = 0usize;
        for (b, t) in baseline.iter().zip(treatment.iter()) {
            if t.trials_run < sims {
                // Abandoned crews ran fewer trials; their (lower-fidelity) stats are not compared.
                // They are provably out of the top-K — the leader-cut guarantee is unit-tested.
                abandoned += 1;
            } else {
                // Un-abandoned crews are identical to the baseline (same seeds, same trial count).
                assert_eq!(
                    t.trials_run, b.trials_run,
                    "full-depth crew should match baseline trial count"
                );
                assert!(
                    (t.win_rate - b.win_rate).abs() < 1e-12,
                    "full-depth crew win rate must match baseline exactly"
                );
                assert!(
                    (t.avg_hull_remaining - b.avg_hull_remaining).abs() < 1e-12,
                    "full-depth crew hull must match baseline exactly"
                );
            }
        }

        assert!(
            abandoned > 0,
            "expected some losing crews to be abandoned (population={}, k={k})",
            candidates.len()
        );

        // The top-ranked crew must be identical under both paths.
        let best_baseline = rank_results(baseline);
        let best_treatment = rank_results(treatment);
        let bb = &best_baseline[0];
        let bt = &best_treatment[0];
        assert_eq!(
            (&bb.captain, &bb.bridge, &bb.below_decks),
            (&bt.captain, &bt.bridge, &bt.below_decks),
            "winner must be preserved by progressive abandonment"
        );
    }

    #[test]
    fn chunked_topk_shared_leader_preserves_survivors() {
        // The shared-leader chunked path (used by the genetic per-generation eval) must keep its
        // top-K survivors byte-identical to the no-abandonment baseline while abandoning hopeless
        // crews. Same borderline matchup (T2/L10) as the exhaustive test for win-rate spread.
        let registry = DataRegistry::load().expect("data registry");
        let ship = "botany_bay";
        let hostile = "38048587";
        let candidates: Vec<CrewCandidate> = CrewGenerator::new()
            .generate_candidates_from_registry(&registry, ship, hostile, 7, None)
            .into_iter()
            .take(80)
            .collect();
        if candidates.len() < 20 {
            return;
        }

        let sims = 2000usize;
        let seed = 7u64;
        let chunk = 8usize;
        let k = 2usize;

        let build_shared = || {
            build_shared_scenario_data_from_registry(
                &registry,
                ship,
                hostile,
                Some(2),
                Some(10),
                None,
                crate::data::support_buffs::SupportBuffScenarioRequest::default(),
                DefenderOpponent::Hostile,
                None,
                None,
            )
        };

        let baseline = run_monte_carlo_parallel_deduped_chunked_with_shared(
            &build_shared(),
            &candidates,
            sims,
            seed,
            None,
            chunk,
            None,
            || true,
        )
        .expect("baseline run");
        let treatment = run_monte_carlo_parallel_deduped_chunked_with_shared(
            &build_shared(),
            &candidates,
            sims,
            seed,
            None,
            chunk,
            Some(k),
            || true,
        )
        .expect("treatment run");

        assert_eq!(baseline.len(), treatment.len());
        let mut abandoned = 0usize;
        for (b, t) in baseline.iter().zip(treatment.iter()) {
            if t.trials_run < sims {
                abandoned += 1;
            } else {
                assert!(
                    (t.win_rate - b.win_rate).abs() < 1e-12,
                    "full-depth crew win rate must match baseline exactly"
                );
            }
        }
        assert!(
            abandoned > 0,
            "expected some losers abandoned across chunks (n={}, k={k})",
            candidates.len()
        );
    }

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
            below_decks_pool_mode: BelowDecksPoolMode::default(),
            seed_population: Vec::new(),
            profile_id: None,
            tiered_scout_sims: None,
            tiered_top_k: None,
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            optimize_history_confirm_cap_mult: None,
            tiered_scout_priority_queue: false,
            tiered_pq_minimal_scout: None,
            tiered_pq_selection_mult: None,
            tiered_pq_abandon_margin: None,
            tiered_random_exploration_pct: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: None,
            prune_analytical_hull_fraction: None,
            prune_static_gate_max_fraction: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            defender_support_buffs: None,
            defender_alliance_debuffs: None,
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            player_defender_officer_crew: None,
            pvp: None,
            enemy_type: crate::combat::EnemyType::RedMovingSpace,
            warm_start: Vec::new(),
            prior_reference_crews: Vec::new(),
            optimize_cache_key: None,
            reuse_fingerprint: None,
            enable_learned_pair_prior: true,
            learned_officer_scores: None,
            local_refinement: None,
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
            below_decks_pool_mode: BelowDecksPoolMode::default(),
            seed_population: Vec::new(),
            profile_id: None,
            tiered_scout_sims: None,
            tiered_top_k: None,
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            optimize_history_confirm_cap_mult: None,
            tiered_scout_priority_queue: false,
            tiered_pq_minimal_scout: None,
            tiered_pq_selection_mult: None,
            tiered_pq_abandon_margin: None,
            tiered_random_exploration_pct: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: Some(4),
            prune_analytical_hull_fraction: None,
            prune_static_gate_max_fraction: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            defender_support_buffs: None,
            defender_alliance_debuffs: None,
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            player_defender_officer_crew: None,
            pvp: None,
            enemy_type: crate::combat::EnemyType::RedMovingSpace,
            warm_start: Vec::new(),
            prior_reference_crews: Vec::new(),
            optimize_cache_key: None,
            reuse_fingerprint: None,
            enable_learned_pair_prior: true,
            learned_officer_scores: None,
            local_refinement: None,
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
            below_decks_pool_mode: BelowDecksPoolMode::default(),
            seed_population: Vec::new(),
            profile_id: Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            tiered_scout_sims: None,
            tiered_top_k: None,
            tiered_scout_uniform: false,
            tiered_confirm_budget_cap_mult: None,
            optimize_history_confirm_cap_mult: None,
            tiered_scout_priority_queue: false,
            tiered_pq_minimal_scout: None,
            tiered_pq_selection_mult: None,
            tiered_pq_abandon_margin: None,
            tiered_random_exploration_pct: None,
            exhaustive_scout_sims: Some(12),
            exhaustive_scout_top_keep: Some(4),
            analytical_prefilter_keep: None,
            prune_analytical_hull_fraction: None,
            prune_static_gate_max_fraction: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            defender_support_buffs: None,
            defender_alliance_debuffs: None,
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            player_defender_officer_crew: None,
            pvp: None,
            enemy_type: crate::combat::EnemyType::RedMovingSpace,
            warm_start: Vec::new(),
            prior_reference_crews: Vec::new(),
            optimize_cache_key: None,
            reuse_fingerprint: None,
            enable_learned_pair_prior: true,
            learned_officer_scores: None,
            local_refinement: None,
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
            below_decks_pool_mode: BelowDecksPoolMode::default(),
            seed_population: Vec::new(),
            profile_id: Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            tiered_scout_sims: Some(320),
            tiered_top_k: Some(10),
            tiered_scout_uniform: true,
            tiered_confirm_budget_cap_mult: None,
            optimize_history_confirm_cap_mult: None,
            tiered_scout_priority_queue: false,
            tiered_pq_minimal_scout: None,
            tiered_pq_selection_mult: None,
            tiered_pq_abandon_margin: None,
            tiered_random_exploration_pct: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            analytical_prefilter_keep: None,
            prune_analytical_hull_fraction: None,
            prune_static_gate_max_fraction: None,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
            constraints: None,
            support_buffs: Vec::new(),
            defender_support_buffs: None,
            defender_alliance_debuffs: None,
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            player_defender_officer_crew: None,
            pvp: None,
            enemy_type: crate::combat::EnemyType::RedMovingSpace,
            warm_start: Vec::new(),
            prior_reference_crews: Vec::new(),
            optimize_cache_key: None,
            reuse_fingerprint: None,
            enable_learned_pair_prior: true,
            learned_officer_scores: None,
            local_refinement: None,
        };
        let mut adaptive = uniform.clone();
        adaptive.tiered_scout_uniform = false;
        let out_u =
            optimize_scenario_with_progress_with_registry(&registry, &uniform, |_| true, || true);
        let out_a =
            optimize_scenario_with_progress_with_registry(&registry, &adaptive, |_| true, || true);
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
            crate::data::support_buffs::SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let shared_high = build_shared_scenario_data_from_registry(
            &registry,
            "amalgam",
            hostile,
            Some(5),
            Some(1),
            None,
            crate::data::support_buffs::SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
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
