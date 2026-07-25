//! Optional two-phase exhaustive Monte Carlo: cheap scout on every candidate, then confirmation
//! Monte Carlo on the top `keep` crews by scout rank (per-crew iterations from ranking-aligned
//! widths, capped at `simulation_count`; remaining crews keep scout stats).

use std::collections::{HashMap, HashSet};

use crate::optimizer::chain::ChainGrindParams;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::SharedScenarioData;
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_scout_phase_with_shared,
    run_monte_carlo_with_shared_variable_iterations, zeroed_loss_result, SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::tiered::{
    apply_confirm_sims_budget_cap, confirm_ranking_uncertainty_width,
    confirm_sims_from_uncertainty_widths, TieredScoutBudgetStats,
};
use crate::optimizer::OptimizeProgressTick;
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};
use tracing::info;

/// Scout every candidate at `scout_sims`, rank, run confirmation Monte Carlo on the top `keep`
/// crews (per-crew iteration counts from scout ranking widths, capped at `full_sims`), then merge.
///
/// When `preconfirmed` is set, matching hashes reuse stored rows for scout and/or confirm (same
/// contract as tiered [`crate::optimizer::tiered::run_tiered_with_registry_with_progress`]).
///
/// Progress: `total_crews` = `candidates.len() + keep`; phases `exhaustive_scout` then `exhaustive_confirm`.
/// Returns `None` if `on_progress` returns false or `eval_should_continue` is false between batches.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_exhaustive_scout_then_full_mc<F, G>(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    scout_sims: usize,
    full_sims: usize,
    keep: usize,
    seed: u64,
    chain_grind: Option<ChainGrindParams>,
    preconfirmed: Option<&HashMap<u64, SimulationResult>>,
    confirm_budget_cap_mult: Option<f64>,
    mut on_progress: F,
    mut eval_should_continue: G,
) -> Option<(Vec<SimulationResult>, TieredScoutBudgetStats)>
where
    F: FnMut(OptimizeProgressTick) -> bool,
    G: FnMut() -> bool,
{
    let total = candidates.len();
    if total == 0 {
        return Some((Vec::new(), TieredScoutBudgetStats::default()));
    }
    let scout_cap = scout_sims.max(1);
    let k = keep.max(1).min(total);
    let total_work = total + k;

    let mut budget = TieredScoutBudgetStats::default();

    if !on_progress(OptimizeProgressTick {
        crews_done: 0,
        total_crews: total_work as u32,
        phase: "exhaustive_scout",
        partial_top: None,
    }) {
        return None;
    }

    let num_batches = monte_carlo_batch_count_for_candidates(total).max(1);
    let ranges = batch_ranges(total, num_batches);
    let scout_batch_total = ranges.len();
    let mut scout_by_hash: HashMap<u64, SimulationResult> = HashMap::with_capacity(total);

    for (batch_index, (start, end)) in ranges.into_iter().enumerate() {
        if !eval_should_continue() {
            return None;
        }
        info!(
            phase = "exhaustive_scout",
            strategy = "exhaustive_two_phase",
            seed,
            batch_index = (batch_index + 1) as u64,
            batch_total = scout_batch_total as u64,
            batch_start = start as u64,
            batch_end = end as u64,
            scout_sims = scout_cap as u64,
            "optimize_exhaustive_scout_batch_started"
        );
        let batch = &candidates[start..end];
        let mut batch_scout: Vec<CrewCandidate> = Vec::new();
        for c in batch {
            let h = crew_candidate_stable_hash(c);
            if let Some(pre) = preconfirmed.and_then(|m| m.get(&h)) {
                scout_by_hash.insert(h, pre.clone());
                continue;
            }
            batch_scout.push(c.clone());
        }
        if !batch_scout.is_empty() {
            let fresh = run_monte_carlo_scout_phase_with_shared(
                shared.clone(),
                &batch_scout,
                scout_cap,
                seed,
                true,
                chain_grind.clone(),
            );
            for (c, r) in batch_scout.iter().zip(fresh) {
                scout_by_hash.insert(crew_candidate_stable_hash(c), r);
            }
        }
        let ordered: Vec<SimulationResult> = candidates[..end]
            .iter()
            .map(|c| {
                scout_by_hash
                    .get(&crew_candidate_stable_hash(c))
                    .expect("scout row for processed prefix")
                    .clone()
            })
            .collect();
        let partial_top = rank_results(ordered)
            .into_iter()
            .take(5)
            .collect::<Vec<_>>();
        if !on_progress(OptimizeProgressTick {
            crews_done: end as u32,
            total_crews: total_work as u32,
            phase: "exhaustive_scout",
            partial_top: Some(partial_top),
        }) {
            return None;
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
    let scout_trials: u64 = scout_ordered.iter().map(|r| r.trials_run as u64).sum();
    budget.coarse_pass_trials = scout_trials;
    budget.scout_trials_final = scout_trials;

    let ranked_scout: Vec<RankedCrewResult> = rank_results(scout_ordered);
    let top_ranked: Vec<RankedCrewResult> = ranked_scout.into_iter().take(k).collect();
    let top_crews: Vec<CrewCandidate> = top_ranked
        .iter()
        .map(|r| CrewCandidate {
            captain: r.captain.clone(),
            bridge: r.bridge.clone(),
            below_decks: r.below_decks.clone(),
        })
        .collect();
    let top_hashes: HashSet<u64> = top_crews.iter().map(crew_candidate_stable_hash).collect();

    let full = full_sims.max(1);
    let uncertainty_widths: Vec<f64> = top_ranked
        .iter()
        .map(confirm_ranking_uncertainty_width)
        .collect();
    let mut confirm_sims = confirm_sims_from_uncertainty_widths(&uncertainty_widths, full);
    apply_confirm_sims_budget_cap(&mut confirm_sims, k, full, confirm_budget_cap_mult);
    for s in &mut confirm_sims {
        *s = (*s).min(full).max(1);
    }

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

    let mut confirmed_by_hash: HashMap<u64, SimulationResult> =
        HashMap::with_capacity(top_crews.len());

    if !pending_crews.is_empty() {
        let n_pending = pending_crews.len();
        let num_batches = monte_carlo_batch_count_for_candidates(n_pending).max(1);
        let ranges = batch_ranges(n_pending, num_batches);
        let mut fresh_by_hash: HashMap<u64, SimulationResult> = HashMap::new();
        for (start, end) in ranges {
            if !eval_should_continue() {
                return None;
            }
            if !on_progress(OptimizeProgressTick {
                crews_done: (total + start) as u32,
                total_crews: total_work as u32,
                phase: "exhaustive_confirm",
                partial_top: None,
            }) {
                return None;
            }
            let part = run_monte_carlo_with_shared_variable_iterations(
                shared.clone(),
                &pending_crews[start..end],
                &pending_sims[start..end],
                seed.wrapping_add(1),
                true,
                chain_grind.clone(),
            );
            for r in part {
                fresh_by_hash.insert(crew_candidate_stable_hash(&r.candidate), r);
            }
        }
        // Fill the unconfirmed slots from this phase's fresh rows, in `pending_crews` order.
        //
        // Driven by an iterator over `pending_crews` rather than an index into it, so a slot count
        // that disagrees with the pending count can never index out of bounds. Rows are looked up
        // and cloned rather than removed: `fresh_by_hash` holds one entry per distinct hash, so a
        // duplicate crew in `pending_crews` would make a second `remove` return `None`. Both used
        // to panic, and release builds abort on panic — taking the server down with the job. The
        // invariants still hold today (`prepend_warm_start_dedupe` makes candidates hash-unique),
        // so the fallbacks are unreachable and assert loudly in debug.
        let mut pending = pending_crews.iter();
        for slot in &mut confirmation_slots {
            if slot.is_none() {
                let Some(crew) = pending.next() else {
                    debug_assert!(false, "more unconfirmed slots than pending confirm crews");
                    break;
                };
                let hash = crew_candidate_stable_hash(crew);
                *slot = Some(fresh_by_hash.get(&hash).cloned().unwrap_or_else(|| {
                    debug_assert!(false, "no fresh exhaustive confirm row (hash {hash})");
                    zeroed_loss_result(crew.clone())
                }));
            }
        }
        debug_assert!(
            pending.next().is_none(),
            "fewer unconfirmed slots than pending confirm crews"
        );
    } else {
        drop(shared);
    }

    // Drain the slots into the confirmed map. Zipping the two together bounds the walk by the
    // shorter of the pair, so a slot vector out of step with `top_crews` degrades instead of
    // panicking (release aborts on panic, which would kill the server, not just this job).
    for (crew, slot) in top_crews.iter().zip(confirmation_slots.iter_mut()) {
        let h = crew_candidate_stable_hash(crew);
        let row = slot.take().unwrap_or_else(|| {
            debug_assert!(false, "exhaustive confirm slot not filled (hash {h})");
            zeroed_loss_result(crew.clone())
        });
        confirmed_by_hash.insert(h, row);
    }
    debug_assert_eq!(
        top_crews.len(),
        confirmation_slots.len(),
        "confirm slots must be 1:1 with top crews"
    );

    let mut merged: Vec<SimulationResult> = Vec::with_capacity(total);
    for c in candidates {
        let h = crew_candidate_stable_hash(c);
        if top_hashes.contains(&h) {
            merged.push(
                confirmed_by_hash
                    .get(&h)
                    .expect("confirmed row for top crew")
                    .clone(),
            );
        } else {
            merged.push(
                scout_by_hash
                    .get(&h)
                    .expect("scout row for non-top crew")
                    .clone(),
            );
        }
    }

    // Match tiered semantics: confirmation stats count only the full-MC phase (top `keep` crews),
    // not scout-only rows left on the rest of the list.
    let confirm_trials_per_crew: Vec<usize> = top_crews
        .iter()
        .map(|c| {
            confirmed_by_hash
                .get(&crew_candidate_stable_hash(c))
                .expect("confirmed row for top crew")
                .trials_run
        })
        .collect();
    budget.confirm_trials_total = confirm_trials_per_crew.iter().map(|t| *t as u64).sum();
    budget.confirm_sims_alloc_min = confirm_trials_per_crew.iter().min().copied().unwrap_or(0);
    budget.confirm_sims_alloc_max = confirm_trials_per_crew.iter().max().copied().unwrap_or(0);

    if !on_progress(OptimizeProgressTick {
        crews_done: total_work as u32,
        total_crews: total_work as u32,
        phase: "exhaustive_confirm",
        partial_top: Some(rank_results(merged.clone()).into_iter().take(5).collect()),
    }) {
        return None;
    }

    Some((merged, budget))
}
