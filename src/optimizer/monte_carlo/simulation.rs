//! Simulation orchestration: run_monte_carlo* and SimulationResult.

use crate::combat::engine::simulate_combat_from_setup_reusing_attacker;
use crate::combat::{
    build_combat_setup_with_officer_stat, simulate_combat_with_defender_faction_and_defender_crew,
    CombatEvent, OpponentFactionTag, SimulationConfig, TraceMode,
};
use crate::data::data_registry::DataRegistry;
use crate::data::support_buffs;
use crate::optimizer::chain::{
    run_chain_trial, secondary_draw, ChainGrindParams, ChainSimulationSummary,
};
use crate::optimizer::crew_generator::CrewCandidate;
use crate::perf_log;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use super::scenario::{
    build_shared_scenario_data_from_registry, build_shared_scenario_data_standalone,
    scenario_to_combat_input_from_shared, stable_scenario_seed_panel, DefenderOpponent,
    PlayerDefenderOfficerCrewOverride, SharedScenarioData,
};

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub candidate: CrewCandidate,
    /// True when crew-health-scaled raw stat effects could not be evaluated;
    /// None for historical results that did not persist this diagnostic.
    pub unsupported_crew_health_effects: Option<bool>,
    /// Monte Carlo trials actually executed (may be less than requested when scout early-stop fires).
    pub trials_run: usize,
    pub win_rate: f64,
    /// Wilson 95% interval lower bound (inclusive), clamped to [0, 1].
    pub win_rate_ci_low: f64,
    pub win_rate_ci_high: f64,
    pub stall_rate: f64,
    pub stall_rate_ci_low: f64,
    pub stall_rate_ci_high: f64,
    pub loss_rate: f64,
    pub loss_rate_ci_low: f64,
    pub loss_rate_ci_high: f64,
    /// Fraction of trials where the attacker won on round 1 (no round-limit stall).
    pub r1_kill_rate: f64,
    pub r1_kill_rate_ci_low: f64,
    pub r1_kill_rate_ci_high: f64,
    /// Mean actual attacker hull / effective maximum hull on wins; zero on losses/stalls.
    pub avg_hull_remaining: f64,
    /// Normal-approx 95% CI for the per-trial mean (hull fraction on wins, 0 on losses).
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
    /// Mean defender (hostile) hull fraction remaining (0–1), averaged over **all** trials.
    pub avg_defender_hull_remaining: f64,
    pub avg_defender_hull_remaining_ci_low: f64,
    pub avg_defender_hull_remaining_ci_high: f64,
    /// When set, `win_rate` is chain primary success rate and `avg_hull_remaining` is the conditional secondary mean.
    pub chain: Option<ChainSimulationSummary>,
    /// Closed-form expected hull damage when ranked by linear eval (no Monte Carlo).
    pub expected_hull_damage: Option<f64>,
}

/// A zeroed, all-loss [`SimulationResult`] for `candidate`.
///
/// Used only on the unreachable branch of the optimizer's row-alignment steps, where a crew
/// must map to exactly one result row. Those steps used to `expect` the row's presence; since
/// release builds set `panic = "abort"`, a broken alignment invariant took the whole server
/// process down rather than failing one job. Degrading to an all-loss row keeps results aligned
/// 1:1 with candidates and sorts the affected crew to the bottom instead of crashing.
///
/// Callers pair this with a `debug_assert!` so a genuinely broken invariant still fails loudly
/// in debug and test builds.
pub(crate) fn zeroed_loss_result(candidate: CrewCandidate) -> SimulationResult {
    SimulationResult {
        candidate,
        unsupported_crew_health_effects: None,
        trials_run: 0,
        win_rate: 0.0,
        win_rate_ci_low: 0.0,
        win_rate_ci_high: 0.0,
        stall_rate: 0.0,
        stall_rate_ci_low: 0.0,
        stall_rate_ci_high: 0.0,
        loss_rate: 1.0,
        loss_rate_ci_low: 1.0,
        loss_rate_ci_high: 1.0,
        r1_kill_rate: 0.0,
        r1_kill_rate_ci_low: 0.0,
        r1_kill_rate_ci_high: 0.0,
        avg_hull_remaining: 0.0,
        avg_hull_remaining_ci_low: 0.0,
        avg_hull_remaining_ci_high: 0.0,
        avg_defender_hull_remaining: 0.0,
        avg_defender_hull_remaining_ci_low: 0.0,
        avg_defender_hull_remaining_ci_high: 0.0,
        chain: None,
        expected_hull_damage: None,
    }
}

/// Stable, role-aware crew set identity for candidate deduplication and benchmark comparisons.
/// Captain identity and the bridge/below-decks distinction matter; order within either
/// non-captain role does not. Never use this key for numerical simulation results:
/// stochastic officer effects consume RNG in seat order.
pub fn crew_candidate_stable_hash(c: &CrewCandidate) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.captain.hash(&mut h);
    c.bridge.len().hash(&mut h);
    let mut bridge: Vec<&str> = c.bridge.iter().map(String::as_str).collect();
    bridge.sort_unstable();
    for s in bridge {
        s.hash(&mut h);
    }
    c.below_decks.len().hash(&mut h);
    let mut below_decks: Vec<&str> = c.below_decks.iter().map(String::as_str).collect();
    below_decks.sort_unstable();
    for s in below_decks {
        s.hash(&mut h);
    }
    h.finish()
}

/// Identity for numerical results, seeds, and persisted history. Keep bridge and below-decks
/// order because the current combat engine evaluates stochastic officer effects in seat order.
pub fn crew_candidate_ordered_hash(c: &CrewCandidate) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.captain.hash(&mut h);
    c.bridge.hash(&mut h);
    c.below_decks.hash(&mut h);
    h.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedPanelPolicy {
    /// Preserve the historical candidate-hashed seed stream.
    CandidateDistinct,
    /// Pair scout comparisons by giving every candidate the same scenario seed stream.
    CommonScenario,
}

#[inline]
fn apply_seed_panel_policy(
    input: &mut super::scenario::CombatSimulationInput,
    shared: &SharedScenarioData,
    requested_seed: u64,
    policy: SeedPanelPolicy,
) {
    if policy == SeedPanelPolicy::CommonScenario {
        input.base_seed = stable_scenario_seed_panel(&shared.ship, &shared.hostile, requested_seed);
    }
}

/// Wilson score 95% two-sided interval for a binomial proportion.
fn wilson_95_interval(successes: usize, trials: usize) -> (f64, f64) {
    if trials == 0 {
        return (0.0, 1.0);
    }
    const Z: f64 = 1.96;
    let n = trials as f64;
    let p = successes as f64 / n;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let rad = Z / denom * ((p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt());
    let lo = (center - rad).clamp(0.0, 1.0);
    let hi = (center + rad).clamp(0.0, 1.0);
    (lo, hi)
}

/// Wilson score upper bound (approx. 95% interval) for binomial win proportion.
/// Used to drop scout iterations for crews that are very unlikely to rank in the top K.
fn win_rate_upper_wilson_95(wins: usize, trials: usize) -> f64 {
    wilson_95_interval(wins, trials).1
}

/// Normal-approx 95% upper bound for a running mean from Welford accumulators (`mean`, `m2`, `n`),
/// clamped to `[0, 1]`. Returns 1.0 for `n < 2` (interval undefined → never prune on it).
fn normal_upper_95(mean: f64, m2: f64, n: usize) -> f64 {
    if n < 2 {
        return 1.0;
    }
    let var = m2 / (n as f64 - 1.0);
    let se = (var / n as f64).sqrt().max(0.0);
    (mean + 1.96 * se).clamp(0.0, 1.0)
}

#[derive(Clone, Copy)]
struct ScoutEarlyStopCfg {
    min_trials: usize,
    check_every: usize,
    /// Stop remaining iterations if upper bound on win rate falls strictly below this.
    eliminate_upper_below: f64,
}

impl ScoutEarlyStopCfg {
    fn for_scout_iterations(max_iterations: usize) -> Self {
        let min_trials = (max_iterations / 8).max(64).min(max_iterations.max(1));
        Self {
            min_trials,
            check_every: 50,
            eliminate_upper_below: 0.055,
        }
    }
}

/// Configuration for progressive abandonment: after `min_trials`, at each `check_every`
/// checkpoint, compare the candidate's Wilson upper bound against the best crew's Wilson
/// lower bound.  If the candidate cannot close the gap (upper < lower - margin), abandon it.
#[derive(Clone, Copy)]
struct ProgressiveAbandonCfg {
    min_trials: usize,
    check_every: usize,
    /// Gap below the best-crew Wilson lower bound before abandoning a candidate.
    /// Conservative default: 0.05 (5 percentage points).
    margin: f64,
}

impl ProgressiveAbandonCfg {
    fn for_scout_iterations(max_iterations: usize) -> Self {
        let min_trials = (max_iterations / 8).max(64).min(max_iterations.max(1));
        Self {
            min_trials,
            check_every: 50,
            margin: 0.05,
        }
    }

    /// Abandonment config for the full Monte Carlo (confirm / exhaustive) pass. Each crew runs at
    /// least `min_trials` before it can be cut, so reported stats for survivors stay stable.
    ///
    /// The margin is on the blended-score scale. Because hull contributes only 0.2 of the score, a
    /// score margin of 0.05 is a 0.25 *hull* gap that almost never exists, so it pruned nothing on
    /// real hull-spread matchups. The default is **0.01** (≈ a 0.05 hull gap, and still ~4× the
    /// score CI half-width at the first checkpoint, so it never over-prunes — verified by the
    /// equivalence tests). Tunable via `KOBAYASHI_EARLYSTOP_MARGIN` / `KOBAYASHI_EARLYSTOP_MIN_TRIALS_DIV`.
    fn for_full_mc(max_iterations: usize) -> Self {
        let div = env_usize("KOBAYASHI_EARLYSTOP_MIN_TRIALS_DIV")
            .unwrap_or(8)
            .max(1);
        let min_trials = (max_iterations / div).max(64).min(max_iterations.max(1));
        Self {
            min_trials,
            check_every: 50,
            margin: env_f64("KOBAYASHI_EARLYSTOP_MARGIN").unwrap_or(0.01),
        }
    }
}

/// Parse a positive f64 env var, ignoring missing/invalid/non-finite values.
fn env_f64(key: &str) -> Option<f64> {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|f| f.is_finite() && *f >= 0.0)
}

/// Parse a positive usize env var, ignoring missing/invalid values.
fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
}

/// A finished crew's ranking **score** plus the lower bound of its 95% interval, ordered by score
/// for the leader heap. The score mirrors how [`rank_results`] orders crews — `0.8·win + 0.2·hull`
/// for single fights, primary success rate for chain grinds — so abandonment prunes against the
/// metric that actually decides the ranking (not win rate alone, which is blind to hull and so
/// useless on saturated-win matchups where every crew wins and hull is the differentiator).
#[derive(Debug, Clone, Copy, PartialEq)]
struct LeaderEntry {
    score: f64,
    ci_low: f64,
}

impl Eq for LeaderEntry {}

impl PartialOrd for LeaderEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LeaderEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Scores are finite in [0, 1]; total_cmp keeps the heap well-ordered regardless.
        self.score.total_cmp(&other.score)
    }
}

/// Shared top-K leaderboard for progressive abandonment.
///
/// Tracks the best `k` finished crews by ranking score. The cut line is the **K-th best** crew's
/// score lower bound; a running candidate whose score upper bound cannot reach `cut − margin`
/// provably cannot enter the top K and is abandoned. With `k == 1` this is the single-best-leader
/// behavior used by the scouting pass; larger `k` preserves the full reported top-K set while
/// pruning the long tail of losers more aggressively.
#[derive(Debug, Default)]
struct TopKLeader {
    k: usize,
    /// Min-heap (by score) of the best `k` finished crews; the root is the K-th best.
    top: BinaryHeap<Reverse<LeaderEntry>>,
}

impl TopKLeader {
    fn new(k: usize) -> Self {
        Self {
            k: k.max(1),
            top: BinaryHeap::new(),
        }
    }

    fn update(&mut self, score: f64, ci_low: f64) {
        let entry = LeaderEntry { score, ci_low };
        if self.top.len() < self.k {
            self.top.push(Reverse(entry));
        } else if let Some(Reverse(worst_of_top)) = self.top.peek() {
            if entry.score > worst_of_top.score {
                self.top.pop();
                self.top.push(Reverse(entry));
            }
        }
    }

    /// Score lower bound of the current K-th best crew, or `None` until `k` crews have finished
    /// (no defensible cut exists before the leaderboard is full).
    fn cut_lower(&self) -> Option<f64> {
        if self.top.len() < self.k {
            return None;
        }
        self.top.peek().map(|Reverse(e)| e.ci_low)
    }

    /// Returns true if a candidate with the given score upper bound can still enter the top K under
    /// the configured margin. Always true until the leaderboard has `k` finished crews.
    fn can_beat_leader(&self, candidate_score_upper: f64, margin: f64) -> bool {
        let Some(cut) = self.cut_lower() else {
            return true;
        };
        candidate_score_upper >= cut - margin
    }
}

/// Zero on non-wins; otherwise use the final effective maximum, including
/// conditional HP effects. Damage dealt and overkill are not survival measures.
#[inline]
pub(super) fn surviving_hull_fraction(
    result: &crate::combat::SimulationResult,
    max_hull: f64,
) -> f64 {
    if !result.attacker_won
        || result.winner_by_round_limit
        || !max_hull.is_finite()
        || max_hull <= 0.0
    {
        return 0.0;
    }
    (result.attacker_hull_remaining / max_hull).clamp(0.0, 1.0)
}

/// Single-fight ranking score from win rate and mean hull remaining, matching [`rank_results`].
#[inline]
fn blended_score(win_rate: f64, avg_hull_remaining: f64) -> f64 {
    0.8 * win_rate + 0.2 * avg_hull_remaining
}

pub(super) fn unsupported_crew_health_effects(
    input: &super::scenario::CombatSimulationInput,
) -> bool {
    unsupported_crew_health_effects_for_crew(
        &input.crew,
        input.defender.hostile_mitigation_params.is_some(),
        input
            .attacker
            .hostile_mitigation_params
            .as_ref()
            .is_some_and(|params| {
                params.defender_stats.armor > 0.0
                    || params.defender_stats.shield_deflection > 0.0
                    || params.defender_stats.dodge > 0.0
            }),
    ) || unsupported_defender_raw_effects(&input.defender_crew)
}

fn unsupported_defender_raw_effects(crew: &crate::combat::CrewConfiguration) -> bool {
    use crate::combat::AbilityEffect;
    crew.seats.iter().any(|seat| {
        matches!(
            seat.ability.effect,
            AbilityEffect::BaseArmorPiercingPerCrewHealth(_)
                | AbilityEffect::BaseMitigationStatsPerCrewHealth(_)
                | AbilityEffect::BaseArmorPiercingAdd(_)
                | AbilityEffect::BaseMitigationStatsAdd(_)
        )
    })
}

fn unsupported_crew_health_effects_for_crew(
    crew: &crate::combat::CrewConfiguration,
    has_outbound_raw: bool,
    has_inbound_raw: bool,
) -> bool {
    use crate::combat::AbilityEffect;
    crew.seats.iter().any(|seat| match seat.ability.effect {
        AbilityEffect::BaseArmorPiercingPerCrewHealth(_)
        | AbilityEffect::BaseMitigationStatsPerCrewHealth(_) => true,
        AbilityEffect::BaseArmorPiercingAdd(value) if value != 0.0 => !has_outbound_raw,
        AbilityEffect::BaseMitigationStatsAdd(value) if value != 0.0 => !has_inbound_raw,
        _ => false,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_candidate_chain_monte_carlo(
    shared: &SharedScenarioData,
    candidate: &CrewCandidate,
    seed: u64,
    max_iterations: usize,
    chain: &ChainGrindParams,
    early_scout: Option<ScoutEarlyStopCfg>,
    best_so_far: Option<&std::sync::Mutex<TopKLeader>>,
    progressive_abandon: Option<ProgressiveAbandonCfg>,
    seed_panel_policy: SeedPanelPolicy,
) -> SimulationResult {
    let mut input = scenario_to_combat_input_from_shared(shared, candidate, seed);
    apply_seed_panel_policy(&mut input, shared, seed, seed_panel_policy);
    let mut primary_ok = 0usize;
    let mut stalls = 0usize;
    let mut losses = 0usize;
    let mut r1_first = 0usize;
    let mut sec_mean = 0.0f64;
    let mut sec_m2 = 0.0f64;
    let mut sec_count = 0usize;
    let mut def_hull_mean = 0.0f64;
    let mut def_hull_m2 = 0.0f64;

    let mut n_done = 0usize;
    while n_done < max_iterations {
        let trial_seed = input.base_seed.wrapping_add(n_done as u64);
        let outcome = run_chain_trial(shared, &input, chain, trial_seed);

        let i = n_done + 1;
        let d = outcome.last_defender_hull_frac;
        let delta_d = d - def_hull_mean;
        def_hull_mean += delta_d / i as f64;
        let delta_d2 = d - def_hull_mean;
        def_hull_m2 += delta_d * delta_d2;

        if outcome.first_link_r1_clean_win {
            r1_first += 1;
        }

        if outcome.primary_success {
            primary_ok += 1;
            let hf = outcome.hull_fraction_end.unwrap_or(0.0);
            let sdraw = secondary_draw(chain.secondary, chain.kills_target, hf);
            sec_count += 1;
            let j = sec_count as f64;
            let delta = sdraw - sec_mean;
            sec_mean += delta / j;
            let delta2 = sdraw - sec_mean;
            sec_m2 += delta * delta2;
        } else if outcome.failed_on_stall {
            stalls += 1;
        } else {
            losses += 1;
        }

        n_done += 1;

        if let Some(cfg) = early_scout {
            if n_done >= cfg.min_trials
                && n_done < max_iterations
                && cfg.check_every > 0
                && n_done.is_multiple_of(cfg.check_every)
                && win_rate_upper_wilson_95(primary_ok, n_done) < cfg.eliminate_upper_below
            {
                break;
            }
        }

        if let (Some(bsf), Some(cfg)) = (best_so_far, progressive_abandon) {
            if n_done >= cfg.min_trials
                && n_done < max_iterations
                && cfg.check_every > 0
                && n_done.is_multiple_of(cfg.check_every)
            {
                // Chain ranking is primary-success dominated; score == primary rate.
                let score_upper = win_rate_upper_wilson_95(primary_ok, n_done);
                let leader = bsf.lock().unwrap();
                if !leader.can_beat_leader(score_upper, cfg.margin) {
                    break;
                }
            }
        }
    }

    let n = n_done;
    let nf = n as f64;
    let win_rate = if n == 0 { 0.0 } else { primary_ok as f64 / nf };
    let stall_rate = if n == 0 { 0.0 } else { stalls as f64 / nf };
    let loss_rate = if n == 0 { 0.0 } else { losses as f64 / nf };
    let r1_kill_rate = if n == 0 { 0.0 } else { r1_first as f64 / nf };

    let avg_cond_secondary = if sec_count == 0 { 0.0 } else { sec_mean };
    let (avg_hull_remaining_ci_low, avg_hull_remaining_ci_high) = if sec_count == 0 {
        (0.0, 0.0)
    } else if sec_count == 1 {
        (avg_cond_secondary, avg_cond_secondary)
    } else {
        let var = sec_m2 / (sec_count as f64 - 1.0);
        let se = (var / sec_count as f64).sqrt().max(0.0);
        const Z: f64 = 1.96;
        ((avg_cond_secondary - Z * se), (avg_cond_secondary + Z * se))
    };

    let (win_rate_ci_low, win_rate_ci_high) = wilson_95_interval(primary_ok, n);
    let (stall_rate_ci_low, stall_rate_ci_high) = wilson_95_interval(stalls, n);
    let (loss_rate_ci_low, loss_rate_ci_high) = wilson_95_interval(losses, n);
    let (r1_kill_rate_ci_low, r1_kill_rate_ci_high) = wilson_95_interval(r1_first, n);

    let avg_defender_hull_remaining = if n == 0 { 0.0 } else { def_hull_mean };
    let (avg_defender_hull_remaining_ci_low, avg_defender_hull_remaining_ci_high) = if n == 0 {
        (0.0, 0.0)
    } else if n == 1 {
        (avg_defender_hull_remaining, avg_defender_hull_remaining)
    } else {
        let var = def_hull_m2 / (n as f64 - 1.0);
        let se = (var / n as f64).sqrt().max(0.0);
        const Z: f64 = 1.96;
        (
            (avg_defender_hull_remaining - Z * se).clamp(0.0, 1.0),
            (avg_defender_hull_remaining + Z * se).clamp(0.0, 1.0),
        )
    };

    let summary = ChainSimulationSummary {
        kills_target: chain.kills_target,
        secondary_objective: chain.secondary,
        primary_success_rate: win_rate,
        primary_ci_low: win_rate_ci_low,
        primary_ci_high: win_rate_ci_high,
        secondary_mean_given_primary: avg_cond_secondary,
        secondary_ci_low: avg_hull_remaining_ci_low,
        secondary_ci_high: avg_hull_remaining_ci_high,
        n_primary_successes: sec_count,
    };

    SimulationResult {
        candidate: candidate.clone(),
        unsupported_crew_health_effects: Some(unsupported_crew_health_effects(&input)),
        trials_run: n,
        win_rate,
        win_rate_ci_low,
        win_rate_ci_high,
        stall_rate,
        stall_rate_ci_low,
        stall_rate_ci_high,
        loss_rate,
        loss_rate_ci_low,
        loss_rate_ci_high,
        r1_kill_rate,
        r1_kill_rate_ci_low,
        r1_kill_rate_ci_high,
        avg_hull_remaining: avg_cond_secondary,
        avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high,
        avg_defender_hull_remaining,
        avg_defender_hull_remaining_ci_low,
        avg_defender_hull_remaining_ci_high,
        chain: Some(summary),
        expected_hull_damage: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_candidate_monte_carlo(
    shared: &SharedScenarioData,
    candidate: &CrewCandidate,
    seed: u64,
    max_iterations: usize,
    early_scout: Option<ScoutEarlyStopCfg>,
    best_so_far: Option<&std::sync::Mutex<TopKLeader>>,
    progressive_abandon: Option<ProgressiveAbandonCfg>,
    seed_panel_policy: SeedPanelPolicy,
) -> SimulationResult {
    let mut input = scenario_to_combat_input_from_shared(shared, candidate, seed);
    apply_seed_panel_policy(&mut input, shared, seed, seed_panel_policy);
    let mut wins = 0usize;
    let mut stalls = 0usize;
    let mut losses = 0usize;
    let mut r1_kills = 0usize;
    let mut surviving_hull_sum = 0.0f64;
    let mut hull_mean = 0.0f64;
    let mut hull_m2 = 0.0f64;
    let mut def_hull_mean = 0.0f64;
    let mut def_hull_m2 = 0.0f64;

    let combat_config = SimulationConfig {
        rounds: input.rounds,
        seed: 0,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: input.weapon_damage_profile_additive_pool,
        profile_weapon_damage_fraction: input.profile_weapon_damage_fraction,
        defender_hull_faction_id: shared
            .hostile_rec
            .as_ref()
            .and_then(|h| h.faction.as_ref().map(|f| f.id))
            .unwrap_or(0),
        defender_hostile_tag_mask: shared.defender_hostile_tag_mask_for_combat(),
        attacker_owner_faction: input.attacker_owner_faction,
        engagement_enemy_types: input.engagement_enemy_types.clone(),
        defender_level: input.defender_level,
        attacker_roster_officer_ids: input.attacker_roster_officer_ids.clone(),
        incoming_shield_mitigation_bonus: input.incoming_shield_mitigation_bonus,
        incoming_shield_mitigation_bonus_rounds: input.incoming_shield_mitigation_bonus_rounds,
        attacker_hyperthermic_decay_fraction: input.attacker_hyperthermic_decay_fraction,
        emit_state_snapshots: false,
    };

    // Precompute values that don't change per trial
    let defender_faction = shared
        .hostile_rec
        .as_ref()
        .map(|h| h.opponent_faction_tag())
        .unwrap_or(OpponentFactionTag::Unknown);
    let defender_ship_type = shared.defender_ship_type_for_combat();
    let attacker_ship_type = shared.attacker_ship_type_for_combat();
    let defender_is_npc = shared.defender_opponent.defender_is_npc_hostile();
    let defender_is_player = shared.defender_opponent.defender_is_player_ship();

    // Build immutable combat setup once for batch reuse
    let setup = build_combat_setup_with_officer_stat(
        &input.attacker,
        &input.defender,
        &combat_config,
        &input.crew,
        defender_faction,
        defender_ship_type,
        attacker_ship_type,
        defender_is_npc,
        defender_is_player,
        &input.defender_crew,
        input.officer_stat_round.clone(),
    );
    let mut attacker_scratch = setup.attacker.clone();

    const BATCH_SIZE: usize = 64;

    let mut n_done = 0usize;
    'batches: while n_done < max_iterations {
        let batch_end = (n_done + BATCH_SIZE).min(max_iterations);

        // Aggregate directly so the optimizer hot path does not allocate seed and result vectors
        // for every 64-trial checkpoint batch. Replay/debug callers retain the public batch API.
        for trial_index in n_done..batch_end {
            let iteration_seed = input.base_seed.wrapping_add(trial_index as u64);
            let result = simulate_combat_from_setup_reusing_attacker(
                &setup,
                iteration_seed,
                &mut attacker_scratch,
            );

            if result.winner_by_round_limit {
                stalls += 1;
            } else if result.attacker_won {
                wins += 1;
            } else {
                losses += 1;
            }

            let hull_draw = surviving_hull_fraction(&result, attacker_scratch.hull_health);
            surviving_hull_sum += hull_draw;
            let i = trial_index + 1;
            let delta = hull_draw - hull_mean;
            hull_mean += delta / i as f64;
            let delta2 = hull_draw - hull_mean;
            hull_m2 += delta * delta2;

            let def_max = input.defender.hull_health.max(1.0);
            let def_draw = (result.defender_hull_remaining / def_max).clamp(0.0, 1.0);
            let delta_d = def_draw - def_hull_mean;
            def_hull_mean += delta_d / i as f64;
            let delta_d2 = def_draw - def_hull_mean;
            def_hull_m2 += delta_d * delta_d2;

            if result.attacker_won && result.rounds_simulated == 1 {
                r1_kills += 1;
            }

            // The requested checkpoint may fall inside a 64-trial batch. Check at the
            // exact completed-trial count so a 500-trial scout can stop at 100, 150, ...
            // without changing the allocation-free combat loop.
            if let Some(cfg) = early_scout {
                if i >= cfg.min_trials
                    && i < max_iterations
                    && cfg.check_every > 0
                    && i.is_multiple_of(cfg.check_every)
                    && win_rate_upper_wilson_95(wins, i) < cfg.eliminate_upper_below
                {
                    n_done = i;
                    break 'batches;
                }
            }
            if let (Some(bsf), Some(cfg)) = (best_so_far, progressive_abandon) {
                if i >= cfg.min_trials
                    && i < max_iterations
                    && cfg.check_every > 0
                    && i.is_multiple_of(cfg.check_every)
                {
                    let win_upper = win_rate_upper_wilson_95(wins, i);
                    let hull_upper = normal_upper_95(hull_mean, hull_m2, i);
                    let score_upper = blended_score(win_upper, hull_upper);
                    let leader = bsf.lock().unwrap();
                    if !leader.can_beat_leader(score_upper, cfg.margin) {
                        n_done = i;
                        break 'batches;
                    }
                }
            }
        }

        n_done = batch_end;
    }

    let n = n_done as f64;
    let win_rate = if n_done == 0 { 0.0 } else { wins as f64 / n };
    let stall_rate = if n_done == 0 { 0.0 } else { stalls as f64 / n };
    let loss_rate = if n_done == 0 { 0.0 } else { losses as f64 / n };
    let r1_kill_rate = if n_done == 0 {
        0.0
    } else {
        r1_kills as f64 / n
    };
    let avg_hull_remaining = if n_done == 0 {
        0.0
    } else {
        surviving_hull_sum / n
    };

    let (win_rate_ci_low, win_rate_ci_high) = wilson_95_interval(wins, n_done);
    let (stall_rate_ci_low, stall_rate_ci_high) = wilson_95_interval(stalls, n_done);
    let (loss_rate_ci_low, loss_rate_ci_high) = wilson_95_interval(losses, n_done);
    let (r1_kill_rate_ci_low, r1_kill_rate_ci_high) = wilson_95_interval(r1_kills, n_done);

    let (avg_hull_remaining_ci_low, avg_hull_remaining_ci_high) = if n_done == 0 {
        (0.0, 0.0)
    } else if n_done == 1 {
        (avg_hull_remaining, avg_hull_remaining)
    } else {
        let var = hull_m2 / (n_done as f64 - 1.0);
        let se = (var / n_done as f64).sqrt().max(0.0);
        const Z: f64 = 1.96;
        (
            (avg_hull_remaining - Z * se).clamp(0.0, 1.0),
            (avg_hull_remaining + Z * se).clamp(0.0, 1.0),
        )
    };

    let avg_defender_hull_remaining = if n_done == 0 { 0.0 } else { def_hull_mean };
    let (avg_defender_hull_remaining_ci_low, avg_defender_hull_remaining_ci_high) = if n_done == 0 {
        (0.0, 0.0)
    } else if n_done == 1 {
        (avg_defender_hull_remaining, avg_defender_hull_remaining)
    } else {
        let var = def_hull_m2 / (n_done as f64 - 1.0);
        let se = (var / n_done as f64).sqrt().max(0.0);
        const Z: f64 = 1.96;
        (
            (avg_defender_hull_remaining - Z * se).clamp(0.0, 1.0),
            (avg_defender_hull_remaining + Z * se).clamp(0.0, 1.0),
        )
    };

    SimulationResult {
        candidate: candidate.clone(),
        unsupported_crew_health_effects: Some(unsupported_crew_health_effects(&input)),
        trials_run: n_done,
        win_rate,
        win_rate_ci_low,
        win_rate_ci_high,
        stall_rate,
        stall_rate_ci_low,
        stall_rate_ci_high,
        loss_rate,
        loss_rate_ci_low,
        loss_rate_ci_high,
        r1_kill_rate,
        r1_kill_rate_ci_low,
        r1_kill_rate_ci_high,
        avg_hull_remaining,
        avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high,
        avg_defender_hull_remaining,
        avg_defender_hull_remaining_ci_low,
        avg_defender_hull_remaining_ci_high,
        chain: None,
        expected_hull_damage: None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_monte_carlo(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    support_buffs: Option<&[String]>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(
        ship,
        hostile,
        candidates,
        iterations,
        seed,
        false,
        support_buffs::SupportBuffScenarioRequest::attacker_only(support_buffs),
        chain_grind,
        defender_opponent,
    )
}

/// Like [run_monte_carlo] but distributes candidates across all CPU cores via Rayon.
/// Use for large candidate lists (e.g. optimizer sweeps). Results order matches input order.
#[allow(clippy::too_many_arguments)]
pub fn run_monte_carlo_parallel(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    support_buffs: Option<&[String]>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(
        ship,
        hostile,
        candidates,
        iterations,
        seed,
        true,
        support_buffs::SupportBuffScenarioRequest::attacker_only(support_buffs),
        chain_grind,
        defender_opponent,
    )
}

/// Monte Carlo for a population that may contain duplicate crews: simulates each distinct crew once
/// and copies rates for duplicates (deterministic, same seeds as evaluating each separately).
#[allow(clippy::too_many_arguments)]
pub fn run_monte_carlo_parallel_deduped(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    support_buffs: Option<&[String]>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
) -> Vec<SimulationResult> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut seen_hashes: HashSet<u64> = HashSet::with_capacity(candidates.len());
    let mut unique_indices: Vec<usize> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let k = crew_candidate_ordered_hash(c);
        if seen_hashes.insert(k) {
            unique_indices.push(i);
        }
    }

    let uniq: Vec<CrewCandidate> = unique_indices
        .iter()
        .map(|&i| candidates[i].clone())
        .collect();

    let uniq_results = run_monte_carlo_parallel(
        ship,
        hostile,
        &uniq,
        iterations,
        seed,
        support_buffs,
        chain_grind.clone(),
        defender_opponent,
    );

    let mut by_hash: HashMap<u64, SimulationResult> = HashMap::with_capacity(uniq_results.len());
    for (j, r) in uniq_results.into_iter().enumerate() {
        let c = &candidates[unique_indices[j]];
        let k = crew_candidate_ordered_hash(c);
        by_hash.insert(
            k,
            SimulationResult {
                candidate: c.clone(),
                unsupported_crew_health_effects: r.unsupported_crew_health_effects,
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
            },
        );
    }

    candidates
        .iter()
        .map(|c| {
            by_hash
                .get(&crew_candidate_ordered_hash(c))
                .expect("dedup MC: hash present")
                .clone()
        })
        .collect()
}

/// Like [`run_monte_carlo_parallel_deduped`], but evaluates distinct crews in sequential chunks of
/// at most `max_unique_per_chunk`, calling `should_continue` before each chunk. Returns `None` if
/// `should_continue` is false (cooperative cancel); otherwise full results in input order.
#[allow(clippy::too_many_arguments)]
pub fn run_monte_carlo_parallel_deduped_chunked(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    support_buffs: Option<&[String]>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
    max_unique_per_chunk: usize,
    mut should_continue: impl FnMut() -> bool,
) -> Option<Vec<SimulationResult>> {
    if candidates.is_empty() {
        return Some(Vec::new());
    }

    let mut seen_hashes: HashSet<u64> = HashSet::with_capacity(candidates.len());
    let mut unique_indices: Vec<usize> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let k = crew_candidate_ordered_hash(c);
        if seen_hashes.insert(k) {
            unique_indices.push(i);
        }
    }

    let uniq: Vec<CrewCandidate> = unique_indices
        .iter()
        .map(|&i| candidates[i].clone())
        .collect();

    let chunk_sz = max_unique_per_chunk.max(1);
    let mut by_hash: HashMap<u64, SimulationResult> = HashMap::with_capacity(uniq.len());
    for chunk in uniq.chunks(chunk_sz) {
        if !should_continue() {
            return None;
        }
        let part = run_monte_carlo_parallel(
            ship,
            hostile,
            chunk,
            iterations,
            seed,
            support_buffs,
            chain_grind.clone(),
            defender_opponent,
        );
        for (c, r) in chunk.iter().zip(part) {
            by_hash.insert(
                crew_candidate_ordered_hash(c),
                SimulationResult {
                    candidate: c.clone(),
                    unsupported_crew_health_effects: r.unsupported_crew_health_effects,
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
                },
            );
        }
    }

    Some(
        candidates
            .iter()
            .map(|c| {
                by_hash
                    .get(&crew_candidate_ordered_hash(c))
                    .expect("dedup chunked MC: hash present")
                    .clone()
            })
            .collect(),
    )
}

/// Like [`run_monte_carlo_parallel_deduped_chunked`] but reuses a pre-built [`SharedScenarioData`]
/// across all chunks instead of rebuilding it from disk on every call.
/// Used by the genetic optimizer where the same (ship, hostile, tier, level, profile) is reused
/// across every generation and every chunk within a generation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_monte_carlo_parallel_deduped_chunked_with_shared(
    shared: &SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    chain_grind: Option<ChainGrindParams>,
    max_unique_per_chunk: usize,
    abandon_top_k: Option<usize>,
    mut should_continue: impl FnMut() -> bool,
) -> Option<Vec<SimulationResult>> {
    if candidates.is_empty() {
        return Some(Vec::new());
    }

    let mut seen_hashes: HashSet<u64> = HashSet::with_capacity(candidates.len());
    let mut unique_indices: Vec<usize> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let k = crew_candidate_ordered_hash(c);
        if seen_hashes.insert(k) {
            unique_indices.push(i);
        }
    }

    let uniq: Vec<CrewCandidate> = unique_indices
        .iter()
        .map(|&i| candidates[i].clone())
        .collect();

    // One leaderboard shared across all chunks so the abandonment cut keeps tightening as crews
    // finish (per-chunk leaders would reset the cut and prune far less). `None` = no abandonment.
    let shared_leader = abandon_top_k.map(|k| {
        (
            ProgressiveAbandonCfg::for_full_mc(iterations.max(1)),
            Arc::new(Mutex::new(TopKLeader::new(k.max(1)))),
        )
    });

    let chunk_sz = max_unique_per_chunk.max(1);
    let mut by_hash: HashMap<u64, SimulationResult> = HashMap::with_capacity(uniq.len());
    for chunk in uniq.chunks(chunk_sz) {
        if !should_continue() {
            return None;
        }
        let part = run_monte_carlo_inner_with_leader(
            shared,
            chunk,
            iterations,
            seed,
            true,
            None,
            shared_leader.clone(),
            SeedPanelPolicy::CandidateDistinct,
            chain_grind.clone(),
        );
        for (c, r) in chunk.iter().zip(part) {
            by_hash.insert(
                crew_candidate_ordered_hash(c),
                SimulationResult {
                    candidate: c.clone(),
                    unsupported_crew_health_effects: r.unsupported_crew_health_effects,
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
                },
            );
        }
    }

    Some(
        candidates
            .iter()
            .map(|c| {
                by_hash
                    .get(&crew_candidate_ordered_hash(c))
                    .expect("dedup chunked MC: hash present")
                    .clone()
            })
            .collect(),
    )
}

/// Like [run_monte_carlo_parallel] but uses [DataRegistry] for officers and ship/hostile resolution (no reload).
/// When ship_tier or ship_level is set, uses data/ships_extended for accurate stats.
#[allow(clippy::too_many_arguments)]
pub fn run_monte_carlo_parallel_with_registry(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    profile_id: Option<&str>,
    support_buffs: support_buffs::SupportBuffScenarioRequest<'_>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
    player_defender_officer_crew: Option<PlayerDefenderOfficerCrewOverride>,
    pvp: Option<super::scenario::PvpScenarioParams>,
) -> (Vec<SimulationResult>, bool) {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
        support_buffs,
        defender_opponent,
        player_defender_officer_crew,
        pvp,
    );
    let placeholder = shared.using_placeholder_combatants;
    (
        run_monte_carlo_with_shared(shared, candidates, iterations, seed, true, chain_grind),
        placeholder,
    )
}

/// Like [run_monte_carlo] but uses [DataRegistry] for officers and ship/hostile resolution (no reload).
/// When ship_tier or ship_level is set, uses data/ships_extended for accurate stats.
#[allow(clippy::too_many_arguments)]
pub fn run_monte_carlo_with_registry(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    profile_id: Option<&str>,
    support_buffs: support_buffs::SupportBuffScenarioRequest<'_>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
    player_defender_officer_crew: Option<PlayerDefenderOfficerCrewOverride>,
    pvp: Option<super::scenario::PvpScenarioParams>,
) -> (Vec<SimulationResult>, bool) {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
        support_buffs,
        defender_opponent,
        player_defender_officer_crew,
        pvp,
    );
    let placeholder = shared.using_placeholder_combatants;
    (
        run_monte_carlo_with_shared(shared, candidates, iterations, seed, false, chain_grind),
        placeholder,
    )
}

/// One Monte Carlo draw replayed with full combat trace — same RNG seed as
/// [`run_candidate_monte_carlo`]: `iteration_seed = base_seed.wrapping_add(sim_index)`.
#[derive(Debug, Clone)]
pub struct MonteCarloSeedReplay {
    pub using_placeholder_combatants: bool,
    pub unsupported_crew_health_effects: bool,
    pub scenario_seed: u64,
    pub sim_index: u64,
    pub base_seed: u64,
    pub iteration_seed: u64,
    pub effective_defender_hull: f64,
    pub attacker_won: bool,
    pub winner_by_round_limit: bool,
    pub rounds_simulated: u32,
    pub total_damage: f64,
    pub total_isolytic_damage: f64,
    pub attacker_hull_remaining: f64,
    pub defender_hull_remaining: f64,
    pub defender_shield_remaining: f64,
    pub trace_event_count: usize,
    pub trace_events_returned: usize,
    pub trace_truncated: bool,
    pub external_buffs: Value,
    pub trace_events: Vec<CombatEvent>,
}

fn external_buffs_trace_payload(shared: &SharedScenarioData) -> Value {
    let aggregate_static_bonuses: BTreeMap<String, f64> = shared
        .support_static_buffs
        .iter()
        .map(|(stat, value)| (stat.clone(), *value))
        .collect();
    let defender_static_bonuses: BTreeMap<String, f64> = shared
        .support_defender_static_buffs
        .iter()
        .map(|(stat, value)| (stat.clone(), *value))
        .collect();
    let attacker_debuff_static_bonuses: BTreeMap<String, f64> = shared
        .support_attacker_debuff_static_buffs
        .iter()
        .map(|(stat, value)| (stat.clone(), *value))
        .collect();

    json!({
        "support_buffs": {
            "resolved_ids": &shared.resolved_support_buffs,
            "unknown_ids": &shared.unknown_support_buff_ids,
            "applied": &shared.applied_support_buffs,
            "aggregate_static_bonuses": aggregate_static_bonuses,
            "aggregate_static_bonuses_note": "Attacker merge: selected support buff static_bonuses routed to attacker plus support-gated imported research bonuses when present.",
            "defender_static_bonuses_vs_player": defender_static_bonuses,
            "defender_static_bonuses_note": "Applied to the defender Combatant only when defender_opponent is player (PvP-shaped).",
            "defender_support_buffs": {
                "resolved_ids": &shared.resolved_defender_support_buffs,
                "unknown_ids": &shared.unknown_defender_support_buff_ids,
                "applied": &shared.applied_defender_support_buffs,
            },
            "defender_alliance_debuffs": {
                "resolved_ids": &shared.resolved_defender_alliance_debuffs,
                "unknown_ids": &shared.unknown_defender_alliance_debuffs_ids,
                "applied": &shared.applied_defender_alliance_debuffs,
                "aggregate_static_bonuses_on_attacker": attacker_debuff_static_bonuses,
            },
        }
    })
}

/// Replay a single iteration from an optimize/simulate Monte Carlo run (`scenario_seed` matches the request seed).
#[allow(clippy::too_many_arguments)]
pub fn replay_optimize_iteration_with_registry(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    candidate: &CrewCandidate,
    scenario_seed: u64,
    sim_index: u64,
    profile_id: Option<&str>,
    max_trace_events: usize,
    support_buffs: Option<&[String]>,
    defender_opponent: DefenderOpponent,
) -> MonteCarloSeedReplay {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
        support_buffs::SupportBuffScenarioRequest::attacker_only(support_buffs),
        defender_opponent,
        None,
        None,
    );
    let input = scenario_to_combat_input_from_shared(&shared, candidate, scenario_seed);
    let external_buffs = external_buffs_trace_payload(&shared);
    let iteration_seed = input.base_seed.wrapping_add(sim_index);
    let effective_defender_hull = input.defender.hull_health;

    let defender_faction = shared
        .hostile_rec
        .as_ref()
        .map(|h| h.opponent_faction_tag())
        .unwrap_or(OpponentFactionTag::Unknown);
    let defender_ship_type = shared.defender_ship_type_for_combat();
    let attacker_ship_type = shared.attacker_ship_type_for_combat();

    let combat_config = SimulationConfig {
        rounds: input.rounds,
        seed: iteration_seed,
        trace_mode: TraceMode::Events,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: input.weapon_damage_profile_additive_pool,
        profile_weapon_damage_fraction: input.profile_weapon_damage_fraction,
        defender_hull_faction_id: shared
            .hostile_rec
            .as_ref()
            .and_then(|h| h.faction.as_ref().map(|f| f.id))
            .unwrap_or(0),
        defender_hostile_tag_mask: shared.defender_hostile_tag_mask_for_combat(),
        attacker_owner_faction: input.attacker_owner_faction,
        engagement_enemy_types: input.engagement_enemy_types.clone(),
        defender_level: input.defender_level,
        attacker_roster_officer_ids: input.attacker_roster_officer_ids.clone(),
        incoming_shield_mitigation_bonus: input.incoming_shield_mitigation_bonus,
        incoming_shield_mitigation_bonus_rounds: input.incoming_shield_mitigation_bonus_rounds,
        attacker_hyperthermic_decay_fraction: input.attacker_hyperthermic_decay_fraction,
        emit_state_snapshots: false,
    };

    let combat = simulate_combat_with_defender_faction_and_defender_crew(
        &input.attacker,
        &input.defender,
        &combat_config,
        &input.crew,
        defender_faction,
        defender_ship_type,
        attacker_ship_type,
        shared.defender_opponent.defender_is_npc_hostile(),
        shared.defender_opponent.defender_is_player_ship(),
        &input.defender_crew,
    );

    let trace_event_count = combat.events.len();
    let max_kept = max_trace_events.max(1);
    let (trace_truncated, trace_events) = if combat.events.len() > max_kept {
        let skip = combat.events.len() - max_kept;
        (
            true,
            combat.events.into_iter().skip(skip).collect::<Vec<_>>(),
        )
    } else {
        (false, combat.events)
    };
    let trace_events_returned = trace_events.len();

    MonteCarloSeedReplay {
        using_placeholder_combatants: shared.using_placeholder_combatants,
        unsupported_crew_health_effects: unsupported_crew_health_effects(&input),
        scenario_seed,
        sim_index,
        base_seed: input.base_seed,
        iteration_seed,
        effective_defender_hull,
        attacker_won: combat.attacker_won,
        winner_by_round_limit: combat.winner_by_round_limit,
        rounds_simulated: combat.rounds_simulated,
        total_damage: combat.total_damage,
        total_isolytic_damage: combat.total_isolytic_damage,
        attacker_hull_remaining: combat.attacker_hull_remaining,
        defender_hull_remaining: combat.defender_hull_remaining,
        defender_shield_remaining: combat.defender_shield_remaining,
        trace_event_count,
        trace_events_returned,
        trace_truncated,
        external_buffs,
        trace_events,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_monte_carlo_with_parallelism(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    support_buffs: support_buffs::SupportBuffScenarioRequest<'_>,
    chain_grind: Option<ChainGrindParams>,
    defender_opponent: DefenderOpponent,
) -> Vec<SimulationResult> {
    let shared = build_shared_scenario_data_standalone(
        ship,
        hostile,
        support_buffs,
        defender_opponent,
        None,
    );
    run_monte_carlo_with_shared(shared, candidates, iterations, seed, parallel, chain_grind)
}

/// Run Monte Carlo using pre-built SharedScenarioData (used by both legacy and registry paths).
pub(crate) fn run_monte_carlo_with_shared(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    chain_grind: Option<ChainGrindParams>,
) -> Vec<SimulationResult> {
    let t0 = perf_log::perf_start();
    let out = run_monte_carlo_inner(
        &shared,
        candidates,
        iterations,
        seed,
        parallel,
        None,
        None,
        SeedPanelPolicy::CandidateDistinct,
        chain_grind,
    );
    perf_log::log_duration(
        &format!(
            "monte_carlo.with_shared(candidates={}, iterations={}, parallel={parallel})",
            candidates.len(),
            iterations
        ),
        t0,
    );
    out
}

/// Full Monte Carlo with top-K progressive abandonment: identical statistics to
/// [`run_monte_carlo_with_shared`] for every crew that survives, but crews whose Wilson upper bound
/// provably cannot reach the current K-th-best cut line stop early. Preserves the reported top-`k`
/// ranking while skipping wasted trials on the long tail of losers.
///
/// This always applies abandonment — the enable/disable policy lives at the call site (the
/// exhaustive path gates on `KOBAYASHI_FULLMC_EARLY_STOP`). NOTE: on this codebase's saturated
/// win/loss matchups abandonment rarely fires, so it is opt-in (see `docs/PERFORMANCE.md`).
pub(crate) fn run_monte_carlo_confirm_topk_with_shared(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    chain_grind: Option<ChainGrindParams>,
    top_k: usize,
) -> Vec<SimulationResult> {
    let t0 = perf_log::perf_start();
    let cfg = ProgressiveAbandonCfg::for_full_mc(iterations.max(1));
    let out = run_monte_carlo_inner(
        &shared,
        candidates,
        iterations,
        seed,
        parallel,
        None,
        Some((cfg, top_k.max(1))),
        SeedPanelPolicy::CandidateDistinct,
        chain_grind,
    );
    perf_log::log_duration(
        &format!(
            "monte_carlo.confirm_topk(candidates={}, iterations={}, top_k={top_k}, parallel={parallel})",
            candidates.len(),
            iterations
        ),
        t0,
    );
    out
}

/// Whether full-MC top-K progressive abandonment is enabled on the exhaustive path. **On by
/// default**: at margin 0.01 the score-aware leader is regression-free and gives ~5–10% on hard
/// "wins-but-bleeds" PvE while preserving the top-K ranking (`docs/PERFORMANCE.md`). Disable with
/// `KOBAYASHI_FULLMC_EARLY_STOP=0` (also accepts `false`/`off`/`no`) to get the plain full pass with
/// byte-identical deep-tail fidelity.
pub(crate) fn full_mc_early_stop_enabled() -> bool {
    match std::env::var("KOBAYASHI_FULLMC_EARLY_STOP") {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        ),
        Err(_) => true,
    }
}

/// Tiered scout phase: same statistics semantics as full MC when no early stop triggers; may use fewer
/// iterations per crew via Wilson-bound elimination (deterministic given the same iteration order).
pub(crate) fn run_monte_carlo_scout_phase_with_shared(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    chain_grind: Option<ChainGrindParams>,
) -> Vec<SimulationResult> {
    let cfg = ScoutEarlyStopCfg::for_scout_iterations(iterations.max(1));
    let abandon = ProgressiveAbandonCfg::for_scout_iterations(iterations.max(1));
    run_monte_carlo_inner(
        &shared,
        candidates,
        iterations,
        seed,
        parallel,
        Some(cfg),
        Some((abandon, 1)),
        SeedPanelPolicy::CommonScenario,
        chain_grind,
    )
}

/// Minimum total work units (candidates × iterations) at which Rayon parallelism pays off.
/// Below this the join/wake cost dominates the actual sim time, so we use a serial loop.
///
/// Tuned for the extreme exploration bench (`pop=128, sims=1`, chunks of 16) where parallel mode
/// spent 80 %+ of samples in `__psynch_cvwait`. Production workloads at `sims=500` are far above
/// this threshold and stay on the parallel path.
const PARALLEL_MIN_WORK_UNITS: usize = 1024;

#[inline]
fn monte_carlo_parallel_worthwhile(candidate_count: usize, total_iterations: usize) -> bool {
    // Parallelism is across candidates, not trials within one candidate. Avoid waking Rayon when
    // there is only one independent task, even if that task contains many simulations.
    candidate_count > 1 && total_iterations >= PARALLEL_MIN_WORK_UNITS
}

/// Run Monte Carlo over `candidates`, creating a fresh top-K leaderboard for this call.
/// The scout pass uses k=1 (single best); the full-MC/confirm pass uses k=top_k so the reported
/// top-K is preserved. For callers that must share one leaderboard across several batched calls
/// (e.g. the genetic per-generation chunked eval), use [`run_monte_carlo_inner_with_leader`].
#[allow(clippy::too_many_arguments)]
fn run_monte_carlo_inner(
    shared: &SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    early_scout: Option<ScoutEarlyStopCfg>,
    abandon: Option<(ProgressiveAbandonCfg, usize)>,
    seed_panel_policy: SeedPanelPolicy,
    chain_grind: Option<ChainGrindParams>,
) -> Vec<SimulationResult> {
    let leader = abandon.map(|(cfg, k)| (cfg, Arc::new(Mutex::new(TopKLeader::new(k.max(1))))));
    run_monte_carlo_inner_with_leader(
        shared,
        candidates,
        iterations,
        seed,
        parallel,
        early_scout,
        leader,
        seed_panel_policy,
        chain_grind,
    )
}

/// Like [`run_monte_carlo_inner`] but takes an externally-owned top-K leaderboard so progressive
/// abandonment can accumulate across multiple calls (the cut line keeps tightening as more crews
/// finish). Pass `None` for no abandonment.
#[allow(clippy::too_many_arguments)]
fn run_monte_carlo_inner_with_leader(
    shared: &SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    early_scout: Option<ScoutEarlyStopCfg>,
    abandon: Option<(ProgressiveAbandonCfg, Arc<Mutex<TopKLeader>>)>,
    seed_panel_policy: SeedPanelPolicy,
    chain_grind: Option<ChainGrindParams>,
) -> Vec<SimulationResult> {
    // Top-K leaderboard for progressive abandonment: candidates that fall hopelessly behind the
    // current cut line can terminate early, saving sim budget.
    let (progressive_abandon, best_so_far) = match abandon {
        Some((cfg, leader)) => (Some(cfg), leader),
        None => (None, Arc::new(Mutex::new(TopKLeader::new(1)))),
    };

    let run_one = |candidate: &CrewCandidate| {
        let result = match chain_grind.as_ref() {
            None => run_candidate_monte_carlo(
                shared,
                candidate,
                seed,
                iterations,
                early_scout,
                Some(&*best_so_far),
                progressive_abandon,
                seed_panel_policy,
            ),
            Some(c) => run_candidate_chain_monte_carlo(
                shared,
                candidate,
                seed,
                iterations,
                c,
                early_scout,
                Some(&*best_so_far),
                progressive_abandon,
                seed_panel_policy,
            ),
        };
        // Update the shared leaderboard so other running candidates can compare at their next
        // checkpoint. Score and its lower bound mirror the ranking metric: blended win+hull for
        // single fights, primary success rate for chain grinds. The blended lower bound sums the
        // component lowers — a conservative (looser) bound, so the cut never over-prunes.
        if progressive_abandon.is_some() {
            let (score, score_low) = if chain_grind.is_some() {
                (result.win_rate, result.win_rate_ci_low)
            } else {
                (
                    blended_score(result.win_rate, result.avg_hull_remaining),
                    blended_score(result.win_rate_ci_low, result.avg_hull_remaining_ci_low),
                )
            };
            let mut best = best_so_far.lock().unwrap();
            best.update(score, score_low);
        }
        result
    };

    // Deduplicate identical crews: compute results only for the first occurrence of each hash,
    // then clone for later duplicates. This avoids re-simulating the same crew composition
    // when it appears multiple times in the candidate list (e.g. from warm-start + generation overlap,
    // or multi-hostile batch consolidation).
    let n = candidates.len();
    let hashes: Vec<u64> = candidates.iter().map(crew_candidate_ordered_hash).collect();
    let mut unique_indices: Vec<usize> = Vec::with_capacity(n);
    let mut seen: HashSet<u64> = HashSet::with_capacity(n);
    for (i, &h) in hashes.iter().enumerate() {
        if seen.insert(h) {
            unique_indices.push(i);
        }
    }
    let parallel = parallel
        && monte_carlo_parallel_worthwhile(
            unique_indices.len(),
            unique_indices.len().saturating_mul(iterations.max(1)),
        );

    if unique_indices.len() == n {
        // All candidates are unique — no dedup overhead.
        if parallel {
            candidates.par_iter().map(run_one).collect()
        } else {
            candidates.iter().map(run_one).collect()
        }
    } else {
        let unique_candidates: Vec<&CrewCandidate> =
            unique_indices.iter().map(|&i| &candidates[i]).collect();
        let unique_results: Vec<SimulationResult> = if parallel {
            unique_candidates.par_iter().map(|c| run_one(c)).collect()
        } else {
            unique_candidates.iter().map(|c| run_one(c)).collect()
        };
        let result_map: HashMap<u64, SimulationResult> = unique_indices
            .iter()
            .zip(unique_results)
            .map(|(&i, r)| (hashes[i], r))
            .collect();
        hashes
            .iter()
            .map(|h| result_map.get(h).expect("hash in result map").clone())
            .collect()
    }
}

/// Like [`run_monte_carlo_with_shared`] but uses a per-candidate iteration count (e.g. tiered
/// confirmation after adaptive budgeting from scout-phase confidence).
fn run_monte_carlo_inner_variable_iterations(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations_per_crew: &[usize],
    seed: u64,
    parallel: bool,
    chain_grind: Option<ChainGrindParams>,
) -> Vec<SimulationResult> {
    debug_assert_eq!(candidates.len(), iterations_per_crew.len());
    let run_at = |idx: usize, candidate: &CrewCandidate| {
        let it = iterations_per_crew.get(idx).copied().unwrap_or(1).max(1);
        match chain_grind.as_ref() {
            None => run_candidate_monte_carlo(
                &shared,
                candidate,
                seed,
                it,
                None,
                None,
                None,
                SeedPanelPolicy::CandidateDistinct,
            ),
            Some(c) => run_candidate_chain_monte_carlo(
                &shared,
                candidate,
                seed,
                it,
                c,
                None,
                None,
                None,
                SeedPanelPolicy::CandidateDistinct,
            ),
        }
    };

    // Deduplicate identical crews (same approach as run_monte_carlo_inner).
    let n = candidates.len();
    let hashes: Vec<u64> = candidates.iter().map(crew_candidate_ordered_hash).collect();
    let mut unique_indices: Vec<usize> = Vec::with_capacity(n);
    let mut seen: HashSet<u64> = HashSet::with_capacity(n);
    for (i, &h) in hashes.iter().enumerate() {
        if seen.insert(h) {
            unique_indices.push(i);
        }
    }
    let unique_iterations = unique_indices
        .iter()
        .map(|&i| iterations_per_crew.get(i).copied().unwrap_or(1).max(1))
        .fold(0usize, usize::saturating_add);
    let parallel =
        parallel && monte_carlo_parallel_worthwhile(unique_indices.len(), unique_iterations);

    if unique_indices.len() == n {
        if parallel {
            candidates
                .par_iter()
                .enumerate()
                .map(|(i, c)| run_at(i, c))
                .collect()
        } else {
            candidates
                .iter()
                .enumerate()
                .map(|(i, c)| run_at(i, c))
                .collect()
        }
    } else {
        let unique_results: Vec<SimulationResult> = if parallel {
            unique_indices
                .par_iter()
                .map(|&i| run_at(i, &candidates[i]))
                .collect()
        } else {
            unique_indices
                .iter()
                .map(|&i| run_at(i, &candidates[i]))
                .collect()
        };
        let result_map: HashMap<u64, SimulationResult> = unique_indices
            .iter()
            .zip(unique_results)
            .map(|(&i, r)| (hashes[i], r))
            .collect();
        hashes
            .iter()
            .map(|h| result_map.get(h).expect("hash in result map").clone())
            .collect()
    }
}

/// Run Monte Carlo with per-candidate iteration counts; `iterations_per_crew.len()` must match `candidates`.
pub(crate) fn run_monte_carlo_with_shared_variable_iterations(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations_per_crew: &[usize],
    seed: u64,
    parallel: bool,
    chain_grind: Option<ChainGrindParams>,
) -> Vec<SimulationResult> {
    if candidates.is_empty() {
        return Vec::new();
    }
    assert_eq!(
        candidates.len(),
        iterations_per_crew.len(),
        "iterations_per_crew must match candidates"
    );
    let t0 = perf_log::perf_start();
    let out = run_monte_carlo_inner_variable_iterations(
        shared,
        candidates,
        iterations_per_crew,
        seed,
        parallel,
        chain_grind,
    );
    perf_log::log_duration(
        &format!(
            "monte_carlo.with_shared_variable(candidates={}, parallel={parallel})",
            candidates.len(),
        ),
        t0,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crew_health_diagnostic_detects_unresolved_and_missing_raw_context() {
        use crate::combat::{
            Ability, AbilityClass, AbilityEffect, CrewConfiguration, CrewSeat, CrewSeatContext,
            TimingWindow,
        };
        let mut crew = CrewConfiguration {
            seats: vec![CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "health scaling".into(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::BaseArmorPiercingPerCrewHealth(8.0),
                    condition: None,
                    weapon_scope: Default::default(),
                },
                false,
            )],
        };
        assert!(unsupported_crew_health_effects_for_crew(&crew, true, true));
        assert!(unsupported_defender_raw_effects(&crew));
        crew.seats[0].ability.effect = AbilityEffect::BaseMitigationStatsPerCrewHealth(18.0);
        assert!(unsupported_crew_health_effects_for_crew(&crew, true, true));
        assert!(unsupported_defender_raw_effects(&crew));
        crew.seats[0].ability.effect = AbilityEffect::BaseArmorPiercingAdd(50.0);
        assert!(unsupported_crew_health_effects_for_crew(&crew, false, true));
        assert!(!unsupported_crew_health_effects_for_crew(&crew, true, true));
        assert!(unsupported_defender_raw_effects(&crew));
        crew.seats[0].ability.effect = AbilityEffect::BaseMitigationStatsAdd(50.0);
        assert!(unsupported_crew_health_effects_for_crew(&crew, true, false));
        assert!(!unsupported_crew_health_effects_for_crew(&crew, true, true));
        assert!(unsupported_defender_raw_effects(&crew));
        crew.seats[0].ability.effect = AbilityEffect::BaseArmorPiercingAdd(0.0);
        assert!(!unsupported_crew_health_effects_for_crew(
            &crew, false, false
        ));
    }

    #[test]
    fn survival_uses_actual_hp_and_effective_maximum_instead_of_overkill() {
        use crate::combat::{
            Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
            CrewSeatContext, ShipType, TimingWindow,
        };
        let attacker = Combatant {
            attack: 60.0,
            hull_health: 1000.0,
            ..Default::default()
        };
        let defender = Combatant {
            attack: 100.0,
            hull_health: 100.0,
            ..Default::default()
        };
        let crew = CrewConfiguration {
            seats: vec![CrewSeatContext::legacy(
                CrewSeat::Ship,
                Ability {
                    name: "conditional_max_hull".into(),
                    class: AbilityClass::ShipAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: false,
                    effect: AbilityEffect::HullHpMultiplier(1.0),
                    condition: None,
                    weapon_scope: Default::default(),
                },
                false,
            )],
        };
        let setup = build_combat_setup_with_officer_stat(
            &attacker,
            &defender,
            &SimulationConfig {
                rounds: 5,
                ..Default::default()
            },
            &crew,
            OpponentFactionTag::Unknown,
            ShipType::Explorer,
            ShipType::Explorer,
            true,
            false,
            &CrewConfiguration::default(),
            None,
        );
        let mut scratch = attacker.clone();
        let result = simulate_combat_from_setup_reusing_attacker(&setup, 7, &mut scratch);
        assert!(result.attacker_won);
        assert_eq!(scratch.hull_health, 2000.0);
        let score = surviving_hull_fraction(&result, scratch.hull_health);
        assert!(score > 0.8 && score < 1.0, "{score}");
        assert_eq!(score, result.attacker_hull_remaining / 2000.0);
        let mut overkill = result.clone();
        overkill.total_damage *= 1000.0;
        assert_eq!(
            surviving_hull_fraction(&overkill, scratch.hull_health),
            score
        );
        overkill.attacker_hull_remaining *= 0.5;
        assert_eq!(
            surviving_hull_fraction(&overkill, scratch.hull_health),
            score * 0.5
        );
        overkill.attacker_won = false;
        assert_eq!(surviving_hull_fraction(&overkill, scratch.hull_health), 0.0);
        overkill.winner_by_round_limit = true;
        assert_eq!(surviving_hull_fraction(&overkill, scratch.hull_health), 0.0);
    }

    #[test]
    fn crew_identity_ignores_order_within_bridge_and_below_decks() {
        let crew = CrewCandidate {
            captain: "Captain".into(),
            bridge: vec!["Zeta".into(), "Alpha".into()],
            below_decks: vec!["D3".into(), "D1".into(), "D2".into()],
        };
        let reordered = CrewCandidate {
            captain: crew.captain.clone(),
            bridge: vec!["Alpha".into(), "Zeta".into()],
            below_decks: vec!["D2".into(), "D3".into(), "D1".into()],
        };
        assert_eq!(
            crew_candidate_stable_hash(&crew),
            crew_candidate_stable_hash(&reordered)
        );
        let different_captain = CrewCandidate {
            captain: "Alpha".into(),
            ..crew.clone()
        };
        assert_ne!(
            crew_candidate_stable_hash(&crew),
            crew_candidate_stable_hash(&different_captain)
        );
        let different_roles = CrewCandidate {
            captain: crew.captain.clone(),
            bridge: vec!["Zeta".into(), "D1".into()],
            below_decks: vec!["Alpha".into(), "D3".into(), "D2".into()],
        };
        assert_ne!(
            crew_candidate_stable_hash(&crew),
            crew_candidate_stable_hash(&different_roles)
        );
    }

    #[test]
    fn supported_stochastic_bridge_permutations_have_distinct_result_keys() {
        let officers =
            crate::lcars::build_officer_model_default().expect("canonical officer model");
        for name in ["Georgiou", "Airiam"] {
            let officer = officers
                .iter()
                .find(|officer| officer.name == name)
                .unwrap();
            assert!(officer
                .bridge_ability
                .as_ref()
                .unwrap()
                .effects
                .iter()
                .any(|effect| {
                    effect.effect_type == "burning"
                        && effect.trigger.as_deref() == Some("on_round_start")
                }));
        }
        let first = CrewCandidate {
            captain: "Nero".into(),
            bridge: vec!["Georgiou".into(), "Airiam".into()],
            below_decks: Vec::new(),
        };
        let reordered = CrewCandidate {
            bridge: vec!["Airiam".into(), "Georgiou".into()],
            ..first.clone()
        };
        assert_eq!(
            crew_candidate_stable_hash(&first),
            crew_candidate_stable_hash(&reordered)
        );
        assert_ne!(
            crew_candidate_ordered_hash(&first),
            crew_candidate_ordered_hash(&reordered)
        );
        let results = run_monte_carlo_parallel_deduped(
            "saladin",
            "2918121098",
            &[first.clone(), reordered.clone()],
            1,
            777,
            None,
            None,
            DefenderOpponent::Hostile,
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].candidate.bridge, first.bridge);
        assert_eq!(results[1].candidate.bridge, reordered.bridge);
    }

    #[test]
    fn wilson_upper_at_zero_wins_decreases_with_n() {
        let u50 = super::win_rate_upper_wilson_95(0, 50);
        let u200 = super::win_rate_upper_wilson_95(0, 200);
        assert!(
            u200 < u50,
            "more data should tighten upper bound: {u50} vs {u200}"
        );
    }

    #[test]
    fn rayon_is_skipped_when_work_cannot_parallelize_or_is_too_small() {
        assert!(!monte_carlo_parallel_worthwhile(1, 100_000));
        assert!(!monte_carlo_parallel_worthwhile(16, 16));
        assert!(monte_carlo_parallel_worthwhile(16, PARALLEL_MIN_WORK_UNITS));
    }

    #[test]
    fn wilson_interval_brackets_sample_proportion() {
        let (lo, hi) = super::wilson_95_interval(50, 100);
        assert!(lo <= 0.5 && 0.5 <= hi, "p=0.5 should lie in [{lo}, {hi}]");
        let (lo0, hi0) = super::wilson_95_interval(0, 50);
        assert!(lo0 <= 0.0 && 0.0 <= hi0);
    }

    #[test]
    fn common_scout_panel_pairs_candidate_seeds_but_confirmation_does_not() {
        let shared = build_shared_scenario_data_standalone(
            "saladin",
            "2918121098",
            support_buffs::SupportBuffScenarioRequest::attacker_only(None),
            DefenderOpponent::Hostile,
            None,
        );
        let a = CrewCandidate {
            captain: "James T. Kirk".into(),
            bridge: vec!["Spock".into(), "Leonard McCoy".into()],
            below_decks: vec![],
        };
        let b = CrewCandidate {
            captain: "Gowron".into(),
            bridge: vec!["Next Gen Riker".into(), "Mitchell".into()],
            below_decks: vec![],
        };
        let seed = 771;
        let mut distinct_a = scenario_to_combat_input_from_shared(&shared, &a, seed);
        let mut distinct_b = scenario_to_combat_input_from_shared(&shared, &b, seed);
        apply_seed_panel_policy(
            &mut distinct_a,
            &shared,
            seed,
            SeedPanelPolicy::CandidateDistinct,
        );
        apply_seed_panel_policy(
            &mut distinct_b,
            &shared,
            seed,
            SeedPanelPolicy::CandidateDistinct,
        );
        assert_ne!(distinct_a.base_seed, distinct_b.base_seed);

        apply_seed_panel_policy(
            &mut distinct_a,
            &shared,
            seed,
            SeedPanelPolicy::CommonScenario,
        );
        apply_seed_panel_policy(
            &mut distinct_b,
            &shared,
            seed,
            SeedPanelPolicy::CommonScenario,
        );
        assert_eq!(distinct_a.base_seed, distinct_b.base_seed);
    }

    #[test]
    fn scout_checks_at_requested_trial_count_inside_a_batch() {
        let shared = build_shared_scenario_data_standalone(
            "saladin",
            "2918121098",
            support_buffs::SupportBuffScenarioRequest::attacker_only(None),
            DefenderOpponent::Hostile,
            None,
        );
        let candidate = CrewCandidate {
            captain: "James T. Kirk".into(),
            bridge: vec!["Spock".into(), "Leonard McCoy".into()],
            below_decks: vec![],
        };
        // A threshold above 1 guarantees elimination at the first eligible check.
        // With a 64-trial batch and check_every=50, that check is trial 100.
        let result = run_candidate_monte_carlo(
            &shared,
            &candidate,
            771,
            500,
            Some(ScoutEarlyStopCfg {
                min_trials: 64,
                check_every: 50,
                eliminate_upper_below: 1.1,
            }),
            None,
            None,
            SeedPanelPolicy::CommonScenario,
        );
        assert_eq!(result.trials_run, 100);
    }

    #[test]
    fn progressive_abandon_checks_at_requested_trial_count_inside_a_batch() {
        let shared = build_shared_scenario_data_standalone(
            "saladin",
            "2918121098",
            support_buffs::SupportBuffScenarioRequest::attacker_only(None),
            DefenderOpponent::Hostile,
            None,
        );
        let candidate = CrewCandidate {
            captain: "James T. Kirk".into(),
            bridge: vec!["Spock".into(), "Leonard McCoy".into()],
            below_decks: vec![],
        };
        // An unreachable synthetic cut guarantees abandonment whenever checked; this
        // isolates scheduling from the candidate's combat outcome.
        let mut leader = TopKLeader::new(1);
        leader.update(2.0, 2.0);
        let leader = Mutex::new(leader);
        let result = run_candidate_monte_carlo(
            &shared,
            &candidate,
            771,
            500,
            None,
            Some(&leader),
            Some(ProgressiveAbandonCfg {
                min_trials: 64,
                check_every: 50,
                margin: 0.0,
            }),
            SeedPanelPolicy::CommonScenario,
        );
        assert_eq!(result.trials_run, 100);
    }

    #[test]
    fn shared_leader_abandons_across_candidate_batches_at_the_checkpoint() {
        let registry = DataRegistry::load().expect("data registry");
        let mut shared = build_shared_scenario_data_from_registry(
            &registry,
            "botany_bay",
            "38048587",
            Some(2),
            Some(10),
            None,
            support_buffs::SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let defender = shared.cached_defender.as_mut().expect("resolved hostile");
        defender.hull_health = 1e15;
        defender.shield_health = 1e15;
        let candidates: Vec<_> = ["test_a", "test_b", "test_c", "test_d"]
            .into_iter()
            .map(|captain| CrewCandidate {
                captain: captain.into(),
                bridge: Vec::new(),
                below_decks: Vec::new(),
            })
            .collect();
        // Two synthetic completed winners with valid normalized scores establish a cut. The
        // deliberately unbeatable defender makes every tested candidate a provable loser.
        let mut board = TopKLeader::new(2);
        board.update(0.9, 0.85);
        board.update(0.9, 0.85);
        let leader = Arc::new(Mutex::new(board));
        let cfg = ProgressiveAbandonCfg {
            min_trials: 64,
            check_every: 50,
            margin: 0.0,
        };
        let mut actual = Vec::new();
        for batch in candidates.chunks(2) {
            actual.extend(run_monte_carlo_inner_with_leader(
                &shared,
                batch,
                500,
                771,
                false,
                None,
                Some((cfg, Arc::clone(&leader))),
                SeedPanelPolicy::CommonScenario,
                None,
            ));
        }
        assert_eq!(actual.len(), candidates.len());
        assert!(actual.iter().all(|row| row.trials_run == 100));
        assert!(actual.iter().all(|row| row.win_rate == 0.0));
    }

    #[test]
    fn topk_leader_no_cut_until_k_finished() {
        let mut leader = TopKLeader::new(3);
        assert!(leader.cut_lower().is_none(), "no cut before k crews finish");
        leader.update(0.9, 0.85);
        leader.update(0.8, 0.75);
        assert!(
            leader.cut_lower().is_none(),
            "still no cut with only 2 of 3 crews"
        );
        // Until the board is full nothing can be abandoned, even a crew with score upper 0.
        assert!(leader.can_beat_leader(0.0, 0.05));
    }

    #[test]
    fn topk_leader_cut_is_kth_best_ci_low() {
        let mut leader = TopKLeader::new(3);
        // Insert in arbitrary order; the cut must track the 3rd-best by score.
        leader.update(0.70, 0.66); // 3rd best
        leader.update(0.90, 0.86); // 1st best
        leader.update(0.80, 0.76); // 2nd best
        let cut = leader.cut_lower().expect("board full");
        assert!(
            (cut - 0.66).abs() < 1e-9,
            "cut should be 3rd-best ci_low (0.66), got {cut}"
        );
        // A better crew evicts the weakest and raises the cut to the new 3rd best.
        leader.update(0.95, 0.91);
        let cut2 = leader.cut_lower().expect("board full");
        assert!(
            (cut2 - 0.76).abs() < 1e-9,
            "cut should rise to new 3rd-best ci_low (0.76), got {cut2}"
        );
    }

    #[test]
    fn topk_leader_abandons_only_hopeless_candidates() {
        let mut leader = TopKLeader::new(1);
        leader.update(0.95, 0.90); // single leader, cut = 0.90
                                   // A crew whose score upper bound (0.02) is far below 0.90 − margin → abandon.
        assert!(
            !leader.can_beat_leader(0.02, 0.05),
            "hopeless crew should be abandoned"
        );
        // A crew whose score upper bound (0.93) clears the cut − margin (0.85) → keep simulating.
        assert!(
            leader.can_beat_leader(0.93, 0.05),
            "contender near the cut should survive"
        );
    }

    #[test]
    fn blended_score_matches_ranking_formula() {
        // Mirrors rank_results: 0.8·win + 0.2·hull.
        assert!((blended_score(1.0, 0.0) - 0.8).abs() < 1e-12);
        assert!((blended_score(1.0, 1.0) - 1.0).abs() < 1e-12);
        assert!((blended_score(0.0, 0.5) - 0.1).abs() < 1e-12);
    }

    #[test]
    fn deduped_chunked_returns_none_when_should_continue_false() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let a = CrewCandidate {
            captain: "A".into(),
            bridge: vec!["B".into(), "C".into()],
            below_decks: vec!["D".into(), "E".into(), "F".into()],
        };
        let b = CrewCandidate {
            captain: "G".into(),
            bridge: vec!["H".into(), "I".into()],
            below_decks: vec!["J".into(), "K".into(), "L".into()],
        };
        let c = CrewCandidate {
            captain: "M".into(),
            bridge: vec!["N".into(), "O".into()],
            below_decks: vec!["P".into(), "Q".into(), "R".into()],
        };
        let pop = vec![a.clone(), b.clone(), c.clone(), a.clone()];
        let calls = AtomicUsize::new(0);
        let out = super::run_monte_carlo_parallel_deduped_chunked(
            "enterprise",
            "swarm",
            &pop,
            4,
            42,
            None,
            None,
            DefenderOpponent::Hostile,
            1,
            || calls.fetch_add(1, Ordering::Relaxed) < 2,
        );
        assert!(
            out.is_none(),
            "expected cooperative stop before third unique chunk"
        );
    }

    #[test]
    fn deduped_mc_matches_full_for_duplicate_crews() {
        let a = CrewCandidate {
            captain: "A".into(),
            bridge: vec!["B".into(), "C".into()],
            below_decks: vec!["D".into(), "E".into(), "F".into()],
        };
        let pop = vec![a.clone(), a.clone()];
        // Build one immutable scenario snapshot for both paths. Rebuilding the standalone
        // scenario here makes this test observe profile/environment mutations from unrelated
        // parallel tests between the two runs, which tests disk timing rather than deduplication.
        let shared = build_shared_scenario_data_standalone(
            "enterprise",
            "swarm",
            support_buffs::SupportBuffScenarioRequest::attacker_only(None),
            DefenderOpponent::Hostile,
            None,
        );
        let full = run_monte_carlo_inner(
            &shared,
            &pop,
            8,
            42,
            false,
            None,
            None,
            SeedPanelPolicy::CandidateDistinct,
            None,
        );
        let deduped = run_monte_carlo_parallel_deduped_chunked_with_shared(
            &shared,
            &pop,
            8,
            42,
            None,
            pop.len(),
            None,
            || true,
        )
        .expect("deduped run should complete");
        assert_eq!(full.len(), deduped.len());
        assert_eq!(full[0].win_rate, deduped[0].win_rate);
        assert_eq!(full[1].win_rate, deduped[1].win_rate);
        assert_eq!(full[0].stall_rate, deduped[0].stall_rate);
    }
}
