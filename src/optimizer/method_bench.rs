//! Scoring support for the cross-method optimizer benchmark (roadmap §1.4).
//!
//! The `optimizer_method_bench` binary owns the lanes and the CLI; this module owns the parts
//! that have to be unit-testable or that need crate-internal access:
//!
//! - **Equal-budget planning** — turn one trial or wall-clock budget into per-lane knobs, so two
//!   methods can be compared without one of them quietly buying more simulation.
//! - **Reference sweep** — evaluate a bounded candidate set deeply, with no analytical prefilter
//!   and no search heuristic, to get a ground truth for recall and regret.
//! - **Prefilter false negatives** — how many reference top-K crews the analytical prefilter drops.
//! - **Seed-panel stability** — spread and agreement of a lane's answer across a seed panel.
//!
//! Everything here is deterministic for a given seed: the reference sweep reuses the production
//! generator and Monte Carlo entry points, and the sampling it does to stay tractable is driven by
//! the same seed the lanes use.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::combat::EnemyType;
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::BelowDecksPoolMode;
use crate::data::support_buffs::SupportBuffScenarioRequest;
use crate::optimizer::crew_generator::{CandidateStrategy, CrewCandidate, CrewGenerator};
use crate::optimizer::monte_carlo::scenario::build_shared_scenario_data_from_registry;
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_parallel_with_registry, DefenderOpponent,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::sort_and_analytical_prefilter;

/// Smallest genetic population the equal-trial planner will propose. Below this a GA degenerates
/// into a random walk, so the planner spends fewer generations rather than shrinking further.
pub const MIN_GENETIC_POPULATION: usize = 8;

/// How lane budgets are normalized before methods are compared.
///
/// Principle 2 of the roadmap ("every search method needs a simple control") only holds if the
/// control and the lane under test spend the same budget. `Native` keeps each lane's own knobs and
/// is the honest default for "how does the product behave today"; the other two make lanes
/// comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "kebab-case")]
pub enum BudgetMode {
    /// Each lane uses its own CLI knobs. Lanes are not budget-comparable.
    #[default]
    Native,
    /// Every lane is sized to the same total Monte Carlo trial budget.
    EqualTrials,
    /// Every lane is sized to the same wall-clock target, using a measured per-lane trial rate.
    EqualWallClock,
}

impl BudgetMode {
    /// Stable label for record output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::EqualTrials => "equal_trials",
            Self::EqualWallClock => "equal_wall_clock",
        }
    }
}

/// Tiered knobs sized to a trial budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TieredTrialPlan {
    pub candidates: usize,
    pub scout_sims: usize,
    pub confirm_sims: usize,
    pub top_k: usize,
    /// Trials the plan expects to spend: `candidates × scout_sims + top_k × confirm_sims`.
    pub projected_trials: u64,
}

/// Size a tiered lane to `trial_budget` by solving for the candidate count.
///
/// Depth (scout and confirm sims) is held fixed because it is what makes a tiered run trustworthy;
/// breadth is the dimension the budget buys. `top_k` is halved only when confirming that many
/// crews would leave no budget to scout them in the first place.
pub fn plan_tiered_equal_trials(
    trial_budget: u64,
    scout_sims: usize,
    confirm_sims: usize,
    top_k: usize,
) -> TieredTrialPlan {
    let scout_sims = scout_sims.max(1);
    let confirm_sims = confirm_sims.max(1);
    let mut top_k = top_k.max(1);
    // Every confirmed crew was scouted first, so `top_k` crews cost at least this much.
    let floor_cost = |k: usize| k as u64 * (scout_sims as u64 + confirm_sims as u64);
    while top_k > 1 && floor_cost(top_k) > trial_budget {
        top_k /= 2;
    }
    let confirm_cost = top_k as u64 * confirm_sims as u64;
    let scout_budget = trial_budget.saturating_sub(confirm_cost);
    let candidates = usize::try_from(scout_budget / scout_sims as u64)
        .unwrap_or(usize::MAX)
        .max(top_k);
    TieredTrialPlan {
        candidates,
        scout_sims,
        confirm_sims,
        top_k,
        projected_trials: candidates as u64 * scout_sims as u64 + confirm_cost,
    }
}

/// Genetic knobs sized to a trial budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GeneticTrialPlan {
    pub population: usize,
    pub generations: usize,
    pub sims_per_eval: usize,
    /// Upper bound on trials: dedupe across generations usually spends less.
    pub projected_trials: u64,
}

/// Size a genetic lane to `trial_budget` by solving for population, then generations.
///
/// Population is reduced first because a shorter run of a healthy population beats a long run of a
/// population too small to recombine. Once population hits [`MIN_GENETIC_POPULATION`], generations
/// absorb the rest of the cut.
pub fn plan_genetic_equal_trials(
    trial_budget: u64,
    generations: usize,
    sims_per_eval: usize,
) -> GeneticTrialPlan {
    let sims_per_eval = sims_per_eval.max(1);
    let mut generations = generations.max(1);
    let min_gen_cost = MIN_GENETIC_POPULATION as u64 * sims_per_eval as u64;
    while generations > 1 && generations as u64 * min_gen_cost > trial_budget {
        generations -= 1;
    }
    let per_generation = generations as u64 * sims_per_eval as u64;
    let population = usize::try_from(trial_budget / per_generation.max(1))
        .unwrap_or(usize::MAX)
        .max(MIN_GENETIC_POPULATION);
    GeneticTrialPlan {
        population,
        generations,
        sims_per_eval,
        projected_trials: population as u64 * generations as u64 * sims_per_eval as u64,
    }
}

/// Candidate count for a flat lane (one fixed-depth Monte Carlo pass per crew) under a budget.
pub fn plan_flat_equal_trials(trial_budget: u64, sims: usize) -> usize {
    usize::try_from(trial_budget / sims.max(1) as u64)
        .unwrap_or(usize::MAX)
        .max(1)
}

/// Convert a measured probe into a trial budget for a wall-clock target.
///
/// `probe_trials` trials took `probe_ms`; the lane gets whatever it can run in `target_ms` at that
/// rate. Returns `None` when the probe was too fast to time (0 ms), because a rate derived from a
/// zero-millisecond measurement is noise, not a measurement.
pub fn trial_budget_for_wall_clock(
    probe_trials: u64,
    probe_ms: u128,
    target_ms: u128,
) -> Option<u64> {
    if probe_trials == 0 || probe_ms == 0 || target_ms == 0 {
        return None;
    }
    let trials_per_ms = probe_trials as f64 / probe_ms as f64;
    let budget = trials_per_ms * target_ms as f64;
    (budget >= 1.0).then_some(budget as u64)
}

/// Fit `ms = fixed + trials / rate` from two probe runs and solve it for a wall-clock target.
///
/// A single probe cannot separate a lane's fixed setup (pool building, candidate generation,
/// shared scenario construction) from its per-trial cost, so it charges setup to every trial,
/// underestimates the rate, and the lane finishes far under its target. Two points of different
/// size separate the two terms.
///
/// Returns `None` when the points do not support a fit — equal or inverted timings, a
/// non-increasing trial count, or a target already consumed by fixed cost.
pub fn trial_budget_from_two_probes(
    small: (u64, u128),
    large: (u64, u128),
    target_ms: u128,
) -> Option<u64> {
    let (small_trials, small_ms) = small;
    let (large_trials, large_ms) = large;
    if large_trials <= small_trials || large_ms <= small_ms || target_ms == 0 {
        return None;
    }
    let marginal_trials = (large_trials - small_trials) as f64;
    let marginal_ms = (large_ms - small_ms) as f64;
    let trials_per_ms = marginal_trials / marginal_ms;
    if !trials_per_ms.is_finite() || trials_per_ms <= 0.0 {
        return None;
    }
    // Fixed cost is whatever the small probe spent beyond its trials at the marginal rate.
    let fixed_ms = (small_ms as f64 - small_trials as f64 / trials_per_ms).max(0.0);
    let usable_ms = target_ms as f64 - fixed_ms;
    if usable_ms <= 0.0 {
        return None;
    }
    let budget = usable_ms * trials_per_ms;
    (budget >= 1.0 && budget.is_finite()).then_some(budget as u64)
}

/// Scenario inputs for a reference sweep. Mirrors the fields the lanes use so the reference and
/// the lanes see the same ship, hostile, roster, and seat rules.
#[derive(Debug, Clone)]
pub struct ReferenceSweepParams<'a> {
    pub ship: &'a str,
    pub hostile: &'a str,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub profile_id: Option<&'a str>,
    pub enemy_type: EnemyType,
    pub below_decks_slots: usize,
    pub below_decks_pool_mode: BelowDecksPoolMode,
    pub seed: u64,
    /// Monte Carlo trials per crew. Should be well above any lane's confirm depth.
    pub sims_per_crew: usize,
    /// Tractability cap on how many crews the sweep evaluates.
    pub max_crews: usize,
}

/// One crew in the reference ranking.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceCrew {
    pub hash: u64,
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub win_rate: f64,
    pub win_rate_ci_low: f64,
    pub score: f32,
}

/// A deep evaluation of a bounded candidate set, used as ground truth for recall and regret.
#[derive(Debug, Clone)]
pub struct ReferenceSweep {
    pub sims_per_crew: usize,
    pub seed: u64,
    /// Crews the generator produced for this scenario.
    pub crews_generated: usize,
    /// Crews actually simulated (equals `crews_generated` unless the cap bound the sweep).
    pub crews_evaluated: usize,
    /// True when the sweep evaluated everything the generator produced, i.e. `max_crews` did not
    /// bind.
    ///
    /// This is **not** a claim of exhaustiveness over the legal crew space: `CrewGenerator`
    /// narrows officer pools before enumerating, so its output is a proposal space, not the space.
    /// Lanes that sample or evolve crews directly from the pools routinely propose crews outside
    /// it — watch `lane_best_crew_in_reference_set` on the lane records.
    pub covers_generator_space: bool,
    pub elapsed_ms: u128,
    /// Full ranked reference, best first.
    pub ranked: Vec<ReferenceCrew>,
    /// Exactly the crews that were simulated, for prefilter scoring against a known truth.
    pub evaluated: Vec<CrewCandidate>,
    /// Independent-seed re-evaluations — see [`ReferenceSweep::confirm`]. Deliberately kept out of
    /// `ranked`, which must stay the enumeration that recall is measured against.
    pub confirmed: HashMap<u64, ReferenceCrew>,
    /// Seed the confirmation pass used, once it has run.
    pub confirm_seed: Option<u64>,
}

impl ReferenceSweep {
    /// Best win rate in the reference, if it evaluated anything.
    pub fn best_win_rate(&self) -> Option<f64> {
        self.ranked.first().map(|r| r.win_rate)
    }

    /// Hashes of the leading `k` reference crews.
    pub fn top_k_hashes(&self, k: usize) -> HashSet<u64> {
        self.ranked.iter().take(k).map(|r| r.hash).collect()
    }

    /// The reference's confirmed evaluation of a crew, falling back to the selection-seed
    /// enumeration when no confirmation pass has judged it.
    pub fn evaluation_of(&self, hash: u64) -> Option<&ReferenceCrew> {
        self.confirmed
            .get(&hash)
            .or_else(|| self.ranked.iter().find(|r| r.hash == hash))
    }

    /// Best confirmed crew, once [`ReferenceSweep::confirm`] has run.
    pub fn confirmed_best(&self) -> Option<&ReferenceCrew> {
        self.confirmed
            .values()
            .max_by(|a, b| a.score.total_cmp(&b.score))
    }

    /// Re-evaluate the reference's leading crews, plus `extra` crews, on an independent seed.
    ///
    /// Two problems make the selection seed the wrong place to measure regret. First, sampling and
    /// evolving lanes propose crews the enumerating generator never emits, so the reference has no
    /// opinion on their winners at all. Second — and worse — every winner here was chosen by
    /// maximizing a noisy score over many crews on one seed, so every winner carries the winner's
    /// curse. Scoring them on the seed that selected them flatters whichever search looked at the
    /// most crews, which is exactly the thing the benchmark is trying to measure.
    ///
    /// One shared, independent seed for both sides removes both problems.
    pub fn confirm(
        &mut self,
        registry: &DataRegistry,
        params: &ReferenceSweepParams<'_>,
        confirm_seed: u64,
        top_m: usize,
        extra: &[CrewCandidate],
    ) {
        let mut wanted: Vec<CrewCandidate> = Vec::new();
        let mut seen: HashSet<u64> = HashSet::new();
        for crew in self
            .ranked
            .iter()
            .take(top_m.max(1))
            .map(|r| CrewCandidate {
                captain: r.captain.clone(),
                bridge: r.bridge.clone(),
                below_decks: r.below_decks.clone(),
            })
            .chain(extra.iter().cloned())
        {
            if seen.insert(crew_candidate_stable_hash(&crew)) {
                wanted.push(crew);
            }
        }
        if wanted.is_empty() {
            return;
        }
        let (results, _) = run_monte_carlo_parallel_with_registry(
            registry,
            params.ship,
            params.hostile,
            params.ship_tier,
            params.ship_level,
            &wanted,
            self.sims_per_crew,
            confirm_seed,
            params.profile_id,
            SupportBuffScenarioRequest::default(),
            None,
            DefenderOpponent::Hostile,
            None,
            None,
        );
        self.confirm_seed = Some(confirm_seed);
        for row in rank_results(results) {
            let hash = crew_candidate_stable_hash(&CrewCandidate {
                captain: row.captain.clone(),
                bridge: row.bridge.clone(),
                below_decks: row.below_decks.clone(),
            });
            self.confirmed.insert(
                hash,
                ReferenceCrew {
                    hash,
                    captain: row.captain,
                    bridge: row.bridge,
                    below_decks: row.below_decks,
                    win_rate: row.win_rate,
                    win_rate_ci_low: row.win_rate_ci_low,
                    score: row.score.value,
                },
            );
        }
    }
}

/// Confirmation seed for a selection seed: deterministic, and far enough away that the two runs
/// share no simulation draws.
pub fn confirmation_seed(selection_seed: u64) -> u64 {
    selection_seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(32)
        ^ 0x5DEE_CE66_D000_0001
}

/// Evaluate a bounded candidate set deeply, with no analytical prefilter and no search strategy.
///
/// The sweep is not exhaustive whenever the generator's own cap binds — `exhaustive` and
/// `crews_generated` say which happened, so a recall number is never read as covering a space it
/// did not cover.
pub fn run_reference_sweep(
    registry: &DataRegistry,
    params: &ReferenceSweepParams<'_>,
) -> ReferenceSweep {
    let max_crews = params.max_crews.max(1);
    let generator = CrewGenerator::with_strategy(CandidateStrategy {
        // One over the cap: a full result set is how we detect that the cap bound the sweep.
        max_candidates: Some(max_crews.saturating_add(1)),
        below_decks_pool_mode: params.below_decks_pool_mode,
        pvp_mode: false,
        enemy_type: params.enemy_type,
        below_decks_slots: params.below_decks_slots,
        constraints: None,
        roster_profile_id: params.profile_id.map(String::from),
        learned_officer_scores: None,
        ..CandidateStrategy::default()
    });
    let mut candidates = generator.generate_candidates_from_registry(
        registry,
        params.ship,
        params.hostile,
        params.seed,
        params.profile_id,
    );
    let crews_generated = candidates.len();
    let covers_generator_space = crews_generated <= max_crews;
    candidates.truncate(max_crews);

    let started = Instant::now();
    let (results, _) = run_monte_carlo_parallel_with_registry(
        registry,
        params.ship,
        params.hostile,
        params.ship_tier,
        params.ship_level,
        &candidates,
        params.sims_per_crew.max(1),
        params.seed,
        params.profile_id,
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    let elapsed_ms = started.elapsed().as_millis();
    let ranked = rank_results(results)
        .into_iter()
        .map(|r| ReferenceCrew {
            hash: crew_candidate_stable_hash(&CrewCandidate {
                captain: r.captain.clone(),
                bridge: r.bridge.clone(),
                below_decks: r.below_decks.clone(),
            }),
            captain: r.captain,
            bridge: r.bridge,
            below_decks: r.below_decks,
            win_rate: r.win_rate,
            win_rate_ci_low: r.win_rate_ci_low,
            score: r.score.value,
        })
        .collect();

    ReferenceSweep {
        sims_per_crew: params.sims_per_crew.max(1),
        seed: params.seed,
        crews_generated,
        crews_evaluated: candidates.len(),
        covers_generator_space,
        elapsed_ms,
        ranked,
        evaluated: candidates,
        confirmed: HashMap::new(),
        confirm_seed: None,
    }
}

/// How a lane's finalists compare to the reference ranking.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceScore {
    /// K used for both sides of the recall ratio.
    pub top_k: usize,
    /// Reference crews available at that K (smaller than `top_k` on tiny spaces).
    pub reference_top_k: usize,
    /// Share of the reference top-K the lane also placed in its own top-K.
    pub top_k_recall: Option<f64>,
    /// Whether the lane's winner is in the reference top-K.
    pub best_crew_in_reference_top_k: Option<bool>,
    pub reference_best_win_rate: Option<f64>,
    pub reference_best_score: Option<f32>,
    /// The lane's own measurement of its winner, at the lane's own simulation depth.
    pub lane_best_win_rate: Option<f64>,
    pub lane_best_score: Option<f32>,
    /// The reference's evaluation of the crew the lane picked. `None` when the reference never
    /// evaluated that crew.
    pub lane_best_crew_reference_win_rate: Option<f64>,
    pub lane_best_crew_reference_score: Option<f32>,
    /// True when the lane's winner came from the reference's own enumeration rather than from a
    /// later judging pass. Either way regret is computable; this says which.
    pub lane_best_crew_in_reference_set: bool,
    /// True when the lane's pick, judged at reference depth, beats everything the reference
    /// enumerated. Expected for sampling and evolving lanes, which propose crews the enumerator
    /// never emits; on an exhaustive reference it means the enumeration missed something.
    pub lane_pick_beats_reference_best: bool,
    /// True when regret was measured on a seed independent of the one that selected the winners.
    /// False means both sides still carry the winner's curse and regret reads optimistically.
    pub regret_confirmed_on_independent_seed: bool,
    /// Reference best minus the reference's own score for the lane's pick, clamped at zero.
    ///
    /// Both sides come from the reference sweep on purpose. Comparing a lane's self-reported score
    /// against the reference's would mix simulation depths — a lane confirming at 600 sims can
    /// "beat" a 400-sim reference on the very same crew, which is noise, not a better answer.
    pub win_rate_regret_vs_reference: Option<f64>,
    /// Ranking-score regret, judged the same way. Use this when `win_rate_discriminates` is false.
    pub score_regret_vs_reference: Option<f64>,
    /// Reference crews tied at the best win rate.
    pub reference_win_rate_ties_at_best: usize,
    /// False when every reference crew shares the best win rate: in PvE most matchups are won or
    /// lost outright, and then win-rate recall and regret measure tie-break noise, not search
    /// quality. The ranking score still separates crews by hull remaining and round-1 kills.
    pub win_rate_discriminates: bool,
}

/// Score a lane's ranked output against the reference sweep.
pub fn score_against_reference(
    ranked: &[RankedCrewResult],
    reference: &ReferenceSweep,
    top_k: usize,
) -> ReferenceScore {
    let top_k = top_k.max(1);
    let reference_hashes = reference.top_k_hashes(top_k);
    let reference_top_k = reference_hashes.len();
    let lane_hashes: HashSet<u64> = ranked
        .iter()
        .take(top_k)
        .map(|r| {
            crew_candidate_stable_hash(&CrewCandidate {
                captain: r.captain.clone(),
                bridge: r.bridge.clone(),
                below_decks: r.below_decks.clone(),
            })
        })
        .collect();
    let recall = (reference_top_k > 0).then(|| {
        lane_hashes.intersection(&reference_hashes).count() as f64 / reference_top_k as f64
    });
    let lane_best = ranked.first().map(|r| r.win_rate);
    let reference_best = reference.best_win_rate();
    let best_in_reference = ranked.first().map(|r| {
        reference_hashes.contains(&crew_candidate_stable_hash(&CrewCandidate {
            captain: r.captain.clone(),
            bridge: r.bridge.clone(),
            below_decks: r.below_decks.clone(),
        }))
    });
    let reference_best_score = reference.ranked.first().map(|r| r.score);
    let lane_best_score = ranked.first().map(|r| r.score.value);
    // Judge the lane's pick with the reference's own numbers, so both sides of the regret come
    // from the same simulation depth.
    let lane_pick = ranked.first().map(|r| {
        crew_candidate_stable_hash(&CrewCandidate {
            captain: r.captain.clone(),
            bridge: r.bridge.clone(),
            below_decks: r.below_decks.clone(),
        })
    });
    let judgement = lane_pick.and_then(|hash| reference.evaluation_of(hash));
    let enumerated = lane_pick
        .map(|hash| reference.ranked.iter().any(|r| r.hash == hash))
        .unwrap_or(false);
    // Both sides of the regret must come from the same pass. When a confirmation pass has run,
    // that is the confirmed set; otherwise fall back to the selection-seed enumeration and say so.
    let confirmed = reference.confirmed_best();
    let regret_basis_win_rate = confirmed.map(|c| c.win_rate).or(reference_best);
    let regret_basis_score = confirmed.map(|c| c.score).or(reference_best_score);
    let regret = match (regret_basis_win_rate, judgement) {
        (Some(basis), Some(pick)) => Some((basis - pick.win_rate).max(0.0)),
        _ => None,
    };
    let score_regret = match (regret_basis_score, judgement) {
        (Some(basis), Some(pick)) => Some((basis as f64 - pick.score as f64).max(0.0)),
        _ => None,
    };
    let beats_reference = matches!(
        (regret_basis_score, judgement),
        (Some(basis), Some(pick)) if pick.score > basis
    );
    let ties = reference_best.map_or(0, |best| {
        reference
            .ranked
            .iter()
            .filter(|r| (r.win_rate - best).abs() < f64::EPSILON)
            .count()
    });
    ReferenceScore {
        top_k,
        reference_top_k,
        top_k_recall: recall,
        best_crew_in_reference_top_k: best_in_reference,
        reference_best_win_rate: reference_best,
        reference_best_score,
        lane_best_win_rate: lane_best,
        lane_best_score,
        lane_best_crew_reference_win_rate: judgement.map(|r| r.win_rate),
        lane_best_crew_reference_score: judgement.map(|r| r.score),
        lane_best_crew_in_reference_set: enumerated,
        lane_pick_beats_reference_best: beats_reference,
        regret_confirmed_on_independent_seed: reference.confirm_seed.is_some(),
        win_rate_regret_vs_reference: regret,
        score_regret_vs_reference: score_regret,
        reference_win_rate_ties_at_best: ties,
        win_rate_discriminates: ties > 0 && ties < reference.ranked.len(),
    }
}

/// What the analytical prefilter costs in reference top-K crews.
#[derive(Debug, Clone, Serialize)]
pub struct PrefilterFalseNegativeScore {
    /// Candidates the prefilter was asked to keep.
    pub keep: usize,
    /// Candidates it was given (the reference's evaluated set).
    pub evaluated: usize,
    pub reference_top_k: usize,
    pub survivors_in_reference_top_k: usize,
    pub dropped_from_reference_top_k: usize,
    /// Dropped share of the reference top-K.
    pub false_negative_rate: Option<f64>,
    /// Best win rate among dropped reference top-K crews — the cost of the worst mistake.
    pub best_dropped_win_rate: Option<f64>,
    /// Reference best minus the best surviving reference crew, clamped at zero: what the prefilter
    /// costs the eventual winner, not just the top-K set.
    pub win_rate_loss_at_best: Option<f64>,
}

/// Run the production analytical prefilter over the reference's evaluated crews and report how many
/// known-good crews it deleted.
///
/// This is the roadmap's "hard-filter false-negative test" for a soft filter: the prefilter is only
/// allowed to prioritize, so every reference top-K crew it drops before Monte Carlo is a crew the
/// search can no longer find.
pub fn score_analytical_prefilter(
    registry: &DataRegistry,
    params: &ReferenceSweepParams<'_>,
    reference: &ReferenceSweep,
    keep: usize,
    top_k: usize,
    enable_learned_pair_prior: bool,
) -> PrefilterFalseNegativeScore {
    let top_k = top_k.max(1);
    let reference_hashes = reference.top_k_hashes(top_k);
    let shared = build_shared_scenario_data_from_registry(
        registry,
        params.ship,
        params.hostile,
        params.ship_tier,
        params.ship_level,
        params.profile_id,
        SupportBuffScenarioRequest::default(),
        DefenderOpponent::Hostile,
        None,
        None,
    );
    let (kept, _) = sort_and_analytical_prefilter(
        &shared,
        reference.evaluated.clone(),
        params.seed,
        Some(keep.max(1)),
        &[],
        &[],
        enable_learned_pair_prior,
    );
    let kept_hashes: HashSet<u64> = kept.iter().map(crew_candidate_stable_hash).collect();
    let survivors = reference_hashes.intersection(&kept_hashes).count();
    let dropped = reference_hashes.len().saturating_sub(survivors);
    let best_dropped = reference
        .ranked
        .iter()
        .take(top_k)
        .filter(|r| !kept_hashes.contains(&r.hash))
        .map(|r| r.win_rate)
        .max_by(f64::total_cmp);
    let best_survivor = reference
        .ranked
        .iter()
        .find(|r| kept_hashes.contains(&r.hash))
        .map(|r| r.win_rate);
    let loss_at_best = match (reference.best_win_rate(), best_survivor) {
        (Some(best), Some(survivor)) => Some((best - survivor).max(0.0)),
        (Some(best), None) => Some(best),
        _ => None,
    };
    PrefilterFalseNegativeScore {
        keep: keep.max(1),
        evaluated: reference.evaluated.len(),
        reference_top_k: reference_hashes.len(),
        survivors_in_reference_top_k: survivors,
        dropped_from_reference_top_k: dropped,
        false_negative_rate: (!reference_hashes.is_empty())
            .then(|| dropped as f64 / reference_hashes.len() as f64),
        best_dropped_win_rate: best_dropped,
        win_rate_loss_at_best: loss_at_best,
    }
}

/// One lane run on one seed, reduced to the fields stability aggregation needs.
#[derive(Debug, Clone)]
pub struct StabilitySample {
    pub case: String,
    pub method: String,
    pub seed: u64,
    pub best_win_rate: Option<f64>,
    pub best_crew_hash: Option<u64>,
    pub top_k_recall: Option<f64>,
    pub win_rate_regret: Option<f64>,
    /// Ranking-score regret vs the reference. The metric to threshold on when win rate saturates.
    pub score_regret: Option<f64>,
    pub elapsed_ms: u128,
    pub trials_run_total: u64,
}

/// Spread and agreement of one lane's answers across a seed panel.
///
/// A lane that wins on average but returns a different crew on every seed is not the same product
/// as a lane that returns the same crew every time; `distinct_best_crews` and
/// `modal_best_crew_share` are what separate them.
#[derive(Debug, Clone, Serialize)]
pub struct StabilityAggregate {
    pub case: String,
    pub method: String,
    pub seeds: usize,
    pub best_win_rate_mean: Option<f64>,
    /// Population standard deviation across the panel.
    pub best_win_rate_stddev: Option<f64>,
    pub best_win_rate_min: Option<f64>,
    pub best_win_rate_max: Option<f64>,
    /// Distinct winning crews across the panel (1 = the lane always agrees with itself).
    pub distinct_best_crews: usize,
    /// Share of seeds that returned the most common winning crew.
    pub modal_best_crew_share: Option<f64>,
    pub top_k_recall_mean: Option<f64>,
    pub win_rate_regret_mean: Option<f64>,
    pub win_rate_regret_max: Option<f64>,
    pub score_regret_mean: Option<f64>,
    pub score_regret_max: Option<f64>,
    pub elapsed_ms_mean: f64,
    pub trials_run_mean: f64,
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn population_stddev(values: &[f64]) -> Option<f64> {
    let mean = mean(values)?;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    Some(variance.sqrt())
}

/// Group samples by (case, method) and summarize each group. Output order is deterministic:
/// case, then method.
pub fn aggregate_stability(samples: &[StabilitySample]) -> Vec<StabilityAggregate> {
    let mut groups: HashMap<(String, String), Vec<&StabilitySample>> = HashMap::new();
    for sample in samples {
        groups
            .entry((sample.case.clone(), sample.method.clone()))
            .or_default()
            .push(sample);
    }
    let mut out: Vec<StabilityAggregate> = groups
        .into_iter()
        .map(|((case, method), rows)| {
            let win_rates: Vec<f64> = rows.iter().filter_map(|r| r.best_win_rate).collect();
            let recalls: Vec<f64> = rows.iter().filter_map(|r| r.top_k_recall).collect();
            let regrets: Vec<f64> = rows.iter().filter_map(|r| r.win_rate_regret).collect();
            let score_regrets: Vec<f64> = rows.iter().filter_map(|r| r.score_regret).collect();
            let mut crew_counts: HashMap<u64, usize> = HashMap::new();
            for hash in rows.iter().filter_map(|r| r.best_crew_hash) {
                *crew_counts.entry(hash).or_default() += 1;
            }
            let hashed_rows: usize = crew_counts.values().sum();
            let modal = crew_counts.values().max().copied();
            StabilityAggregate {
                case,
                method,
                seeds: rows.len(),
                best_win_rate_mean: mean(&win_rates),
                best_win_rate_stddev: population_stddev(&win_rates),
                best_win_rate_min: win_rates.iter().copied().reduce(f64::min),
                best_win_rate_max: win_rates.iter().copied().reduce(f64::max),
                distinct_best_crews: crew_counts.len(),
                modal_best_crew_share: modal
                    .filter(|_| hashed_rows > 0)
                    .map(|m| m as f64 / hashed_rows as f64),
                top_k_recall_mean: mean(&recalls),
                win_rate_regret_mean: mean(&regrets),
                win_rate_regret_max: regrets.iter().copied().reduce(f64::max),
                score_regret_mean: mean(&score_regrets),
                score_regret_max: score_regrets.iter().copied().reduce(f64::max),
                elapsed_ms_mean: rows.iter().map(|r| r.elapsed_ms as f64).sum::<f64>()
                    / rows.len() as f64,
                trials_run_mean: rows.iter().map(|r| r.trials_run_total as f64).sum::<f64>()
                    / rows.len() as f64,
            }
        })
        .collect();
    out.sort_by(|a, b| a.case.cmp(&b.case).then_with(|| a.method.cmp(&b.method)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::optimizer::ranking::RankingScore;

    fn crew(captain: &str) -> CrewCandidate {
        CrewCandidate {
            captain: captain.to_string(),
            bridge: vec!["b1".to_string(), "b2".to_string()],
            below_decks: vec!["d1".to_string()],
        }
    }

    fn ranked_row(captain: &str, win_rate: f64) -> RankedCrewResult {
        let crew = crew(captain);
        RankedCrewResult {
            captain: crew.captain,
            bridge: crew.bridge,
            below_decks: crew.below_decks,
            trials_run: 100,
            win_rate,
            win_rate_ci_low: win_rate,
            win_rate_ci_high: win_rate,
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.0,
            loss_rate: 1.0 - win_rate,
            loss_rate_ci_low: 1.0 - win_rate,
            loss_rate_ci_high: 1.0 - win_rate,
            r1_kill_rate: 0.0,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 0.0,
            avg_hull_remaining: 0.0,
            avg_hull_remaining_ci_low: 0.0,
            avg_hull_remaining_ci_high: 0.0,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            score: RankingScore {
                value: win_rate as f32,
            },
            chain: None,
            expected_hull_damage: None,
        }
    }

    /// Reference sweep with the given captains ranked best-first at descending win rates.
    fn reference(captains: &[&str]) -> ReferenceSweep {
        let ranked: Vec<ReferenceCrew> = captains
            .iter()
            .enumerate()
            .map(|(i, captain)| {
                let c = crew(captain);
                let win_rate = 1.0 - i as f64 * 0.1;
                ReferenceCrew {
                    hash: crew_candidate_stable_hash(&c),
                    captain: c.captain,
                    bridge: c.bridge,
                    below_decks: c.below_decks,
                    win_rate,
                    win_rate_ci_low: win_rate,
                    score: win_rate as f32,
                }
            })
            .collect();
        ReferenceSweep {
            sims_per_crew: 2_000,
            seed: 7,
            crews_generated: captains.len(),
            crews_evaluated: captains.len(),
            covers_generator_space: true,
            elapsed_ms: 1,
            evaluated: captains.iter().map(|c| crew(c)).collect(),
            ranked,
            confirmed: HashMap::new(),
            confirm_seed: None,
        }
    }

    fn sample(case: &str, method: &str, seed: u64, win_rate: f64, hash: u64) -> StabilitySample {
        StabilitySample {
            case: case.to_string(),
            method: method.to_string(),
            seed,
            best_win_rate: Some(win_rate),
            best_crew_hash: Some(hash),
            top_k_recall: Some(0.5),
            win_rate_regret: Some(0.1),
            score_regret: Some(0.01),
            elapsed_ms: 100,
            trials_run_total: 1_000,
        }
    }

    #[test]
    fn tiered_plan_spends_close_to_the_budget() {
        let plan = plan_tiered_equal_trials(100_000, 80, 600, 12);
        assert!(plan.projected_trials <= 100_000, "{plan:?} overspent");
        assert!(
            plan.projected_trials as f64 >= 100_000.0 * 0.99,
            "{plan:?} left budget unspent"
        );
        assert_eq!(plan.top_k, 12);
        assert_eq!(plan.scout_sims, 80);
        assert_eq!(plan.confirm_sims, 600);
    }

    #[test]
    fn tiered_plan_shrinks_top_k_when_confirmation_alone_would_overrun() {
        // 12 confirmations at 600 sims each cannot fit in 2_000 trials.
        let plan = plan_tiered_equal_trials(2_000, 80, 600, 12);
        assert!(plan.top_k < 12, "{plan:?} kept an unaffordable top_k");
        assert!(
            plan.candidates >= plan.top_k,
            "{plan:?} cannot confirm its own top_k"
        );
    }

    #[test]
    fn tiered_plan_stays_runnable_on_a_tiny_budget() {
        let plan = plan_tiered_equal_trials(1, 80, 600, 12);
        assert_eq!(plan.top_k, 1);
        assert!(plan.candidates >= 1);
    }

    #[test]
    fn tiered_plan_buys_more_candidates_with_more_budget() {
        let small = plan_tiered_equal_trials(50_000, 80, 600, 12);
        let large = plan_tiered_equal_trials(200_000, 80, 600, 12);
        assert!(large.candidates > small.candidates);
    }

    #[test]
    fn genetic_plan_keeps_a_viable_population() {
        let plan = plan_genetic_equal_trials(1_000, 20, 120);
        assert!(plan.population >= MIN_GENETIC_POPULATION);
        assert!(
            plan.generations < 20,
            "{plan:?} should have traded generations away"
        );
    }

    #[test]
    fn genetic_plan_spends_a_large_budget_on_population() {
        let plan = plan_genetic_equal_trials(192_000, 20, 120);
        assert_eq!(plan.generations, 20);
        assert_eq!(plan.population, 80);
        assert_eq!(plan.projected_trials, 192_000);
    }

    #[test]
    fn flat_plan_divides_budget_by_depth() {
        assert_eq!(plan_flat_equal_trials(60_000, 600), 100);
        assert_eq!(plan_flat_equal_trials(1, 600), 1);
    }

    #[test]
    fn wall_clock_budget_scales_with_the_measured_rate() {
        // 10_000 trials in 100 ms = 100 trials/ms; a 1_000 ms target buys 100_000.
        assert_eq!(
            trial_budget_for_wall_clock(10_000, 100, 1_000),
            Some(100_000)
        );
    }

    #[test]
    fn two_probe_fit_removes_fixed_setup_cost() {
        // Lane costs 50 ms of setup plus 1 ms per 100 trials.
        let small = (10_000u64, 150u128);
        let large = (20_000u64, 250u128);
        // A 1_050 ms target leaves 1_000 ms of trial time = 100_000 trials.
        assert_eq!(
            trial_budget_from_two_probes(small, large, 1_050),
            Some(100_000)
        );
    }

    #[test]
    fn two_probe_fit_beats_single_point_when_setup_dominates() {
        let small = (10_000u64, 150u128);
        let large = (20_000u64, 250u128);
        let single = trial_budget_for_wall_clock(10_000, 150, 1_050).unwrap();
        let fitted = trial_budget_from_two_probes(small, large, 1_050).unwrap();
        assert!(
            fitted > single,
            "single-point {single} should undershoot the fitted {fitted}"
        );
    }

    #[test]
    fn two_probe_fit_refuses_degenerate_points() {
        assert_eq!(trial_budget_from_two_probes((10, 5), (10, 9), 1_000), None);
        assert_eq!(trial_budget_from_two_probes((10, 9), (20, 9), 1_000), None);
        // Target smaller than the fixed cost the fit implies.
        assert_eq!(
            trial_budget_from_two_probes((10_000, 150), (20_000, 250), 10),
            None
        );
    }

    #[test]
    fn wall_clock_budget_refuses_an_untimed_probe() {
        assert_eq!(trial_budget_for_wall_clock(10_000, 0, 1_000), None);
        assert_eq!(trial_budget_for_wall_clock(0, 100, 1_000), None);
    }

    #[test]
    fn recall_counts_reference_crews_the_lane_also_ranked() {
        let reference = reference(&["a", "b", "c", "d"]);
        // Lane top-3 shares "a" and "c" with the reference top-3.
        let ranked = vec![
            ranked_row("a", 0.95),
            ranked_row("c", 0.85),
            ranked_row("z", 0.5),
        ];
        let score = score_against_reference(&ranked, &reference, 3);
        assert_eq!(score.reference_top_k, 3);
        assert!((score.top_k_recall.unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(score.best_crew_in_reference_top_k, Some(true));
    }

    #[test]
    fn regret_is_reference_best_minus_the_references_own_view_of_the_lane_pick() {
        let reference = reference(&["a", "b", "c"]);
        // The lane reports 0.99 for crew "b"; the reference measured "b" at 0.9.
        let ranked = vec![ranked_row("b", 0.99)];
        let score = score_against_reference(&ranked, &reference, 3);
        assert_eq!(score.reference_best_win_rate, Some(1.0));
        assert_eq!(score.lane_best_win_rate, Some(0.99));
        assert_eq!(score.lane_best_crew_reference_win_rate, Some(0.9));
        // Regret uses the reference's 0.9, not the lane's optimistic 0.99.
        assert!((score.win_rate_regret_vs_reference.unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn a_lane_reporting_a_better_number_than_the_reference_earns_no_credit_for_it() {
        // Same crew, deeper sims, a slightly higher self-reported win rate. Regret stays at the
        // reference's own gap so extra depth cannot masquerade as a better answer.
        let reference = reference(&["a", "b"]);
        let ranked = vec![ranked_row("b", 1.0)];
        let score = score_against_reference(&ranked, &reference, 2);
        assert_eq!(score.lane_best_win_rate, Some(1.0));
        assert!((score.win_rate_regret_vs_reference.unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn a_pick_outside_the_reference_set_reports_no_regret_rather_than_a_wrong_one() {
        let reference = reference(&["a", "b"]);
        let ranked = vec![ranked_row("z", 1.0)];
        let score = score_against_reference(&ranked, &reference, 2);
        assert!(!score.lane_best_crew_in_reference_set);
        assert_eq!(score.win_rate_regret_vs_reference, None);
        assert_eq!(score.score_regret_vs_reference, None);
        assert_eq!(score.best_crew_in_reference_top_k, Some(false));
    }

    #[test]
    fn scoring_an_empty_lane_yields_no_recall_claim() {
        let reference = reference(&["a", "b"]);
        let score = score_against_reference(&[], &reference, 2);
        assert_eq!(score.top_k_recall, Some(0.0));
        assert_eq!(score.lane_best_win_rate, None);
        assert_eq!(score.win_rate_regret_vs_reference, None);
        assert_eq!(score.best_crew_in_reference_top_k, None);
    }

    #[test]
    fn recall_denominator_shrinks_to_the_reference_size() {
        let reference = reference(&["a"]);
        let ranked = vec![ranked_row("a", 1.0)];
        let score = score_against_reference(&ranked, &reference, 10);
        assert_eq!(score.reference_top_k, 1);
        assert_eq!(score.top_k_recall, Some(1.0));
    }

    #[test]
    fn stability_aggregate_reports_spread_and_agreement() {
        let samples = vec![
            sample("case_a", "tiered", 1, 0.90, 11),
            sample("case_a", "tiered", 2, 0.80, 11),
            sample("case_a", "tiered", 3, 0.70, 22),
        ];
        let aggregates = aggregate_stability(&samples);
        assert_eq!(aggregates.len(), 1);
        let a = &aggregates[0];
        assert_eq!(a.seeds, 3);
        assert!((a.best_win_rate_mean.unwrap() - 0.80).abs() < 1e-9);
        assert!((a.best_win_rate_stddev.unwrap() - 0.081_649_658).abs() < 1e-6);
        assert_eq!(a.best_win_rate_min, Some(0.70));
        assert_eq!(a.best_win_rate_max, Some(0.90));
        assert_eq!(a.distinct_best_crews, 2);
        assert!((a.modal_best_crew_share.unwrap() - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn stability_aggregate_groups_by_case_and_method_in_order() {
        let samples = vec![
            sample("case_b", "tiered", 1, 0.5, 1),
            sample("case_a", "genetic", 1, 0.5, 1),
            sample("case_a", "tiered", 1, 0.5, 1),
        ];
        let aggregates = aggregate_stability(&samples);
        let keys: Vec<(&str, &str)> = aggregates
            .iter()
            .map(|a| (a.case.as_str(), a.method.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("case_a", "genetic"),
                ("case_a", "tiered"),
                ("case_b", "tiered")
            ]
        );
    }

    #[test]
    fn stability_aggregate_tolerates_missing_metrics() {
        let samples = vec![StabilitySample {
            case: "case_a".to_string(),
            method: "linear_eval".to_string(),
            seed: 1,
            best_win_rate: None,
            best_crew_hash: None,
            top_k_recall: None,
            win_rate_regret: None,
            score_regret: None,
            elapsed_ms: 5,
            trials_run_total: 0,
        }];
        let aggregates = aggregate_stability(&samples);
        assert_eq!(aggregates[0].best_win_rate_mean, None);
        assert_eq!(aggregates[0].distinct_best_crews, 0);
        assert_eq!(aggregates[0].modal_best_crew_share, None);
    }
}
