//! Simulation orchestration: run_monte_carlo* and SimulationResult.

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use crate::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, CombatEvent, OpponentFactionTag,
    SimulationConfig, TraceMode,
};
use crate::data::data_registry::DataRegistry;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::perf_log;

use super::crew_resolution::seeded_variance;
use super::scenario::{
    build_shared_scenario_data_from_registry, build_shared_scenario_data_standalone,
    scenario_to_combat_input_from_shared, SharedScenarioData,
};

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub candidate: CrewCandidate,
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
    pub avg_hull_remaining: f64,
    /// Normal-approx 95% CI for the per-trial mean (hull fraction on wins, 0 on losses).
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
}

/// Stable hash for deduplicating identical crews in GA populations (same process = deterministic).
pub fn crew_candidate_stable_hash(c: &CrewCandidate) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.captain.hash(&mut h);
    for s in &c.bridge {
        s.hash(&mut h);
    }
    for s in &c.below_decks {
        s.hash(&mut h);
    }
    h.finish()
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

fn run_candidate_monte_carlo(
    shared: &SharedScenarioData,
    candidate: &CrewCandidate,
    seed: u64,
    max_iterations: usize,
    early_scout: Option<ScoutEarlyStopCfg>,
) -> SimulationResult {
    let input = scenario_to_combat_input_from_shared(shared, candidate, seed);
    let mut wins = 0usize;
    let mut stalls = 0usize;
    let mut losses = 0usize;
    let mut r1_kills = 0usize;
    let mut surviving_hull_sum = 0.0f64;
    let mut hull_mean = 0.0f64;
    let mut hull_m2 = 0.0f64;

    let mut combat_config = SimulationConfig {
        rounds: input.rounds,
        seed: 0,
        trace_mode: TraceMode::Off,
    };

    let mut n_done = 0usize;
    while n_done < max_iterations {
        let iteration_seed = input.base_seed.wrapping_add(n_done as u64);
        combat_config.seed = iteration_seed;
        let defender_faction = shared
            .hostile_rec
            .as_ref()
            .map(|h| h.opponent_faction_tag())
            .unwrap_or(OpponentFactionTag::Unknown);
        let result = simulate_combat_with_defender_faction_and_defender_crew(
            &input.attacker,
            &input.defender,
            combat_config,
            &input.crew,
            defender_faction,
            &input.defender_crew,
        );
        let effective_hull = input.defender_hull * seeded_variance(iteration_seed);

        if result.winner_by_round_limit {
            stalls += 1;
        } else if result.attacker_won {
            wins += 1;
        } else {
            losses += 1;
        }

        let hull_draw = if result.attacker_won {
            let remaining = if result.winner_by_round_limit {
                (result.attacker_hull_remaining / input.attacker.hull_health.max(1.0)).clamp(0.0, 1.0)
            } else {
                ((result.total_damage - effective_hull) / effective_hull).clamp(0.0, 1.0)
            };
            surviving_hull_sum += remaining;
            remaining
        } else {
            0.0
        };
        let i = n_done + 1;
        let delta = hull_draw - hull_mean;
        hull_mean += delta / i as f64;
        let delta2 = hull_draw - hull_mean;
        hull_m2 += delta * delta2;

        if result.attacker_won
            && !result.winner_by_round_limit
            && result.rounds_simulated == 1
        {
            r1_kills += 1;
        }

        n_done += 1;

        if let Some(cfg) = early_scout {
            if n_done >= cfg.min_trials
                && n_done < max_iterations
                && cfg.check_every > 0
                && (n_done % cfg.check_every == 0)
                && win_rate_upper_wilson_95(wins, n_done) < cfg.eliminate_upper_below
            {
                break;
            }
        }
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

    SimulationResult {
        candidate: candidate.clone(),
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
    }
}

pub fn run_monte_carlo(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(
        ship,
        hostile,
        candidates,
        iterations,
        seed,
        false,
    )
}

/// Like [run_monte_carlo] but distributes candidates across all CPU cores via Rayon.
/// Use for large candidate lists (e.g. optimizer sweeps). Results order matches input order.
pub fn run_monte_carlo_parallel(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(
        ship,
        hostile,
        candidates,
        iterations,
        seed,
        true,
    )
}

/// Monte Carlo for a population that may contain duplicate crews: simulates each distinct crew once
/// and copies rates for duplicates (deterministic, same seeds as evaluating each separately).
pub fn run_monte_carlo_parallel_deduped(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
) -> Vec<SimulationResult> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut seen_hashes: HashSet<u64> = HashSet::with_capacity(candidates.len());
    let mut unique_indices: Vec<usize> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let k = crew_candidate_stable_hash(c);
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
    );

    let mut by_hash: HashMap<u64, SimulationResult> =
        HashMap::with_capacity(uniq_results.len());
    for (j, r) in uniq_results.into_iter().enumerate() {
        let c = &candidates[unique_indices[j]];
        let k = crew_candidate_stable_hash(c);
        by_hash.insert(
            k,
            SimulationResult {
                candidate: c.clone(),
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
            },
        );
    }

    candidates
        .iter()
        .map(|c| {
            by_hash
                .get(&crew_candidate_stable_hash(c))
                .expect("dedup MC: hash present")
                .clone()
        })
        .collect()
}

/// Like [run_monte_carlo_parallel] but uses [DataRegistry] for officers and ship/hostile resolution (no reload).
/// When ship_tier or ship_level is set, uses data/ships_extended for accurate stats.
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
) -> (Vec<SimulationResult>, bool) {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
    );
    let placeholder = shared.using_placeholder_combatants;
    (
        run_monte_carlo_with_shared(shared, candidates, iterations, seed, true),
        placeholder,
    )
}

/// Like [run_monte_carlo] but uses [DataRegistry] for officers and ship/hostile resolution (no reload).
/// When ship_tier or ship_level is set, uses data/ships_extended for accurate stats.
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
) -> (Vec<SimulationResult>, bool) {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
    );
    let placeholder = shared.using_placeholder_combatants;
    (
        run_monte_carlo_with_shared(shared, candidates, iterations, seed, false),
        placeholder,
    )
}

/// One Monte Carlo draw replayed with full combat trace — same RNG seed as
/// [`run_candidate_monte_carlo`]: `iteration_seed = base_seed.wrapping_add(sim_index)`.
#[derive(Debug, Clone)]
pub struct MonteCarloSeedReplay {
    pub using_placeholder_combatants: bool,
    pub scenario_seed: u64,
    pub sim_index: u64,
    pub base_seed: u64,
    pub iteration_seed: u64,
    pub effective_defender_hull: f64,
    pub attacker_won: bool,
    pub winner_by_round_limit: bool,
    pub rounds_simulated: u32,
    pub total_damage: f64,
    pub attacker_hull_remaining: f64,
    pub defender_hull_remaining: f64,
    pub defender_shield_remaining: f64,
    pub trace_event_count: usize,
    pub trace_events_returned: usize,
    pub trace_truncated: bool,
    pub trace_events: Vec<CombatEvent>,
}

/// Replay a single iteration from an optimize/simulate Monte Carlo run (`scenario_seed` matches the request seed).
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
) -> MonteCarloSeedReplay {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
    );
    let input = scenario_to_combat_input_from_shared(&shared, candidate, scenario_seed);
    let iteration_seed = input.base_seed.wrapping_add(sim_index);
    let effective_defender_hull = input.defender_hull * seeded_variance(iteration_seed);

    let defender_faction = shared
        .hostile_rec
        .as_ref()
        .map(|h| h.opponent_faction_tag())
        .unwrap_or(OpponentFactionTag::Unknown);

    let combat_config = SimulationConfig {
        rounds: input.rounds,
        seed: iteration_seed,
        trace_mode: TraceMode::Events,
    };

    let combat = simulate_combat_with_defender_faction_and_defender_crew(
        &input.attacker,
        &input.defender,
        combat_config,
        &input.crew,
        defender_faction,
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
        scenario_seed,
        sim_index,
        base_seed: input.base_seed,
        iteration_seed,
        effective_defender_hull,
        attacker_won: combat.attacker_won,
        winner_by_round_limit: combat.winner_by_round_limit,
        rounds_simulated: combat.rounds_simulated,
        total_damage: combat.total_damage,
        attacker_hull_remaining: combat.attacker_hull_remaining,
        defender_hull_remaining: combat.defender_hull_remaining,
        defender_shield_remaining: combat.defender_shield_remaining,
        trace_event_count,
        trace_events_returned,
        trace_truncated,
        trace_events,
    }
}

fn run_monte_carlo_with_parallelism(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
) -> Vec<SimulationResult> {
    let shared = build_shared_scenario_data_standalone(ship, hostile);
    run_monte_carlo_with_shared(shared, candidates, iterations, seed, parallel)
}

/// Run Monte Carlo using pre-built SharedScenarioData (used by both legacy and registry paths).
pub(crate) fn run_monte_carlo_with_shared(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
) -> Vec<SimulationResult> {
    let t0 = perf_log::perf_start();
    let out = run_monte_carlo_inner(
        shared,
        candidates,
        iterations,
        seed,
        parallel,
        None,
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

/// Tiered scout phase: same statistics semantics as full MC when no early stop triggers; may use fewer
/// iterations per crew via Wilson-bound elimination (deterministic given the same iteration order).
pub(crate) fn run_monte_carlo_scout_phase_with_shared(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
) -> Vec<SimulationResult> {
    let cfg = ScoutEarlyStopCfg::for_scout_iterations(iterations.max(1));
    run_monte_carlo_inner(shared, candidates, iterations, seed, parallel, Some(cfg))
}

fn run_monte_carlo_inner(
    shared: SharedScenarioData,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
    early_scout: Option<ScoutEarlyStopCfg>,
) -> Vec<SimulationResult> {
    let run_one = |candidate: &CrewCandidate| {
        run_candidate_monte_carlo(&shared, candidate, seed, iterations, early_scout)
    };

    if parallel {
        candidates.par_iter().map(run_one).collect()
    } else {
        candidates.iter().map(run_one).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_upper_at_zero_wins_decreases_with_n() {
        let u50 = super::win_rate_upper_wilson_95(0, 50);
        let u200 = super::win_rate_upper_wilson_95(0, 200);
        assert!(u200 < u50, "more data should tighten upper bound: {u50} vs {u200}");
    }

    #[test]
    fn wilson_interval_brackets_sample_proportion() {
        let (lo, hi) = super::wilson_95_interval(50, 100);
        assert!(lo <= 0.5 && 0.5 <= hi, "p=0.5 should lie in [{lo}, {hi}]");
        let (lo0, hi0) = super::wilson_95_interval(0, 50);
        assert!(lo0 <= 0.0 && 0.0 <= hi0);
    }

    #[test]
    fn deduped_mc_matches_full_for_duplicate_crews() {
        let a = CrewCandidate {
            captain: "A".into(),
            bridge: vec!["B".into(), "C".into()],
            below_decks: vec!["D".into(), "E".into(), "F".into()],
        };
        let pop = vec![a.clone(), a.clone()];
        let full = run_monte_carlo_parallel(
            "enterprise",
            "swarm",
            &pop,
            8,
            42,
        );
        let deduped = run_monte_carlo_parallel_deduped(
            "enterprise",
            "swarm",
            &pop,
            8,
            42,
        );
        assert_eq!(full.len(), deduped.len());
        assert_eq!(full[0].win_rate, deduped[0].win_rate);
        assert_eq!(full[1].win_rate, deduped[1].win_rate);
        assert_eq!(full[0].stall_rate, deduped[0].stall_rate);
    }
}
