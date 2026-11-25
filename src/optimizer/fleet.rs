//! Multi-Ship Fleet Allocator (Track B / roadmap §4).
//!
//! Real STFC players run 2–5 drydocks simultaneously (e.g. Xindi, Borg, Swarm, Dominion, PvP),
//! and an officer can only serve on one ship at a time. Greedy dock-by-dock assignment often
//! starves later docks by spending contested staples (Pike, Moreau, Chen, Khan, Cadet Uhura,
//! Janeway, etc.) where a dock already had a 98% alternative.
//!
//! This allocator solves the global disjoint-officer fleet assignment problem in three stages:
//!
//! 1. **Per-dock diverse candidate bank discovery** — runs the multi-lane `Portfolio` optimizer
//!    independently per dock and extracts a high-diversity bank of up to 28 confirmed legal crews
//!    (top elites + distinct captains + low-overlap backups).
//! 2. **Exact Branch-and-Bound disjoint-officer solver** — represents crew officer sets as
//!    stack-allocated bitmasks and searches for the globally optimal non-overlapping fleet under
//!    either `TotalScore` (weighted sum of dock scores) or `MinRegret` (maximize worst-dock
//!    retention vs. standalone ceiling).
//! 3. **Contention fallback & diagnostics** — if a tiny roster exhausts a dock's Stage-1 bank,
//!    runs a targeted constrained `Portfolio` pass excluding already-claimed officers, then emits
//!    per-dock retention percentages and contested-officer explanations.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::combat::EnemyType;
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::BelowDecksPoolMode;
use crate::data::support_buffs::SupportBuffScenarioRequest;
use crate::mechanics::crew_fidelity::{CrewFidelityEvaluator, CrewFidelitySummary};
use crate::optimizer::chain::{ChainGrindParams, ChainSimulationSummary};
use crate::optimizer::constraints::{normalize_officer_name, CrewSearchConstraints};
use crate::optimizer::counterfactual::crew_officer_norms;
use crate::optimizer::crew_generator::{
    resolve_below_decks_slots_for_ship, CrewCandidate, DEFAULT_BELOW_DECKS_SLOTS,
};
use crate::optimizer::monte_carlo::scenario::{
    DefenderOpponent, PlayerDefenderOfficerCrewOverride, PvpScenarioParams,
    ScenarioEngagementOverride,
};
use crate::optimizer::ranking::RankedCrewResult;
use crate::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizerStrategy,
};

/// Maximum confirmed diverse crews retained per dock for the exact Branch-and-Bound solver.
const MAX_BANK_PER_DOCK: usize = 28;
/// Upper bound on Branch-and-Bound recursion nodes (keeps worst-case 5-dock solve < 10ms).
const MAX_BB_NODES: usize = 2_000_000;
/// Bitmask word capacity (8 × 64 = 512 distinct officers across dock banks; exceeds full catalog).
const MASK_WORDS: usize = 8;

/// Global fleet optimization objective across simultaneous drydocks.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FleetObjective {
    /// Maximize the priority-weighted sum of dock scores across the fleet.
    #[default]
    TotalScore,
    /// Maximize the minimum retention (`assigned_score / standalone_best_score`) across all docks,
    /// breaking ties by priority-weighted total score.
    MinRegret,
}

/// Specification for one drydock mission inside a multi-ship fleet allocation request.
#[derive(Debug, Clone)]
pub struct FleetSlotSpec {
    pub label: String,
    pub ship: String,
    pub hostile: String,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub below_decks_slots: Option<usize>,
    pub support_buffs: Vec<String>,
    pub chain_grind: Option<ChainGrindParams>,
    pub priority_weight: f64,
    pub fixed_officers: Vec<String>,
    pub excluded_officers: Vec<String>,
    pub enemy_type: EnemyType,
    pub engagement_override: ScenarioEngagementOverride,
}

/// Top-level parameters for a multi-ship fleet allocation run.
#[derive(Debug, Clone)]
pub struct FleetAllocationRequest {
    pub slots: Vec<FleetSlotSpec>,
    pub objective: FleetObjective,
    pub sims_per_crew: usize,
    pub candidates_per_slot: usize,
    pub seed: u64,
    pub profile_id: Option<String>,
    pub unavailable_officers: Vec<String>,
    pub below_decks_pool_mode: BelowDecksPoolMode,
}

/// Compact confirmed crew summary for a dock assignment or standalone comparison.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct FleetCrewSummary {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub score: f64,
    pub win_rate: f64,
    pub win_rate_ci_low: f64,
    pub win_rate_ci_high: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub r1_kill_rate: f64,
    pub avg_hull_remaining: f64,
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
    pub trials_run: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainSimulationSummary>,
    pub fidelity: CrewFidelitySummary,
}

/// Allocation outcome for one drydock mission.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct FleetSlotAssignment {
    pub slot_index: usize,
    pub label: String,
    pub ship: String,
    pub hostile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_tier: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_level: Option<u32>,
    pub priority_weight: f64,
    pub candidates_evaluated: usize,
    /// Optimal non-overlapping crew assigned to this dock (`None` only if no legal crew exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_crew: Option<FleetCrewSummary>,
    /// Best crew this dock achieves when given exclusive access to the full roster.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standalone_best: Option<FleetCrewSummary>,
    /// `assigned.score / standalone_best.score * 100.0` (100.0 if identical or zero ceiling).
    pub retention_pct: f64,
    /// `standalone_best.score - assigned.score` (>= 0 when sharing incurred opportunity cost).
    pub score_loss_vs_standalone: f64,
    /// Officers from this dock's standalone-best crew that were yielded to another dock.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub yielded_officers: Vec<String>,
}

/// Diagnostic explaining how a contested officer (wanted by 2+ docks' standalone-best crews) was
/// resolved by the global solver.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ContestedOfficerReport {
    pub officer: String,
    pub wanted_by_docks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to_dock: Option<String>,
    pub reason: String,
}

/// Complete multi-ship fleet allocation response.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct FleetAllocationOutcome {
    pub objective: FleetObjective,
    pub seed: u64,
    pub sims_per_crew: usize,
    pub candidates_per_slot: usize,
    pub slots: Vec<FleetSlotAssignment>,
    pub total_fleet_score: f64,
    pub standalone_upper_bound: f64,
    pub fleet_efficiency_pct: f64,
    pub min_dock_retention_pct: f64,
    pub contested_officers: Vec<ContestedOfficerReport>,
    pub conflict_free: bool,
    pub solver_mode: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[inline]
fn ranked_scalar_score(r: &RankedCrewResult) -> f64 {
    if let Some(ref c) = r.chain {
        let hull = c
            .ending_hull_mean_given_primary
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        0.8 * c.primary_success_rate + 0.2 * hull
    } else {
        0.8 * r.win_rate + 0.2 * r.avg_hull_remaining
    }
}

fn ranked_to_fleet_summary(
    r: &RankedCrewResult,
    fidelity_eval: &CrewFidelityEvaluator<'_>,
) -> FleetCrewSummary {
    let fidelity = fidelity_eval.evaluate(&r.captain, &r.bridge, &r.below_decks);
    FleetCrewSummary {
        captain: r.captain.clone(),
        bridge: r.bridge.clone(),
        below_decks: r.below_decks.clone(),
        score: ranked_scalar_score(r),
        win_rate: r.win_rate,
        win_rate_ci_low: r.win_rate_ci_low,
        win_rate_ci_high: r.win_rate_ci_high,
        stall_rate: r.stall_rate,
        loss_rate: r.loss_rate,
        r1_kill_rate: r.r1_kill_rate,
        avg_hull_remaining: r.avg_hull_remaining,
        avg_hull_remaining_ci_low: r.avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high: r.avg_hull_remaining_ci_high,
        trials_run: r.trials_run,
        chain: r.chain.clone(),
        fidelity,
    }
}

fn summary_officer_norms(c: &FleetCrewSummary) -> Vec<String> {
    crew_officer_norms(&CrewCandidate {
        captain: c.captain.clone(),
        bridge: c.bridge.clone(),
        below_decks: c.below_decks.clone(),
    })
}

fn summary_officer_display_map(c: &FleetCrewSummary) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for raw in std::iter::once(c.captain.as_str())
        .chain(c.bridge.iter().map(String::as_str))
        .chain(c.below_decks.iter().map(String::as_str))
    {
        let n = normalize_officer_name(raw);
        if !n.is_empty() {
            map.entry(n).or_insert_with(|| raw.to_string());
        }
    }
    map
}

/// Select a compact, high-diversity bank of up to `MAX_BANK_PER_DOCK` crews from a dock's ranked
/// results so the global solver has both top elites and low-overlap alternatives.
fn select_diverse_dock_bank(
    ranked: &[RankedCrewResult],
    fidelity_eval: &CrewFidelityEvaluator<'_>,
) -> Vec<FleetCrewSummary> {
    if ranked.is_empty() {
        return Vec::new();
    }
    let all: Vec<FleetCrewSummary> = ranked
        .iter()
        .map(|r| ranked_to_fleet_summary(r, fidelity_eval))
        .collect();
    if all.len() <= MAX_BANK_PER_DOCK {
        return all;
    }

    let mut chosen_indices: Vec<usize> = Vec::with_capacity(MAX_BANK_PER_DOCK);
    let mut chosen_set: HashSet<usize> = HashSet::new();

    // 1. Always take the top 8 elites by score.
    for i in 0..8.min(all.len()) {
        chosen_indices.push(i);
        chosen_set.insert(i);
    }

    // 2. Add the highest-scoring crew for every unseen captain.
    let mut seen_captains: HashSet<String> = chosen_indices
        .iter()
        .map(|&i| normalize_officer_name(&all[i].captain))
        .collect();
    for (i, item) in all.iter().enumerate() {
        if chosen_indices.len() >= MAX_BANK_PER_DOCK - 6 {
            break;
        }
        if chosen_set.contains(&i) {
            continue;
        }
        let cap_norm = normalize_officer_name(&item.captain);
        if seen_captains.insert(cap_norm) {
            chosen_indices.push(i);
            chosen_set.insert(i);
        }
    }

    // 3. Add crews with the fewest officers overlapping the standalone #1 crew (great conflict
    //    breakers when #1's whole bridge is contested).
    let top_officers: HashSet<String> = summary_officer_norms(&all[0]).into_iter().collect();
    let mut low_overlap: Vec<(usize, usize)> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| !chosen_set.contains(i))
        .map(|(i, item)| {
            let overlap = summary_officer_norms(item)
                .iter()
                .filter(|n| top_officers.contains(*n))
                .count();
            (i, overlap)
        })
        .collect();
    low_overlap.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| all[b.0].score.total_cmp(&all[a.0].score))
    });
    for (i, _) in low_overlap {
        if chosen_indices.len() >= MAX_BANK_PER_DOCK {
            break;
        }
        if chosen_set.insert(i) {
            chosen_indices.push(i);
        }
    }

    chosen_indices.sort_by(|&a, &b| {
        all[b]
            .score
            .total_cmp(&all[a].score)
            .then_with(|| a.cmp(&b))
    });
    chosen_indices.into_iter().map(|i| all[i].clone()).collect()
}

#[derive(Debug, Clone, Copy, Default)]
struct OfficerBitmask {
    words: [u64; MASK_WORDS],
}

impl OfficerBitmask {
    #[inline]
    fn set(&mut self, bit: usize) {
        let w = bit / 64;
        if w < MASK_WORDS {
            self.words[w] |= 1u64 << (bit % 64);
        }
    }

    #[inline]
    fn intersects(&self, other: &Self) -> bool {
        for i in 0..MASK_WORDS {
            if (self.words[i] & other.words[i]) != 0 {
                return true;
            }
        }
        false
    }

    #[inline]
    fn union(&self, other: &Self) -> Self {
        let mut out = *self;
        for i in 0..MASK_WORDS {
            out.words[i] |= other.words[i];
        }
        out
    }
}

struct BankEntry {
    orig_idx: usize,
    score: f64,
    retention: f64,
    mask: OfficerBitmask,
    /// Overflow officer indices (>= MASK_WORDS * 64), almost always empty.
    overflow: Vec<usize>,
}

impl BankEntry {
    #[inline]
    fn conflicts_with(&self, used_mask: &OfficerBitmask, used_overflow: &[usize]) -> bool {
        if self.mask.intersects(used_mask) {
            return true;
        }
        if !self.overflow.is_empty() && !used_overflow.is_empty() {
            for id in &self.overflow {
                if used_overflow.contains(id) {
                    return true;
                }
            }
        }
        false
    }
}

struct BranchAndBoundResult {
    /// Selected index into each dock's bank (ordered by original dock index).
    chosen: Option<Vec<usize>>,
    nodes_visited: usize,
    hit_node_cap: bool,
}

fn solve_disjoint_branch_and_bound(
    banks: &[Vec<FleetCrewSummary>],
    weights: &[f64],
    objective: FleetObjective,
) -> BranchAndBoundResult {
    let m = banks.len();
    if m == 0 || banks.iter().any(|b| b.is_empty()) {
        return BranchAndBoundResult {
            chosen: None,
            nodes_visited: 0,
            hit_node_cap: false,
        };
    }

    // Build dense integer IDs for all distinct officer norms appearing across dock banks.
    let mut officer_ids: HashMap<String, usize> = HashMap::new();
    let mut intern = |norm: &str| -> usize {
        if let Some(&id) = officer_ids.get(norm) {
            return id;
        }
        let next = officer_ids.len();
        officer_ids.insert(norm.to_string(), next);
        next
    };

    let mut encoded_banks: Vec<Vec<BankEntry>> = Vec::with_capacity(m);
    for bank in banks {
        let standalone_best = bank[0].score.max(0.0);
        let mut entries = Vec::with_capacity(bank.len());
        for (idx, crew) in bank.iter().enumerate() {
            let mut mask = OfficerBitmask::default();
            let mut overflow = Vec::new();
            for norm in summary_officer_norms(crew) {
                let id = intern(&norm);
                if id < MASK_WORDS * 64 {
                    mask.set(id);
                } else {
                    overflow.push(id);
                }
            }
            let retention = if standalone_best > 1e-9 {
                (crew.score / standalone_best).clamp(0.0, 2.0)
            } else {
                1.0
            };
            entries.push(BankEntry {
                orig_idx: idx,
                score: crew.score,
                retention,
                mask,
                overflow,
            });
        }
        encoded_banks.push(entries);
    }

    // Visit docks with smaller banks first (more constrained first) to prune faster.
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| {
        encoded_banks[a]
            .len()
            .cmp(&encoded_banks[b].len())
            .then_with(|| weights[b].total_cmp(&weights[a]))
            .then_with(|| a.cmp(&b))
    });

    // Suffix upper bound on weighted score for pruning.
    let mut suffix_max_weighted = vec![0.0_f64; m + 1];
    for depth in (0..m).rev() {
        let d = order[depth];
        let best_s = encoded_banks[d].first().map(|e| e.score).unwrap_or(0.0);
        suffix_max_weighted[depth] = suffix_max_weighted[depth + 1] + weights[d] * best_s;
    }

    struct SearchState<'a> {
        encoded_banks: &'a [Vec<BankEntry>],
        weights: &'a [f64],
        order: &'a [usize],
        suffix_max_weighted: &'a [f64],
        objective: FleetObjective,
        best_primary: f64,
        best_secondary: f64,
        best_choice: Option<Vec<usize>>,
        current_choice: Vec<usize>,
        nodes: usize,
        hit_cap: bool,
    }

    fn dfs(
        st: &mut SearchState<'_>,
        depth: usize,
        used_mask: OfficerBitmask,
        used_overflow: &mut Vec<usize>,
        acc_weighted: f64,
        cur_min_ret: f64,
    ) {
        if st.nodes >= MAX_BB_NODES {
            st.hit_cap = true;
            return;
        }
        st.nodes += 1;

        let m = st.order.len();
        if depth == m {
            let (primary, secondary) = match st.objective {
                FleetObjective::TotalScore => (acc_weighted, cur_min_ret),
                FleetObjective::MinRegret => (cur_min_ret, acc_weighted),
            };
            let better = st.best_choice.is_none()
                || primary > st.best_primary + 1e-12
                || ((primary - st.best_primary).abs() <= 1e-12
                    && secondary > st.best_secondary + 1e-12);
            if better {
                st.best_primary = primary;
                st.best_secondary = secondary;
                st.best_choice = Some(st.current_choice.clone());
            }
            return;
        }

        // Upper-bound pruning.
        if st.best_choice.is_some() {
            match st.objective {
                FleetObjective::TotalScore => {
                    if acc_weighted + st.suffix_max_weighted[depth] <= st.best_primary + 1e-12 {
                        return;
                    }
                }
                FleetObjective::MinRegret => {
                    if cur_min_ret < st.best_primary - 1e-12 {
                        return;
                    }
                    if (cur_min_ret - st.best_primary).abs() <= 1e-12
                        && acc_weighted + st.suffix_max_weighted[depth] <= st.best_secondary + 1e-12
                    {
                        return;
                    }
                }
            }
        }

        let dock = st.order[depth];
        let w = st.weights[dock];
        for entry in &st.encoded_banks[dock] {
            if entry.conflicts_with(&used_mask, used_overflow) {
                continue;
            }
            let next_weighted = acc_weighted + w * entry.score;
            let next_min_ret = cur_min_ret.min(entry.retention);
            if st.best_choice.is_some() {
                match st.objective {
                    FleetObjective::TotalScore => {
                        if next_weighted + st.suffix_max_weighted[depth + 1]
                            <= st.best_primary + 1e-12
                        {
                            // Bank is sorted descending by score -> no later entry can beat bound!
                            break;
                        }
                    }
                    FleetObjective::MinRegret => {
                        if next_min_ret < st.best_primary - 1e-12 {
                            continue;
                        }
                        if (next_min_ret - st.best_primary).abs() <= 1e-12
                            && next_weighted + st.suffix_max_weighted[depth + 1]
                                <= st.best_secondary + 1e-12
                        {
                            continue;
                        }
                    }
                }
            }

            st.current_choice[dock] = entry.orig_idx;
            let next_mask = used_mask.union(&entry.mask);
            let prev_ov_len = used_overflow.len();
            if !entry.overflow.is_empty() {
                used_overflow.extend_from_slice(&entry.overflow);
            }
            dfs(
                st,
                depth + 1,
                next_mask,
                used_overflow,
                next_weighted,
                next_min_ret,
            );
            used_overflow.truncate(prev_ov_len);
            if st.hit_cap {
                return;
            }
        }
    }

    let mut st = SearchState {
        encoded_banks: &encoded_banks,
        weights,
        order: &order,
        suffix_max_weighted: &suffix_max_weighted,
        objective,
        best_primary: f64::NEG_INFINITY,
        best_secondary: f64::NEG_INFINITY,
        best_choice: None,
        current_choice: vec![0; m],
        nodes: 0,
        hit_cap: false,
    };
    let mut used_overflow = Vec::new();
    dfs(
        &mut st,
        0,
        OfficerBitmask::default(),
        &mut used_overflow,
        0.0,
        f64::INFINITY,
    );

    BranchAndBoundResult {
        chosen: st.best_choice,
        nodes_visited: st.nodes,
        hit_node_cap: st.hit_cap,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_dock_portfolio_discovery(
    registry: &DataRegistry,
    slot: &FleetSlotSpec,
    sims_per_crew: usize,
    candidates_per_slot: usize,
    seed: u64,
    profile_id: Option<&str>,
    below_decks_pool_mode: BelowDecksPoolMode,
    extra_excluded: &[String],
) -> Vec<RankedCrewResult> {
    let below_decks_slots = slot.below_decks_slots.unwrap_or_else(|| {
        resolve_below_decks_slots_for_ship(&slot.ship, slot.ship_tier, slot.ship_level, None)
    });
    let mut exclude = slot.excluded_officers.clone();
    exclude.extend(extra_excluded.iter().cloned());
    let constraints = if slot.fixed_officers.is_empty() && exclude.is_empty() {
        None
    } else {
        Some(CrewSearchConstraints {
            must_include: slot.fixed_officers.clone(),
            exclude,
            ..Default::default()
        })
    };

    let scenario = OptimizationScenario {
        ship: &slot.ship,
        hostile: &slot.hostile,
        ship_tier: slot.ship_tier,
        ship_level: slot.ship_level,
        simulation_count: sims_per_crew,
        seed,
        max_candidates: Some(candidates_per_slot),
        strategy: OptimizerStrategy::Portfolio,
        below_decks_pool_mode,
        seed_population: Vec::new(),
        profile_id,
        tiered_scout_sims: None,
        tiered_top_k: Some(MAX_BANK_PER_DOCK.min(candidates_per_slot)),
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
        below_decks_slots: below_decks_slots.min(DEFAULT_BELOW_DECKS_SLOTS.max(below_decks_slots)),
        constraints,
        support_buffs: slot.support_buffs.clone(),
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        chain_grind: slot.chain_grind.clone(),
        defender_opponent: DefenderOpponent::Hostile,
        player_defender_officer_crew: None::<PlayerDefenderOfficerCrewOverride>,
        pvp: None::<PvpScenarioParams>,
        enemy_type: slot.enemy_type,
        engagement_override: slot.engagement_override,
        engagement_enemy_types: None,
        warm_start: Vec::new(),
        prior_reference_crews: Vec::new(),
        optimize_cache_key: None,
        reuse_fingerprint: None,
        enable_learned_pair_prior: false,
        learned_officer_scores: None,
        local_refinement: None,
        portfolio_racing_trial_budget: None,
    };

    let _ = SupportBuffScenarioRequest::default();
    let outcome =
        optimize_scenario_with_progress_with_registry(registry, &scenario, |_| true, || true);
    outcome.ranked
}

/// Allocate officer-disjoint crews across `2..=5` simultaneous ship+hostile drydock missions.
pub fn allocate_fleet_with_registry(
    registry: &DataRegistry,
    request: &FleetAllocationRequest,
) -> FleetAllocationOutcome {
    let sims = request.sims_per_crew.clamp(64, 3_000);
    let cand_budget = request.candidates_per_slot.clamp(16, 256);
    let fidelity_eval = CrewFidelityEvaluator::from_registry(registry);
    let weights: Vec<f64> = request
        .slots
        .iter()
        .map(|s| {
            if s.priority_weight.is_finite() && s.priority_weight > 0.0 {
                s.priority_weight
            } else {
                1.0
            }
        })
        .collect();

    // Stage 1: Independent Portfolio discovery per dock -> diverse confirmed banks.
    let mut dock_banks: Vec<Vec<FleetCrewSummary>> = Vec::with_capacity(request.slots.len());
    let mut dock_candidates_eval: Vec<usize> = Vec::with_capacity(request.slots.len());
    for (idx, slot) in request.slots.iter().enumerate() {
        let dock_seed = request
            .seed
            .wrapping_add((idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let ranked = run_dock_portfolio_discovery(
            registry,
            slot,
            sims,
            cand_budget,
            dock_seed,
            request.profile_id.as_deref(),
            request.below_decks_pool_mode,
            &request.unavailable_officers,
        );
        dock_candidates_eval.push(ranked.len());
        dock_banks.push(select_diverse_dock_bank(&ranked, &fidelity_eval));
    }

    let standalone_bests: Vec<Option<FleetCrewSummary>> =
        dock_banks.iter().map(|b| b.first().cloned()).collect();

    // Stage 2: Exact Branch-and-Bound disjoint-officer optimization.
    let bb = solve_disjoint_branch_and_bound(&dock_banks, &weights, request.objective);
    let mut warnings = Vec::new();
    let solver_mode;
    let mut assigned: Vec<Option<FleetCrewSummary>> = vec![None; request.slots.len()];

    if let Some(chosen_indices) = bb.chosen {
        solver_mode = if bb.hit_node_cap {
            warnings.push(format!(
                "Branch-and-bound reached the {} node budget; returning best feasible disjoint fleet found.",
                bb.nodes_visited
            ));
            "branch_and_bound_bounded".to_string()
        } else {
            "exact_branch_and_bound".to_string()
        };
        for (i, bank_idx) in chosen_indices.into_iter().enumerate() {
            assigned[i] = dock_banks[i].get(bank_idx).cloned();
        }
    } else {
        // Stage 3: Heavy contention fallback — sequentially solve docks in descending priority
        // order, excluding officers already claimed by higher-priority docks.
        solver_mode = "sequential_contention_fallback".to_string();
        warnings.push(
            "Initial diverse banks had no fully disjoint combination; ran priority-ordered contention repair."
                .to_string(),
        );
        let mut priority_order: Vec<usize> = (0..request.slots.len()).collect();
        priority_order.sort_by(|&a, &b| weights[b].total_cmp(&weights[a]).then_with(|| a.cmp(&b)));

        let mut claimed_norms: HashSet<String> = HashSet::new();
        let mut claimed_names: Vec<String> = request.unavailable_officers.clone();

        for dock_idx in priority_order {
            // Try an existing non-conflicting crew from Stage 1 bank first.
            let from_bank = dock_banks[dock_idx].iter().find(|c| {
                summary_officer_norms(c)
                    .iter()
                    .all(|n| !claimed_norms.contains(n))
            });
            let picked = if let Some(c) = from_bank {
                Some(c.clone())
            } else {
                let dock_seed = request
                    .seed
                    .wrapping_add(((dock_idx + 100) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let fallback_ranked = run_dock_portfolio_discovery(
                    registry,
                    &request.slots[dock_idx],
                    sims,
                    cand_budget,
                    dock_seed,
                    request.profile_id.as_deref(),
                    request.below_decks_pool_mode,
                    &claimed_names,
                );
                fallback_ranked
                    .first()
                    .map(|r| ranked_to_fleet_summary(r, &fidelity_eval))
            };
            if let Some(crew) = picked {
                for (norm, disp) in summary_officer_display_map(&crew) {
                    if claimed_norms.insert(norm) {
                        claimed_names.push(disp);
                    }
                }
                assigned[dock_idx] = Some(crew);
            }
        }
    }

    // Verify strict conflict-free invariant across all assigned docks.
    let mut global_used: HashSet<String> = HashSet::new();
    let mut conflict_free = true;
    let mut officer_to_assigned_dock: HashMap<String, usize> = HashMap::new();
    for (dock_idx, crew_opt) in assigned.iter().enumerate() {
        if let Some(crew) = crew_opt {
            for norm in summary_officer_norms(crew) {
                if !global_used.insert(norm.clone()) {
                    conflict_free = false;
                } else {
                    officer_to_assigned_dock.insert(norm, dock_idx);
                }
            }
        }
    }

    // Stage 4: Build per-dock opportunity cost & contested-officer diagnostics.
    let mut total_fleet_score = 0.0_f64;
    let mut standalone_upper_bound = 0.0_f64;
    let mut min_dock_retention_pct = 100.0_f64;

    let mut standalone_wants: HashMap<String, (String, Vec<usize>)> = HashMap::new();
    for (dock_idx, sb_opt) in standalone_bests.iter().enumerate() {
        if let Some(sb) = sb_opt {
            for (norm, display) in summary_officer_display_map(sb) {
                let entry = standalone_wants
                    .entry(norm)
                    .or_insert_with(|| (display, Vec::new()));
                if !entry.1.contains(&dock_idx) {
                    entry.1.push(dock_idx);
                }
            }
        }
    }

    let mut slot_assignments = Vec::with_capacity(request.slots.len());
    for (dock_idx, slot) in request.slots.iter().enumerate() {
        let w = weights[dock_idx];
        let sb = standalone_bests[dock_idx].clone();
        let asg = assigned[dock_idx].clone();
        let sb_score = sb.as_ref().map(|c| c.score).unwrap_or(0.0);
        let asg_score = asg.as_ref().map(|c| c.score).unwrap_or(0.0);

        standalone_upper_bound += w * sb_score;
        total_fleet_score += w * asg_score;

        let retention_pct = if sb_score > 1e-9 {
            (asg_score / sb_score) * 100.0
        } else if asg.is_some() || sb.is_none() {
            100.0
        } else {
            0.0
        };
        min_dock_retention_pct = min_dock_retention_pct.min(retention_pct);
        let score_loss = (sb_score - asg_score).max(0.0);

        let mut yielded_officers = Vec::new();
        if let Some(ref sb_crew) = sb {
            let asg_norms: HashSet<String> = asg
                .as_ref()
                .map(|c| summary_officer_norms(c).into_iter().collect())
                .unwrap_or_default();
            for (norm, disp) in summary_officer_display_map(sb_crew) {
                if !asg_norms.contains(&norm) {
                    if let Some(&winner_dock) = officer_to_assigned_dock.get(&norm) {
                        if winner_dock != dock_idx {
                            yielded_officers.push(disp);
                        }
                    }
                }
            }
            yielded_officers.sort();
        }

        slot_assignments.push(FleetSlotAssignment {
            slot_index: dock_idx,
            label: slot.label.clone(),
            ship: slot.ship.clone(),
            hostile: slot.hostile.clone(),
            ship_tier: slot.ship_tier,
            ship_level: slot.ship_level,
            priority_weight: w,
            candidates_evaluated: dock_candidates_eval[dock_idx],
            assigned_crew: asg,
            standalone_best: sb,
            retention_pct,
            score_loss_vs_standalone: score_loss,
            yielded_officers,
        });
    }

    let fleet_efficiency_pct = if standalone_upper_bound > 1e-9 {
        (total_fleet_score / standalone_upper_bound) * 100.0
    } else {
        100.0
    };

    let mut contested_officers: Vec<ContestedOfficerReport> = standalone_wants
        .into_iter()
        .filter(|(_, (_, docks))| docks.len() >= 2)
        .map(|(norm, (display, docks))| {
            let wanted_labels: Vec<String> = docks
                .iter()
                .map(|&di| request.slots[di].label.clone())
                .collect();
            let winner = officer_to_assigned_dock.get(&norm).copied();
            let assigned_label = winner.map(|di| request.slots[di].label.clone());
            let reason = match winner {
                Some(w_idx) => {
                    let other_notes: Vec<String> = docks
                        .iter()
                        .filter(|&&di| di != w_idx)
                        .map(|&di| {
                            format!(
                                "{} retained {:.1}%",
                                request.slots[di].label, slot_assignments[di].retention_pct
                            )
                        })
                        .collect();
                    format!(
                        "Assigned to {} ({:.1}% retention); {}",
                        request.slots[w_idx].label,
                        slot_assignments[w_idx].retention_pct,
                        other_notes.join(", ")
                    )
                }
                None => "Not needed in global optimal disjoint fleet (alternate synergy crews scored higher jointly).".to_string(),
            };
            ContestedOfficerReport {
                officer: display,
                wanted_by_docks: wanted_labels,
                assigned_to_dock: assigned_label,
                reason,
            }
        })
        .collect();
    contested_officers.sort_by(|a, b| {
        b.wanted_by_docks
            .len()
            .cmp(&a.wanted_by_docks.len())
            .then_with(|| a.officer.cmp(&b.officer))
    });

    FleetAllocationOutcome {
        objective: request.objective,
        seed: request.seed,
        sims_per_crew: sims,
        candidates_per_slot: cand_budget,
        slots: slot_assignments,
        total_fleet_score,
        standalone_upper_bound,
        fleet_efficiency_pct,
        min_dock_retention_pct,
        contested_officers,
        conflict_free,
        solver_mode,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::profile_index::DEMO_PROFILE_ID;
    use crate::mechanics::crew_fidelity::CrewFidelitySummary;

    fn dummy_crew(cap: &str, br: &[&str], score: f64) -> FleetCrewSummary {
        FleetCrewSummary {
            captain: cap.to_string(),
            bridge: br.iter().map(|s| s.to_string()).collect(),
            below_decks: Vec::new(),
            score,
            win_rate: score,
            win_rate_ci_low: score,
            win_rate_ci_high: score,
            stall_rate: 0.0,
            loss_rate: 1.0 - score,
            r1_kill_rate: 0.0,
            avg_hull_remaining: score,
            avg_hull_remaining_ci_low: score,
            avg_hull_remaining_ci_high: score,
            trials_run: 100,
            chain: None,
            fidelity: CrewFidelitySummary {
                implemented: 1,
                partial: 0,
                ignored: 0,
                total_combat_effects: 1,
                coverage_pct: 100.0,
                grade: "A".into(),
                caveats: Vec::new(),
            },
        }
    }

    #[test]
    fn branch_and_bound_beats_naive_greedy_on_contested_officer() {
        // Dock 0: Pike (0.95) vs Kirk (0.93) -> small 0.02 loss if yielding Pike.
        // Dock 1: Pike (0.90) vs Spock (0.60) -> huge 0.30 loss if yielding Pike!
        // Naive greedy gives Pike to Dock 0 (0.95 + 0.60 = 1.55).
        // Exact B&B gives Pike to Dock 1 and Kirk to Dock 0 (0.93 + 0.90 = 1.83)!
        let banks = vec![
            vec![
                dummy_crew("Pike", &["Moreau", "Chen"], 0.95),
                dummy_crew("Kirk", &["Uhura", "Bones"], 0.93),
            ],
            vec![
                dummy_crew("Pike", &["Khan", "TLann"], 0.90),
                dummy_crew("Spock", &["Scotty", "Sulu"], 0.60),
            ],
        ];
        let res = solve_disjoint_branch_and_bound(&banks, &[1.0, 1.0], FleetObjective::TotalScore);
        assert_eq!(res.chosen, Some(vec![1, 0]));
    }

    #[test]
    fn min_regret_objective_protects_weakest_dock() {
        // Dock 0: A=0.90, B=0.72 (ret 80%)
        // Dock 1: A=0.50, C=0.45 (ret 90%)
        // TotalScore prefers A on Dock 0: 0.90 + 0.45 = 1.35 vs 0.72 + 0.50 = 1.22.
        // MinRegret ALSO prefers A on Dock 0 (min ret = 90% vs 80%)!
        // Now flip retention:
        // Dock 0: A=1.00, B=0.90 (ret 90%)
        // Dock 1: A=0.50, C=0.30 (ret 60%)
        // TotalScore gives A to Dock 1 (0.90 + 0.50 = 1.40 > 1.00 + 0.30 = 1.30).
        // Wait: test a case where TotalScore and MinRegret deliberately disagree!
        // Dock 0 (weight 2.0): A=1.00, B=0.80 (ret 80%)
        // Dock 1 (weight 1.0): A=0.50, C=0.35 (ret 70%)
        // TotalScore: (A on D0) 2*1.0 + 1*0.35 = 2.35 vs (A on D1) 2*0.80 + 1*0.50 = 2.10 -> picks [0, 1]
        // MinRegret:  (A on D0) min(1.0, 0.70)=0.70 vs (A on D1) min(0.80, 1.0)=0.80 -> picks [1, 0]!
        let banks = vec![
            vec![
                dummy_crew("Pike", &["Moreau"], 1.00),
                dummy_crew("Kirk", &["Spock"], 0.80),
            ],
            vec![
                dummy_crew("Pike", &["Chen"], 0.50),
                dummy_crew("Janeway", &["Doctor"], 0.35),
            ],
        ];
        let total =
            solve_disjoint_branch_and_bound(&banks, &[2.0, 1.0], FleetObjective::TotalScore);
        assert_eq!(total.chosen, Some(vec![0, 1]));

        let regret =
            solve_disjoint_branch_and_bound(&banks, &[2.0, 1.0], FleetObjective::MinRegret);
        assert_eq!(regret.chosen, Some(vec![1, 0]));
    }

    #[test]
    fn allocate_two_ship_demo_fleet_is_conflict_free_and_deterministic() {
        let registry = DataRegistry::load().expect("registry");
        let req = FleetAllocationRequest {
            slots: vec![
                FleetSlotSpec {
                    label: "Dock 1 · Saladin".into(),
                    ship: "uss_saladin".into(),
                    hostile: "1140710508".into(),
                    ship_tier: None,
                    ship_level: None,
                    below_decks_slots: Some(2),
                    support_buffs: Vec::new(),
                    chain_grind: None,
                    priority_weight: 1.0,
                    fixed_officers: Vec::new(),
                    excluded_officers: Vec::new(),
                    enemy_type: EnemyType::RedMovingSpace,
                    engagement_override: ScenarioEngagementOverride::default(),
                },
                FleetSlotSpec {
                    label: "Dock 2 · NX-01".into(),
                    ship: "enterprise_nx_01".into(),
                    hostile: "1120713907".into(),
                    ship_tier: None,
                    ship_level: None,
                    below_decks_slots: Some(2),
                    support_buffs: Vec::new(),
                    chain_grind: None,
                    priority_weight: 1.0,
                    fixed_officers: Vec::new(),
                    excluded_officers: Vec::new(),
                    enemy_type: EnemyType::RedMovingSpace,
                    engagement_override: ScenarioEngagementOverride::default(),
                },
            ],
            objective: FleetObjective::TotalScore,
            sims_per_crew: 80,
            candidates_per_slot: 32,
            seed: 19,
            profile_id: Some(DEMO_PROFILE_ID.to_string()),
            unavailable_officers: Vec::new(),
            below_decks_pool_mode: BelowDecksPoolMode::default(),
        };

        let out1 = allocate_fleet_with_registry(&registry, &req);
        let out2 = allocate_fleet_with_registry(&registry, &req);

        assert!(out1.conflict_free, "fleet assignment must be conflict-free");
        assert_eq!(out1.slots.len(), 2);
        assert!(out1.slots[0].assigned_crew.is_some());
        assert!(out1.slots[1].assigned_crew.is_some());
        assert!(
            (out1.total_fleet_score - out2.total_fleet_score).abs() < 1e-12,
            "fleet allocation must be deterministic"
        );
        let mut all_officers = HashSet::new();
        for slot in &out1.slots {
            for norm in summary_officer_norms(slot.assigned_crew.as_ref().unwrap()) {
                assert!(
                    all_officers.insert(norm.clone()),
                    "duplicate officer {norm} assigned across docks!"
                );
            }
        }
    }
}
