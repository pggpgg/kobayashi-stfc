//! Tiered simulation: two-pass strategy (cheap scouting pass → expensive confirmation).
//! Phase 1: low sims per crew to prune; Phase 2: full Monte Carlo on top N only.

use crate::data::data_registry::DataRegistry;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::build_shared_scenario_data_from_registry;
use crate::optimizer::monte_carlo::{
    run_monte_carlo_scout_phase_with_shared, run_monte_carlo_with_shared, SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};

/// Default sims per crew for the scouting pass.
pub const DEFAULT_SCOUT_SIMS: usize = 500;
/// Default number of top crews to run full confirmation.
pub const DEFAULT_TOP_K: usize = 20;

/// Runs tiered optimization with registry: scouting pass then full MC on top K.
/// Progress callback: (crews_done, total_crews) where total_crews = num_candidates + top_k;
/// during scouting, crews_done is 0..num_candidates; during confirmation, crews_done is num_candidates + 0..confirmed.
/// Returns false to abort.
pub fn run_tiered_with_registry_with_progress<F>(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    candidates: Vec<CrewCandidate>,
    scout_sims: usize,
    full_sims: usize,
    top_k: usize,
    seed: u64,
    profile_id: Option<&str>,
    allow_duplicate_officers: bool,
    mut on_progress: F,
) -> Vec<RankedCrewResult>
where
    F: FnMut(u32, u32) -> bool,
{
    let total_candidates = candidates.len();
    if total_candidates == 0 {
        return Vec::new();
    }

    let k = top_k.min(total_candidates);
    let total_work = total_candidates + k;
    if !on_progress(0, total_work as u32) {
        return Vec::new();
    }

    // Build scenario once per phase; avoids reloading officers/profile for every batch.
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        None,
        None,
        profile_id,
        allow_duplicate_officers,
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
        );
        scout_results.extend(batch_results);
        if !on_progress(end as u32, total_work as u32) {
            return Vec::new();
        }
    }

    // Rank scouting results and take top K
    let ranked_scout = rank_results(scout_results);
    let top_crews: Vec<CrewCandidate> = ranked_scout
        .into_iter()
        .take(k)
        .map(|r| CrewCandidate {
            captain: r.captain,
            bridge: r.bridge,
            below_decks: r.below_decks,
        })
        .collect();

    // Phase 2: full MC on top K
    let full_sims = full_sims.max(1);
    let confirmation_results = run_monte_carlo_with_shared(
        shared,
        &top_crews,
        full_sims,
        seed.wrapping_add(1), // distinct seed for confirmation phase
        true,
    );

    if !on_progress(total_work as u32, total_work as u32) {
        return Vec::new();
    }

    rank_results(confirmation_results)
}
