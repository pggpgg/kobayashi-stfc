//! Unified Multi-Lane Discovery Portfolio optimizer (`OptimizerStrategy::Portfolio`).
//!
//! Combines five complementary components under one deterministic, budget-bounded pipeline:
//! 1. **Multi-lane candidate generation** over the full scenario-eligible officer pools:
//!    - Optional user / heuristic `warm_start` crews (`warm_start`)
//!    - 6-beam slot-by-slot LCARS Diversity Beam Search (`beam_diversity`)
//!    - Stratified Random exploration across captain strata & below-decks families (`random_stratified`)
//! 2. **Multi-fidelity Hyperband / Successive-Halving CRN Racing** (`racing.rs`):
//!    - Screens wide candidate sets through incremental trial brackets (`16 -> 40 -> 80` or
//!      `16 -> 48 -> 144`) with zero wasted trials, CI-overlap retention, and captain-stratum protection.
//! 3. **Quality-Diversity (MAP-Elites) Archive** (`qd_archive.rs`):
//!    - Maintains the best crew per `(captain_group, speed_bucket, survival_bucket)` cell.
//! 4. **Paired-CRN 1-Slot Elite Neighborhood Expansion** (`portfolio_neighbor`):
//!    - Generates high-leverage legal 1-slot substitutions around the top diverse Stage-2 racing
//!      elites (using scenario-ranked LCARS seats plus strided full-pool backups) and races them on
//!      the exact same CRN seed panel before promotion.
//! 5. **Diverse Archive Promotion + Independent-Seed Confirmation**:
//!    - Promotes global leaders plus diverse QD archive elites (`qd_archive`), then confirms every
//!      finalist on a decorrelated confirmation seed so racing selection noise never leaks into
//!      reported win rates or rankings.

use std::collections::{BTreeMap, HashMap, HashSet};
use tracing::info;

use crate::data::data_registry::DataRegistry;
use crate::optimizer::beam::{
    propose_diversity_beam_crews_with_seats, BeamSearchParams, RankedOfficerSeats,
    METHOD_BEAM_DIVERSITY,
};
use crate::optimizer::constraints::{
    filter_candidates, normalize_officer_name, CrewSearchConstraints,
};
use crate::optimizer::crew_generator::{CrewCandidate, BRIDGE_SLOTS};
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, SharedScenarioData,
};
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_with_shared,
    run_monte_carlo_with_shared_variable_iterations, SimulationResult,
};
use crate::optimizer::qd_archive::{
    captain_group_lookup_from_registry, resolve_captain_group, BehaviorCell,
    QualityDiversityArchive,
};
use crate::optimizer::racing::{run_hyperband_racing_with_shared, RacingConfig};
use crate::optimizer::random_stratified::{
    sample_stratified_random_crews, StratifiedSampleParams, METHOD_RANDOM_STRATIFIED,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::tiered::{
    apply_confirm_sims_budget_cap, confirm_ranking_uncertainty_width,
    confirm_sims_from_uncertainty_widths, tiered_top_k_for_workload, TieredScoutBudgetStats,
};
use crate::optimizer::{
    apply_crew_constraints, scenario_support_buff_request, OptimizationScenario,
    OptimizeCandidateFunnel, OptimizeProgressTick,
};
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};

pub const METHOD_PORTFOLIO_WARM_START: &str = "warm_start";
pub const METHOD_PORTFOLIO_NEIGHBOR: &str = "portfolio_neighbor";
pub const METHOD_QD_ARCHIVE: &str = "qd_archive";

/// Default initial candidate pool size when `max_candidates` is omitted for Portfolio.
pub const DEFAULT_PORTFOLIO_CANDIDATES: usize = 512;
/// Default deepest racing bracket cap when `tiered_scout_sims` is omitted for Portfolio.
pub const DEFAULT_PORTFOLIO_SCOUT_CAP: usize = 144;
/// Maximum behavioral cells retained in the MAP-Elites archive.
const DEFAULT_ARCHIVE_MAX_CELLS: usize = 128;

/// Workload-scaled deepest racing bracket cap when `tiered_scout_sims` is omitted.
/// Always stays below `confirm_sims` (at most half when `confirm_sims > 16`) so small smoke runs
/// never spend more trials racing than confirming.
pub fn default_portfolio_scout_cap(n_initial: usize, confirm_sims: usize) -> usize {
    let base = if n_initial <= 32 {
        64
    } else if n_initial <= 128 {
        96
    } else {
        DEFAULT_PORTFOLIO_SCOUT_CAP
    };
    if confirm_sims <= 16 {
        confirm_sims.max(1)
    } else {
        base.min((confirm_sims / 2).max(16))
    }
}

/// Workload-scaled confirmation finalist count when `tiered_top_k` is omitted.
/// Honors small candidate caps (`n_initial < 20`) instead of confirming beyond the requested scope.
pub fn default_portfolio_top_k(n_initial: usize) -> usize {
    tiered_top_k_for_workload(n_initial).min(n_initial.max(1))
}

/// Default Stage-3 1-slot elite neighborhood budget when no explicit racing trial cap is active.
/// Scales proportionally with `target_total` so tiny smoke runs stay fast while default UI (`256`)
/// and full (`512`) searches get deep local refinement around their top diverse elites.
pub fn default_portfolio_neighbor_budget(target_total: usize) -> usize {
    ((target_total.saturating_mul(3)) / 8).clamp(6, 180)
}

/// Decorrelated confirmation seed for portfolio finalists so selection noise from racing
/// never biases reported confirmation win rates or rankings.
#[inline]
pub fn portfolio_confirmation_seed(selection_seed: u64) -> u64 {
    selection_seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(17)
        ^ 0x506F_7274_436F_6E66 // "PortConf"
}

/// Complete output of [`run_portfolio_with_registry_with_progress`].
#[derive(Debug, Clone)]
pub struct PortfolioOutcome {
    pub ranked: Vec<RankedCrewResult>,
    pub candidate_funnel: OptimizeCandidateFunnel,
    pub resolved_params: (usize, usize, usize),
    pub scout_budget: TieredScoutBudgetStats,
    pub random_exploration_hashes: HashSet<u64>,
    pub proposing_methods_by_hash: HashMap<u64, Vec<String>>,
    pub qd_cells_by_hash: HashMap<u64, String>,
}

fn record_method(map: &mut HashMap<u64, Vec<String>>, hash: u64, label: &str) {
    let entry = map.entry(hash).or_default();
    if !entry.iter().any(|m| m == label) {
        entry.push(label.to_string());
    }
}

/// Build an ordered list of replacement officer names for a seat: top `lead_n` scenario-ranked
/// officers plus up to `tail_stride_n` deterministically strided officers from the remaining pool
/// so officers with unmodeled or exotic mechanics remain reachable in 1-slot local neighborhoods.
fn neighbor_replacement_pool(
    ranked_seat: &[String],
    lead_n: usize,
    tail_stride_n: usize,
) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::with_capacity(lead_n + tail_stride_n);
    for name in ranked_seat.iter().take(lead_n) {
        out.push(name.as_str());
    }
    if ranked_seat.len() > lead_n && tail_stride_n > 0 {
        let rest = &ranked_seat[lead_n..];
        let step = (rest.len() / tail_stride_n).max(1);
        for name in rest.iter().step_by(step).take(tail_stride_n) {
            out.push(name.as_str());
        }
    }
    out
}

/// Generate high-leverage legal 1-slot neighbors (captain, bridge, below-decks swaps) around the
/// top diverse Stage-2 racing elites, excluding any crew already in `already_seen`.
fn generate_elite_neighbors(
    seed_elites: &[CrewCandidate],
    ranked_seats: &RankedOfficerSeats,
    constraints: Option<&CrewSearchConstraints>,
    already_seen: &mut HashSet<u64>,
    max_neighbors: usize,
) -> Vec<CrewCandidate> {
    if seed_elites.is_empty() || max_neighbors == 0 {
        return Vec::new();
    }

    let cap_repl = neighbor_replacement_pool(&ranked_seats.captains, 14, 4);
    let br_repl = neighbor_replacement_pool(&ranked_seats.bridge, 18, 4);
    let bd_repl = neighbor_replacement_pool(&ranked_seats.below_decks, 16, 4);

    let mut raw: Vec<CrewCandidate> = Vec::with_capacity(max_neighbors.min(320));
    let mut local_seen: HashSet<u64> = HashSet::new();

    // Interleave across seed elites (bridge swaps -> captain swaps -> below-decks swaps) so every
    // elite gets neighborhood coverage even when `max_neighbors` truncates.
    for elite in seed_elites {
        let occupied: HashSet<String> = std::iter::once(&elite.captain)
            .chain(elite.bridge.iter())
            .chain(elite.below_decks.iter())
            .map(|n| normalize_officer_name(n))
            .filter(|n| !n.is_empty())
            .collect();

        // 1. Bridge 1-slot swaps (highest leverage for synergy + damage/mitigation pairings).
        for slot_idx in 0..elite.bridge.len().min(BRIDGE_SLOTS) {
            for &cand_name in &br_repl {
                let norm = normalize_officer_name(cand_name);
                if norm.is_empty() || occupied.contains(&norm) {
                    continue;
                }
                let mut next = elite.clone();
                next.bridge[slot_idx] = cand_name.to_string();
                let h = crew_candidate_stable_hash(&next);
                if !already_seen.contains(&h) && local_seen.insert(h) {
                    raw.push(next);
                }
            }
        }

        // 2. Captain 1-slot swaps (keeping proven bridge + below-decks core intact).
        for &cand_name in &cap_repl {
            let norm = normalize_officer_name(cand_name);
            if norm.is_empty() || occupied.contains(&norm) {
                continue;
            }
            let mut next = elite.clone();
            next.captain = cand_name.to_string();
            let h = crew_candidate_stable_hash(&next);
            if !already_seen.contains(&h) && local_seen.insert(h) {
                raw.push(next);
            }
        }

        // 3. Below-decks 1-slot swaps.
        for slot_idx in 0..elite.below_decks.len() {
            for &cand_name in &bd_repl {
                let norm = normalize_officer_name(cand_name);
                if norm.is_empty() || occupied.contains(&norm) {
                    continue;
                }
                let mut next = elite.clone();
                next.below_decks[slot_idx] = cand_name.to_string();
                let h = crew_candidate_stable_hash(&next);
                if !already_seen.contains(&h) && local_seen.insert(h) {
                    raw.push(next);
                }
            }
        }
    }

    let filtered = match constraints {
        Some(c) if !c.is_empty() => filter_candidates(raw, c),
        _ => raw,
    };

    let mut accepted = Vec::with_capacity(filtered.len().min(max_neighbors));
    for crew in filtered {
        if accepted.len() >= max_neighbors {
            break;
        }
        let h = crew_candidate_stable_hash(&crew);
        if already_seen.insert(h) {
            accepted.push(crew);
        }
    }
    accepted
}

/// Select up to `max_elites` diverse racing elites (top 2 global + distinct captain-group leaders)
/// as seeds for Stage-3 1-slot neighborhood expansion.
fn select_neighborhood_seed_elites(
    sorted_raced: &[SimulationResult],
    group_lookup: &HashMap<String, String>,
    max_elites: usize,
) -> Vec<CrewCandidate> {
    if sorted_raced.is_empty() || max_elites == 0 {
        return Vec::new();
    }
    let mut seeds: Vec<CrewCandidate> = Vec::with_capacity(max_elites);
    let mut seen_hashes: HashSet<u64> = HashSet::new();
    let mut seen_groups: HashSet<String> = HashSet::new();

    // Always seed from top 2 overall Stage-2 leaders.
    for row in sorted_raced.iter().take(2.min(max_elites)) {
        let h = crew_candidate_stable_hash(&row.candidate);
        if seen_hashes.insert(h) {
            seen_groups.insert(resolve_captain_group(&row.candidate.captain, group_lookup));
            seeds.push(row.candidate.clone());
        }
    }

    // Add leaders from distinct captain groups so neighborhood refinement explores multiple archetypes.
    for row in sorted_raced.iter().skip(2) {
        if seeds.len() >= max_elites {
            break;
        }
        let g = resolve_captain_group(&row.candidate.captain, group_lookup);
        if !seen_groups.contains(&g) {
            let h = crew_candidate_stable_hash(&row.candidate);
            if seen_hashes.insert(h) {
                seen_groups.insert(g);
                seeds.push(row.candidate.clone());
            }
        }
    }

    // Fill any remaining slots with next highest-ranked crews.
    for row in sorted_raced {
        if seeds.len() >= max_elites {
            break;
        }
        let h = crew_candidate_stable_hash(&row.candidate);
        if seen_hashes.insert(h) {
            seeds.push(row.candidate.clone());
        }
    }

    seeds
}

/// Sort `SimulationResult` rows into the exact ranking order of [`rank_results`], keeping the
/// deepest evaluation per distinct stable crew hash.
fn sort_simulation_results_by_rank(results: Vec<SimulationResult>) -> Vec<SimulationResult> {
    let mut by_hash: BTreeMap<u64, SimulationResult> = BTreeMap::new();
    for row in results {
        let h = crew_candidate_stable_hash(&row.candidate);
        match by_hash.get(&h) {
            Some(existing) if existing.trials_run > row.trials_run => {}
            _ => {
                by_hash.insert(h, row);
            }
        }
    }
    let deduped: Vec<SimulationResult> = by_hash.values().cloned().collect();
    let ranked = rank_results(deduped);
    let mut ordered = Vec::with_capacity(ranked.len());
    for r in ranked {
        let h = crew_candidate_stable_hash(&CrewCandidate {
            captain: r.captain,
            bridge: r.bridge,
            below_decks: r.below_decks,
        });
        if let Some(sim) = by_hash.remove(&h) {
            ordered.push(sim);
        }
    }
    ordered
}

/// Run the full multi-lane discovery portfolio using `DataRegistry` with cooperative progress and
/// cancellation.
pub(crate) fn run_portfolio_with_registry_with_progress<F, G>(
    registry: &DataRegistry,
    scenario: &OptimizationScenario<'_>,
    mut on_progress: F,
    mut eval_should_continue: G,
) -> PortfolioOutcome
where
    F: FnMut(OptimizeProgressTick) -> bool,
    G: FnMut() -> bool,
{
    let shared: SharedScenarioData = build_shared_scenario_data_from_registry(
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

    let target_total = scenario
        .max_candidates
        .unwrap_or(DEFAULT_PORTFOLIO_CANDIDATES)
        .max(1);
    let beam_target = ((target_total as f64) * 0.55).round().max(1.0) as usize;
    let random_target = target_total.saturating_sub(beam_target).max(1);

    let mut proposing_methods_by_hash: HashMap<u64, Vec<String>> =
        HashMap::with_capacity(target_total * 2);
    let mut seen_hashes: HashSet<u64> = HashSet::with_capacity(target_total * 2);
    let mut initial_candidates: Vec<CrewCandidate> = Vec::with_capacity(target_total + 32);

    // Lane 1: Warm-start crews (eligibility + constraints filtered).
    let warm_filtered = apply_crew_constraints(scenario.warm_start.clone(), scenario);
    let warm_count = warm_filtered.len();
    for crew in warm_filtered {
        let h = crew_candidate_stable_hash(&crew);
        record_method(
            &mut proposing_methods_by_hash,
            h,
            METHOD_PORTFOLIO_WARM_START,
        );
        if seen_hashes.insert(h) {
            initial_candidates.push(crew);
        }
    }

    // Lane 2: 6-beam slot-by-slot LCARS Diversity Beam Search over full legal pools.
    let (beam_crews, ranked_seats) = propose_diversity_beam_crews_with_seats(
        registry,
        &shared,
        &BeamSearchParams {
            target_count: beam_target,
            below_decks_slots: scenario.below_decks_slots,
            below_decks_pool_mode: scenario.below_decks_pool_mode,
            enemy_type: scenario.enemy_type,
            profile_id: scenario.profile_id,
            constraints: scenario.constraints.as_ref(),
        },
    );
    let beam_crews = apply_crew_constraints(beam_crews, scenario);
    let beam_count = beam_crews.len();
    for crew in beam_crews {
        let h = crew_candidate_stable_hash(&crew);
        record_method(&mut proposing_methods_by_hash, h, METHOD_BEAM_DIVERSITY);
        if seen_hashes.insert(h) {
            initial_candidates.push(crew);
        }
    }

    // Lane 3: Stratified Random exploration over full legal pools (bypasses static bias).
    // If beam produced fewer crews than `beam_target` (e.g. tight constraints), hand leftover
    // candidate slots to stratified random so total breadth stays high.
    let effective_random_target = target_total
        .saturating_sub(initial_candidates.len())
        .max(random_target);
    let random_crews = sample_stratified_random_crews(
        registry,
        &StratifiedSampleParams {
            count: effective_random_target,
            seed: scenario.seed,
            below_decks_slots: scenario.below_decks_slots,
            below_decks_pool_mode: scenario.below_decks_pool_mode,
            enemy_type: scenario.enemy_type,
            profile_id: scenario.profile_id,
            constraints: scenario.constraints.as_ref(),
            exclude_hashes: Some(&seen_hashes),
        },
    );
    let random_crews = apply_crew_constraints(random_crews, scenario);
    let mut random_exploration_hashes: HashSet<u64> = HashSet::with_capacity(random_crews.len());
    for crew in random_crews {
        let h = crew_candidate_stable_hash(&crew);
        record_method(&mut proposing_methods_by_hash, h, METHOD_RANDOM_STRATIFIED);
        random_exploration_hashes.insert(h);
        if seen_hashes.insert(h) {
            initial_candidates.push(crew);
        }
    }

    let generated_total = beam_count + random_exploration_hashes.len();
    let n_initial = initial_candidates.len();
    let scout_cap = scenario
        .tiered_scout_sims
        .unwrap_or_else(|| default_portfolio_scout_cap(n_initial, scenario.simulation_count))
        .max(1);
    let top_k = scenario
        .tiered_top_k
        .unwrap_or_else(|| default_portfolio_top_k(n_initial))
        .max(1);

    if n_initial == 0 || !eval_should_continue() {
        return PortfolioOutcome {
            ranked: Vec::new(),
            candidate_funnel: OptimizeCandidateFunnel::with_counts(
                generated_total,
                warm_count,
                n_initial,
                n_initial,
                None,
                n_initial,
                0,
            ),
            resolved_params: (n_initial, scout_cap, top_k),
            scout_budget: TieredScoutBudgetStats::default(),
            random_exploration_hashes,
            proposing_methods_by_hash,
            qd_cells_by_hash: HashMap::new(),
        };
    }

    let group_lookup = captain_group_lookup_from_registry(registry);
    let total_racing_budget = scenario.portfolio_racing_trial_budget;

    // Split optional racing trial budget: ~78% for initial multi-lane racing, ~22% reserved for
    // Stage-3 1-slot elite neighborhood expansion (with any Stage-2 surplus carried forward).
    let stage2_budget = total_racing_budget.map(|b| {
        if b >= 4_000 {
            (b.saturating_mul(78)) / 100
        } else {
            b
        }
    });

    info!(
        strategy = "portfolio",
        seed = scenario.seed,
        warm_start = warm_count as u64,
        beam_candidates = beam_count as u64,
        random_candidates = random_exploration_hashes.len() as u64,
        initial_candidates = n_initial as u64,
        scout_cap = scout_cap as u64,
        top_k = top_k as u64,
        racing_trial_budget = total_racing_budget.unwrap_or(0),
        "optimize_portfolio_started"
    );

    // Stage 2: Multi-fidelity Hyperband / Successive-Halving CRN racing on initial pool.
    let stage2_cfg = RacingConfig::for_workload(n_initial, scout_cap, top_k, stage2_budget);
    let stage2 = run_hyperband_racing_with_shared(
        &shared,
        &initial_candidates,
        scenario.seed,
        scenario.chain_grind.as_ref(),
        &stage2_cfg,
        &group_lookup,
        None,
        |tick| eval_should_continue() && on_progress(tick),
    );

    let mut budget_stats = TieredScoutBudgetStats::default();
    if let Some(b0) = stage2.brackets.first() {
        budget_stats.coarse_pass_trials += b0.trials_executed;
    }
    for b in stage2.brackets.iter().skip(1) {
        budget_stats.refine_pass_trials += b.trials_executed;
    }
    budget_stats.scout_trials_executed_total += stage2.executed_trials;

    if stage2.cancelled || stage2.all_results.is_empty() {
        return PortfolioOutcome {
            ranked: Vec::new(),
            candidate_funnel: OptimizeCandidateFunnel::with_counts(
                generated_total,
                warm_count,
                n_initial,
                n_initial,
                None,
                n_initial,
                0,
            ),
            resolved_params: (n_initial, scout_cap, top_k),
            scout_budget: budget_stats,
            random_exploration_hashes,
            proposing_methods_by_hash,
            qd_cells_by_hash: HashMap::new(),
        };
    }

    let mut all_raced = stage2.all_results;
    let mut archive = QualityDiversityArchive::new(DEFAULT_ARCHIVE_MAX_CELLS);
    let mut qd_cells_by_hash: HashMap<u64, String> = HashMap::with_capacity(all_raced.len() * 2);
    for sim in &all_raced {
        let cell_label = archive.insert(sim, &group_lookup);
        qd_cells_by_hash.insert(crew_candidate_stable_hash(&sim.candidate), cell_label);
    }

    // Stage 3: Paired-CRN 1-slot elite neighborhood expansion around top diverse Stage-2 elites.
    let remaining_racing_budget = total_racing_budget
        .map(|total| total.saturating_sub(budget_stats.scout_trials_executed_total));
    let can_expand_neighbors =
        eval_should_continue() && remaining_racing_budget.is_none_or(|rem| rem >= 640);

    let mut neighbor_count = 0usize;
    if can_expand_neighbors {
        let sorted_stage2 = sort_simulation_results_by_rank(all_raced.clone());
        let max_elites = if target_total <= 24 { 2 } else { 4 };
        let seed_elites =
            select_neighborhood_seed_elites(&sorted_stage2, &group_lookup, max_elites);
        let max_neighbors = match remaining_racing_budget {
            Some(rem) => ((rem / 28).clamp(24, 240)) as usize,
            None => default_portfolio_neighbor_budget(target_total),
        };
        let neighbors = generate_elite_neighbors(
            &seed_elites,
            &ranked_seats,
            scenario.constraints.as_ref(),
            &mut seen_hashes,
            max_neighbors,
        );
        let neighbors = apply_crew_constraints(neighbors, scenario);
        neighbor_count = neighbors.len();

        if !neighbors.is_empty() {
            for crew in &neighbors {
                let h = crew_candidate_stable_hash(crew);
                record_method(&mut proposing_methods_by_hash, h, METHOD_PORTFOLIO_NEIGHBOR);
            }
            let mut stage3_cfg = RacingConfig::for_workload(
                neighbors.len(),
                scout_cap,
                top_k.min(8),
                remaining_racing_budget,
            );
            // Neighbors are already focused around proven elites; concentrate depth quickly.
            stage3_cfg.protect_captain_strata = false;
            stage3_cfg.max_protected_strata = 0;

            let stage3 = run_hyperband_racing_with_shared(
                &shared,
                &neighbors,
                scenario.seed, // Same CRN seed panel as Stage 2 for paired comparisons!
                scenario.chain_grind.as_ref(),
                &stage3_cfg,
                &group_lookup,
                None,
                |tick| eval_should_continue() && on_progress(tick),
            );
            if let Some(b0) = stage3.brackets.first() {
                budget_stats.coarse_pass_trials += b0.trials_executed;
            }
            for b in stage3.brackets.iter().skip(1) {
                budget_stats.refine_pass_trials += b.trials_executed;
            }
            budget_stats.scout_trials_executed_total += stage3.executed_trials;

            if !stage3.cancelled {
                for sim in &stage3.all_results {
                    let cell_label = archive.insert(sim, &group_lookup);
                    qd_cells_by_hash.insert(crew_candidate_stable_hash(&sim.candidate), cell_label);
                }
                all_raced.extend(stage3.all_results);
            }
        }
    }

    budget_stats.scout_trials_final = all_raced.iter().map(|r| r.trials_run as u64).sum();
    let total_screened = n_initial + neighbor_count;

    // Stage 4: Quality-Diversity archive promotion + independent-seed confirmation.
    let sorted_raced = sort_simulation_results_by_rank(all_raced);
    let diverse_quota = (top_k / 3).clamp(1, 6);
    let (promoted_sims, archive_promoted_hashes) =
        archive.promote_for_confirmation(&sorted_raced, top_k, diverse_quota, &group_lookup);

    for h in &archive_promoted_hashes {
        record_method(&mut proposing_methods_by_hash, *h, METHOD_QD_ARCHIVE);
    }

    let confirm_crews: Vec<CrewCandidate> =
        promoted_sims.iter().map(|s| s.candidate.clone()).collect();
    let n_confirm = confirm_crews.len();
    let base_confirm_sims = scenario.simulation_count.max(1);

    // Respect optional tiered_confirm_budget_cap_mult; otherwise run exact `base_confirm_sims`
    // per finalist so equal-trials budgeting is exact and variance is balanced across finalists.
    let per_crew_confirm_sims: Option<Vec<usize>> =
        scenario.tiered_confirm_budget_cap_mult.map(|cap_mult| {
            let ranked_promoted = rank_results(promoted_sims.clone());
            let widths: Vec<f64> = ranked_promoted
                .iter()
                .map(confirm_ranking_uncertainty_width)
                .collect();
            let mut alloc = confirm_sims_from_uncertainty_widths(&widths, base_confirm_sims);
            apply_confirm_sims_budget_cap(&mut alloc, n_confirm, base_confirm_sims, Some(cap_mult));
            alloc
        });

    let confirm_seed = portfolio_confirmation_seed(scenario.seed);
    let num_batches = monte_carlo_batch_count_for_candidates(n_confirm.max(1)).max(1);
    let ranges = batch_ranges(n_confirm, num_batches);
    let mut confirmed_results: Vec<SimulationResult> = Vec::with_capacity(n_confirm);

    for (start, end) in ranges {
        if !eval_should_continue()
            || !on_progress(OptimizeProgressTick {
                crews_done: start as u32,
                total_crews: n_confirm as u32,
                phase: "portfolio_confirm",
                partial_top: None,
            })
        {
            return PortfolioOutcome {
                ranked: Vec::new(),
                candidate_funnel: OptimizeCandidateFunnel::with_counts(
                    generated_total,
                    warm_count,
                    n_initial,
                    total_screened,
                    None,
                    total_screened,
                    0,
                ),
                resolved_params: (total_screened, scout_cap, top_k),
                scout_budget: budget_stats,
                random_exploration_hashes,
                proposing_methods_by_hash,
                qd_cells_by_hash,
            };
        }

        let part = match per_crew_confirm_sims.as_ref() {
            Some(alloc) => run_monte_carlo_with_shared_variable_iterations(
                shared.clone(),
                &confirm_crews[start..end],
                &alloc[start..end],
                confirm_seed,
                true,
                scenario.chain_grind.clone(),
            ),
            None => run_monte_carlo_with_shared(
                shared.clone(),
                &confirm_crews[start..end],
                base_confirm_sims,
                confirm_seed,
                true,
                scenario.chain_grind.clone(),
            ),
        };
        confirmed_results.extend(part);
    }

    // Refresh behavioral cell labels using the independent-seed confirmed metrics.
    for sim in &confirmed_results {
        let g = resolve_captain_group(&sim.candidate.captain, &group_lookup);
        let cell = BehaviorCell::from_result(sim, g);
        qd_cells_by_hash.insert(crew_candidate_stable_hash(&sim.candidate), cell.label());
    }

    let trial_per_crew: Vec<usize> = confirmed_results.iter().map(|r| r.trials_run).collect();
    budget_stats.confirm_trials_total = trial_per_crew.iter().map(|t| *t as u64).sum();
    budget_stats.confirm_sims_alloc_min = trial_per_crew.iter().min().copied().unwrap_or(0);
    budget_stats.confirm_sims_alloc_max = trial_per_crew.iter().max().copied().unwrap_or(0);

    let ranked = rank_results(confirmed_results);
    let partial_top = ranked.iter().take(5).cloned().collect();
    let _ = on_progress(OptimizeProgressTick {
        crews_done: n_confirm as u32,
        total_crews: n_confirm as u32,
        phase: "portfolio_confirm",
        partial_top: Some(partial_top),
    });

    info!(
        strategy = "portfolio",
        seed = scenario.seed,
        confirm_seed,
        screened_crews = total_screened as u64,
        neighbor_crews = neighbor_count as u64,
        archive_cells = archive.len() as u64,
        confirmed_crews = ranked.len() as u64,
        scout_trials_executed = budget_stats.scout_trials_executed_total,
        confirm_trials_total = budget_stats.confirm_trials_total,
        "optimize_portfolio_completed"
    );

    let mut funnel = OptimizeCandidateFunnel::with_counts(
        generated_total,
        warm_count,
        n_initial,
        total_screened,
        None,
        total_screened,
        ranked.len(),
    );
    if !random_exploration_hashes.is_empty() {
        funnel.random_exploration_candidates = Some(random_exploration_hashes.len());
    }

    PortfolioOutcome {
        ranked,
        candidate_funnel: funnel,
        resolved_params: (total_screened, scout_cap, top_k),
        scout_budget: budget_stats,
        random_exploration_hashes,
        proposing_methods_by_hash,
        qd_cells_by_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::EnemyType;
    use crate::data::heuristics::BelowDecksPoolMode;
    use crate::data::profile_index::DEMO_PROFILE_ID;
    use crate::optimizer::OptimizerStrategy;

    fn sample_scenario(
        max_candidates: usize,
        racing_budget: Option<u64>,
    ) -> OptimizationScenario<'static> {
        OptimizationScenario {
            ship: "uss_saladin",
            hostile: "1140710508",
            simulation_count: 60,
            seed: 7,
            max_candidates: Some(max_candidates),
            strategy: OptimizerStrategy::Portfolio,
            below_decks_pool_mode: BelowDecksPoolMode::Strict,
            profile_id: Some(DEMO_PROFILE_ID),
            tiered_scout_sims: Some(48),
            tiered_top_k: Some(6),
            below_decks_slots: 3,
            enemy_type: EnemyType::RedMovingSpace,
            portfolio_racing_trial_budget: racing_budget,
            ..OptimizationScenario::default()
        }
    }

    #[test]
    fn portfolio_is_deterministic_respects_budget_and_records_provenance() {
        let registry = DataRegistry::load().expect("registry loads");
        let scenario = sample_scenario(64, Some(2_400));

        let out1 =
            run_portfolio_with_registry_with_progress(&registry, &scenario, |_| true, || true);
        let out2 =
            run_portfolio_with_registry_with_progress(&registry, &scenario, |_| true, || true);

        assert!(!out1.ranked.is_empty());
        assert_eq!(out1.ranked.len(), out2.ranked.len());
        assert!(out1.ranked.len() <= 6);
        assert!(
            out1.scout_budget.scout_trials_executed_total <= 2_400,
            "racing trials {} exceeded cap 2400",
            out1.scout_budget.scout_trials_executed_total
        );
        assert_eq!(
            out1.scout_budget.scout_trials_executed_total,
            out2.scout_budget.scout_trials_executed_total
        );
        assert_eq!(
            out1.scout_budget.confirm_trials_total,
            out2.scout_budget.confirm_trials_total
        );

        for (a, b) in out1.ranked.iter().zip(out2.ranked.iter()) {
            assert_eq!(a.captain, b.captain);
            assert_eq!(a.bridge, b.bridge);
            assert_eq!(a.below_decks, b.below_decks);
            assert_eq!(a.win_rate.to_bits(), b.win_rate.to_bits());
            assert_eq!(a.trials_run, 60);

            let h = crew_candidate_stable_hash(&CrewCandidate {
                captain: a.captain.clone(),
                bridge: a.bridge.clone(),
                below_decks: a.below_decks.clone(),
            });
            let methods = out1.proposing_methods_by_hash.get(&h);
            assert!(
                methods.is_some_and(|m| !m.is_empty()),
                "every confirmed crew must carry proposing_methods provenance"
            );
            assert!(
                out1.qd_cells_by_hash.contains_key(&h),
                "every confirmed crew must carry a qd_cell label"
            );
        }
    }

    #[test]
    fn portfolio_respects_constraints() {
        let registry = DataRegistry::load().expect("registry loads");
        let mut scenario = sample_scenario(40, Some(1_600));
        scenario.constraints = Some(CrewSearchConstraints {
            captain_must_be: vec!["Kirk".to_string()],
            exclude: vec!["Spock".to_string()],
            ..CrewSearchConstraints::default()
        });

        let out =
            run_portfolio_with_registry_with_progress(&registry, &scenario, |_| true, || true);
        assert!(!out.ranked.is_empty());
        for row in &out.ranked {
            assert_eq!(normalize_officer_name(&row.captain), "kirk");
            assert!(row
                .bridge
                .iter()
                .chain(row.below_decks.iter())
                .all(|o| normalize_officer_name(o) != "spock"));
        }
    }

    #[test]
    fn portfolio_workload_defaults_scale_for_small_interactive_runs() {
        assert_eq!(default_portfolio_scout_cap(12, 1_000), 64);
        assert_eq!(default_portfolio_scout_cap(12, 80), 40);
        assert_eq!(default_portfolio_scout_cap(100, 5_000), 96);
        assert_eq!(default_portfolio_scout_cap(256, 5_000), 144);
        assert_eq!(default_portfolio_top_k(12), 12);
        assert_eq!(default_portfolio_top_k(256), 20);
        assert_eq!(default_portfolio_neighbor_budget(12), 6);
        assert_eq!(default_portfolio_neighbor_budget(256), 96);
        assert_eq!(default_portfolio_neighbor_budget(512), 180);

        let registry = DataRegistry::load().expect("registry loads");
        let mut scenario = sample_scenario(12, None);
        scenario.tiered_scout_sims = None;
        scenario.tiered_top_k = None;
        scenario.simulation_count = 100;

        let out =
            run_portfolio_with_registry_with_progress(&registry, &scenario, |_| true, || true);
        assert!(
            out.ranked.len() >= 2 && out.ranked.len() <= 12,
            "small interactive portfolio run must return 2..=12 rows, got {}",
            out.ranked.len()
        );
        assert!(
            out.resolved_params.0 <= 24,
            "small max_candidates=12 should bound total screened crews (got {})",
            out.resolved_params.0
        );
        assert!(
            out.resolved_params.1 <= 50,
            "default scout_cap should stay <= confirm_sims/2 (got {})",
            out.resolved_params.1
        );
    }
}
