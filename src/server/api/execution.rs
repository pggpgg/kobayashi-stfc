//! Execution layer: run optimize, job store, and response types.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::OwnedSemaphorePermit;
use tracing::{info, info_span, warn};

use crate::data::budget_telemetry::{maybe_append_row, BudgetTelemetryRow};
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::{
    expand_crews, filter_heuristic_seed_crews, load_seed_file, BelowDecksStrategy,
    ParsedHeuristicsCrew, DEFAULT_HEURISTICS_DIR,
};
use crate::data::import::{load_imported_roster, roster_import_fallback_warning_message};
use crate::data::officer::normalize_officer_lookup_key;
use crate::data::optimize_history;
use crate::data::optimize_observations::{
    maybe_append_rows as maybe_append_observation_rows, stable_text_hash, OptimizeObservationRow,
};
use crate::data::support_buffs;
use crate::lcars::LcarsOfficer;
use crate::optimizer::constraints::{filter_candidates, CrewSearchConstraints};
use crate::optimizer::crew_generator::{
    build_officer_pools_from_registry, resolve_below_decks_slots_for_ship,
    search_space_reduction_report, CandidateStrategy, CrewCandidate, BRIDGE_SLOTS,
};
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_with_shared,
    scenario::{build_shared_scenario_data_from_registry, DefenderOpponent},
    SimulationResult,
};
use crate::optimizer::ranking::{apply_novelty_mmr_if_configured, rank_results, RankedCrewResult};
use crate::optimizer::tiered::TieredScoutBudgetStats;
use crate::optimizer::{
    count_effective_optimize_candidates, enforce_candidate_optimization_eligibility_with_registry,
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizeCandidateFunnel,
    OptimizeProgressTick, OptimizerStrategy,
};
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};

use super::requests::{
    below_decks_pool_mode_resolved, build_crew_search_constraints, chain_grind_params_from_request,
    parse_below_decks_strategy, parse_strategy, relax_below_decks_combat_strictness,
    ChainGrindRequest, OptimizePayloadError, OptimizeRequest, ValidationErrorResponse,
    ValidationIssue, DEFAULT_SIMS,
};

#[derive(Debug)]
enum OptimizeGatherError {
    Cancelled { phase: Option<String> },
    Validation(ValidationErrorResponse),
}

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
        OptimizerStrategy::LinearEval => "linear_eval",
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

/// Build a guaranteed "proven" warm-start crew so candidate generation always *reaches* a strong
/// below-decks lineup — a low `max_candidates` over a many-slot ship (or a tightened eligibility
/// pool) can otherwise fail to generate any viable crew even when one exists.
///
/// Uses the same `Relaxed`-mode, scenario- and roster-filtered pools as legality enforcement (so the
/// crew is eligible by construction), honoring `constraints`. The below-decks pool is curated-first
/// (see `data/optimizer/below_decks_priority.txt`), so this picks the strongest eligible captain, two
/// distinct bridge officers, and the top curated below-decks officers. Returns `None` when no legal
/// crew can be formed (e.g. `below_decks_slots == 0` or too few eligible officers) — behavior is then
/// identical to today. The returned crew is prepended to the warm-start set and revalidated/deduped
/// downstream like any other warm-start crew.
fn curated_proven_warm_start_crew(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    enemy_type: crate::combat::EnemyType,
    constraints: Option<&CrewSearchConstraints>,
) -> Option<CrewCandidate> {
    if below_decks_slots == 0 {
        return None;
    }
    let pools = build_officer_pools_from_registry(
        registry,
        crate::data::heuristics::BelowDecksPoolMode::Relaxed,
        Some(enemy_type),
        profile_id,
        below_decks_slots,
        constraints,
    )?;
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut take_distinct = |pool: &[String], n: usize| -> Vec<String> {
        let mut out = Vec::with_capacity(n);
        for name in pool {
            if out.len() == n {
                break;
            }
            let key = name.trim().to_ascii_lowercase();
            if key.is_empty() || used.contains(&key) {
                continue;
            }
            used.insert(key);
            out.push(name.clone());
        }
        out
    };
    let captain = take_distinct(&pools.captains, 1).pop()?;
    let bridge = take_distinct(&pools.bridge, BRIDGE_SLOTS);
    if bridge.len() != BRIDGE_SLOTS {
        return None;
    }
    let below_decks = take_distinct(&pools.below_decks, below_decks_slots);
    if below_decks.len() != below_decks_slots {
        return None;
    }
    Some(CrewCandidate {
        captain,
        bridge,
        below_decks,
    })
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

/// Resolve the combat scenario for eligibility filtering from a request: explicit `enemy_type`
/// wins; else PvP → PvpSpace; else infer from the (cached) hostile record; else RedMovingSpace.
/// The hostile is only resolved when inference is actually needed.
fn resolve_request_enemy_type(
    registry: &DataRegistry,
    request: &OptimizeRequest,
) -> crate::combat::EnemyType {
    let explicit = request.enemy_type.as_deref();
    let pvp = request
        .defender_ship
        .as_deref()
        .is_some_and(|ship| !ship.trim().is_empty());
    let infer_from_hostile = !pvp
        && explicit
            .and_then(crate::data::officer_eligibility::enemy_type_from_str)
            .is_none();
    let hostile = if infer_from_hostile {
        registry.resolve_hostile(request.hostile.trim())
    } else {
        None
    };
    crate::data::officer_eligibility::resolve_enemy_type(explicit, pvp, hostile.as_ref())
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
        below_decks_pool_mode: below_decks_pool_mode_resolved(request),
        pvp_mode: request
            .defender_ship
            .as_deref()
            .is_some_and(|ship| !ship.trim().is_empty()),
        enemy_type: resolve_request_enemy_type(registry, request),
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

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct CrewRecommendation {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    /// Method/source path that produced or injected this row (e.g. `exhaustive_mc`, `warm_start`).
    pub method_provenance: String,
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
    /// Closed-form expected hull damage when ranked by linear eval (no Monte Carlo).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_hull_damage: Option<f64>,
}

/// Counts-only echo of active optimize constraints (for clients / debugging).
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct OptimizeConstraintsSummary {
    pub must_include: usize,
    pub exclude: usize,
    pub groups: usize,
    pub captain_must_be: bool,
    pub bridge_must_include: usize,
    pub below_decks_must_include: usize,
}

fn crew_recommendation_from_ranked(
    r: &RankedCrewResult,
    method_provenance: impl Into<String>,
) -> CrewRecommendation {
    CrewRecommendation {
        captain: r.captain.clone(),
        bridge: r.bridge.clone(),
        below_decks: r.below_decks.clone(),
        method_provenance: method_provenance.into(),
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
        expected_hull_damage: r.expected_hull_damage,
    }
}

fn progress_phase_method_provenance(phase: &str) -> &'static str {
    match phase {
        "heuristics" => "heuristics",
        "genetic" => "genetic",
        "tiered_scout" | "tiered_scout_refine" | "tiered_confirm" => "tiered_confirmed",
        "exhaustive_scout" | "exhaustive_confirm" => "exhaustive_two_phase",
        "linear_eval" => "linear_eval",
        _ => "monte_carlo",
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

#[derive(Debug, Clone, Default, Serialize, schemars::JsonSchema)]
pub struct OptimizerRolePoolTelemetry {
    pub captains: u32,
    pub bridge: u32,
    pub below_decks: u32,
}

impl OptimizerRolePoolTelemetry {
    fn new(captains: usize, bridge: usize, below_decks: usize) -> Self {
        Self {
            captains: telemetry_count(captains),
            bridge: telemetry_count(bridge),
            below_decks: telemetry_count(below_decks),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, schemars::JsonSchema)]
pub struct OptimizerFunnelTelemetry {
    /// Full-catalog raw role pools before ban/eligibility filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_role_pool: Option<OptimizerRolePoolTelemetry>,
    /// Full-catalog role pools after the curation ban list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banned_role_pool: Option<OptimizerRolePoolTelemetry>,
    /// Full-catalog role pools after ban/eligibility filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligible_role_pool: Option<OptimizerRolePoolTelemetry>,
    /// Production role pools after roster/profile narrowing, before explicit constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roster_role_pool: Option<OptimizerRolePoolTelemetry>,
    /// Production role pools after roster/profile narrowing and explicit constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_role_pool: Option<OptimizerRolePoolTelemetry>,
    /// Heuristic-expanded candidates after roster/seat legality and explicit constraints.
    pub heuristic_candidates: u32,
    /// Warm-start candidates sent into the main optimizer scenario (including fast-discovery merges).
    pub warm_start_candidates: u32,
    /// Raw generated candidates before warm-start merge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_candidates: Option<u32>,
    /// Candidate count after warm-start prepend and stable-hash dedupe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_warm_start_dedupe: Option<u32>,
    /// Candidate count after explicit optimize constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_constraints: Option<u32>,
    /// Analytical prefilter input count when truncation ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytical_prefilter_from: Option<u32>,
    /// Analytical prefilter kept count when truncation ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytical_prefilter_kept: Option<u32>,
    /// Candidate count entering scout / cheap-evaluation phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scout_candidates: Option<u32>,
    /// Candidate count entering confirmation or final ranking output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_candidates: Option<u32>,
    /// Rows returned after standalone heuristic and/or main optimizer results are merged.
    pub final_result_count: u32,
    /// Coarse wall-clock time spent in each optimize phase, milliseconds.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub phase_durations_ms: BTreeMap<String, u64>,
    /// Phase observed when a background optimize job was cancelled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_point: Option<String>,
}

fn telemetry_count(n: usize) -> u32 {
    n.min(u32::MAX as usize) as u32
}

fn telemetry_optional_count(n: Option<usize>) -> Option<u32> {
    n.map(telemetry_count)
}

impl OptimizerFunnelTelemetry {
    fn new(heuristic_candidates: usize, warm_start_candidates: usize) -> Self {
        Self {
            heuristic_candidates: telemetry_count(heuristic_candidates),
            warm_start_candidates: telemetry_count(warm_start_candidates),
            ..Self::default()
        }
    }

    fn apply_optimizer_funnel(&mut self, funnel: OptimizeCandidateFunnel) {
        self.warm_start_candidates = telemetry_count(funnel.warm_start_candidates);
        self.generated_candidates = telemetry_optional_count(funnel.generated_candidates);
        self.after_warm_start_dedupe = telemetry_optional_count(funnel.after_warm_start_dedupe);
        self.after_constraints = telemetry_optional_count(funnel.after_constraints);
        self.analytical_prefilter_from = telemetry_optional_count(funnel.analytical_prefilter_from);
        self.analytical_prefilter_kept = telemetry_optional_count(funnel.analytical_prefilter_kept);
        self.scout_candidates = telemetry_optional_count(funnel.scout_candidates);
        self.confirmed_candidates = telemetry_optional_count(funnel.confirmed_candidates);
    }

    fn apply_phase_durations(&mut self, phase_durations_ms: BTreeMap<String, u64>) {
        self.phase_durations_ms = phase_durations_ms;
    }
}

fn elapsed_ms_since(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn record_phase_duration(
    phase_durations_ms: &mut BTreeMap<String, u64>,
    phase: &'static str,
    start: Instant,
) {
    let elapsed = elapsed_ms_since(start);
    phase_durations_ms
        .entry(phase.to_string())
        .and_modify(|n| *n = n.saturating_add(elapsed))
        .or_insert(elapsed);
}

fn candidate_hash_set(crews: &[CrewCandidate]) -> HashSet<u64> {
    crews.iter().map(crew_candidate_stable_hash).collect()
}

fn ranked_crew_hash(r: &RankedCrewResult) -> u64 {
    crew_candidate_stable_hash(&CrewCandidate {
        captain: r.captain.clone(),
        bridge: r.bridge.clone(),
        below_decks: r.below_decks.clone(),
    })
}

fn recommendation_method_provenance(
    meta: &OptimizeGatherMeta,
    r: &RankedCrewResult,
) -> &'static str {
    if matches!(meta.strategy, OptimizerStrategy::LinearEval) {
        return "linear_eval";
    }
    let h = ranked_crew_hash(r);
    if meta.heuristic_hashes.contains(&h) {
        return if meta.fast_discovery || meta.is_seeded_genetic {
            "heuristic_seed"
        } else {
            "heuristics"
        };
    }
    if meta.curated_warm_start_hashes.contains(&h) {
        return "curated_warm_start";
    }
    if meta.warm_start_hashes.contains(&h) {
        return "warm_start";
    }
    if meta.heuristics_only {
        return "heuristics";
    }
    if meta.is_seeded_genetic {
        return "seeded_genetic";
    }
    match meta.strategy {
        OptimizerStrategy::Exhaustive if meta.exhaustive_adaptive_budget.is_some() => {
            "exhaustive_two_phase"
        }
        OptimizerStrategy::Exhaustive => "exhaustive_mc",
        OptimizerStrategy::Genetic => "genetic",
        OptimizerStrategy::Tiered => "tiered_confirmed",
        OptimizerStrategy::LinearEval => "linear_eval",
    }
}

fn non_empty_string_slice(value: Option<&Vec<String>>) -> Option<&[String]> {
    value.map(Vec::as_slice).filter(|slice| !slice.is_empty())
}

fn append_optimize_observations(
    request: &OptimizeRequest,
    meta: &OptimizeGatherMeta,
    ranked_results: &[RankedCrewResult],
    profile_id: Option<&str>,
    sims: u32,
    seed: u64,
) {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let strategy = optimizer_strategy_to_api_label(meta.strategy);
    let profile_hash = profile_id.map(stable_text_hash);
    let chain_enabled = request.chain.as_ref().is_some_and(|chain| chain.enabled);
    let chain_kills_target = request.chain.as_ref().and_then(|chain| chain.kills_target);
    let support_buffs = non_empty_string_slice(request.support_buffs.as_ref());
    let defender_support_buffs = non_empty_string_slice(request.defender_support_buffs.as_ref());
    let defender_alliance_debuffs =
        non_empty_string_slice(request.defender_alliance_debuffs.as_ref());
    let rows: Vec<OptimizeObservationRow<'_>> = ranked_results
        .iter()
        .map(|r| OptimizeObservationRow {
            schema_version: 1,
            ts_ms,
            profile_id,
            profile_hash,
            simulator_version: env!("CARGO_PKG_VERSION"),
            ship: request.ship.as_str(),
            hostile: request.hostile.as_str(),
            ship_tier: request.ship_tier,
            ship_level: request.ship_level,
            below_decks_slots: meta.below_decks_slots as u32,
            enemy_type: request.enemy_type.as_deref(),
            support_buffs,
            defender_support_buffs,
            defender_alliance_debuffs,
            chain_enabled,
            chain_kills_target,
            seed,
            sims_requested: sims,
            trials_run: r.trials_run,
            strategy,
            method_provenance: recommendation_method_provenance(meta, r),
            crew_hash: ranked_crew_hash(r),
            captain: r.captain.as_str(),
            bridge: &r.bridge,
            below_decks: &r.below_decks,
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
            score: r.score.value,
            expected_hull_damage: r.expected_hull_damage,
        })
        .collect();
    maybe_append_observation_rows(profile_id, &rows);
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
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
    /// Candidate-count funnel for this optimize run.
    pub optimizer_funnel: OptimizerFunnelTelemetry,
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
    /// When true, history-backed redundancy anchors were requested for novelty MMR (see `approximate_notes`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub novelty_history_anchors: Option<bool>,
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

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
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

/// When relaxed below-decks mode is on, sort each seed line's `below_decks_candidates` by descending
/// LCARS Attack+Defense+Health at the roster tier/level when LCARS + roster data exist; otherwise no-op.
fn sort_heuristic_parsed_crews_below_decks_by_officer_power(
    crews: &mut [ParsedHeuristicsCrew],
    registry: &DataRegistry,
    profile_id: Option<&str>,
    officer_index: &HashMap<String, crate::data::officer::Officer>,
) {
    let Some(lcars_slice) = registry.lcars_officers() else {
        return;
    };
    let mut lcars_by_id: HashMap<&str, &LcarsOfficer> = HashMap::with_capacity(lcars_slice.len());
    for o in lcars_slice {
        lcars_by_id.entry(o.id.as_str()).or_insert(o);
    }
    let roster_path =
        crate::optimizer::crew_generator::roster_import_json_path_for_profile(profile_id);
    let roster_entries = load_imported_roster(&roster_path);

    for crew in crews.iter_mut() {
        if crew.below_decks_candidates.len() <= 1 {
            continue;
        }
        crew.below_decks_candidates.sort_by(|a, b| {
            let sum = |name: &str| -> f64 {
                let key = normalize_officer_lookup_key(name);
                let Some(off) = officer_index.get(&key) else {
                    return 0.0;
                };
                let Some(lo) = lcars_by_id.get(off.id.as_str()) else {
                    return 0.0;
                };
                let (rank, level) = roster_entries
                    .as_ref()
                    .and_then(|entries| {
                        entries
                            .iter()
                            .find(|e| e.canonical_officer_id == off.id)
                            .map(|e| (e.rank, e.level.map(|x| x as u32)))
                    })
                    .unwrap_or((None, None));
                let lvl = lo.resolve_level(level, rank).unwrap_or(1);
                lo.stats_at_level(lvl)
                    .map(|s| s.attack + s.defense + s.health)
                    .unwrap_or(0.0)
            };
            let pa = sum(a);
            let pb = sum(b);
            pb.partial_cmp(&pa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
    }
}

/// Load heuristics seeds and expand them into CrewCandidates.
pub fn load_heuristics_candidates(
    registry: &DataRegistry,
    seed_names: &[String],
    bd_strategy: BelowDecksStrategy,
    below_decks_slots: usize,
    relax_below_decks: bool,
    profile_id: Option<&str>,
) -> Vec<CrewCandidate> {
    let canonical_names: Vec<String> = registry.officers().iter().map(|o| o.name.clone()).collect();
    seed_names
        .iter()
        .flat_map(|name| {
            let parsed = load_seed_file(name, DEFAULT_HEURISTICS_DIR, Some(&canonical_names));
            let mut parsed =
                filter_heuristic_seed_crews(parsed, registry.officer_index(), !relax_below_decks);
            if relax_below_decks {
                sort_heuristic_parsed_crews_below_decks_by_officer_power(
                    &mut parsed,
                    registry,
                    profile_id,
                    registry.officer_index(),
                );
            }
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
    optimizer_funnel: OptimizerFunnelTelemetry,
    heuristic_hashes: HashSet<u64>,
    warm_start_hashes: HashSet<u64>,
    curated_warm_start_hashes: HashSet<u64>,
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
    /// Display names for support buffs whose direct static bonuses are inactive (NPC defender).
    defender_static_support_inactive_labels: Vec<String>,
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
        REGISTRY.with_state_mut(job_id, |state| {
            state.total_crews = h_total;
            state.phase = Some("heuristics".to_string());
        });
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
        REGISTRY.with_state_mut(job_id, |state| {
            state.crews_done = h_total;
            state.progress = if heuristics_only { 100 } else { 10 };
            let ranked = rank_results(results.to_vec());
            state.progress_preview = Some(
                ranked
                    .iter()
                    .take(5)
                    .map(|r| crew_recommendation_from_ranked(r, "heuristics"))
                    .collect(),
            );
        });
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
                REGISTRY.with_state_mut(job_id, |state| {
                    state.progress = progress;
                    state.crews_done = crews_done;
                    state.total_crews = total_crews;
                    state.phase = Some(tick.phase.to_string());
                    if let Some(partial) = tick.partial_top.as_ref() {
                        state.progress_preview = Some(
                            partial
                                .iter()
                                .take(5)
                                .map(|r| {
                                    crew_recommendation_from_ranked(
                                        r,
                                        progress_phase_method_provenance(tick.phase),
                                    )
                                })
                                .collect(),
                        );
                    }
                });
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

    fn current_phase(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Job { job_id, .. } => REGISTRY.get(job_id).and_then(|state| state.phase),
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
        expected_hull_damage: r.expected_hull_damage,
    }
}

fn cancelled_error(
    sink: &OptimizeProgressSink,
    fallback_phase: &'static str,
) -> OptimizeGatherError {
    let phase = sink
        .current_phase()
        .or_else(|| Some(fallback_phase.to_string()));
    warn!(
        cancellation_phase = phase.as_deref().unwrap_or("unknown"),
        "optimize_cancelled"
    );
    OptimizeGatherError::Cancelled { phase }
}

/// Shared Monte Carlo + optimizer scenario execution. Sync and background jobs use the same logic.
fn gather_optimize_simulation_results(
    registry: &DataRegistry,
    request: &OptimizeRequest,
    profile_id: Option<&str>,
    sink: &mut OptimizeProgressSink,
) -> Result<(Vec<SimulationResult>, OptimizeGatherMeta), OptimizeGatherError> {
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
    let gather_started = Instant::now();
    let prepare_started = Instant::now();
    let mut phase_durations_ms = BTreeMap::new();
    let below_decks_slots = resolve_below_decks_slots_for_ship(
        &request.ship,
        request.ship_tier,
        request.ship_level,
        request.below_decks_slots,
    );
    let crew_constraints = build_crew_search_constraints(request);
    // Resolve the combat scenario for eligibility filtering (explicit `enemy_type`, else inferred).
    let enemy_type = resolve_request_enemy_type(registry, request);
    let pool_mode = below_decks_pool_mode_resolved(request);
    let search_space_report =
        search_space_reduction_report(registry, enemy_type, pool_mode, below_decks_slots);
    let raw_stage = search_space_report.raw();
    let raw_role_pool = OptimizerRolePoolTelemetry::new(
        raw_stage.captains,
        raw_stage.bridge,
        raw_stage.below_decks,
    );
    let banned_stage = search_space_report
        .stages
        .get(1)
        .unwrap_or_else(|| search_space_report.final_stage());
    let banned_role_pool = OptimizerRolePoolTelemetry::new(
        banned_stage.captains,
        banned_stage.bridge,
        banned_stage.below_decks,
    );
    let eligible_stage = search_space_report.final_stage();
    let eligible_role_pool = OptimizerRolePoolTelemetry::new(
        eligible_stage.captains,
        eligible_stage.bridge,
        eligible_stage.below_decks,
    );
    let roster_role_pool = build_officer_pools_from_registry(
        registry,
        pool_mode,
        Some(enemy_type),
        profile_id,
        below_decks_slots,
        None,
    )
    .map(|pools| {
        OptimizerRolePoolTelemetry::new(
            pools.captains.len(),
            pools.bridge.len(),
            pools.below_decks.len(),
        )
    });
    let final_role_pool = build_officer_pools_from_registry(
        registry,
        pool_mode,
        Some(enemy_type),
        profile_id,
        below_decks_slots,
        crew_constraints.as_ref(),
    )
    .map(|pools| {
        OptimizerRolePoolTelemetry::new(
            pools.captains.len(),
            pools.bridge.len(),
            pools.below_decks.len(),
        )
    });
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

    let defender_static_support_inactive_labels = match (
        registry.support_buffs_catalog(),
        request.support_buffs.as_deref(),
    ) {
        (Some(cat), Some(sb))
            if !sb.is_empty() && !request.defender_opponent.defender_is_player_ship() =>
        {
            support_buffs::inactive_defender_static_support_buff_labels(cat, sb, false)
        }
        _ => Vec::new(),
    };

    let relax_bd = relax_below_decks_combat_strictness(request);
    let mut h_candidates = if heuristics_seeds_nonempty {
        load_heuristics_candidates(
            registry,
            heuristics_seeds,
            bd_strategy,
            below_decks_slots,
            relax_bd,
            profile_id,
        )
    } else {
        Vec::new()
    };
    let (h_candidates_legal, h_legality) = enforce_candidate_optimization_eligibility_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        enemy_type,
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
    let heuristic_hashes = candidate_hash_set(&h_candidates);

    let mut dto_warm = warm_start_crews_from_request_dtos(request);
    // Prepend a guaranteed proven-strong crew so candidate generation always reaches a viable
    // below-decks lineup (a low max_candidates over a many-slot ship can otherwise generate none).
    // The enforce call below validates/rejects it; it is deduped against generated crews downstream.
    let curated_warm_start = curated_proven_warm_start_crew(
        registry,
        profile_id,
        below_decks_slots,
        enemy_type,
        crew_constraints.as_ref(),
    );
    let curated_warm_start_hash = curated_warm_start.as_ref().map(crew_candidate_stable_hash);
    if let Some(crew) = curated_warm_start {
        dto_warm.insert(0, crew);
    }
    let (dto_warm, warm_legality) = enforce_candidate_optimization_eligibility_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        enemy_type,
        dto_warm,
    );
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
    let warm_start_hashes = candidate_hash_set(&scenario_warm_start);
    let curated_warm_start_hashes: HashSet<u64> = curated_warm_start_hash
        .into_iter()
        .filter(|h| warm_start_hashes.contains(h))
        .collect();
    let mut optimizer_funnel =
        OptimizerFunnelTelemetry::new(h_candidates.len(), scenario_warm_start.len());
    optimizer_funnel.raw_role_pool = Some(raw_role_pool);
    optimizer_funnel.banned_role_pool = Some(banned_role_pool);
    optimizer_funnel.eligible_role_pool = Some(eligible_role_pool);
    optimizer_funnel.roster_role_pool = roster_role_pool;
    optimizer_funnel.final_role_pool = final_role_pool;

    let prior_reference_crews_raw = match (profile_id, cache_key_normalized.as_deref()) {
        (Some(pid), Some(key)) => {
            optimize_history::prior_reference_crews_for_matchup_priors(pid, key, &chain_grind)
        }
        _ => Vec::new(),
    };
    let prior_refs_in = prior_reference_crews_raw.len();
    let (prior_reference_crews, prior_legality) =
        enforce_candidate_optimization_eligibility_with_registry(
            registry,
            profile_id,
            below_decks_slots,
            enemy_type,
            prior_reference_crews_raw,
        );
    let dropped_illegal_prior_refs = prior_legality.dropped_wrong_shape
        + prior_legality.dropped_duplicates
        + prior_legality.dropped_seat_incompatible;
    debug_assert_eq!(
        prior_refs_in,
        prior_reference_crews.len() + dropped_illegal_prior_refs
    );

    let mut learned_officer_scores = match profile_id {
        Some(pid) => {
            let scores = optimize_history::load_officer_scores(pid);
            if scores.is_empty() {
                None
            } else {
                Some(scores)
            }
        }
        _ => None,
    };

    // Auto-tune exploration parameters from learning signals (Phase B of the local
    // learning loop).  Only fills in values the user did not specify.
    let (auto_tuned_confirm_cap, auto_tuned_abandon_margin) =
        match (profile_id, cache_key_normalized.as_deref()) {
            (Some(pid), Some(key)) => {
                let file = optimize_history::load_history_file(pid);
                let signals = file
                    .entries
                    .get(key)
                    .map(|entry| {
                        crate::optimizer::learning_signals::compute_learning_signals(entry)
                    })
                    .unwrap_or_default();

                if !signals.has_data() {
                    (None, None)
                } else {
                    // Auto-tune confirm budget cap from stagnation:
                    //   high stagnation (>0.8) → shrink confirm budget (0.75, exploit)
                    //   low stagnation (<0.3)  → grow confirm budget (1.5, explore more)
                    let cap = if signals.captain_bridge_stagnation > 0.8 {
                        Some(0.75f64)
                    } else if signals.captain_bridge_stagnation < 0.3 {
                        Some(1.5)
                    } else {
                        Some(1.0)
                    };

                    // Auto-tune PQ abandon margin from top_margin:
                    //   large margin (>0.15) → tight abandon (0.02, clear winner)
                    //   small margin (<0.05) → loose abandon (0.10, tight race)
                    let abandon = if signals.top_margin > 0.15 {
                        Some(0.02f64)
                    } else if signals.top_margin < 0.05 {
                        Some(0.10)
                    } else {
                        None // keep default 0.05
                    };

                    // Auto-tune epsilon in officer scores from diversity:
                    //   low diversity (<0.3)  → higher epsilon (0.30, explore more)
                    //   high diversity (>0.7) → lower epsilon (0.10, exploit more)
                    if let Some(ref mut scores) = learned_officer_scores {
                        if signals.officer_diversity < 0.3 {
                            scores.set_epsilon(0.30);
                        } else if signals.officer_diversity > 0.7 {
                            scores.set_epsilon(0.10);
                        }
                        // else keep default 0.20
                    }

                    (cap, abandon)
                }
            }
            _ => (None, None),
        };

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

    let player_defender_officer_crew =
        match super::resolve_player_defender_officer_crew_for_optimize(
            registry,
            profile_id,
            below_decks_slots,
            request.defender_crew.as_ref(),
        ) {
            Ok(v) => v,
            Err(msg) => {
                return Err(OptimizeGatherError::Validation(ValidationErrorResponse {
                    status: "error",
                    message: "Validation failed",
                    errors: vec![ValidationIssue {
                        field: "defender_crew",
                        messages: vec![msg],
                    }],
                }));
            }
        };

    if matches!(strategy, OptimizerStrategy::Genetic) && player_defender_officer_crew.is_some() {
        return Err(OptimizeGatherError::Validation(ValidationErrorResponse {
            status: "error",
            message: "Validation failed",
            errors: vec![ValidationIssue {
                field: "defender_crew",
                messages: vec![
                    "defender_crew is not supported with strategy genetic (use tiered or exhaustive)"
                        .to_string(),
                ],
            }],
        }));
    }

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

    let pvp = crate::optimizer::monte_carlo::pvp_scenario_params_from_api_fields(
        request.defender_ship.as_deref(),
        request.defender_ship_tier,
        request.defender_ship_level,
        request.defender_profile_id.as_deref(),
    );
    let scenario_hostile: String = pvp
        .as_ref()
        .map(|p| p.defender_ship.clone())
        .unwrap_or_else(|| request.hostile.trim().to_string());
    let support_buff_request = support_buffs::SupportBuffScenarioRequest::from_api_options(
        request.support_buffs.as_deref(),
        request.defender_support_buffs.as_deref(),
        request.defender_alliance_debuffs.as_deref(),
    );
    let shared_scenario = build_shared_scenario_data_from_registry(
        registry,
        &request.ship,
        scenario_hostile.as_str(),
        request.ship_tier,
        request.ship_level,
        profile_id,
        support_buff_request,
        if pvp.is_some() {
            DefenderOpponent::Player
        } else {
            request.defender_opponent
        },
        player_defender_officer_crew.clone(),
        pvp.clone(),
    );
    let using_placeholder_combatants = shared_scenario.using_placeholder_combatants;
    record_phase_duration(&mut phase_durations_ms, "prepare", prepare_started);

    let mut all_results: Vec<SimulationResult> = if heuristics_seeds_nonempty
        && !is_seeded_genetic
        && !fast_discovery
        && strategy != OptimizerStrategy::LinearEval
    {
        let heuristics_started = Instant::now();
        let h_total = h_candidates.len() as u32;
        sink.on_heuristics_start(h_total);
        let h_len = h_candidates.len();
        let num_batches = monte_carlo_batch_count_for_candidates(h_len).max(1);
        let ranges = batch_ranges(h_len, num_batches);
        let mut results: Vec<SimulationResult> = Vec::with_capacity(h_len);
        for (start, end) in ranges {
            if sink.job_cancelled() {
                record_phase_duration(&mut phase_durations_ms, "heuristics", heuristics_started);
                return Err(cancelled_error(sink, "heuristics"));
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
        record_phase_duration(&mut phase_durations_ms, "heuristics", heuristics_started);
        info!(
            heuristic_results = results.len() as u64,
            "optimize_heuristics_monte_carlo_complete"
        );
        results
    } else {
        Vec::new()
    };

    let analytical_prefilter = if !heuristics_only {
        let optimizer_started = Instant::now();
        let optimizer_phase = optimizer_strategy_to_api_label(strategy);
        let scenario = OptimizationScenario {
            ship: &request.ship,
            hostile: scenario_hostile.as_str(),
            ship_tier: request.ship_tier,
            ship_level: request.ship_level,
            simulation_count: sims as usize,
            seed,
            max_candidates: request.max_candidates.map(|n| n as usize),
            strategy,
            below_decks_pool_mode: below_decks_pool_mode_resolved(request),
            seed_population: if is_seeded_genetic {
                h_candidates.clone()
            } else {
                Vec::new()
            },
            profile_id,
            tiered_scout_sims: request.tiered_scout_sims.map(|n| n as usize),
            tiered_top_k: request.tiered_top_k.map(|n| n as usize),
            tiered_scout_uniform: matches!(request.tiered_scout_uniform, Some(true)),
            tiered_confirm_budget_cap_mult: request
                .tiered_confirm_budget_cap_mult
                .or(auto_tuned_confirm_cap),
            tiered_scout_priority_queue: false,
            tiered_pq_minimal_scout: None,
            tiered_pq_selection_mult: None,
            tiered_pq_abandon_margin: auto_tuned_abandon_margin,
            exhaustive_scout_sims: request
                .exhaustive_scout_sims
                .map(|n| n as usize)
                .filter(|_| strategy == OptimizerStrategy::Exhaustive),
            exhaustive_scout_top_keep: request
                .exhaustive_scout_top_keep
                .map(|n| n as usize)
                .filter(|_| strategy == OptimizerStrategy::Exhaustive),
            analytical_prefilter_keep: request.analytical_prefilter_keep.map(|n| n as usize),
            prune_analytical_hull_fraction: None,
            prune_static_gate_max_fraction: None,
            below_decks_slots,
            constraints: crew_constraints.clone(),
            support_buffs: request.support_buffs.clone().unwrap_or_default(),
            defender_support_buffs: request.defender_support_buffs.clone(),
            defender_alliance_debuffs: request.defender_alliance_debuffs.clone(),
            chain_grind: chain_grind.clone(),
            defender_opponent: if pvp.is_some() {
                DefenderOpponent::Player
            } else {
                request.defender_opponent
            },
            player_defender_officer_crew: player_defender_officer_crew.clone(),
            pvp: pvp.clone(),
            enemy_type,
            warm_start: scenario_warm_start,
            prior_reference_crews,
            optimize_cache_key: cache_key_normalized.clone(),
            enable_learned_pair_prior: request.enable_learned_pair_prior.unwrap_or(true),
            learned_officer_scores,
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
            record_phase_duration(&mut phase_durations_ms, optimizer_phase, optimizer_started);
            return Err(cancelled_error(sink, optimizer_phase));
        }
        record_phase_duration(&mut phase_durations_ms, optimizer_phase, optimizer_started);
        optimizer_funnel.apply_optimizer_funnel(outcome.candidate_funnel);
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
        // Update and persist learned officer scores from the just-completed ranked results.
        if strategy != OptimizerStrategy::LinearEval {
            if let Some(pid) = profile_id {
                if !outcome.ranked.is_empty() {
                    let mut scores = optimize_history::load_officer_scores(pid);
                    scores.update_from_results(&outcome.ranked, &request.hostile, &request.ship);
                    let _ = optimize_history::save_officer_scores(pid, &scores);
                }
            }
        }
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
    optimizer_funnel.final_result_count = telemetry_count(all_results.len());
    record_phase_duration(&mut phase_durations_ms, "total", gather_started);
    optimizer_funnel.apply_phase_durations(phase_durations_ms);

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
        optimizer_funnel,
        heuristic_hashes,
        warm_start_hashes,
        curated_warm_start_hashes,
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
        defender_static_support_inactive_labels,
    };
    info!(
        effective_strategy = optimizer_strategy_to_api_label(meta.strategy),
        strategy_auto = meta.strategy_auto,
        heuristics_only = meta.heuristics_only,
        analytical_prefilter_applied = meta.analytical_prefilter.is_some(),
        raw_role_captains = meta.optimizer_funnel.raw_role_pool.as_ref().map(|p| p.captains).unwrap_or(0),
        raw_role_bridge = meta.optimizer_funnel.raw_role_pool.as_ref().map(|p| p.bridge).unwrap_or(0),
        raw_role_below_decks = meta.optimizer_funnel.raw_role_pool.as_ref().map(|p| p.below_decks).unwrap_or(0),
        banned_role_captains = meta.optimizer_funnel.banned_role_pool.as_ref().map(|p| p.captains).unwrap_or(0),
        banned_role_bridge = meta.optimizer_funnel.banned_role_pool.as_ref().map(|p| p.bridge).unwrap_or(0),
        banned_role_below_decks = meta.optimizer_funnel.banned_role_pool.as_ref().map(|p| p.below_decks).unwrap_or(0),
        eligible_role_captains = meta.optimizer_funnel.eligible_role_pool.as_ref().map(|p| p.captains).unwrap_or(0),
        eligible_role_bridge = meta.optimizer_funnel.eligible_role_pool.as_ref().map(|p| p.bridge).unwrap_or(0),
        eligible_role_below_decks = meta.optimizer_funnel.eligible_role_pool.as_ref().map(|p| p.below_decks).unwrap_or(0),
        roster_role_captains = meta.optimizer_funnel.roster_role_pool.as_ref().map(|p| p.captains).unwrap_or(0),
        roster_role_bridge = meta.optimizer_funnel.roster_role_pool.as_ref().map(|p| p.bridge).unwrap_or(0),
        roster_role_below_decks = meta.optimizer_funnel.roster_role_pool.as_ref().map(|p| p.below_decks).unwrap_or(0),
        final_role_captains = meta.optimizer_funnel.final_role_pool.as_ref().map(|p| p.captains).unwrap_or(0),
        final_role_bridge = meta.optimizer_funnel.final_role_pool.as_ref().map(|p| p.bridge).unwrap_or(0),
        final_role_below_decks = meta.optimizer_funnel.final_role_pool.as_ref().map(|p| p.below_decks).unwrap_or(0),
        generated_candidates = meta.optimizer_funnel.generated_candidates.unwrap_or(0),
        scout_candidates = meta.optimizer_funnel.scout_candidates.unwrap_or(0),
        confirmed_candidates = meta.optimizer_funnel.confirmed_candidates.unwrap_or(0),
        optimize_history_confirm_hits = meta.optimize_history_confirm_hits,
        optimize_history_wrote = meta.optimize_history_wrote,
        final_result_count = all_results.len() as u64,
        phase_durations_ms = ?meta.optimizer_funnel.phase_durations_ms,
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
            raw_role_captains: meta
                .optimizer_funnel
                .raw_role_pool
                .as_ref()
                .map(|p| p.captains),
            raw_role_bridge: meta
                .optimizer_funnel
                .raw_role_pool
                .as_ref()
                .map(|p| p.bridge),
            raw_role_below_decks: meta
                .optimizer_funnel
                .raw_role_pool
                .as_ref()
                .map(|p| p.below_decks),
            banned_role_captains: meta
                .optimizer_funnel
                .banned_role_pool
                .as_ref()
                .map(|p| p.captains),
            banned_role_bridge: meta
                .optimizer_funnel
                .banned_role_pool
                .as_ref()
                .map(|p| p.bridge),
            banned_role_below_decks: meta
                .optimizer_funnel
                .banned_role_pool
                .as_ref()
                .map(|p| p.below_decks),
            eligible_role_captains: meta
                .optimizer_funnel
                .eligible_role_pool
                .as_ref()
                .map(|p| p.captains),
            eligible_role_bridge: meta
                .optimizer_funnel
                .eligible_role_pool
                .as_ref()
                .map(|p| p.bridge),
            eligible_role_below_decks: meta
                .optimizer_funnel
                .eligible_role_pool
                .as_ref()
                .map(|p| p.below_decks),
            roster_role_captains: meta
                .optimizer_funnel
                .roster_role_pool
                .as_ref()
                .map(|p| p.captains),
            roster_role_bridge: meta
                .optimizer_funnel
                .roster_role_pool
                .as_ref()
                .map(|p| p.bridge),
            roster_role_below_decks: meta
                .optimizer_funnel
                .roster_role_pool
                .as_ref()
                .map(|p| p.below_decks),
            final_role_captains: meta
                .optimizer_funnel
                .final_role_pool
                .as_ref()
                .map(|p| p.captains),
            final_role_bridge: meta
                .optimizer_funnel
                .final_role_pool
                .as_ref()
                .map(|p| p.bridge),
            final_role_below_decks: meta
                .optimizer_funnel
                .final_role_pool
                .as_ref()
                .map(|p| p.below_decks),
            heuristic_candidates: meta.optimizer_funnel.heuristic_candidates,
            warm_start_candidates: meta.optimizer_funnel.warm_start_candidates,
            generated_candidates: meta.optimizer_funnel.generated_candidates,
            after_warm_start_dedupe: meta.optimizer_funnel.after_warm_start_dedupe,
            after_constraints: meta.optimizer_funnel.after_constraints,
            analytical_prefilter_from: meta.optimizer_funnel.analytical_prefilter_from,
            analytical_prefilter_kept: meta.optimizer_funnel.analytical_prefilter_kept,
            scout_candidates: meta.optimizer_funnel.scout_candidates,
            confirmed_candidates: meta.optimizer_funnel.confirmed_candidates,
            phase_durations_ms: meta.optimizer_funnel.phase_durations_ms.clone(),
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
    profile_id: Option<&str>,
) -> OptimizeResponse {
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    let seed = request.seed.unwrap_or(0);
    let chain_grind = request
        .chain
        .as_ref()
        .and_then(|c| chain_grind_params_from_request(c).ok().flatten());

    let mut novelty_anchor_storage: Vec<RankedCrewResult> = Vec::new();
    if request.novelty_lambda.is_some() && request.novelty_history_anchors == Some(true) {
        if let Some(pid) = profile_id {
            if let Some(ck) = request.optimize_cache_key.as_ref().and_then(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }) {
                novelty_anchor_storage =
                    optimize_history::novelty_anchor_rows_for_profile_cache_key(
                        pid,
                        &ck,
                        &chain_grind,
                    );
            }
        }
    }
    let novelty_history_anchors_slice: &[RankedCrewResult] = novelty_anchor_storage.as_slice();

    let mut ranked_results = rank_results(all_results);
    if request.novelty_lambda.is_some() {
        ranked_results = apply_novelty_mmr_if_configured(
            ranked_results,
            request.novelty_lambda,
            request.novelty_diverse_top.map(|n| n as usize),
            request.novelty_pool.map(|n| n as usize),
            novelty_history_anchors_slice,
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
            OptimizerStrategy::LinearEval => "linear_eval",
        }
    };
    let mut notes: Vec<&'static str> = if matches!(meta.strategy, OptimizerStrategy::LinearEval) {
        vec![
            "Linear eval ranks crews by closed-form expected hull damage over the fight length; win rates were not simulated.",
            "Results are deterministic for the same ship, hostile, and seed (crew generation only).",
        ]
    } else {
        vec![
            "Results are deterministic for the same ship, hostile, simulation count, and seed.",
            "Per-crew 95% intervals: Wilson score for win/stall/loss/R1-kill rates; normal approximation for mean hull score per trial (hull fraction on wins, 0 on losses).",
        ]
    };
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
        let mut s = "The leading recommendations use novelty-aware ordering (maximal marginal relevance on officer sets). Remaining rows stay in strength order.".to_string();
        if request.novelty_history_anchors == Some(true) {
            if !novelty_anchor_storage.is_empty() {
                s.push_str(
                    " Persisted optimize_history crews were used as extra redundancy anchors.",
                );
            } else {
                s.push_str(
                    " novelty_history_anchors was enabled but no matching history rows were loaded (missing cache key, profile, entry, or chain fingerprint mismatch).",
                );
            }
        }
        approximate_notes.push(s);
    }
    if matches!(meta.strategy, OptimizerStrategy::LinearEval) {
        approximate_notes.push(
            "Linear eval: rankings use closed-form expected hull damage; win rates were not simulated."
                .to_string(),
        );
    }

    append_optimize_observations(request, meta, &ranked_results, profile_id, sims, seed);

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
    if !meta.defender_static_support_inactive_labels.is_empty() {
        warnings.push(format!(
            "Direct static bonuses for support buff(s) {} apply only vs a player-shaped defender (defender_opponent: player); they are ignored vs NPC hostiles.",
            meta.defender_static_support_inactive_labels.join(", ")
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
            optimizer_funnel: meta.optimizer_funnel.clone(),
            chain: request.chain.clone(),
            effective_strategy: optimizer_strategy_to_api_label(meta.strategy).to_string(),
            strategy_auto: meta.strategy_auto,
            requested_strategy: request.strategy.clone(),
            novelty_lambda: request.novelty_lambda,
            novelty_diverse_top: request.novelty_diverse_top,
            novelty_pool: request.novelty_pool,
            novelty_history_anchors: request.novelty_history_anchors,
            fast_discovery: meta.fast_discovery.then_some(true),
            optimize_history_confirm_hits: (meta.optimize_history_confirm_hits > 0)
                .then_some(meta.optimize_history_confirm_hits),
            optimize_history_wrote: meta.optimize_history_wrote.then_some(true),
            tiered_scout_budget: meta.tiered_scout_budget,
            exhaustive_adaptive_budget: meta.exhaustive_adaptive_budget,
        },
        recommendations: ranked_results
            .iter()
            .map(|r| crew_recommendation_from_ranked(r, recommendation_method_provenance(meta, r)))
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
        match gather_optimize_simulation_results(registry, request, profile_id, &mut sink) {
            Ok(x) => x,
            Err(OptimizeGatherError::Cancelled { .. }) => {
                panic!("sync optimize does not cancel");
            }
            Err(OptimizeGatherError::Validation(e)) => {
                return Err(OptimizePayloadError::Validation(e))
            }
        };
    let duration_ms = start.elapsed().as_millis() as u64;
    let response = build_optimize_response(request, all_results, duration_ms, &meta, profile_id);
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
    pub cancellation_point: Option<String>,
    pub progress_preview: Option<Vec<CrewRecommendation>>,
    pub result: Option<OptimizeResponse>,
    pub error: Option<String>,
    /// Unix-millis at insertion. Read by the shared [`crate::server::job_registry::JobRegistry`]
    /// for oldest-finished eviction.
    pub started_at_ms: u128,
}

impl crate::server::job_registry::JobState for OptimizeJobState {
    fn started_at_ms(&self) -> u128 {
        self.started_at_ms
    }
    fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            OptimizeJobStatus::Done | OptimizeJobStatus::Error
        )
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct OptimizeStartResponse {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_point: Option<String>,
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

/// Process-wide registry of optimize jobs. Plumbing (HashMap + cancel flags + counter +
/// eviction + poison recovery) is provided by [`crate::server::job_registry::JobRegistry`];
/// shared with the sensitivity-job module.
static REGISTRY: crate::server::job_registry::JobRegistry<OptimizeJobState> =
    crate::server::job_registry::JobRegistry::new();

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
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
    let job_id = REGISTRY.next_id("opt");
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

    REGISTRY.insert(
        job_id.clone(),
        OptimizeJobState {
            status: OptimizeJobStatus::Running,
            progress: 0,
            crews_done: 0,
            total_crews: 0,
            phase: None,
            cancellation_point: None,
            progress_preview: None,
            result: None,
            error: None,
            started_at_ms: now_ms(),
        },
        cancel_flag.clone(),
        MAX_OPTIMIZE_JOBS_RETAINED,
    );

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
                let response = build_optimize_response(
                    &request,
                    all_results,
                    duration_ms,
                    &meta,
                    profile_owned.as_deref(),
                );
                info!(
                    job_id = %job_id_thread,
                    duration_ms,
                    recommendations = response.recommendations.len() as u64,
                    effective_strategy = %response.scenario.effective_strategy,
                    strategy_auto = response.scenario.strategy_auto,
                    "optimize_job_completed"
                );
                REGISTRY.with_state_mut(&job_id_thread, |state| {
                    state.status = OptimizeJobStatus::Done;
                    state.progress = 100;
                    state.phase = None;
                    state.cancellation_point = None;
                    state.progress_preview = None;
                    state.result = Some(response);
                });
            }
            Err(OptimizeGatherError::Cancelled { phase }) => {
                warn!(
                    job_id = %job_id_thread,
                    cancellation_phase = phase.as_deref().unwrap_or("unknown"),
                    "optimize_job_cancelled"
                );
                REGISTRY.with_state_mut(&job_id_thread, |state| {
                    state.status = OptimizeJobStatus::Error;
                    state.phase = None;
                    state.cancellation_point = phase.clone();
                    state.error = Some(match phase {
                        Some(phase) => format!("Cancelled during {phase}"),
                        None => "Cancelled".to_string(),
                    });
                });
            }
            Err(OptimizeGatherError::Validation(resp)) => {
                warn!(job_id = %job_id_thread, ?resp, "optimize_job_validation_failed");
                let err_json = serde_json::to_string(&resp)
                    .unwrap_or_else(|_| "validation failed".to_string());
                REGISTRY.with_state_mut(&job_id_thread, |state| {
                    state.status = OptimizeJobStatus::Error;
                    state.error = Some(err_json);
                });
            }
        }
        REGISTRY.remove_cancel(&job_id_thread);
        info!(job_id = %job_id_thread, "optimize_job_cleanup");
    });

    Ok(OptimizeStartResponse { job_id })
}

pub fn get_job_status(job_id: &str) -> Result<OptimizeStatusResponse, OptimizeStatusError> {
    let state = REGISTRY.get(job_id).ok_or(OptimizeStatusError::NotFound)?;
    let status = match &state.status {
        OptimizeJobStatus::Running => "running",
        OptimizeJobStatus::Done => "done",
        OptimizeJobStatus::Error => "error",
    };
    let elapsed_s = ((now_ms().saturating_sub(state.started_at_ms)) as f64) / 1000.0;
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
        cancellation_point: state.cancellation_point.clone(),
        throughput_crews_per_sec,
        eta_seconds,
        progress_preview: state.progress_preview.clone(),
        result: state.result.clone(),
        error: state.error.clone(),
    })
}

pub fn cancel_job(job_id: &str) -> Result<(), OptimizeStatusError> {
    if REGISTRY.cancel(job_id) {
        Ok(())
    } else {
        Err(OptimizeStatusError::NotFound)
    }
}

/// Insert a synthetic optimize job for integration tests (SSE / status polling).
#[doc(hidden)]
pub fn seed_optimize_job_for_tests(job_id: &str, state: OptimizeJobState) {
    REGISTRY.insert(
        job_id.to_string(),
        state,
        Arc::new(AtomicBool::new(false)),
        MAX_OPTIMIZE_JOBS_RETAINED,
    );
}

/// Mutate a seeded test job; returns `false` when the id is absent.
#[doc(hidden)]
pub fn patch_optimize_job_for_tests(
    job_id: &str,
    patch: impl FnOnce(&mut OptimizeJobState),
) -> bool {
    REGISTRY.with_state_mut(job_id, patch).is_some()
}

// Note: the previous `parse_job_timestamp_reads_opt_prefix` and
// `prune_drops_oldest_completed_first` unit tests covered helpers that have moved into
// the shared `crate::server::job_registry` module; equivalent tests live there.

#[cfg(test)]
mod curated_warm_start_tests {
    use super::*;
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;

    #[test]
    fn curated_proven_warm_start_crew_is_wellformed_distinct_and_curated_led() {
        let registry = DataRegistry::load().expect("registry");
        let crew = curated_proven_warm_start_crew(
            &registry,
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            3,
            crate::combat::EnemyType::RedMovingSpace,
            None,
        )
        .expect("curated crew forms for a 3-slot ship over the full catalog");
        assert_eq!(crew.bridge.len(), BRIDGE_SLOTS);
        assert_eq!(crew.below_decks.len(), 3);
        // All seats are distinct officers.
        let mut all = vec![crew.captain.to_ascii_lowercase()];
        all.extend(crew.bridge.iter().map(|s| s.to_ascii_lowercase()));
        all.extend(crew.below_decks.iter().map(|s| s.to_ascii_lowercase()));
        let distinct: std::collections::HashSet<&String> = all.iter().collect();
        assert_eq!(distinct.len(), all.len(), "all crew seats must be distinct");
        // At least one curated below-decks officer leads the seeded below-decks crew.
        let curated = crate::data::below_decks_priority::curated_below_decks_priority();
        assert!(
            crew.below_decks
                .iter()
                .any(|n| curated.iter().any(|c| c.eq_ignore_ascii_case(n))),
            "seeded below-decks crew should include curated officers"
        );
    }

    #[test]
    fn curated_proven_warm_start_crew_none_for_zero_below_decks_slots() {
        let registry = DataRegistry::load().expect("registry");
        assert!(curated_proven_warm_start_crew(
            &registry,
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            0,
            crate::combat::EnemyType::RedMovingSpace,
            None,
        )
        .is_none());
    }
}
