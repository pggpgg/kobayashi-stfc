//! Tiered simulation: two-pass strategy (cheap scouting pass → expensive confirmation).
//! Phase 1: low sims per crew to prune; Phase 2: full Monte Carlo on top N only.
//!
//! # Adaptive scout (default)
//!
//! When [`run_tiered_with_registry_with_progress`] is called with `scout_adaptive: true`, scouting
//! uses two stages:
//! 1. **Coarse** — every non-cached crew is simulated at [`scout_coarse_sims_from_cap`] trials
//!    (derived from the resolved ceiling `S`, candidate count `N`, and top-`K`; see function docs).
//! 2. **Refine** — crews whose **95% win-rate Wilson interval** overlaps the **top-K cut band**
//!    get a fresh scout at the full cap `S` (replacing the coarse row). The cut band is the union
//!    of Wilson intervals for coarse ranks `K−1` through `K−1 + SCOUT_REFINE_BAND_BUFFER` (0-based
//!    ranks on the strength-sorted coarse list). If more than [`max_scout_refine_contenders`]
//!    crews overlap, the widest Wilson bands are kept (most uncertainty near the cut).
//!
//! When `scout_adaptive` is false, behavior matches legacy tiered: a single scout pass at `S` for
//! every non-cached crew.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use tracing::info;

use crate::data::data_registry::DataRegistry;
use crate::optimizer::chain::ChainGrindParams;
use crate::optimizer::chain::ChainSimulationSummary;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, DefenderOpponent, SharedScenarioData,
};
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_scout_phase_with_shared,
    run_monte_carlo_with_shared_variable_iterations, SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::OptimizeProgressTick;
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};

/// Default sims per crew for the scouting pass.
pub const DEFAULT_SCOUT_SIMS: usize = 500;
/// Default number of top crews to run full confirmation.
pub const DEFAULT_TOP_K: usize = 20;

/// Extra coarse ranks (beyond `K−1`) whose Wilson intervals merge into the top-K cut band.
const SCOUT_REFINE_BAND_BUFFER: usize = 2;

/// Scouting sims per crew when the client omits `tiered_scout_sims`, scaled by how many crews
/// survive into the tiered path (after constraints and optional analytical prefilter).
/// Large pools spend enough to rank reliably; very large pools trim scout cost so total scouting
/// work does not grow linearly without bound.
pub fn tiered_scout_sims_for_workload(n_candidates: usize) -> usize {
    let n = n_candidates.max(1);
    let sims = if n >= 20_000 {
        300
    } else if n >= 12_000 {
        350
    } else if n >= 8_000 {
        400
    } else {
        DEFAULT_SCOUT_SIMS
    };
    sims.max(280)
}

/// Confirmation depth when the client omits `tiered_top_k`, scaled by candidate count so wide
/// searches promote more crews from scout noise into full Monte Carlo.
pub fn tiered_top_k_for_workload(n_candidates: usize) -> usize {
    let n = n_candidates.max(1);
    let k = if n >= 20_000 {
        40
    } else if n >= 10_000 {
        32
    } else if n >= 5_000 {
        28
    } else if n >= 2_500 {
        24
    } else {
        DEFAULT_TOP_K
    };
    k.min(56)
}

/// Coarse scout iterations per crew from the resolved tiered scout ceiling `cap_s`,
/// optionally nudged by candidate pool size and `top_k` relative to `N`.
pub fn scout_coarse_sims_from_cap(cap_s: usize, n_candidates: usize, top_k: usize) -> usize {
    let s = cap_s.max(1);
    if s <= 2 {
        return 1;
    }
    let mut q = s.saturating_mul(9).saturating_div(20).max(200);
    q = q.min(s.saturating_sub(1)).max(1);

    // Large pools: trim coarse slightly so total scout work does not grow without bound.
    let n = n_candidates.max(1);
    if n >= 20_000 {
        q = q.saturating_mul(92).saturating_div(100).max(200);
        q = q.min(s.saturating_sub(1)).max(1);
    }

    // Wide top-K relative to N: nudge coarse up so the Wilson cut band is a bit sharper.
    let k = top_k.max(1);
    if n >= 100 && k.saturating_mul(4) >= n {
        q = ((q as f64) * 1.05).round() as usize;
        q = q.clamp(200, s.saturating_sub(1)).max(1);
    }

    q
}

/// Scalar uncertainty width aligned with how [`rank_results`] scores crews (win + hull blend, or chain lexicographic).
pub(crate) fn confirm_ranking_uncertainty_width(r: &RankedCrewResult) -> f64 {
    if let Some(ref c) = r.chain {
        return chain_ranking_uncertainty_width(c);
    }
    let w_win = (r.win_rate_ci_high - r.win_rate_ci_low).max(1e-12);
    let w_hull = (r.avg_hull_remaining_ci_high - r.avg_hull_remaining_ci_low).max(0.0);
    // Conservative: noisy hull matters for the 0.8·win + 0.2·hull score proxy.
    w_win.max(w_hull).max(1e-12)
}

fn chain_ranking_uncertainty_width(c: &ChainSimulationSummary) -> f64 {
    let w_p = (c.primary_ci_high - c.primary_ci_low).max(1e-12);
    let w_s = (c.secondary_ci_high - c.secondary_ci_low).max(0.0);
    // Secondary breaks ties after primary; widen budget when either signal is noisy.
    w_p.max(0.35 * w_s).max(1e-12)
}

/// Upper bound on how many crews may receive a full-cap scout refine pass.
pub fn max_scout_refine_contenders(k: usize, n: usize) -> usize {
    k.saturating_mul(64).min(4096).max(k.min(n))
}

/// Uncertainty widths from the scouting pass for each top-K crew (scout ranking order).
/// Wider width ⇒ lower confidence ⇒ more confirmation simulations.
pub(crate) fn confirm_sims_from_uncertainty_widths(
    uncertainty_widths: &[f64],
    base_full_sims: usize,
) -> Vec<usize> {
    let k = uncertainty_widths.len();
    if k == 0 {
        return Vec::new();
    }
    let base = base_full_sims.max(1);
    if k == 1 {
        return vec![base];
    }

    let widths: Vec<f64> = uncertainty_widths
        .iter()
        .copied()
        .map(|w| w.max(1e-9))
        .collect();
    let mean_w = widths.iter().sum::<f64>() / k as f64;
    let mean_w = mean_w.max(1e-9);

    // Floor near half the baseline budget so clearly "settled" scouts can spend less in confirm;
    // ceiling allows extra work when Wilson bounds are still wide.
    let lo = (base / 2).max(256);
    let hi = base.saturating_mul(3).min(50_000).max(lo);

    widths
        .iter()
        .map(|w| {
            let mult = (w / mean_w).clamp(0.5, 1.6);
            let raw = (base as f64 * mult).round() as usize;
            raw.clamp(lo, hi)
        })
        .collect()
}

/// When `cap_mult` is `Some(f)`, shrink per-crew confirmation targets so the sum does not exceed
/// `floor(f * k * base_full)` (after `base_full` clamping), while keeping at least one trial per crew.
/// No-op when `cap_mult` is absent or non-finite.
pub(crate) fn apply_confirm_sims_budget_cap(
    sims: &mut [usize],
    k: usize,
    base_full: usize,
    cap_mult: Option<f64>,
) {
    let Some(mult) = cap_mult.filter(|m| m.is_finite() && *m > 0.0) else {
        return;
    };
    let kk = k.max(1);
    let base = base_full.max(1);
    let cap_total = (mult * kk as f64 * base as f64).floor() as usize;
    let cap_total = cap_total.max(sims.len());
    let sum: usize = sims.iter().sum();
    if sum <= cap_total {
        return;
    }
    let scale = cap_total as f64 / sum as f64;
    for s in sims.iter_mut() {
        *s = ((*s as f64) * scale).floor().max(1.0) as usize;
    }
    loop {
        let sum2: usize = sims.iter().sum();
        if sum2 <= cap_total {
            break;
        }
        let Some((i, _)) = sims
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| *v)
            .filter(|(_, v)| **v > 1)
        else {
            break;
        };
        sims[i] -= 1;
    }
}

/// Crew hashes that should receive a full-cap scout refine (`S` trials), excluding preconfirmed.
pub(crate) fn scout_refine_candidate_hashes(
    ranked: &[RankedCrewResult],
    k: usize,
    band_buffer: usize,
    max_contenders: usize,
) -> HashSet<u64> {
    let n = ranked.len();
    if n == 0 || k == 0 {
        return HashSet::new();
    }
    let ki = k.min(n);
    let i0 = ki.saturating_sub(1);
    let i1 = (i0 + band_buffer).min(n.saturating_sub(1));
    let mut band_lo = ranked[i0].win_rate_ci_low;
    let mut band_hi = ranked[i0].win_rate_ci_high;
    if let Some(tail) = ranked.get((i0 + 1)..=i1) {
        for r in tail {
            band_lo = band_lo.min(r.win_rate_ci_low);
            band_hi = band_hi.max(r.win_rate_ci_high);
        }
    }

    let mut scored: Vec<(u64, f64)> = Vec::new();
    for r in ranked {
        let overlaps = r.win_rate_ci_low <= band_hi && r.win_rate_ci_high >= band_lo;
        if overlaps {
            let h = crew_candidate_stable_hash(&CrewCandidate {
                captain: r.captain.clone(),
                bridge: r.bridge.clone(),
                below_decks: r.below_decks.clone(),
            });
            let w = (r.win_rate_ci_high - r.win_rate_ci_low).max(1e-12);
            scored.push((h, w));
        }
    }
    if scored.len() > max_contenders {
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(max_contenders);
    }
    scored.into_iter().map(|(h, _)| h).collect()
}

/// Trial accounting for tiered scouting (coarse pass, optional refine pass, final per-crew totals).
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct TieredScoutBudgetStats {
    /// Sum of `trials_run` after the coarse scout pass (one row per crew).
    pub coarse_pass_trials: u64,
    /// Sum of `trials_run` from refine-only Monte Carlo runs (second pass on contenders).
    pub refine_pass_trials: u64,
    /// Sum of final per-crew `trials_run` after coarse + refine (used for total scout cost).
    pub scout_trials_final: u64,
    /// Sum of `trials_run` across the top-K confirmation phase (includes optimize-history cache hits).
    pub confirm_trials_total: u64,
    /// Min / max confirmation iterations allocated per top-K crew (after optional global cap shrink).
    pub confirm_sims_alloc_min: usize,
    pub confirm_sims_alloc_max: usize,
}

fn simulation_trials_sum(rows: &[SimulationResult]) -> u64 {
    rows.iter().map(|r| r.trials_run as u64).sum()
}

#[allow(clippy::too_many_arguments)]
fn run_tiered_scout_batches<F>(
    candidates: &[CrewCandidate],
    ranges: &[(usize, usize)],
    scout_by_hash: &mut HashMap<u64, SimulationResult>,
    shared: SharedScenarioData,
    scout_iterations: usize,
    seed: u64,
    chain_grind: &Option<ChainGrindParams>,
    preconfirmed: Option<&HashMap<u64, SimulationResult>>,
    only_hashes: Option<&HashSet<u64>>,
    refine_pass_accum: Option<&RefCell<u64>>,
    phase_label: &'static str,
    total_work: u32,
    total_batches: usize,
    n_candidates: usize,
    on_progress: &mut F,
) -> bool
where
    F: FnMut(OptimizeProgressTick) -> bool,
{
    for (batch_index, (start, end)) in ranges.iter().copied().enumerate() {
        info!(
            phase = phase_label,
            strategy = "tiered",
            seed,
            batch_index = (batch_index + 1) as u64,
            batch_total = total_batches as u64,
            batch_start = start as u64,
            batch_end = end as u64,
            batch_candidates = (end - start) as u64,
            total_candidates = n_candidates as u64,
            scout_sims = scout_iterations as u64,
            "optimize_sim_batch_started"
        );
        let mut batch_scout: Vec<CrewCandidate> = Vec::new();
        for c in &candidates[start..end] {
            let h = crew_candidate_stable_hash(c);
            if let Some(pre) = preconfirmed.and_then(|m| m.get(&h)) {
                scout_by_hash.insert(h, pre.clone());
                continue;
            }
            if let Some(filter) = only_hashes {
                if !filter.contains(&h) {
                    continue;
                }
            }
            batch_scout.push(c.clone());
        }
        if !batch_scout.is_empty() {
            let fresh = run_monte_carlo_scout_phase_with_shared(
                shared.clone(),
                &batch_scout,
                scout_iterations,
                seed,
                true,
                chain_grind.clone(),
            );
            if let Some(acc) = refine_pass_accum {
                let add = fresh.iter().map(|r| r.trials_run as u64).sum::<u64>();
                *acc.borrow_mut() += add;
            }
            for (c, sim) in batch_scout.iter().zip(fresh) {
                scout_by_hash.insert(crew_candidate_stable_hash(c), sim);
            }
        }
        info!(
            phase = phase_label,
            strategy = "tiered",
            seed,
            batch_index = (batch_index + 1) as u64,
            batch_total = total_batches as u64,
            crews_done = end as u64,
            total_candidates = n_candidates as u64,
            "optimize_sim_batch_completed"
        );
        // Progress ticks run after each batch; only `candidates[..end]` is guaranteed filled
        // (later indices are processed in subsequent batches).
        let ordered: Vec<SimulationResult> = candidates[..end]
            .iter()
            .map(|c| {
                scout_by_hash
                    .get(&crew_candidate_stable_hash(c))
                    .expect("scout row for every candidate in processed prefix")
                    .clone()
            })
            .collect();
        let partial_top = rank_results(ordered)
            .into_iter()
            .take(5)
            .collect::<Vec<_>>();
        if !on_progress(OptimizeProgressTick {
            crews_done: end as u32,
            total_crews: total_work,
            phase: phase_label,
            partial_top: Some(partial_top),
        }) {
            return false;
        }
    }
    true
}

/// Runs tiered optimization with registry: scouting pass then full MC on top K.
/// Progress: `total_crews` = num_candidates + top_k; during scouting, `crews_done` is 0..num_candidates;
/// after confirmation, `crews_done` reaches `total_crews`. Phases: `tiered_scout`, `tiered_scout_refine`, `tiered_confirm`.
/// Returns false from `on_progress` to abort.
#[allow(clippy::too_many_arguments)]
pub fn run_tiered_with_registry_with_progress<F>(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    candidates: Vec<CrewCandidate>,
    scout_sims: usize,
    full_sims: usize,
    top_k: usize,
    seed: u64,
    profile_id: Option<&str>,
    support_buffs: Option<&[String]>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
    // When set, matching crews skip scout and/or confirm Monte Carlo using stored aggregates.
    preconfirmed: Option<&HashMap<u64, SimulationResult>>,
    scout_adaptive: bool,
    // When `Some(f)`, shrink confirmation totals so `sum ≤ floor(f * K * full_sims)`.
    confirm_budget_cap_mult: Option<f64>,
    mut on_progress: F,
) -> (Vec<RankedCrewResult>, TieredScoutBudgetStats)
where
    F: FnMut(OptimizeProgressTick) -> bool,
{
    let mut budget = TieredScoutBudgetStats::default();
    let total_candidates = candidates.len();
    if total_candidates == 0 {
        return (Vec::new(), budget);
    }

    let k = top_k.min(total_candidates);
    let total_work = total_candidates + k;
    if !on_progress(OptimizeProgressTick {
        crews_done: 0,
        total_crews: total_work as u32,
        phase: "tiered_scout",
        partial_top: None,
    }) {
        return (Vec::new(), budget);
    }

    // Build scenario once per phase; avoids reloading officers/profile for every batch.
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
        support_buffs,
        defender_opponent,
    );

    let scout_cap = scout_sims.max(1);
    let coarse_sims = if scout_adaptive {
        scout_coarse_sims_from_cap(scout_cap, total_candidates, k)
    } else {
        scout_cap
    };
    let skip_refine = !scout_adaptive || coarse_sims >= scout_cap;

    let num_batches = monte_carlo_batch_count_for_candidates(total_candidates);
    let ranges = batch_ranges(total_candidates, num_batches);
    let total_batches = ranges.len();

    let mut scout_by_hash: HashMap<u64, SimulationResult> =
        HashMap::with_capacity(total_candidates);

    if !run_tiered_scout_batches(
        &candidates,
        &ranges,
        &mut scout_by_hash,
        shared.clone(),
        coarse_sims,
        seed,
        &chain_grind,
        preconfirmed,
        None,
        None,
        "tiered_scout",
        total_work as u32,
        total_batches,
        total_candidates,
        &mut on_progress,
    ) {
        return (Vec::new(), budget);
    }

    let scout_ordered_coarse: Vec<SimulationResult> = candidates
        .iter()
        .map(|c| {
            scout_by_hash
                .get(&crew_candidate_stable_hash(c))
                .expect("scout row for every candidate")
                .clone()
        })
        .collect();
    budget.coarse_pass_trials = simulation_trials_sum(&scout_ordered_coarse);

    if !skip_refine {
        let ranked_coarse = rank_results(scout_ordered_coarse.clone());
        let max_c = max_scout_refine_contenders(k, total_candidates);
        let mut refine_hashes =
            scout_refine_candidate_hashes(&ranked_coarse, k, SCOUT_REFINE_BAND_BUFFER, max_c);
        if let Some(pre) = preconfirmed {
            refine_hashes.retain(|h| !pre.contains_key(h));
        }
        if !refine_hashes.is_empty() {
            if !on_progress(OptimizeProgressTick {
                crews_done: total_candidates as u32,
                total_crews: total_work as u32,
                phase: "tiered_scout_refine",
                partial_top: None,
            }) {
                return (Vec::new(), budget);
            }
            let refine_pass_accum = RefCell::new(0u64);
            if !run_tiered_scout_batches(
                &candidates,
                &ranges,
                &mut scout_by_hash,
                shared.clone(),
                scout_cap,
                seed,
                &chain_grind,
                preconfirmed,
                Some(&refine_hashes),
                Some(&refine_pass_accum),
                "tiered_scout_refine",
                total_work as u32,
                total_batches,
                total_candidates,
                &mut on_progress,
            ) {
                return (Vec::new(), budget);
            }
            budget.refine_pass_trials = refine_pass_accum.into_inner();
        }
    }

    let scout_ordered: Vec<SimulationResult> = candidates
        .iter()
        .map(|c| {
            scout_by_hash
                .get(&crew_candidate_stable_hash(c))
                .expect("scout row for every candidate")
                .clone()
        })
        .collect();
    budget.scout_trials_final = simulation_trials_sum(&scout_ordered);

    info!(
        phase = "tiered_scout",
        strategy = "tiered",
        seed,
        scout_coarse_pass_trials = budget.coarse_pass_trials,
        scout_refine_pass_trials = budget.refine_pass_trials,
        scout_trials_final = budget.scout_trials_final,
        scout_adaptive = scout_adaptive,
        "optimize_tiered_scout_budget"
    );

    // Rank scouting results and take top K (ranking-aligned uncertainty widths for confirm budget).
    let ranked_scout = rank_results(scout_ordered);
    let top_ranked: Vec<RankedCrewResult> = ranked_scout.into_iter().take(k).collect();
    let uncertainty_widths: Vec<f64> = top_ranked
        .iter()
        .map(confirm_ranking_uncertainty_width)
        .collect();
    let mut confirm_sims =
        confirm_sims_from_uncertainty_widths(&uncertainty_widths, full_sims.max(1));
    apply_confirm_sims_budget_cap(
        &mut confirm_sims,
        k,
        full_sims.max(1),
        confirm_budget_cap_mult,
    );
    let top_crews: Vec<CrewCandidate> = top_ranked
        .into_iter()
        .map(|r| CrewCandidate {
            captain: r.captain,
            bridge: r.bridge,
            below_decks: r.below_decks,
        })
        .collect();

    // Phase 2: full MC on top K (per-crew iterations scale with scout confidence / Wilson width).
    let mut confirmation_slots: Vec<Option<SimulationResult>> = vec![None; top_crews.len()];
    let mut pending_crews: Vec<CrewCandidate> = Vec::new();
    let mut pending_sims: Vec<usize> = Vec::new();
    for (i, crew) in top_crews.iter().enumerate() {
        let h = crew_candidate_stable_hash(crew);
        if let Some(pre) = preconfirmed.and_then(|m| m.get(&h)) {
            confirmation_slots[i] = Some(pre.clone());
        } else {
            pending_crews.push(crew.clone());
            pending_sims.push(confirm_sims[i]);
        }
    }
    info!(
        phase = "tiered_confirm",
        strategy = "tiered",
        seed,
        confirm_seed = seed.wrapping_add(1),
        top_k = k as u64,
        cached_confirm_crews = (top_crews.len() - pending_crews.len()) as u64,
        pending_confirm_crews = pending_crews.len() as u64,
        "optimize_tiered_confirm_started"
    );
    let confirmation_results: Vec<SimulationResult> = if pending_crews.is_empty() {
        drop(shared);
        confirmation_slots
            .into_iter()
            .map(|o| o.expect("preconfirmed fills all top-K slots"))
            .collect()
    } else {
        let n_pending = pending_crews.len();
        let num_batches = monte_carlo_batch_count_for_candidates(n_pending).max(1);
        let ranges = batch_ranges(n_pending, num_batches);
        let mut fresh: Vec<SimulationResult> = Vec::with_capacity(n_pending);
        for (start, end) in ranges {
            if !on_progress(OptimizeProgressTick {
                crews_done: (total_candidates + start) as u32,
                total_crews: total_work as u32,
                phase: "tiered_confirm",
                partial_top: None,
            }) {
                return (Vec::new(), budget);
            }
            let part = run_monte_carlo_with_shared_variable_iterations(
                shared.clone(),
                &pending_crews[start..end],
                &pending_sims[start..end],
                seed.wrapping_add(1), // distinct seed for confirmation phase
                true,
                chain_grind.clone(),
            );
            fresh.extend(part);
        }
        let mut fi = 0usize;
        for slot in &mut confirmation_slots {
            if slot.is_none() {
                *slot = Some(fresh[fi].clone());
                fi += 1;
            }
        }
        assert_eq!(
            fi,
            fresh.len(),
            "pending confirm slots must match fresh MC rows"
        );
        confirmation_slots
            .into_iter()
            .map(|o| o.expect("tiered confirm slot filled"))
            .collect()
    };
    info!(
        phase = "tiered_confirm",
        strategy = "tiered",
        seed = seed.wrapping_add(1),
        confirmed_crews = confirmation_results.len() as u64,
        "optimize_tiered_confirm_completed"
    );

    let partial_top = rank_results(confirmation_results.clone())
        .into_iter()
        .take(5)
        .collect::<Vec<_>>();
    if !on_progress(OptimizeProgressTick {
        crews_done: total_work as u32,
        total_crews: total_work as u32,
        phase: "tiered_confirm",
        partial_top: Some(partial_top),
    }) {
        return (Vec::new(), budget);
    }

    let trial_per_crew: Vec<usize> = confirmation_results.iter().map(|r| r.trials_run).collect();
    budget.confirm_trials_total = trial_per_crew.iter().map(|t| *t as u64).sum();
    budget.confirm_sims_alloc_min = trial_per_crew.iter().min().copied().unwrap_or(0);
    budget.confirm_sims_alloc_max = trial_per_crew.iter().max().copied().unwrap_or(0);

    (rank_results(confirmation_results), budget)
}

#[cfg(test)]
mod workload_tests {
    use super::{
        apply_confirm_sims_budget_cap, confirm_ranking_uncertainty_width,
        confirm_sims_from_uncertainty_widths, max_scout_refine_contenders,
        scout_coarse_sims_from_cap, scout_refine_candidate_hashes, tiered_scout_sims_for_workload,
        tiered_top_k_for_workload, DEFAULT_SCOUT_SIMS, DEFAULT_TOP_K,
    };
    use crate::optimizer::crew_generator::CrewCandidate;
    use crate::optimizer::monte_carlo::crew_candidate_stable_hash;
    use crate::optimizer::ranking::{RankedCrewResult, RankingScore};

    fn ranked_row(captain: &str, win_lo: f64, win_hi: f64) -> RankedCrewResult {
        let win_rate = (win_lo + win_hi) / 2.0;
        RankedCrewResult {
            captain: captain.to_string(),
            bridge: vec!["b1".into(), "b2".into()],
            below_decks: vec!["d1".into(), "d2".into(), "d3".into()],
            trials_run: 100,
            win_rate,
            win_rate_ci_low: win_lo,
            win_rate_ci_high: win_hi,
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.0,
            loss_rate: 0.0,
            loss_rate_ci_low: 0.0,
            loss_rate_ci_high: 0.0,
            r1_kill_rate: 0.0,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 0.0,
            avg_hull_remaining: 0.5,
            avg_hull_remaining_ci_low: 0.0,
            avg_hull_remaining_ci_high: 1.0,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            score: RankingScore {
                value: (win_rate * 0.8 + 0.5 * 0.2) as f32,
            },
            chain: None,
        }
    }

    #[test]
    fn tiered_scout_sims_small_pool_matches_default() {
        assert_eq!(tiered_scout_sims_for_workload(100), DEFAULT_SCOUT_SIMS);
        assert_eq!(tiered_scout_sims_for_workload(7_999), DEFAULT_SCOUT_SIMS);
    }

    #[test]
    fn tiered_scout_sims_large_pool_steps_down() {
        assert_eq!(tiered_scout_sims_for_workload(8_000), 400);
        assert_eq!(tiered_scout_sims_for_workload(12_000), 350);
        assert_eq!(tiered_scout_sims_for_workload(20_000), 300);
    }

    #[test]
    fn tiered_top_k_scales_then_caps() {
        assert_eq!(tiered_top_k_for_workload(100), DEFAULT_TOP_K);
        assert_eq!(tiered_top_k_for_workload(2_500), 24);
        assert_eq!(tiered_top_k_for_workload(20_000), 40);
        assert_eq!(tiered_top_k_for_workload(100_000), 40);
        assert!(tiered_top_k_for_workload(200_000) <= 56);
    }

    #[test]
    fn scout_coarse_below_cap() {
        assert!(scout_coarse_sims_from_cap(500, 100, 20) < 500);
        assert!(scout_coarse_sims_from_cap(500, 100, 20) >= 200);
    }

    #[test]
    fn scout_coarse_large_pool_trims_vs_small_pool() {
        let small = scout_coarse_sims_from_cap(500, 8_000, 20);
        let large = scout_coarse_sims_from_cap(500, 20_000, 20);
        assert!(
            large <= small,
            "large pool should not increase coarse vs mid pool"
        );
    }

    #[test]
    fn combined_width_exceeds_win_only_when_hull_ci_wide() {
        let mut tight_hull = ranked_row("a", 0.45, 0.55);
        tight_hull.avg_hull_remaining_ci_low = 0.48;
        tight_hull.avg_hull_remaining_ci_high = 0.52;
        let mut wide_hull = ranked_row("b", 0.45, 0.55);
        wide_hull.avg_hull_remaining_ci_low = 0.05;
        wide_hull.avg_hull_remaining_ci_high = 0.95;
        let w_tight = confirm_ranking_uncertainty_width(&tight_hull);
        let w_wide = confirm_ranking_uncertainty_width(&wide_hull);
        assert!(
            w_wide > w_tight,
            "wide hull CI should raise ranking uncertainty width"
        );
    }

    #[test]
    fn confirm_budget_cap_scales_down_total() {
        let base = 5_000usize;
        let mut sims = vec![8_000usize, 8_000, 8_000];
        apply_confirm_sims_budget_cap(&mut sims, 3, base, Some(1.2));
        let after: usize = sims.iter().sum();
        let cap = (1.2_f64 * 3.0 * base as f64).floor() as usize;
        assert!(after <= cap, "after={after} cap={cap} sims={sims:?}");
        assert!(after < 24_000, "expected shrink from 24k trials");
        assert!(sims.iter().all(|&s| s >= 1));
    }

    #[test]
    fn scout_refine_hashes_include_overlap_near_k() {
        // Strength order best-first (same as `rank_results` on typical MC rows).
        // With n=4 and k=3, the cut band merges ranks K−1..K−1+buffer ⇒ indices 2..3 only (the tail),
        // so the strong head stays out of the band merge unless its Wilson interval reaches down.
        let mut rows: Vec<RankedCrewResult> = vec![
            ranked_row("c0", 0.90, 1.0),
            ranked_row("c1", 0.85, 0.95),
            ranked_row("c2", 0.55, 0.65),
            ranked_row("c3", 0.05, 0.10),
        ];
        rows.sort_by(|a, b| {
            b.score
                .value
                .total_cmp(&a.score.value)
                .then_with(|| b.win_rate.total_cmp(&a.win_rate))
        });
        let k = 3usize;
        let max_c = max_scout_refine_contenders(k, rows.len());
        let hs = scout_refine_candidate_hashes(&rows, k, 2, max_c);
        let h0 = crew_candidate_stable_hash(&CrewCandidate {
            captain: "c0".into(),
            bridge: vec!["b1".into(), "b2".into()],
            below_decks: vec!["d1".into(), "d2".into(), "d3".into()],
        });
        let h2 = crew_candidate_stable_hash(&CrewCandidate {
            captain: "c2".into(),
            bridge: vec!["b1".into(), "b2".into()],
            below_decks: vec!["d1".into(), "d2".into(), "d3".into()],
        });
        let h3 = crew_candidate_stable_hash(&CrewCandidate {
            captain: "c3".into(),
            bridge: vec!["b1".into(), "b2".into()],
            below_decks: vec!["d1".into(), "d2".into(), "d3".into()],
        });
        assert!(hs.contains(&h2), "expected c2 in refine set");
        assert!(hs.contains(&h3), "expected c3 in refine set");
        assert!(
            !hs.contains(&h0),
            "strong head should not overlap the tail-only cut band"
        );
    }

    #[test]
    fn confirm_sims_uniform_widths_match_base() {
        let base = 5_000usize;
        let w = 0.2f64;
        let sims = confirm_sims_from_uncertainty_widths(&[w, w, w], base);
        assert_eq!(sims, vec![base, base, base]);
    }

    #[test]
    fn confirm_sims_wider_scout_width_gets_more_iterations() {
        let base = 4_000usize;
        // Wider Wilson band ⇒ lower confidence ⇒ more confirmation sims for that crew.
        let sims = confirm_sims_from_uncertainty_widths(&[0.1, 0.5], base);
        assert_eq!(sims.len(), 2);
        assert!(
            sims[1] > sims[0],
            "expected wider CI to increase confirm budget: {:?}",
            sims
        );
    }

    #[test]
    fn confirm_sims_wide_hull_width_increases_vs_win_only() {
        let base = 4_000usize;
        let win_only = [0.1_f64, 0.1];
        let sims_win = confirm_sims_from_uncertainty_widths(&win_only, base);
        let combined = [
            confirm_ranking_uncertainty_width(&{
                let mut r = ranked_row("x", 0.45, 0.55);
                r.avg_hull_remaining_ci_low = 0.0;
                r.avg_hull_remaining_ci_high = 0.95;
                r
            }),
            confirm_ranking_uncertainty_width(&{
                let mut r = ranked_row("y", 0.45, 0.55);
                r.avg_hull_remaining_ci_low = 0.0;
                r.avg_hull_remaining_ci_high = 0.95;
                r
            }),
        ];
        let sims_combined = confirm_sims_from_uncertainty_widths(&combined, base);
        let sum_win: usize = sims_win.iter().sum();
        let sum_combined: usize = sims_combined.iter().sum();
        assert!(
            sum_combined >= sum_win,
            "combined hull+win widths should not reduce total confirm vs win-only: win={sum_win} combined={sum_combined}"
        );
    }
}
