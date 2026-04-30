//! Execution layer: run optimize, job store, and response types.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::OwnedSemaphorePermit;
use tracing::{info, info_span, warn};

use crate::data::budget_telemetry::{maybe_append_row, BudgetTelemetryRow};
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::{
    expand_crews, load_seed_file, BelowDecksStrategy, DEFAULT_HEURISTICS_DIR,
};
use crate::data::import::roster_import_fallback_warning_message;
use crate::data::optimize_history;
use crate::optimizer::constraints::{filter_candidates, CrewSearchConstraints};
use crate::optimizer::crew_generator::{
    resolve_below_decks_slots_for_ship, CandidateStrategy, CrewCandidate,
};
use crate::optimizer::monte_carlo::{
    run_monte_carlo_with_shared, scenario::build_shared_scenario_data_from_registry,
    SimulationResult,
};
use crate::optimizer::ranking::{apply_novelty_mmr_if_configured, rank_results, RankedCrewResult};
use crate::optimizer::tiered::TieredScoutBudgetStats;
use crate::optimizer::{
    count_effective_optimize_candidates, enforce_candidate_legality_with_registry,
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizeProgressTick,
    OptimizerStrategy,
};
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};

use super::requests::{
    build_crew_search_constraints, chain_grind_params_from_request, parse_below_decks_strategy,
    parse_strategy, ChainGrindRequest, OptimizePayloadError, OptimizeRequest, DEFAULT_SIMS,
};

/// When `strategy` is omitted, use tiered scout→confirm if the capped candidate count is at least this.
/// Tuned so medium/large roster searches default to two-phase MC instead of full sims on every crew.
const TIERED_AUTO_THRESHOLD: usize = 400;

/// Cap on heuristic-expanded crews merged into warm-start for `fast_discovery` (below-decks exploration can explode).
const FAST_DISCOVERY_MAX_HEURISTIC_WARM: usize = 480;

fn optimizer_strategy_to_api_label(s: OptimizerStrategy) -> &'static str {
    match s {
        OptimizerStrategy::Exhaustive => "exhaustive",
        OptimizerStrategy::Genetic => "genetic",
        OptimizerStrategy::Tiered => "tiered",
    }
}

fn warm_start_crews_from_request_dtos(request: &OptimizeRequest) -> Vec<CrewCandidate> {
    request
        .warm_start_crews
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|dto| CrewCandidate {
                    captain: dto.captain.trim().to_string(),
                    bridge: dto.bridge.clone(),
                    below_decks: dto.below_decks.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Heuristic template crews first, then client `warm_start_crews` (dedupe happens later in the optimizer).
fn merge_fast_discovery_warm_start(
    mut heuristic_crews: Vec<CrewCandidate>,
    dto_warm: Vec<CrewCandidate>,
) -> (Vec<CrewCandidate>, bool) {
    let cap_hit = heuristic_crews.len() > FAST_DISCOVERY_MAX_HEURISTIC_WARM;
    if cap_hit {
        heuristic_crews.truncate(FAST_DISCOVERY_MAX_HEURISTIC_WARM);
    }
    let mut out = heuristic_crews;
    out.extend(dto_warm);
    (out, cap_hit)
}

/// Resolve effective optimizer strategy; `strategy_auto` is true only when the client omitted `strategy`
/// and we picked tiered vs exhaustive from candidate volume (not used for genetic or heuristics-only).
#[allow(clippy::too_many_arguments)]
fn resolve_effective_optimize_strategy(
    registry: &DataRegistry,
    request: &OptimizeRequest,
    below_decks_slots: usize,
    heuristics_only: bool,
    seed: u64,
    profile_id: Option<&str>,
    crew_constraints: Option<&CrewSearchConstraints>,
    warm_start_for_count: &[CrewCandidate],
) -> (OptimizerStrategy, bool) {
    if let Some(ref raw) = request.strategy {
        let strategy = parse_strategy(Some(raw));
        info!(
            requested_strategy = %raw,
            effective_strategy = optimizer_strategy_to_api_label(strategy),
            strategy_auto = false,
            heuristics_only,
            "optimize_strategy_resolved"
        );
        return (strategy, false);
    }
    if heuristics_only {
        info!(
            requested_strategy = "auto",
            effective_strategy = optimizer_strategy_to_api_label(OptimizerStrategy::Exhaustive),
            strategy_auto = false,
            heuristics_only = true,
            "optimize_strategy_resolved"
        );
        return (OptimizerStrategy::Exhaustive, false);
    }
    let strat = CandidateStrategy {
        max_candidates: request.max_candidates.map(|n| n as usize),
        only_below_decks_with_ability: request.prioritize_below_decks_ability.unwrap_or(false),
        below_decks_slots,
        constraints: crew_constraints.cloned(),
        roster_profile_id: profile_id.filter(|s| !s.is_empty()).map(String::from),
        ..CandidateStrategy::default()
    };
    // Must match crews after generation, warm-start prepend, and constraint filter.
    let n = count_effective_optimize_candidates(
        registry,
        request.ship.trim(),
        request.hostile.trim(),
        seed,
        profile_id,
        strat,
        warm_start_for_count,
    );
    let strategy = if n >= TIERED_AUTO_THRESHOLD {
        OptimizerStrategy::Tiered
    } else {
        OptimizerStrategy::Exhaustive
    };
    info!(
        requested_strategy = "auto",
        effective_strategy = optimizer_strategy_to_api_label(strategy),
        strategy_auto = true,
        heuristics_only = false,
        effective_candidates = n as u64,
        tiered_auto_threshold = TIERED_AUTO_THRESHOLD as u64,
        "optimize_strategy_resolved"
    );
    (strategy, true)
}

#[derive(Debug, Clone, Serialize)]
pub struct CrewRecommendation {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub win_rate: f64,
    pub win_rate_ci_low: f64,
    pub win_rate_ci_high: f64,
    pub stall_rate: f64,
    pub stall_rate_ci_low: f64,
    pub stall_rate_ci_high: f64,
    pub loss_rate: f64,
    pub loss_rate_ci_low: f64,
    pub loss_rate_ci_high: f64,
    pub r1_kill_rate: f64,
    pub r1_kill_rate_ci_low: f64,
    pub r1_kill_rate_ci_high: f64,
    pub avg_hull_remaining: f64,
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
    pub avg_defender_hull_remaining: f64,
    pub avg_defender_hull_remaining_ci_low: f64,
    pub avg_defender_hull_remaining_ci_high: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<crate::optimizer::ChainSimulationSummary>,
}

/// Counts-only echo of active optimize constraints (for clients / debugging).
#[derive(Debug, Clone, Serialize)]
pub struct OptimizeConstraintsSummary {
    pub must_include: usize,
    pub exclude: usize,
    pub groups: usize,
    pub captain_must_be: bool,
    pub bridge_must_include: usize,
    pub below_decks_must_include: usize,
}

fn crew_recommendation_from_ranked(r: &RankedCrewResult) -> CrewRecommendation {
    CrewRecommendation {
        captain: r.captain.clone(),
        bridge: r.bridge.clone(),
        below_decks: r.below_decks.clone(),
        win_rate: r.win_rate,
        win_rate_ci_low: r.win_rate_ci_low,
        win_rate_ci_high: r.win_rate_ci_high,
        stall_rate: r.stall_rate,
        stall_rate_ci_low: r.stall_rate_ci_low,
        stall_rate_ci_high: r.stall_rate_ci_high,
        loss_rate: r.loss_rate,
        loss_rate_ci_low: r.loss_rate_ci_low,
        loss_rate_ci_high: r.loss_rate_ci_high,
        r1_kill_rate: r.r1_kill_rate,
        r1_kill_rate_ci_low: r.r1_kill_rate_ci_low,
        r1_kill_rate_ci_high: r.r1_kill_rate_ci_high,
        avg_hull_remaining: r.avg_hull_remaining,
        avg_hull_remaining_ci_low: r.avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high: r.avg_hull_remaining_ci_high,
        avg_defender_hull_remaining: r.avg_defender_hull_remaining,
        avg_defender_hull_remaining_ci_low: r.avg_defender_hull_remaining_ci_low,
        avg_defender_hull_remaining_ci_high: r.avg_defender_hull_remaining_ci_high,
        chain: r.chain.clone(),
    }
}

fn summarize_constraints(
    con: Option<&CrewSearchConstraints>,
) -> Option<OptimizeConstraintsSummary> {
    let c = con?;
    if c.is_empty() {
        return None;
    }
    Some(OptimizeConstraintsSummary {
        must_include: c.must_include.len(),
        exclude: c.exclude.len(),
        groups: c.groups.len(),
        captain_must_be: !c.captain_must_be.is_empty(),
        bridge_must_include: c.bridge_must_include.len(),
        below_decks_must_include: c.below_decks_must_include.len(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioSummary {
    pub ship: String,
    pub hostile: String,
    pub sims: u32,
    pub seed: u64,
    /// Resolved below-decks slot count used for candidate generation.
    pub below_decks_slots: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimize_constraints: Option<OptimizeConstraintsSummary>,
    /// Requested cap on crews after analytical ranking (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytical_prefilter_keep: Option<u32>,
    /// Crew count before analytical truncation (only when truncation ran).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytical_prefilter_from: Option<u32>,
    /// Crew count after analytical truncation (only when truncation ran).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytical_prefilter_kept: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainGrindRequest>,
    /// Strategy actually run (`exhaustive`, `tiered`, or `genetic`).
    pub effective_strategy: String,
    /// True when `strategy` was omitted and the server chose tiered vs exhaustive from candidate count.
    pub strategy_auto: bool,
    /// Echo of the client `strategy` field when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub novelty_lambda: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub novelty_diverse_top: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub novelty_pool: Option<u32>,
    /// When true, heuristic seed crews were merged into warm-start for the main optimize path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_discovery: Option<bool>,
    /// Tiered runs: crews that reused persisted confirmation stats from `optimize_history.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimize_history_confirm_hits: Option<u32>,
    /// True when this run updated `optimize_history.json` for `optimize_cache_key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimize_history_wrote: Option<bool>,
    /// Tiered runs: scout/confirm trial accounting (see `TieredScoutBudgetStats`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_scout_budget: Option<TieredScoutBudgetStats>,
    /// Exhaustive two-phase runs: scout-then-full-MC trial accounting (same struct shape as tiered budget stats).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exhaustive_adaptive_budget: Option<TieredScoutBudgetStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeResponse {
    pub status: &'static str,
    pub engine: &'static str,
    pub scenario: ScenarioSummary,
    pub recommendations: Vec<CrewRecommendation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub notes: Vec<&'static str>,
    /// Extra human-readable notes (e.g. approximate pre-filter semantics).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub approximate_notes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Load heuristics seeds and expand them into CrewCandidates.
pub fn load_heuristics_candidates(
    registry: &DataRegistry,
    seed_names: &[String],
    bd_strategy: BelowDecksStrategy,
    below_decks_slots: usize,
) -> Vec<CrewCandidate> {
    let canonical_names: Vec<String> = registry.officers().iter().map(|o| o.name.clone()).collect();
    seed_names
        .iter()
        .flat_map(|name| {
            let parsed = load_seed_file(name, DEFAULT_HEURISTICS_DIR, Some(&canonical_names));
            let candidates = expand_crews(parsed, below_decks_slots, bd_strategy);
            candidates.into_iter().map(|c| CrewCandidate {
                captain: c.captain,
                bridge: c.bridge,
                below_decks: c.below_decks,
            })
        })
        .collect()
}

/// Metadata from the shared optimize gather path (sync + async jobs).
#[derive(Clone)]
struct OptimizeGatherMeta {
    strategy: OptimizerStrategy,
    /// True when `strategy` was omitted and the server auto-selected tiered vs exhaustive.
    strategy_auto: bool,
    is_seeded_genetic: bool,
    heuristics_only: bool,
    heuristics_seeds_nonempty: bool,
    /// Client asked for `fast_discovery` but every heuristic line resolved to zero crews after filters.
    fast_discovery_no_resolved_crews: bool,
    /// Heuristic seeds merged into warm-start (`fast_discovery`); no standalone all-seed Monte Carlo pass.
    fast_discovery: bool,
    /// True when heuristic warm-start merge hit [`FAST_DISCOVERY_MAX_HEURISTIC_WARM`].
    fast_discovery_heuristic_cap_hit: bool,
    using_placeholder_combatants: bool,
    /// `Some((generated, kept))` when analytical pre-filter truncated the candidate list.
    analytical_prefilter: Option<(usize, usize)>,
    below_decks_slots: usize,
    optimize_constraints: Option<OptimizeConstraintsSummary>,
    optimize_history_confirm_hits: u32,
    optimize_history_wrote: bool,
    tiered_scout_budget: Option<TieredScoutBudgetStats>,
    exhaustive_adaptive_budget: Option<TieredScoutBudgetStats>,
    dropped_illegal_warm_start: usize,
    dropped_illegal_heuristics: usize,
    dropped_illegal_prior_refs: usize,
    roster_filter_warning: Option<String>,
}

/// Progress / cancellation hooks for optimize. Sync path uses [`OptimizeProgressSink::None`].
enum OptimizeProgressSink {
    None,
    Job {
        job_id: String,
        cancel: Arc<AtomicBool>,
        heuristics_seeds_nonempty: bool,
        /// Filled by [`gather_optimize_simulation_results`] once candidates are loaded.
        is_seeded_genetic: bool,
        /// When true, skip the 0–10% progress slice reserved for standalone heuristic Monte Carlo.
        skip_heuristic_standalone_mc: bool,
    },
}

impl OptimizeProgressSink {
    fn on_heuristics_start(&self, h_total: u32) {
        let Self::Job { job_id, .. } = self else {
            return;
        };
        info!(
            job_id = %job_id,
            phase = "heuristics",
            crews_done = 0u32,
            total_crews = h_total,
            "optimize_phase_started"
        );
        let mut map = lock_jobs();
        if let Some(state) = map.get_mut(job_id) {
            state.total_crews = h_total;
            state.phase = Some("heuristics".to_string());
        }
    }

    fn on_heuristics_complete(
        &self,
        heuristics_only: bool,
        h_total: u32,
        results: &[SimulationResult],
    ) {
        let Self::Job { job_id, .. } = self else {
            return;
        };
        info!(
            job_id = %job_id,
            phase = "heuristics",
            crews_done = h_total,
            total_crews = h_total,
            heuristics_only,
            top_preview_size = results.len().min(5) as u64,
            "optimize_phase_completed"
        );
        let mut map = lock_jobs();
        if let Some(state) = map.get_mut(job_id) {
            state.crews_done = h_total;
            state.progress = if heuristics_only { 100 } else { 10 };
            let ranked = rank_results(results.to_vec());
            state.progress_preview = Some(
                ranked
                    .iter()
                    .take(5)
                    .map(crew_recommendation_from_ranked)
                    .collect(),
            );
        }
    }

    fn on_optimize_tick(&mut self, tick: OptimizeProgressTick) -> bool {
        match self {
            Self::None => true,
            Self::Job {
                job_id,
                cancel,
                heuristics_seeds_nonempty,
                is_seeded_genetic,
                skip_heuristic_standalone_mc,
            } => {
                if cancel.load(Ordering::Relaxed) {
                    return false;
                }
                let base_progress = if *heuristics_seeds_nonempty
                    && !*is_seeded_genetic
                    && !*skip_heuristic_standalone_mc
                {
                    10u8
                } else {
                    0u8
                };
                let crews_done = tick.crews_done;
                let total_crews = tick.total_crews;
                let progress = if total_crews == 0 {
                    base_progress
                } else {
                    let pct =
                        (crews_done as f64 / total_crews as f64) * (100.0 - base_progress as f64);
                    (base_progress as f64 + pct).round().min(100.0) as u8
                };
                let mut map = lock_jobs();
                if let Some(state) = map.get_mut(job_id) {
                    state.progress = progress;
                    state.crews_done = crews_done;
                    state.total_crews = total_crews;
                    state.phase = Some(tick.phase.to_string());
                    if let Some(partial) = tick.partial_top.as_ref() {
                        state.progress_preview = Some(
                            partial
                                .iter()
                                .take(5)
                                .map(crew_recommendation_from_ranked)
                                .collect(),
                        );
                    }
                }
                info!(
                    job_id = %job_id,
                    phase = tick.phase,
                    crews_done,
                    total_crews,
                    progress,
                    partial_top_size = tick.partial_top.as_ref().map_or(0, |top| top.len()) as u64,
                    "optimize_progress_tick"
                );
                true
            }
        }
    }

    fn job_cancelled(&self) -> bool {
        match self {
            Self::None => false,
            Self::Job { cancel, .. } => cancel.load(Ordering::Relaxed),
        }
    }
}

fn ranked_crew_to_simulation_result(r: RankedCrewResult) -> SimulationResult {
    SimulationResult {
        candidate: CrewCandidate {
            captain: r.captain,
            bridge: r.bridge,
            below_decks: r.below_decks,
        },
        trials_run: r.trials_run,
        win_rate: r.win_rate,
        win_rate_ci_low: r.win_rate_ci_low,
        win_rate_ci_high: r.win_rate_ci_high,
        stall_rate: r.stall_rate,
        stall_rate_ci_low: r.stall_rate_ci_low,
        stall_rate_ci_high: r.stall_rate_ci_high,
        loss_rate: r.loss_rate,
        loss_rate_ci_low: r.loss_rate_ci_low,
        loss_rate_ci_high: r.loss_rate_ci_high,
        r1_kill_rate: r.r1_kill_rate,
        r1_kill_rate_ci_low: r.r1_kill_rate_ci_low,
        r1_kill_rate_ci_high: r.r1_kill_rate_ci_high,
        avg_hull_remaining: r.avg_hull_remaining,
        avg_hull_remaining_ci_low: r.avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high: r.avg_hull_remaining_ci_high,
        avg_defender_hull_remaining: r.avg_defender_hull_remaining,
        avg_defender_hull_remaining_ci_low: r.avg_defender_hull_remaining_ci_low,
        avg_defender_hull_remaining_ci_high: r.avg_defender_hull_remaining_ci_high,
        chain: r.chain.clone(),
    }
}

/// Shared Monte Carlo + optimizer scenario execution. Sync and background jobs use the same logic.
fn gather_optimize_simulation_results(
    registry: &DataRegistry,
    request: &OptimizeRequest,
    profile_id: Option<&str>,
    sink: &mut OptimizeProgressSink,
) -> Result<(Vec<SimulationResult>, OptimizeGatherMeta), ()> {
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    let seed = request.seed.unwrap_or(0);
    let chain_grind = request
        .chain
        .as_ref()
        .and_then(|c| chain_grind_params_from_request(c).ok().flatten());
    let heuristics_only = request.heuristics_only.unwrap_or(false);
    let bd_strategy = parse_below_decks_strategy(request.below_decks_strategy.as_ref());
    let heuristics_seeds = request.heuristics_seeds.as_deref().unwrap_or(&[]);
    let heuristics_seeds_nonempty = !heuristics_seeds.is_empty();
    let gather_span = info_span!(
        "optimize_gather",
        ship = %request.ship,
        hostile = %request.hostile,
        seed,
        sims,
        requested_strategy = request.strategy.as_deref().unwrap_or("auto"),
        heuristics_only,
        heuristics_seed_count = heuristics_seeds.len() as u64,
        profile_id_present = profile_id.is_some()
    );
    let _gather_span = gather_span.enter();
    let below_decks_slots = resolve_below_decks_slots_for_ship(
        &request.ship,
        request.ship_tier,
        request.ship_level,
        request.below_decks_slots,
    );
    let crew_constraints = build_crew_search_constraints(request);
    let cache_key_normalized = request.optimize_cache_key.as_ref().and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    let mut optimize_history_confirm_hits = 0u32;
    let mut optimize_history_wrote = false;
    let mut tiered_scout_budget_for_response: Option<TieredScoutBudgetStats> = None;
    let mut exhaustive_adaptive_budget_for_response: Option<TieredScoutBudgetStats> = None;

    let mut h_candidates = if heuristics_seeds_nonempty {
        load_heuristics_candidates(registry, heuristics_seeds, bd_strategy, below_decks_slots)
    } else {
        Vec::new()
    };
    let (h_candidates_legal, h_legality) = enforce_candidate_legality_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        h_candidates,
    );
    h_candidates = h_candidates_legal;
    if let Some(ref c) = crew_constraints {
        h_candidates = filter_candidates(h_candidates, c);
    }
    info!(
        heuristics_candidates = h_candidates.len() as u64,
        "optimize_heuristics_candidates_ready"
    );

    let dto_warm = warm_start_crews_from_request_dtos(request);
    let (dto_warm, warm_legality) =
        enforce_candidate_legality_with_registry(registry, profile_id, below_decks_slots, dto_warm);
    let fast_discovery_requested =
        request.fast_discovery == Some(true) && heuristics_seeds_nonempty && !heuristics_only;
    let fast_discovery_no_resolved_crews = fast_discovery_requested && h_candidates.is_empty();
    let fast_discovery = fast_discovery_requested && !h_candidates.is_empty();
    info!(
        fast_discovery_requested,
        fast_discovery_enabled = fast_discovery,
        fast_discovery_no_resolved_crews,
        "optimize_fast_discovery_resolution"
    );

    let (scenario_warm_start, fast_discovery_heuristic_cap_hit) = if fast_discovery {
        merge_fast_discovery_warm_start(h_candidates.clone(), dto_warm)
    } else {
        (dto_warm, false)
    };

    let prior_reference_crews_raw = match (profile_id, cache_key_normalized.as_deref()) {
        (Some(pid), Some(key)) => {
            optimize_history::prior_reference_crews_for_matchup_priors(pid, key, &chain_grind)
        }
        _ => Vec::new(),
    };
    let prior_refs_in = prior_reference_crews_raw.len();
    let (prior_reference_crews, prior_legality) = enforce_candidate_legality_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        prior_reference_crews_raw,
    );
    let dropped_illegal_prior_refs = prior_legality.dropped_wrong_shape
        + prior_legality.dropped_duplicates
        + prior_legality.dropped_seat_incompatible;
    debug_assert_eq!(
        prior_refs_in,
        prior_reference_crews.len() + dropped_illegal_prior_refs
    );

    let (strategy, strategy_auto) = resolve_effective_optimize_strategy(
        registry,
        request,
        below_decks_slots,
        heuristics_only,
        seed,
        profile_id,
        crew_constraints.as_ref(),
        &scenario_warm_start,
    );

    let is_seeded_genetic = strategy == OptimizerStrategy::Genetic && !h_candidates.is_empty();
    info!(
        effective_strategy = optimizer_strategy_to_api_label(strategy),
        strategy_auto, is_seeded_genetic, "optimize_execution_mode_selected"
    );

    if let OptimizeProgressSink::Job {
        is_seeded_genetic: sink_sg,
        skip_heuristic_standalone_mc: sink_skip,
        ..
    } = sink
    {
        *sink_sg = is_seeded_genetic;
        *sink_skip = fast_discovery && !is_seeded_genetic;
    }

    let shared_scenario = build_shared_scenario_data_from_registry(
        registry,
        &request.ship,
        &request.hostile,
        request.ship_tier,
        request.ship_level,
        profile_id,
        request.support_buffs.as_deref(),
        request.defender_opponent,
    );
    let using_placeholder_combatants = shared_scenario.using_placeholder_combatants;

    let mut all_results: Vec<SimulationResult> =
        if heuristics_seeds_nonempty && !is_seeded_genetic && !fast_discovery {
            let h_total = h_candidates.len() as u32;
            sink.on_heuristics_start(h_total);
            let h_len = h_candidates.len();
            let num_batches = monte_carlo_batch_count_for_candidates(h_len).max(1);
            let ranges = batch_ranges(h_len, num_batches);
            let mut results: Vec<SimulationResult> = Vec::with_capacity(h_len);
            for (start, end) in ranges {
                if sink.job_cancelled() {
                    warn!("optimize_cancelled");
                    return Err(());
                }
                let batch = &h_candidates[start..end];
                let batch_results = run_monte_carlo_with_shared(
                    shared_scenario.clone(),
                    batch,
                    sims as usize,
                    seed,
                    true,
                    chain_grind.clone(),
                );
                results.extend(batch_results);
            }
            sink.on_heuristics_complete(heuristics_only, h_total, &results);
            info!(
                heuristic_results = results.len() as u64,
                "optimize_heuristics_monte_carlo_complete"
            );
            results
        } else {
            Vec::new()
        };

    let analytical_prefilter = if !heuristics_only {
        let scenario = OptimizationScenario {
            ship: &request.ship,
            hostile: &request.hostile,
            ship_tier: request.ship_tier,
            ship_level: request.ship_level,
            simulation_count: sims as usize,
            seed,
            max_candidates: request.max_candidates.map(|n| n as usize),
            strategy,
            only_below_decks_with_ability: request.prioritize_below_decks_ability.unwrap_or(false),
            seed_population: if is_seeded_genetic {
                h_candidates.clone()
            } else {
                Vec::new()
            },
            profile_id,
            tiered_scout_sims: request.tiered_scout_sims.map(|n| n as usize),
            tiered_top_k: request.tiered_top_k.map(|n| n as usize),
            tiered_scout_uniform: matches!(request.tiered_scout_uniform, Some(true)),
            tiered_confirm_budget_cap_mult: request.tiered_confirm_budget_cap_mult,
            exhaustive_scout_sims: request
                .exhaustive_scout_sims
                .map(|n| n as usize)
                .filter(|_| strategy == OptimizerStrategy::Exhaustive),
            exhaustive_scout_top_keep: request
                .exhaustive_scout_top_keep
                .map(|n| n as usize)
                .filter(|_| strategy == OptimizerStrategy::Exhaustive),
            analytical_prefilter_keep: request.analytical_prefilter_keep.map(|n| n as usize),
            below_decks_slots,
            constraints: crew_constraints.clone(),
            support_buffs: request.support_buffs.clone().unwrap_or_default(),
            chain_grind: chain_grind.clone(),
            defender_opponent: request.defender_opponent,
            warm_start: scenario_warm_start,
            prior_reference_crews,
            optimize_cache_key: cache_key_normalized.clone(),
            enable_learned_pair_prior: request.enable_learned_pair_prior.unwrap_or(true),
        };
        let cancel_for_eval: Option<Arc<AtomicBool>> = match &*sink {
            OptimizeProgressSink::Job { cancel, .. } => Some(Arc::clone(cancel)),
            OptimizeProgressSink::None => None,
        };
        let outcome = optimize_scenario_with_progress_with_registry(
            registry,
            &scenario,
            |tick| sink.on_optimize_tick(tick),
            || match cancel_for_eval.as_ref() {
                None => true,
                Some(c) => !c.load(Ordering::Relaxed),
            },
        );
        if sink.job_cancelled() {
            warn!("optimize_cancelled");
            return Err(());
        }
        optimize_history_confirm_hits = outcome.optimize_history_confirm_hits;
        tiered_scout_budget_for_response = outcome.tiered_scout_budget;
        exhaustive_adaptive_budget_for_response = outcome.exhaustive_adaptive_budget;
        if strategy == OptimizerStrategy::Tiered {
            if let (Some(pid), Some(key), Some((n, scout, tk))) = (
                profile_id,
                cache_key_normalized.as_ref(),
                outcome.tiered_resolved,
            ) {
                let tiered_scout_allocator = if matches!(request.tiered_scout_uniform, Some(true)) {
                    0u8
                } else {
                    1u8
                };
                let entry = optimize_history::build_entry_from_ranked(
                    sims,
                    seed,
                    scout,
                    tk,
                    n,
                    tiered_scout_allocator,
                    &chain_grind,
                    optimize_history::TIERED_BUDGET_POLICY_V2,
                    request.tiered_confirm_budget_cap_mult.map(|x| x as f32),
                    &outcome.ranked,
                );
                optimize_history_wrote =
                    optimize_history::upsert_entry(pid, key.as_str(), entry).is_ok();
            }
        }
        if strategy == OptimizerStrategy::Exhaustive {
            if let (Some(pid), Some(key)) = (profile_id, cache_key_normalized.as_ref()) {
                if outcome.exhaustive_adaptive_budget.is_some() && !outcome.ranked.is_empty() {
                    if let (Some(scout_u32), Some(keep_u32)) = (
                        request.exhaustive_scout_sims,
                        request.exhaustive_scout_top_keep,
                    ) {
                        let n = outcome.ranked.len();
                        let entry = optimize_history::build_entry_from_ranked_exhaustive_two_phase(
                            sims,
                            seed,
                            n,
                            scout_u32 as usize,
                            keep_u32 as usize,
                            optimize_history::EXHAUSTIVE_CONFIRM_POLICY_WIDTH_V1,
                            &chain_grind,
                            request.tiered_confirm_budget_cap_mult.map(|x| x as f32),
                            &outcome.ranked,
                        );
                        optimize_history_wrote |=
                            optimize_history::upsert_entry(pid, key.as_str(), entry).is_ok();
                    }
                }
            }
        }
        let pf = outcome.analytical_prefilter;
        all_results.extend(
            outcome
                .ranked
                .into_iter()
                .map(ranked_crew_to_simulation_result),
        );
        info!(
            strategy = optimizer_strategy_to_api_label(strategy),
            optimize_history_confirm_hits,
            optimize_history_wrote,
            ranked_results = all_results.len() as u64,
            "optimize_main_phase_complete"
        );
        pf
    } else {
        None
    };

    let meta = OptimizeGatherMeta {
        strategy,
        strategy_auto,
        is_seeded_genetic,
        heuristics_only,
        heuristics_seeds_nonempty,
        fast_discovery_no_resolved_crews,
        fast_discovery,
        fast_discovery_heuristic_cap_hit,
        using_placeholder_combatants,
        analytical_prefilter,
        below_decks_slots,
        optimize_constraints: summarize_constraints(crew_constraints.as_ref()),
        optimize_history_confirm_hits,
        optimize_history_wrote,
        tiered_scout_budget: tiered_scout_budget_for_response,
        exhaustive_adaptive_budget: exhaustive_adaptive_budget_for_response,
        dropped_illegal_warm_start: warm_legality.dropped_wrong_shape
            + warm_legality.dropped_duplicates
            + warm_legality.dropped_seat_incompatible,
        dropped_illegal_heuristics: h_legality.dropped_wrong_shape
            + h_legality.dropped_duplicates
            + h_legality.dropped_seat_incompatible,
        dropped_illegal_prior_refs,
        roster_filter_warning: roster_import_fallback_warning_message(profile_id),
    };
    info!(
        effective_strategy = optimizer_strategy_to_api_label(meta.strategy),
        strategy_auto = meta.strategy_auto,
        heuristics_only = meta.heuristics_only,
        analytical_prefilter_applied = meta.analytical_prefilter.is_some(),
        optimize_history_confirm_hits = meta.optimize_history_confirm_hits,
        optimize_history_wrote = meta.optimize_history_wrote,
        final_result_count = all_results.len() as u64,
        "optimize_gather_complete"
    );

    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    maybe_append_row(
        profile_id,
        &BudgetTelemetryRow {
            ts_ms,
            ship: request.ship.as_str(),
            hostile: request.hostile.as_str(),
            strategy: optimizer_strategy_to_api_label(meta.strategy),
            result_crews: all_results.len(),
            tiered_scout_trials_final: meta
                .tiered_scout_budget
                .as_ref()
                .map(|b| b.scout_trials_final),
            tiered_confirm_trials_total: meta
                .tiered_scout_budget
                .as_ref()
                .map(|b| b.confirm_trials_total),
            exhaustive_scout_trials_final: meta
                .exhaustive_adaptive_budget
                .as_ref()
                .map(|b| b.scout_trials_final),
            exhaustive_confirm_trials_total: meta
                .exhaustive_adaptive_budget
                .as_ref()
                .map(|b| b.confirm_trials_total),
            optimize_history_confirm_hits: meta.optimize_history_confirm_hits,
            optimize_history_wrote: meta.optimize_history_wrote,
        },
    );

    Ok((all_results, meta))
}

fn build_optimize_response(
    request: &OptimizeRequest,
    all_results: Vec<SimulationResult>,
    duration_ms: u64,
    meta: &OptimizeGatherMeta,
) -> OptimizeResponse {
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    let seed = request.seed.unwrap_or(0);
    let mut ranked_results = rank_results(all_results);
    if request.novelty_lambda.is_some() {
        ranked_results = apply_novelty_mmr_if_configured(
            ranked_results,
            request.novelty_lambda,
            request.novelty_diverse_top.map(|n| n as usize),
            request.novelty_pool.map(|n| n as usize),
        );
    }

    let engine = if meta.heuristics_only {
        "heuristics"
    } else if meta.is_seeded_genetic {
        "seeded_genetic"
    } else {
        match meta.strategy {
            OptimizerStrategy::Exhaustive => "optimizer_v1",
            OptimizerStrategy::Genetic => "genetic",
            OptimizerStrategy::Tiered => "tiered",
        }
    };
    let mut notes = vec![
        "Results are deterministic for the same ship, hostile, simulation count, and seed.",
        "Per-crew 95% intervals: Wilson score for win/stall/loss/R1-kill rates; normal approximation for mean hull score per trial (hull fraction on wins, 0 on losses).",
    ];
    if meta.is_seeded_genetic {
        notes.insert(0, "GA population seeded with heuristics crews.");
    } else if meta.fast_discovery {
        notes.insert(
            0,
            "Fast discovery: heuristic seed crews were merged into the main optimize warm-start list (approximate analytical rank, then Monte Carlo with your chosen strategy).",
        );
    } else if meta.heuristics_seeds_nonempty {
        notes.insert(0, "Heuristics crews were evaluated first.");
    }
    if request.chain.as_ref().map(|c| c.enabled).unwrap_or(false) {
        notes.push(
            "Chain grind: attacker hull carries between fights; shields reset to full each fight. win_rate is P(completing the chain); avg_hull_remaining is the secondary mean given success.",
        );
    }

    let mut approximate_notes = Vec::new();
    if meta.fast_discovery {
        approximate_notes.push(
            "fast_discovery: seed template crews skip the standalone all-seed Monte Carlo pass; they compete with generated candidates through the same approximate rank then Monte Carlo path."
                .to_string(),
        );
    }
    if let Some((generated, kept)) = meta.analytical_prefilter {
        approximate_notes.push(format!(
            "Approximate analytical pre-filter (closed-form expected hull damage to defender, not win rate) kept {kept} of {generated} crews before Monte Carlo."
        ));
    } else if request.analytical_prefilter_keep.is_some()
        && matches!(meta.strategy, OptimizerStrategy::Genetic)
    {
        approximate_notes.push(
            "analytical_prefilter_keep was ignored because the genetic strategy builds its own population."
                .to_string(),
        );
    }
    if request.chain.as_ref().map(|c| c.enabled).unwrap_or(false)
        && request.analytical_prefilter_keep.is_some()
        && !matches!(meta.strategy, OptimizerStrategy::Genetic)
    {
        approximate_notes
            .push("analytical_prefilter_keep was skipped for chain grind mode.".to_string());
    }
    if meta.strategy_auto {
        approximate_notes.push(format!(
            "Optimizer strategy was chosen automatically (effective: {}); tiered is used when capped candidate count is at least {}.",
            optimizer_strategy_to_api_label(meta.strategy),
            TIERED_AUTO_THRESHOLD
        ));
    }
    if request.analytical_prefilter_keep.is_none()
        && meta.analytical_prefilter.is_some()
        && !meta.heuristics_only
        && !matches!(meta.strategy, OptimizerStrategy::Genetic)
        && !request.chain.as_ref().is_some_and(|c| c.enabled)
    {
        approximate_notes.push(
            "analytical_prefilter_keep was derived automatically from candidate count (closed-form hull-damage proxy, not win rate)."
                .to_string(),
        );
    }
    if request.novelty_lambda.is_some() {
        approximate_notes.push(
            "The leading recommendations use novelty-aware ordering (maximal marginal relevance on officer sets). Remaining rows stay in strength order."
                .to_string(),
        );
    }

    let mut warnings = Vec::new();
    if meta.fast_discovery_no_resolved_crews {
        warnings.push(
            "fast_discovery was requested but no heuristic crews resolved after filters; falling back to standard heuristic handling (if any) without warm-start merge."
                .to_string(),
        );
    }
    if meta.fast_discovery_heuristic_cap_hit {
        warnings.push(format!(
            "fast_discovery merged at most {FAST_DISCOVERY_MAX_HEURISTIC_WARM} heuristic-expanded crews into warm-start; additional seed combinations were truncated."
        ));
    }
    if meta.using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats. Results do not reflect real ship/hostile values."
                .to_string(),
        );
    }
    if let Some(message) = &meta.roster_filter_warning {
        warnings.push(message.clone());
    }
    let dropped_total = meta.dropped_illegal_warm_start
        + meta.dropped_illegal_heuristics
        + meta.dropped_illegal_prior_refs;
    if dropped_total > 0 {
        let mut segs: Vec<String> = Vec::new();
        if meta.dropped_illegal_warm_start > 0 {
            segs.push(format!("warm-start: {}", meta.dropped_illegal_warm_start));
        }
        if meta.dropped_illegal_heuristics > 0 {
            segs.push(format!("heuristics: {}", meta.dropped_illegal_heuristics));
        }
        if meta.dropped_illegal_prior_refs > 0 {
            segs.push(format!(
                "optimize-history priors: {}",
                meta.dropped_illegal_prior_refs
            ));
        }
        warnings.push(format!(
            "Ignored {dropped_total} injected crew(s) not roster/seat legal ({}).",
            segs.join("; ")
        ));
    }

    OptimizeResponse {
        status: "ok",
        engine,
        scenario: ScenarioSummary {
            ship: request.ship.clone(),
            hostile: request.hostile.clone(),
            sims,
            seed,
            below_decks_slots: meta.below_decks_slots as u32,
            optimize_constraints: meta.optimize_constraints.clone(),
            analytical_prefilter_keep: request.analytical_prefilter_keep,
            analytical_prefilter_from: meta.analytical_prefilter.map(|(g, _)| g as u32),
            analytical_prefilter_kept: meta.analytical_prefilter.map(|(_, k)| k as u32),
            chain: request.chain.clone(),
            effective_strategy: optimizer_strategy_to_api_label(meta.strategy).to_string(),
            strategy_auto: meta.strategy_auto,
            requested_strategy: request.strategy.clone(),
            novelty_lambda: request.novelty_lambda,
            novelty_diverse_top: request.novelty_diverse_top,
            novelty_pool: request.novelty_pool,
            fast_discovery: meta.fast_discovery.then_some(true),
            optimize_history_confirm_hits: (meta.optimize_history_confirm_hits > 0)
                .then_some(meta.optimize_history_confirm_hits),
            optimize_history_wrote: meta.optimize_history_wrote.then_some(true),
            tiered_scout_budget: meta.tiered_scout_budget,
            exhaustive_adaptive_budget: meta.exhaustive_adaptive_budget,
        },
        recommendations: ranked_results
            .iter()
            .map(crew_recommendation_from_ranked)
            .collect(),
        duration_ms: Some(duration_ms),
        notes,
        approximate_notes,
        warnings,
    }
}

/// Run optimization (assumes request already validated). Returns response or serialization error.
pub fn run_optimize(
    registry: &DataRegistry,
    request: &OptimizeRequest,
    profile_id: Option<&str>,
) -> Result<OptimizeResponse, OptimizePayloadError> {
    let seed = request.seed.unwrap_or(0);
    let span = info_span!(
        "optimize_sync_run",
        ship = %request.ship,
        hostile = %request.hostile,
        seed,
        requested_strategy = request.strategy.as_deref().unwrap_or("auto"),
        profile_id_present = profile_id.is_some()
    );
    let _span_guard = span.enter();
    let start = Instant::now();
    let mut sink = OptimizeProgressSink::None;
    let (all_results, meta) =
        gather_optimize_simulation_results(registry, request, profile_id, &mut sink)
            .expect("sync optimize does not cancel");
    let duration_ms = start.elapsed().as_millis() as u64;
    let response = build_optimize_response(request, all_results, duration_ms, &meta);
    info!(
        duration_ms,
        recommendations = response.recommendations.len() as u64,
        effective_strategy = %response.scenario.effective_strategy,
        strategy_auto = response.scenario.strategy_auto,
        "optimize_sync_completed"
    );
    Ok(response)
}

// --- Optimize job store (for progress polling) ---

#[derive(Debug, Clone)]
pub enum OptimizeJobStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct OptimizeJobState {
    pub status: OptimizeJobStatus,
    pub progress: u8,
    pub crews_done: u32,
    pub total_crews: u32,
    pub phase: Option<String>,
    pub progress_preview: Option<Vec<CrewRecommendation>>,
    pub result: Option<OptimizeResponse>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeStartResponse {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeStatusResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crews_done: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_crews: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// Best-effort crews/sec for phases where `crews_done` / `total_crews` are crew counts (not generations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_crews_per_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_preview: Option<Vec<CrewRecommendation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<OptimizeResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Cap on stored job records (running + finished). Oldest **completed** jobs are dropped first
/// when over limit so the global map cannot grow without bound.
const MAX_OPTIMIZE_JOBS_RETAINED: usize = 128;

static OPTIMIZE_JOB_COUNTER: OnceLock<AtomicU64> = OnceLock::new();
static OPTIMIZE_JOBS: OnceLock<Mutex<HashMap<String, OptimizeJobState>>> = OnceLock::new();
static OPTIMIZE_CANCEL_FLAGS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn optimize_jobs() -> &'static Mutex<HashMap<String, OptimizeJobState>> {
    OPTIMIZE_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn optimize_cancel_flags() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    OPTIMIZE_CANCEL_FLAGS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Lock the optimize job map. On a poisoned mutex (peer thread panicked while holding the lock),
/// recover the guard with `PoisonError::into_inner` so API handlers keep working instead of
/// panicking the process.
fn lock_jobs() -> MutexGuard<'static, HashMap<String, OptimizeJobState>> {
    optimize_jobs().lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_cancel_flags() -> MutexGuard<'static, HashMap<String, Arc<AtomicBool>>> {
    optimize_cancel_flags()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn next_job_id() -> String {
    let counter = OPTIMIZE_JOB_COUNTER.get_or_init(|| AtomicU64::new(0));
    let n = counter.fetch_add(1, Ordering::Relaxed);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("opt_{}_{}", ms, n)
}

/// Parse `opt_<millis>_<counter>` for eviction ordering (unknown shape → 0 = evicted first among ties).
fn parse_optimize_job_timestamp_ms(job_id: &str) -> u128 {
    job_id
        .strip_prefix("opt_")
        .and_then(|rest| rest.split('_').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Drop oldest finished jobs until `map.len() <= max_entries`. Running jobs are never removed.
fn prune_completed_optimize_jobs_over_cap(
    map: &mut HashMap<String, OptimizeJobState>,
    cancel_flags: &mut HashMap<String, Arc<AtomicBool>>,
    max_entries: usize,
) {
    while map.len() > max_entries {
        let Some(oldest_id) = map
            .iter()
            .filter(|(_, st)| {
                matches!(
                    st.status,
                    OptimizeJobStatus::Done | OptimizeJobStatus::Error
                )
            })
            .map(|(id, _)| (parse_optimize_job_timestamp_ms(id), id.clone()))
            .min_by(|(a_ts, a_id), (b_ts, b_id)| a_ts.cmp(b_ts).then_with(|| a_id.cmp(b_id)))
            .map(|(_, id)| id)
        else {
            break;
        };
        map.remove(&oldest_id);
        cancel_flags.remove(&oldest_id);
    }
}

#[derive(Debug)]
pub enum OptimizeStatusError {
    NotFound,
    Serialize(serde_json::Error),
}

impl std::fmt::Display for OptimizeStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Job not found"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OptimizeStatusError {}

/// Start an optimize job in the background; returns job_id. Caller must have validated request.
/// `cpu_permit` is held until the job thread finishes so `/api/optimize/start` shares the same
/// CPU concurrency budget as `/api/optimize` and `/api/simulate`.
pub fn start_optimize_job(
    registry: Arc<DataRegistry>,
    request: OptimizeRequest,
    profile_id: Option<&str>,
    cpu_permit: OwnedSemaphorePermit,
) -> Result<OptimizeStartResponse, OptimizePayloadError> {
    let job_id = next_job_id();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let heuristics_seeds_nonempty = request
        .heuristics_seeds
        .as_ref()
        .is_some_and(|s| !s.is_empty());
    info!(
        job_id = %job_id,
        ship = %request.ship,
        hostile = %request.hostile,
        seed = request.seed.unwrap_or(0),
        requested_strategy = request.strategy.as_deref().unwrap_or("auto"),
        heuristics_seeds_nonempty,
        profile_id_present = profile_id.is_some(),
        "optimize_job_started"
    );

    {
        let mut map = lock_jobs();
        map.insert(
            job_id.clone(),
            OptimizeJobState {
                status: OptimizeJobStatus::Running,
                progress: 0,
                crews_done: 0,
                total_crews: 0,
                phase: None,
                progress_preview: None,
                result: None,
                error: None,
            },
        );
        let mut cancel_flags = lock_cancel_flags();
        cancel_flags.insert(job_id.clone(), cancel_flag.clone());
        prune_completed_optimize_jobs_over_cap(
            &mut map,
            &mut cancel_flags,
            MAX_OPTIMIZE_JOBS_RETAINED,
        );
    }

    let job_id_thread = job_id.clone();
    let profile_owned = profile_id.map(String::from);

    std::thread::spawn(move || {
        let job_span = info_span!(
            "optimize_job_run",
            job_id = %job_id_thread,
            ship = %request.ship,
            hostile = %request.hostile,
            seed = request.seed.unwrap_or(0),
            requested_strategy = request.strategy.as_deref().unwrap_or("auto"),
            profile_id_present = profile_owned.is_some()
        );
        let _job_span = job_span.enter();
        let _cpu_permit = cpu_permit;
        let start = Instant::now();
        let mut sink = OptimizeProgressSink::Job {
            job_id: job_id_thread.clone(),
            cancel: cancel_flag.clone(),
            heuristics_seeds_nonempty,
            is_seeded_genetic: false,
            skip_heuristic_standalone_mc: false,
        };
        let gather = gather_optimize_simulation_results(
            registry.as_ref(),
            &request,
            profile_owned.as_deref(),
            &mut sink,
        );

        match gather {
            Ok((all_results, meta)) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let response = build_optimize_response(&request, all_results, duration_ms, &meta);
                info!(
                    job_id = %job_id_thread,
                    duration_ms,
                    recommendations = response.recommendations.len() as u64,
                    effective_strategy = %response.scenario.effective_strategy,
                    strategy_auto = response.scenario.strategy_auto,
                    "optimize_job_completed"
                );
                let mut map = lock_jobs();
                if let Some(state) = map.get_mut(&job_id_thread) {
                    state.status = OptimizeJobStatus::Done;
                    state.progress = 100;
                    state.phase = None;
                    state.progress_preview = None;
                    state.result = Some(response);
                }
            }
            Err(()) => {
                warn!(job_id = %job_id_thread, "optimize_job_cancelled");
                let mut map = lock_jobs();
                if let Some(state) = map.get_mut(&job_id_thread) {
                    state.status = OptimizeJobStatus::Error;
                    state.error = Some("Cancelled".to_string());
                }
            }
        }
        lock_cancel_flags().remove(&job_id_thread);
        info!(job_id = %job_id_thread, "optimize_job_cleanup");
    });

    Ok(OptimizeStartResponse { job_id })
}

pub fn get_job_status(job_id: &str) -> Result<OptimizeStatusResponse, OptimizeStatusError> {
    let map = lock_jobs();
    let state = map.get(job_id).ok_or(OptimizeStatusError::NotFound)?;
    let status = match &state.status {
        OptimizeJobStatus::Running => "running",
        OptimizeJobStatus::Done => "done",
        OptimizeJobStatus::Error => "error",
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let started_ms = parse_optimize_job_timestamp_ms(job_id);
    let elapsed_s = ((now_ms.saturating_sub(started_ms)) as f64) / 1000.0;
    let crew_like_phase = state.phase.as_deref().is_none_or(|p| {
        matches!(
            p,
            "heuristics"
                | "monte_carlo"
                | "tiered_scout"
                | "tiered_scout_refine"
                | "tiered_confirm"
                | "exhaustive_scout"
                | "exhaustive_confirm"
        )
    });
    let (throughput_crews_per_sec, eta_seconds) =
        if matches!(state.status, OptimizeJobStatus::Running)
            && crew_like_phase
            && elapsed_s > 0.05
            && state.crews_done > 0
            && state.total_crews > state.crews_done
        {
            let tp = state.crews_done as f64 / elapsed_s;
            let remaining = (state.total_crews - state.crews_done) as f64;
            let eta = if tp > 1e-6 {
                Some((remaining / tp).ceil().max(0.0) as u64)
            } else {
                None
            };
            (Some(tp), eta)
        } else {
            (None, None)
        };
    Ok(OptimizeStatusResponse {
        status: status.to_string(),
        progress: Some(state.progress),
        crews_done: Some(state.crews_done),
        total_crews: Some(state.total_crews),
        phase: state.phase.clone(),
        throughput_crews_per_sec,
        eta_seconds,
        progress_preview: state.progress_preview.clone(),
        result: state.result.clone(),
        error: state.error.clone(),
    })
}

pub fn cancel_job(job_id: &str) -> Result<(), OptimizeStatusError> {
    let flag = {
        let flags = lock_cancel_flags();
        flags
            .get(job_id)
            .cloned()
            .ok_or(OptimizeStatusError::NotFound)?
    };
    flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod optimize_job_store_tests {
    use super::*;

    fn done_state() -> OptimizeJobState {
        OptimizeJobState {
            status: OptimizeJobStatus::Done,
            progress: 100,
            crews_done: 1,
            total_crews: 1,
            phase: None,
            progress_preview: None,
            result: None,
            error: None,
        }
    }

    #[test]
    fn parse_job_timestamp_reads_opt_prefix() {
        assert_eq!(
            parse_optimize_job_timestamp_ms("opt_1700000000123_0"),
            1700000000123
        );
        assert_eq!(parse_optimize_job_timestamp_ms("opt_99_7"), 99);
        assert_eq!(parse_optimize_job_timestamp_ms("bad"), 0);
    }

    #[test]
    fn prune_drops_oldest_completed_first() {
        let mut map = HashMap::new();
        let mut flags = HashMap::new();
        map.insert("opt_100_0".to_string(), done_state());
        map.insert("opt_200_1".to_string(), done_state());
        map.insert("opt_300_2".to_string(), done_state());
        map.insert(
            "opt_400_run".to_string(),
            OptimizeJobState {
                status: OptimizeJobStatus::Running,
                progress: 0,
                crews_done: 0,
                total_crews: 0,
                phase: None,
                progress_preview: None,
                result: None,
                error: None,
            },
        );
        flags.insert("opt_100_0".to_string(), Arc::new(AtomicBool::new(false)));
        flags.insert("opt_200_1".to_string(), Arc::new(AtomicBool::new(false)));
        flags.insert("opt_300_2".to_string(), Arc::new(AtomicBool::new(false)));
        flags.insert("opt_400_run".to_string(), Arc::new(AtomicBool::new(false)));

        prune_completed_optimize_jobs_over_cap(&mut map, &mut flags, 2);
        assert_eq!(map.len(), 2);
        assert!(!map.contains_key("opt_100_0"));
        assert!(!map.contains_key("opt_200_1"));
        assert!(map.contains_key("opt_300_2"));
        assert!(map.contains_key("opt_400_run"));
    }
}
