//! Local refinement and large-neighborhood repair (roadmap §1.1).
//!
//! Hill-climbs around finalist crews produced by tiered/genetic/exhaustive search. The search
//! space of whole crews is astronomically large, but the *neighborhood* of a good crew is small
//! and cheap to enumerate exactly, so a finalist can usually be improved by one or two officer
//! substitutions that no sampled global pass happened to try.
//!
//! Neighborhoods, in the order the roadmap specifies them:
//!
//! - one-slot bridge swaps and one-slot below-decks swaps ([`RefinementKind::LocalSwap`])
//! - captain swaps that preserve the compatible support officers ([`RefinementKind::LocalCaptainSwap`])
//! - destroy-repair neighborhoods that vacate two or three slots and rebuild them from the
//!   ranked pool ([`RefinementKind::LargeNeighborhoodRepair`])
//!
//! Budget discipline follows tiered: **scout depth first, then confirm only improvements.** Each
//! round scouts the incumbent alongside its neighbors so the comparison is like-for-like at equal
//! depth, promotes only neighbors that beat the incumbent's scout score, and pays full confirmation
//! sims on that shortlist alone. A round that finds no scout-level improvement stops the climb —
//! the incumbent is a local optimum at the depth we can afford.
//!
//! Scoring goes through [`rank_results`] rather than a local copy of the blend, so refinement can
//! never rank crews on a different scale than the pipeline it feeds (this matters for chain grinds,
//! which rank lexicographically rather than on the 0.8/0.2 blend).
//!
//! Every accepted improvement records the crew it came from, the slots that changed, and the
//! measured before/after score, so the UI can explain *how* refinement improved a recommendation
//! instead of silently substituting a different crew.

use std::collections::{HashMap, HashSet};

use super::constraints::{normalize_officer_name, CrewSearchConstraints};
use super::crew_generator::{CrewCandidate, OfficerPools, BRIDGE_SLOTS};
use super::monte_carlo::scenario::SharedScenarioData;
use super::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_confirm_topk_with_shared,
    run_monte_carlo_scout_phase_with_shared, SimulationResult,
};
use super::ranking::rank_results;
use crate::optimizer::chain::ChainGrindParams;

/// Which neighborhood produced a refined crew. Derived from the diff against the source finalist
/// rather than from the move that happened to land last, so a crew that took two hops through
/// different neighborhoods is labeled by what actually changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefinementKind {
    /// Exactly one non-captain slot differs from the source crew.
    LocalSwap,
    /// The captain differs (and at most the captain).
    LocalCaptainSwap,
    /// Two or more slots differ.
    LargeNeighborhoodRepair,
}

impl RefinementKind {
    /// Method-provenance label surfaced on API result rows.
    pub fn method_label(self) -> &'static str {
        match self {
            RefinementKind::LocalSwap => "local_swap",
            RefinementKind::LocalCaptainSwap => "local_captain_swap",
            RefinementKind::LargeNeighborhoodRepair => "large_neighborhood_repair",
        }
    }
}

/// Which seat changed. Bridge/below-decks indices are positions in the canonicalized crew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrewSlot {
    Captain,
    Bridge(usize),
    BelowDecks(usize),
}

/// One seat's before/after officer names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotChange {
    pub slot: CrewSlot,
    pub from: String,
    pub to: String,
}

/// Why a refined crew is in the results, and what it beat.
#[derive(Debug, Clone)]
pub struct RefinementProvenance {
    pub kind: RefinementKind,
    /// The finalist this crew was refined from (canonicalized).
    pub source_crew: CrewCandidate,
    pub changed_slots: Vec<SlotChange>,
    /// Confirmed score of the source finalist.
    pub baseline_score: f64,
    /// Confirmed score of the refined crew.
    pub refined_score: f64,
}

impl RefinementProvenance {
    /// Measured gain over the source finalist. Always positive for accepted improvements.
    pub fn gain(&self) -> f64 {
        self.refined_score - self.baseline_score
    }
}

/// Tuning for a refinement pass. Defaults are deliberately conservative: refinement runs *after*
/// the main search has already spent its budget, so it must be a small marginal cost rather than a
/// second full search.
#[derive(Debug, Clone)]
pub struct LocalRefinementParams {
    /// How many top finalists to hill-climb from.
    pub seed_crews: usize,
    /// Maximum accepted moves per seed crew (each round costs one scout batch + one confirm batch).
    pub max_rounds: usize,
    /// Sims per crew in the scout comparison.
    pub scout_sims: usize,
    /// Sims per crew when confirming promoted neighbors.
    pub confirm_sims: usize,
    /// Cap on neighbors scouted per round. Full single-slot enumeration is
    /// `|captains| + 2·|bridge| + k·|below_decks|`, which is large for a full roster.
    pub max_neighbors_per_round: usize,
    /// Neighbors promoted to confirmation per round.
    pub confirm_top_k: usize,
    /// Slots vacated per destroy-repair neighbor. 0 disables the neighborhood.
    pub destroy_repair_slots: usize,
    /// Repair variants generated per vacated slot subset.
    pub destroy_repair_variants: usize,
}

impl Default for LocalRefinementParams {
    fn default() -> Self {
        Self {
            seed_crews: 3,
            max_rounds: 3,
            scout_sims: 60,
            confirm_sims: 400,
            max_neighbors_per_round: 96,
            confirm_top_k: 4,
            destroy_repair_slots: 2,
            destroy_repair_variants: 2,
        }
    }
}

/// Budget/effect accounting for one refinement pass. Reported so a run can be judged on whether
/// refinement earned the sims it spent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalRefinementStats {
    pub seeds_refined: usize,
    pub rounds_run: usize,
    pub neighbors_generated: usize,
    pub neighbors_scouted: usize,
    pub neighbors_confirmed: usize,
    pub improvements_accepted: usize,
}

/// Confirmed refined crews plus their provenance, keyed by canonical crew hash.
#[derive(Debug, Clone, Default)]
pub struct LocalRefinementOutcome {
    /// Confirmed simulation results for refined crews, ready to merge into the pipeline's ranking.
    pub results: Vec<SimulationResult>,
    pub provenance: HashMap<u64, RefinementProvenance>,
    pub stats: LocalRefinementStats,
}

/// Sort bridge and below-decks into a canonical order.
///
/// Both are unordered seat groups, so the same crew can arrive with different Vec orders depending
/// on which search path produced it. `crew_candidate_stable_hash` hashes in Vec order, so without
/// this two spellings of one crew dedup as distinct and get simulated twice. Matches the
/// canonicalization `genetic::repair_crew` already applies for the same reason.
pub fn canonicalize_crew(crew: &mut CrewCandidate) {
    crew.bridge.sort();
    crew.below_decks.sort();
}

fn canonical(crew: &CrewCandidate) -> CrewCandidate {
    let mut c = crew.clone();
    canonicalize_crew(&mut c);
    c
}

fn canonical_hash(crew: &CrewCandidate) -> u64 {
    crew_candidate_stable_hash(&canonical(crew))
}

/// Normalized names currently occupying the crew, used to keep an officer off two seats at once.
fn occupied_keys(crew: &CrewCandidate) -> HashSet<String> {
    let mut keys = HashSet::new();
    if !crew.captain.trim().is_empty() {
        keys.insert(normalize_officer_name(&crew.captain));
    }
    for name in crew.bridge.iter().chain(crew.below_decks.iter()) {
        if !name.trim().is_empty() {
            keys.insert(normalize_officer_name(name));
        }
    }
    keys
}

/// Slot count of a crew: captain + bridge + below-decks.
fn total_slots(crew: &CrewCandidate) -> usize {
    1 + BRIDGE_SLOTS + crew.below_decks.len()
}

fn slot_at(index: usize) -> CrewSlot {
    if index == 0 {
        CrewSlot::Captain
    } else if index <= BRIDGE_SLOTS {
        CrewSlot::Bridge(index - 1)
    } else {
        CrewSlot::BelowDecks(index - 1 - BRIDGE_SLOTS)
    }
}

fn officer_at(crew: &CrewCandidate, slot: CrewSlot) -> &str {
    match slot {
        CrewSlot::Captain => crew.captain.as_str(),
        CrewSlot::Bridge(i) => crew.bridge.get(i).map(String::as_str).unwrap_or(""),
        CrewSlot::BelowDecks(i) => crew.below_decks.get(i).map(String::as_str).unwrap_or(""),
    }
}

fn set_officer(crew: &mut CrewCandidate, slot: CrewSlot, name: &str) {
    match slot {
        CrewSlot::Captain => crew.captain = name.to_string(),
        CrewSlot::Bridge(i) => {
            if let Some(s) = crew.bridge.get_mut(i) {
                *s = name.to_string();
            }
        }
        CrewSlot::BelowDecks(i) => {
            if let Some(s) = crew.below_decks.get_mut(i) {
                *s = name.to_string();
            }
        }
    }
}

/// The pool a seat draws from.
fn pool_for_slot(pools: &OfficerPools, slot: CrewSlot) -> &[String] {
    match slot {
        CrewSlot::Captain => &pools.captains,
        CrewSlot::Bridge(_) => &pools.bridge,
        CrewSlot::BelowDecks(_) => &pools.below_decks,
    }
}

/// Diff the officers of one unordered seat group.
///
/// Bridge and below-decks are *sets* of seats, not ordered lists, so a positional comparison is
/// meaningless: replacing `Br A` with `Br C` in `[Br A, Br B]` re-sorts to `[Br B, Br C]` and would
/// read as two positional changes when one officer actually changed. Instead, diff as multisets and
/// pair each departing officer with an arriving one. Group sizes are fixed, so the two lists have
/// equal length and the pairing is total.
///
/// The reported index is where the arriving officer sits in the canonicalized refined crew, so it
/// is a real seat rather than an artifact of the pairing order.
fn group_diff(
    from_group: &[String],
    to_group: &[String],
    make_slot: impl Fn(usize) -> CrewSlot,
) -> Vec<SlotChange> {
    let from_keys: Vec<String> = from_group
        .iter()
        .map(|s| normalize_officer_name(s))
        .collect();
    let to_keys: Vec<String> = to_group.iter().map(|s| normalize_officer_name(s)).collect();

    let departed: Vec<&String> = from_group
        .iter()
        .filter(|n| !to_keys.contains(&normalize_officer_name(n)))
        .collect();
    let arrived: Vec<(usize, &String)> = to_group
        .iter()
        .enumerate()
        .filter(|(_, n)| !from_keys.contains(&normalize_officer_name(n)))
        .collect();

    departed
        .into_iter()
        .zip(arrived)
        .map(|(from, (index, to))| SlotChange {
            slot: make_slot(index),
            from: from.clone(),
            to: to.clone(),
        })
        .collect()
}

/// Diff two crews, after canonicalization.
///
/// Canonicalizing first means a pure reordering reads as "no change" rather than as swaps, which is
/// what makes the derived [`RefinementKind`] trustworthy.
pub fn diff_crews(source: &CrewCandidate, refined: &CrewCandidate) -> Vec<SlotChange> {
    let a = canonical(source);
    let b = canonical(refined);
    let mut changes = Vec::new();
    if normalize_officer_name(&a.captain) != normalize_officer_name(&b.captain) {
        changes.push(SlotChange {
            slot: CrewSlot::Captain,
            from: a.captain.clone(),
            to: b.captain.clone(),
        });
    }
    changes.extend(group_diff(&a.bridge, &b.bridge, CrewSlot::Bridge));
    changes.extend(group_diff(
        &a.below_decks,
        &b.below_decks,
        CrewSlot::BelowDecks,
    ));
    changes
}

/// Classify a refinement by what changed relative to the source crew.
pub fn kind_from_changes(changes: &[SlotChange]) -> RefinementKind {
    let captain_changed = changes.iter().any(|c| matches!(c.slot, CrewSlot::Captain));
    match changes.len() {
        0 | 1 if captain_changed => RefinementKind::LocalCaptainSwap,
        0 | 1 => RefinementKind::LocalSwap,
        _ => RefinementKind::LargeNeighborhoodRepair,
    }
}

/// A generated neighbor awaiting evaluation.
#[derive(Debug, Clone)]
struct Neighbor {
    crew: CrewCandidate,
    hash: u64,
}

/// Enumerate every legal one-officer substitution of `crew`.
///
/// Deterministic: iterates slots in index order and each pool in its existing (already
/// rank-sorted, for below-decks) order. Officers already seated elsewhere in the crew are skipped,
/// which is what keeps every neighbor duplicate-free by construction rather than by later
/// filtering.
fn enumerate_single_slot_neighbors(
    crew: &CrewCandidate,
    pools: &OfficerPools,
    seen: &HashSet<u64>,
) -> Vec<Neighbor> {
    let base = canonical(crew);
    let occupied = occupied_keys(&base);
    let mut out = Vec::new();
    let mut local_seen: HashSet<u64> = HashSet::new();

    for index in 0..total_slots(&base) {
        let slot = slot_at(index);
        let current_key = normalize_officer_name(officer_at(&base, slot));
        for name in pool_for_slot(pools, slot) {
            let key = normalize_officer_name(name);
            // Skip the incumbent officer and anyone already seated elsewhere.
            if key == current_key || occupied.contains(&key) {
                continue;
            }
            let mut neighbor = base.clone();
            set_officer(&mut neighbor, slot, name);
            canonicalize_crew(&mut neighbor);
            let hash = crew_candidate_stable_hash(&neighbor);
            if seen.contains(&hash) || !local_seen.insert(hash) {
                continue;
            }
            out.push(Neighbor {
                crew: neighbor,
                hash,
            });
        }
    }
    out
}

/// Vacate `remove` slots and rebuild them from the ranked pools.
///
/// Deterministic by construction — slot subsets are enumerated in lexicographic index order and
/// each vacated seat is refilled from the front of its pool, offset by the variant number. Because
/// `OfficerPools::below_decks` arrives sorted by curated rank and power, "front of pool" is a
/// meaningful greedy repair rather than an arbitrary pick, so this needs no RNG and stays
/// reproducible across runs.
fn enumerate_destroy_repair_neighbors(
    crew: &CrewCandidate,
    pools: &OfficerPools,
    remove: usize,
    variants: usize,
    seen: &HashSet<u64>,
    budget: usize,
) -> Vec<Neighbor> {
    let base = canonical(crew);
    let slots = total_slots(&base);
    if remove == 0 || remove > slots || variants == 0 || budget == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut local_seen: HashSet<u64> = HashSet::new();
    let mut subset = (0..remove).collect::<Vec<usize>>();

    loop {
        for variant in 0..variants {
            let mut neighbor = base.clone();
            // Vacate first so the repair sees all freed seats at once — that is what makes this a
            // genuine multi-slot neighborhood rather than a sequence of independent swaps.
            for &index in &subset {
                set_officer(&mut neighbor, slot_at(index), "");
            }
            let mut occupied = occupied_keys(&neighbor);
            let mut filled_all = true;
            for &index in &subset {
                let slot = slot_at(index);
                let pool = pool_for_slot(pools, slot);
                let pick = pool
                    .iter()
                    .filter(|name| !occupied.contains(&normalize_officer_name(name)))
                    .nth(variant);
                match pick {
                    Some(name) => {
                        occupied.insert(normalize_officer_name(name));
                        set_officer(&mut neighbor, slot, name);
                    }
                    None => {
                        filled_all = false;
                        break;
                    }
                }
            }
            if !filled_all {
                continue;
            }
            canonicalize_crew(&mut neighbor);
            let hash = crew_candidate_stable_hash(&neighbor);
            if hash == crew_candidate_stable_hash(&base)
                || seen.contains(&hash)
                || !local_seen.insert(hash)
            {
                continue;
            }
            out.push(Neighbor {
                crew: neighbor,
                hash,
            });
            if out.len() >= budget {
                return out;
            }
        }

        // Advance to the next slot subset in lexicographic order.
        let mut i = remove;
        while i > 0 {
            i -= 1;
            if subset[i] != i + slots - remove {
                subset[i] += 1;
                for j in i + 1..remove {
                    subset[j] = subset[j - 1] + 1;
                }
                break;
            }
            if i == 0 {
                return out;
            }
        }
    }
}

/// Score every result through the shared ranking so refinement can never use a different scale
/// than the pipeline it feeds. Returns canonical-hash → score.
fn scores_by_hash(results: &[SimulationResult]) -> HashMap<u64, f64> {
    rank_results(results.to_vec())
        .into_iter()
        .map(|r| {
            let crew = CrewCandidate {
                captain: r.captain,
                bridge: r.bridge,
                below_decks: r.below_decks,
            };
            (canonical_hash(&crew), f64::from(r.score.value))
        })
        .collect()
}

/// Parameters that stay fixed for a whole refinement pass.
///
/// Crate-visible because `SharedScenarioData` is: refinement runs inside the optimizer pipeline,
/// which has already paid to resolve the scenario once and passes it down by reference.
pub(crate) struct RefinementContext<'a> {
    pub shared: &'a SharedScenarioData,
    pub pools: &'a OfficerPools,
    pub constraints: Option<&'a CrewSearchConstraints>,
    pub chain_grind: Option<ChainGrindParams>,
    pub seed: u64,
}

/// Hill-climb around each finalist in `seeds` (crew paired with its already-confirmed score).
///
/// Returns confirmed results for refined crews only — the seeds themselves are already in the
/// caller's ranking and are not re-emitted.
pub(crate) fn refine_finalists(
    ctx: &RefinementContext<'_>,
    seeds: &[(CrewCandidate, f64)],
    params: &LocalRefinementParams,
    mut should_continue: impl FnMut() -> bool,
) -> LocalRefinementOutcome {
    let mut outcome = LocalRefinementOutcome::default();
    if params.seed_crews == 0 || params.max_rounds == 0 {
        return outcome;
    }

    // Shared across seeds: never evaluate the same crew twice in one pass, even when two
    // finalists' neighborhoods overlap (they usually do — finalists tend to be near each other).
    let mut evaluated: HashSet<u64> = HashSet::new();
    for (crew, _) in seeds {
        evaluated.insert(canonical_hash(crew));
    }

    for (seed_index, (seed_crew, seed_score)) in seeds.iter().take(params.seed_crews).enumerate() {
        if !should_continue() {
            break;
        }
        let source = canonical(seed_crew);
        let mut incumbent = source.clone();
        let mut incumbent_score = *seed_score;
        outcome.stats.seeds_refined += 1;

        for round in 0..params.max_rounds {
            if !should_continue() {
                break;
            }

            let mut neighbors = enumerate_single_slot_neighbors(&incumbent, ctx.pools, &evaluated);
            if params.destroy_repair_slots > 0 {
                let remaining = params
                    .max_neighbors_per_round
                    .saturating_sub(neighbors.len());
                neighbors.extend(enumerate_destroy_repair_neighbors(
                    &incumbent,
                    ctx.pools,
                    params.destroy_repair_slots,
                    params.destroy_repair_variants,
                    &evaluated,
                    remaining,
                ));
            }

            // Search constraints came from the user, so a neighbor that violates them is not a
            // candidate at all — drop before spending any sims.
            if let Some(constraints) = ctx.constraints {
                neighbors.retain(|n| constraints.satisfies(&n.crew));
            }
            neighbors.truncate(params.max_neighbors_per_round);
            outcome.stats.neighbors_generated += neighbors.len();
            if neighbors.is_empty() {
                break;
            }

            // Scout the incumbent in the same batch: comparing a shallow neighbor score against a
            // deeply-confirmed incumbent score would reject good neighbors on depth alone.
            let mut batch: Vec<CrewCandidate> = Vec::with_capacity(neighbors.len() + 1);
            batch.push(incumbent.clone());
            batch.extend(neighbors.iter().map(|n| n.crew.clone()));

            let round_seed = ctx
                .seed
                .wrapping_add((seed_index as u64).wrapping_mul(0x9E37_79B9))
                .wrapping_add((round as u64).wrapping_mul(0x85EB_CA6B));
            let scout = run_monte_carlo_scout_phase_with_shared(
                ctx.shared.clone(),
                &batch,
                params.scout_sims.max(1),
                round_seed,
                true,
                ctx.chain_grind.clone(),
            );
            outcome.stats.neighbors_scouted += neighbors.len();
            for n in &neighbors {
                evaluated.insert(n.hash);
            }

            let scout_scores = scores_by_hash(&scout);
            let incumbent_scout = scout_scores
                .get(&crew_candidate_stable_hash(&incumbent))
                .copied()
                .unwrap_or(f64::NEG_INFINITY);

            let mut promising: Vec<(&Neighbor, f64)> = neighbors
                .iter()
                .filter_map(|n| scout_scores.get(&n.hash).map(|s| (n, *s)))
                .filter(|(_, s)| *s > incumbent_scout)
                .collect();
            if promising.is_empty() {
                // Local optimum at scout depth.
                break;
            }
            promising.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.hash.cmp(&b.0.hash)));
            promising.truncate(params.confirm_top_k.max(1));

            let confirm_batch: Vec<CrewCandidate> =
                promising.iter().map(|(n, _)| n.crew.clone()).collect();
            let confirmed = run_monte_carlo_confirm_topk_with_shared(
                ctx.shared.clone(),
                &confirm_batch,
                params.confirm_sims.max(1),
                round_seed.wrapping_add(1),
                true,
                ctx.chain_grind.clone(),
                confirm_batch.len(),
            );
            outcome.stats.neighbors_confirmed += confirm_batch.len();
            outcome.stats.rounds_run += 1;

            let confirmed_scores = scores_by_hash(&confirmed);
            let best = confirmed
                .iter()
                .filter_map(|r| {
                    confirmed_scores
                        .get(&canonical_hash(&r.candidate))
                        .map(|s| (r, *s))
                })
                .max_by(|a, b| {
                    a.1.total_cmp(&b.1).then_with(|| {
                        canonical_hash(&b.0.candidate).cmp(&canonical_hash(&a.0.candidate))
                    })
                });

            let Some((best_result, best_score)) = best else {
                break;
            };
            if best_score <= incumbent_score {
                // Confirmation did not reproduce the scout-level gain — keep the incumbent and
                // stop rather than drifting on noise.
                break;
            }

            let refined = canonical(&best_result.candidate);
            let changes = diff_crews(&source, &refined);
            let hash = crew_candidate_stable_hash(&refined);
            outcome.provenance.insert(
                hash,
                RefinementProvenance {
                    kind: kind_from_changes(&changes),
                    source_crew: source.clone(),
                    changed_slots: changes,
                    baseline_score: *seed_score,
                    refined_score: best_score,
                },
            );
            outcome.results.push(best_result.clone());
            outcome.stats.improvements_accepted += 1;

            incumbent = refined;
            incumbent_score = best_score;
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pools() -> OfficerPools {
        OfficerPools {
            captains: vec!["Cap A".into(), "Cap B".into(), "Cap C".into()],
            bridge: vec!["Br A".into(), "Br B".into(), "Br C".into(), "Br D".into()],
            below_decks: vec!["Bd A".into(), "Bd B".into(), "Bd C".into(), "Bd D".into()],
        }
    }

    fn crew() -> CrewCandidate {
        CrewCandidate {
            captain: "Cap A".into(),
            bridge: vec!["Br A".into(), "Br B".into()],
            below_decks: vec!["Bd A".into(), "Bd B".into()],
        }
    }

    #[test]
    fn canonicalize_makes_seat_order_irrelevant_to_the_crew_hash() {
        let a = CrewCandidate {
            captain: "Cap A".into(),
            bridge: vec!["Br B".into(), "Br A".into()],
            below_decks: vec!["Bd B".into(), "Bd A".into()],
        };
        assert_eq!(canonical_hash(&a), canonical_hash(&crew()));
    }

    #[test]
    fn single_slot_neighbors_change_exactly_one_seat_and_never_duplicate_an_officer() {
        let base = crew();
        let neighbors = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        assert!(!neighbors.is_empty());
        for n in &neighbors {
            let changes = diff_crews(&base, &n.crew);
            assert_eq!(
                changes.len(),
                1,
                "expected a one-seat diff, got {changes:?} for {:?}",
                n.crew
            );
            let mut keys: Vec<String> = std::iter::once(normalize_officer_name(&n.crew.captain))
                .chain(n.crew.bridge.iter().map(|s| normalize_officer_name(s)))
                .chain(n.crew.below_decks.iter().map(|s| normalize_officer_name(s)))
                .collect();
            let before = keys.len();
            keys.sort();
            keys.dedup();
            assert_eq!(before, keys.len(), "officer seated twice in {:?}", n.crew);
        }
    }

    #[test]
    fn single_slot_neighbors_are_unique_and_exclude_the_source_crew() {
        let base = crew();
        let neighbors = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        let mut hashes: Vec<u64> = neighbors.iter().map(|n| n.hash).collect();
        let before = hashes.len();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(before, hashes.len(), "duplicate neighbors generated");
        assert!(!hashes.contains(&canonical_hash(&base)));
    }

    #[test]
    fn already_seen_crews_are_not_regenerated() {
        let base = crew();
        let all = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        let seen: HashSet<u64> = all.iter().take(3).map(|n| n.hash).collect();
        let filtered = enumerate_single_slot_neighbors(&base, &pools(), &seen);
        assert_eq!(filtered.len(), all.len() - 3);
        for n in &filtered {
            assert!(!seen.contains(&n.hash));
        }
    }

    #[test]
    fn enumeration_is_deterministic_across_calls() {
        let base = crew();
        let a = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        let b = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        let names = |v: &[Neighbor]| {
            v.iter()
                .map(|n| n.crew.clone())
                .collect::<Vec<CrewCandidate>>()
        };
        assert_eq!(names(&a), names(&b));
    }

    #[test]
    fn captain_swap_preserves_support_officers() {
        let base = crew();
        let neighbors = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        let captain_swaps: Vec<&Neighbor> = neighbors
            .iter()
            .filter(|n| n.crew.captain != base.captain)
            .collect();
        assert!(!captain_swaps.is_empty());
        for n in captain_swaps {
            assert_eq!(n.crew.bridge, base.bridge);
            assert_eq!(n.crew.below_decks, base.below_decks);
            let changes = diff_crews(&base, &n.crew);
            assert_eq!(
                kind_from_changes(&changes),
                RefinementKind::LocalCaptainSwap
            );
        }
    }

    #[test]
    fn non_captain_single_swaps_classify_as_local_swap() {
        let base = crew();
        let neighbors = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        let others: Vec<&Neighbor> = neighbors
            .iter()
            .filter(|n| n.crew.captain == base.captain)
            .collect();
        assert!(!others.is_empty());
        for n in others {
            let changes = diff_crews(&base, &n.crew);
            assert_eq!(kind_from_changes(&changes), RefinementKind::LocalSwap);
        }
    }

    #[test]
    fn destroy_repair_changes_multiple_slots_and_classifies_as_large_neighborhood() {
        let base = crew();
        let neighbors =
            enumerate_destroy_repair_neighbors(&base, &pools(), 2, 2, &HashSet::new(), 32);
        assert!(!neighbors.is_empty());
        let mut saw_multi = false;
        for n in &neighbors {
            let changes = diff_crews(&base, &n.crew);
            assert!(!changes.is_empty());
            if changes.len() >= 2 {
                saw_multi = true;
                assert_eq!(
                    kind_from_changes(&changes),
                    RefinementKind::LargeNeighborhoodRepair
                );
            }
            let mut keys: Vec<String> = std::iter::once(normalize_officer_name(&n.crew.captain))
                .chain(n.crew.bridge.iter().map(|s| normalize_officer_name(s)))
                .chain(n.crew.below_decks.iter().map(|s| normalize_officer_name(s)))
                .collect();
            let before = keys.len();
            keys.sort();
            keys.dedup();
            assert_eq!(before, keys.len(), "officer seated twice in {:?}", n.crew);
        }
        assert!(saw_multi, "destroy-repair produced no multi-slot neighbor");
    }

    #[test]
    fn destroy_repair_respects_its_budget() {
        let base = crew();
        let neighbors =
            enumerate_destroy_repair_neighbors(&base, &pools(), 2, 2, &HashSet::new(), 3);
        assert!(neighbors.len() <= 3);
    }

    #[test]
    fn destroy_repair_is_deterministic() {
        let base = crew();
        let a = enumerate_destroy_repair_neighbors(&base, &pools(), 2, 2, &HashSet::new(), 16);
        let b = enumerate_destroy_repair_neighbors(&base, &pools(), 2, 2, &HashSet::new(), 16);
        assert_eq!(
            a.iter().map(|n| n.hash).collect::<Vec<u64>>(),
            b.iter().map(|n| n.hash).collect::<Vec<u64>>()
        );
    }

    #[test]
    fn disabled_destroy_repair_generates_nothing() {
        let base = crew();
        assert!(
            enumerate_destroy_repair_neighbors(&base, &pools(), 0, 2, &HashSet::new(), 16)
                .is_empty()
        );
        assert!(
            enumerate_destroy_repair_neighbors(&base, &pools(), 2, 0, &HashSet::new(), 16)
                .is_empty()
        );
    }

    #[test]
    fn empty_pools_yield_no_neighbors() {
        let empty = OfficerPools {
            captains: Vec::new(),
            bridge: Vec::new(),
            below_decks: Vec::new(),
        };
        assert!(enumerate_single_slot_neighbors(&crew(), &empty, &HashSet::new()).is_empty());
    }

    #[test]
    fn diff_ignores_pure_reordering() {
        let a = crew();
        let b = CrewCandidate {
            captain: "Cap A".into(),
            bridge: vec!["Br B".into(), "Br A".into()],
            below_decks: vec!["Bd B".into(), "Bd A".into()],
        };
        assert!(diff_crews(&a, &b).is_empty());
    }

    #[test]
    fn method_labels_match_the_roadmap_names() {
        assert_eq!(RefinementKind::LocalSwap.method_label(), "local_swap");
        assert_eq!(
            RefinementKind::LocalCaptainSwap.method_label(),
            "local_captain_swap"
        );
        assert_eq!(
            RefinementKind::LargeNeighborhoodRepair.method_label(),
            "large_neighborhood_repair"
        );
    }

    #[test]
    fn provenance_gain_is_the_measured_improvement() {
        let p = RefinementProvenance {
            kind: RefinementKind::LocalSwap,
            source_crew: crew(),
            changed_slots: Vec::new(),
            baseline_score: 0.60,
            refined_score: 0.72,
        };
        assert!((p.gain() - 0.12).abs() < 1e-9);
    }

    #[test]
    fn constraints_reject_neighbors_that_drop_a_required_officer() {
        let base = crew();
        let constraints = CrewSearchConstraints {
            must_include: vec!["Br A".into()],
            ..Default::default()
        };
        let neighbors = enumerate_single_slot_neighbors(&base, &pools(), &HashSet::new());
        let kept: Vec<&Neighbor> = neighbors
            .iter()
            .filter(|n| constraints.satisfies(&n.crew))
            .collect();
        assert!(kept.len() < neighbors.len(), "constraint filtered nothing");
        for n in kept {
            assert!(
                n.crew
                    .bridge
                    .iter()
                    .chain(n.crew.below_decks.iter())
                    .any(|s| normalize_officer_name(s) == normalize_officer_name("Br A"))
                    || normalize_officer_name(&n.crew.captain) == normalize_officer_name("Br A")
            );
        }
    }
}
