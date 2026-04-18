//! Tiered simulation: two-pass strategy (cheap scouting pass → expensive confirmation).
//! Phase 1: low sims per crew to prune; Phase 2: full Monte Carlo on top N only.

use crate::data::data_registry::DataRegistry;
use crate::optimizer::chain::ChainGrindParams;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, DefenderOpponent,
};
use crate::optimizer::monte_carlo::{
    run_monte_carlo_scout_phase_with_shared, run_monte_carlo_with_shared_variable_iterations,
    SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::OptimizeProgressTick;
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};

/// Default sims per crew for the scouting pass.
pub const DEFAULT_SCOUT_SIMS: usize = 500;
/// Default number of top crews to run full confirmation.
pub const DEFAULT_TOP_K: usize = 20;

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

/// Wilson-interval widths from the scouting pass (95% win-rate CI) for each top-K crew, ordered by
/// scout ranking. Wider width ⇒ lower confidence ⇒ more confirmation simulations.
pub(crate) fn confirm_sims_from_scout_wilson_widths(
    win_rate_ci_widths: &[f64],
    base_full_sims: usize,
) -> Vec<usize> {
    let k = win_rate_ci_widths.len();
    if k == 0 {
        return Vec::new();
    }
    let base = base_full_sims.max(1);
    if k == 1 {
        return vec![base];
    }

    let widths: Vec<f64> = win_rate_ci_widths
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

/// Runs tiered optimization with registry: scouting pass then full MC on top K.
/// Progress: `total_crews` = num_candidates + top_k; during scouting, `crews_done` is 0..num_candidates;
/// after confirmation, `crews_done` reaches `total_crews`. Phases: `tiered_scout`, `tiered_confirm`.
/// Returns false to abort.
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
    mut on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(OptimizeProgressTick) -> bool,
{
    let total_candidates = candidates.len();
    if total_candidates == 0 {
        return Vec::new();
    }

    let k = top_k.min(total_candidates);
    let total_work = total_candidates + k;
    if !on_progress(OptimizeProgressTick {
        crews_done: 0,
        total_crews: total_work as u32,
        phase: "tiered_scout",
        partial_top: None,
    }) {
        return Vec::new();
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

    // Phase 1: scouting with few sims (Wilson early-stop may reduce per-crew iterations).
    let scout_sims = scout_sims.max(1);
    let num_batches = monte_carlo_batch_count_for_candidates(total_candidates);
    let ranges = batch_ranges(total_candidates, num_batches);
    let mut scout_results: Vec<SimulationResult> = Vec::with_capacity(total_candidates);

    for (start, end) in ranges {
        let batch = &candidates[start..end];
        let batch_results = run_monte_carlo_scout_phase_with_shared(
            shared.clone(),
            batch,
            scout_sims,
            seed,
            true,
            chain_grind.clone(),
        );
        scout_results.extend(batch_results);
        let partial_top = rank_results(scout_results.clone())
            .into_iter()
            .take(5)
            .collect::<Vec<_>>();
        if !on_progress(OptimizeProgressTick {
            crews_done: end as u32,
            total_crews: total_work as u32,
            phase: "tiered_scout",
            partial_top: Some(partial_top),
        }) {
            return Vec::new();
        }
    }

    // Rank scouting results and take top K (reuse Wilson widths for adaptive confirm budget).
    let ranked_scout = rank_results(scout_results);
    let top_ranked: Vec<RankedCrewResult> = ranked_scout.into_iter().take(k).collect();
    let wilson_widths: Vec<f64> = top_ranked
        .iter()
        .map(|r| r.win_rate_ci_high - r.win_rate_ci_low)
        .collect();
    let confirm_sims = confirm_sims_from_scout_wilson_widths(&wilson_widths, full_sims.max(1));
    let top_crews: Vec<CrewCandidate> = top_ranked
        .into_iter()
        .map(|r| CrewCandidate {
            captain: r.captain,
            bridge: r.bridge,
            below_decks: r.below_decks,
        })
        .collect();

    // Phase 2: full MC on top K (per-crew iterations scale with scout confidence / Wilson width).
    let confirmation_results = run_monte_carlo_with_shared_variable_iterations(
        shared,
        &top_crews,
        &confirm_sims,
        seed.wrapping_add(1), // distinct seed for confirmation phase
        true,
        chain_grind,
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
        return Vec::new();
    }

    rank_results(confirmation_results)
}

#[cfg(test)]
mod workload_tests {
    use super::{
        confirm_sims_from_scout_wilson_widths, tiered_scout_sims_for_workload,
        tiered_top_k_for_workload, DEFAULT_SCOUT_SIMS, DEFAULT_TOP_K,
    };

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
    fn confirm_sims_uniform_widths_match_base() {
        let base = 5_000usize;
        let w = 0.2f64;
        let sims = confirm_sims_from_scout_wilson_widths(&[w, w, w], base);
        assert_eq!(sims, vec![base, base, base]);
    }

    #[test]
    fn confirm_sims_wider_scout_width_gets_more_iterations() {
        let base = 4_000usize;
        // Wider Wilson band ⇒ lower confidence ⇒ more confirmation sims for that crew.
        let sims = confirm_sims_from_scout_wilson_widths(&[0.1, 0.5], base);
        assert_eq!(sims.len(), 2);
        assert!(
            sims[1] > sims[0],
            "expected wider CI to increase confirm budget: {:?}",
            sims
        );
    }
}
