//! Side-by-side Monte Carlo distributions for a small set of crews (compare view).

use std::collections::HashMap;

use rayon::prelude::*;

use crate::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, OpponentFactionTag, SimulationConfig,
    SimulationResult as CombatSimResult, TraceMode,
};
use crate::data::data_registry::DataRegistry;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::crew_resolution::seeded_variance;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, scenario_to_combat_input_from_shared,
    CombatSimulationInput, DefenderOpponent, SharedScenarioData,
};

use serde::Serialize;

/// Buckets for rounds-to-decision on clean wins (1..=last index; last bucket includes higher rounds).
const ROUNDS_BUCKETS: u32 = 20;

const PROC_LABELS: &[&str] = &[
    "burning_trigger",
    "assimilated_trigger",
    "hull_breach_trigger",
    "shots_bonus_trigger",
    "morale_activation",
    "proc_triggers",
];

#[derive(Debug, Clone, Serialize)]
pub struct CompareCrewDistribution {
    pub captain: String,
    pub trials: u32,
    pub wins: u32,
    pub stalls: u32,
    pub losses: u32,
    /// `(round_label, count)` with `round_label` in 1..=ROUNDS_BUCKETS (last bucket merges tail).
    pub rounds_histogram: Vec<(u32, u32)>,
    /// Ten bins for attacker hull fraction on clean wins: [0,0.1) … [0.9,1.0].
    pub hull_remaining_bins: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proc_rates: Option<HashMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompareCrewsOutcome {
    pub crews: Vec<CompareCrewDistribution>,
    pub using_placeholder_combatants: bool,
}

fn simulate_trial(
    shared: &SharedScenarioData,
    input: &CombatSimulationInput,
    iteration_seed: u64,
    trace: TraceMode,
) -> CombatSimResult {
    let mut combat_config = SimulationConfig {
        rounds: input.rounds,
        seed: iteration_seed,
        trace_mode: trace,
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
        emit_state_snapshots: false,
    };
    let defender_faction = shared
        .hostile_rec
        .as_ref()
        .map(|h| h.opponent_faction_tag())
        .unwrap_or(OpponentFactionTag::Unknown);
    let defender_ship_type = shared.defender_ship_type_for_combat();
    let attacker_ship_type = shared.attacker_ship_type_for_combat();
    combat_config.seed = iteration_seed;
    simulate_combat_with_defender_faction_and_defender_crew(
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
    )
}

fn hull_fraction_on_win(
    input: &CombatSimulationInput,
    result: &CombatSimResult,
    iteration_seed: u64,
) -> f64 {
    let effective_hull = input.defender_hull * seeded_variance(iteration_seed);
    if result.winner_by_round_limit {
        (result.attacker_hull_remaining / input.attacker.hull_health.max(1.0)).clamp(0.0, 1.0)
    } else {
        ((result.total_damage - effective_hull) / effective_hull).clamp(0.0, 1.0)
    }
}

fn hull_bin_index(frac: f64) -> usize {
    let idx = (frac * 10.0).floor() as i32;
    idx.clamp(0, 9) as usize
}

fn rounds_bucket(r: u32) -> u32 {
    r.clamp(1, ROUNDS_BUCKETS)
}

fn collect_proc_counts(
    shared: &SharedScenarioData,
    candidate: &CrewCandidate,
    base_seed: u64,
    proc_trials: u32,
) -> HashMap<String, f64> {
    let mut totals: HashMap<String, f64> = HashMap::new();
    if proc_trials == 0 {
        return totals;
    }
    let input_template = scenario_to_combat_input_from_shared(shared, candidate, base_seed);
    let n = proc_trials as f64;
    for t in 0..proc_trials {
        let iteration_seed = base_seed
            .wrapping_add(10_000_000)
            .wrapping_add(u64::from(t));
        let result = simulate_trial(shared, &input_template, iteration_seed, TraceMode::Events);
        let mut seen: HashMap<String, u32> = HashMap::new();
        for e in &result.events {
            if PROC_LABELS.contains(&e.event_type.as_str()) {
                *seen.entry(e.event_type.clone()).or_insert(0) += 1;
            }
        }
        for (k, c) in seen {
            *totals.entry(k).or_insert(0.0) += f64::from(c);
        }
    }
    totals.iter_mut().for_each(|(_, v)| *v /= n);
    totals
}

fn distribution_for_crew(
    shared: &SharedScenarioData,
    candidate: &CrewCandidate,
    iterations: usize,
    base_seed: u64,
    proc_sample_trials: u32,
) -> CompareCrewDistribution {
    let input_template = scenario_to_combat_input_from_shared(shared, candidate, base_seed);
    let mut wins = 0u32;
    let mut stalls = 0u32;
    let mut losses = 0u32;
    let mut rounds_hist: HashMap<u32, u32> = HashMap::new();
    let mut hull_bins = vec![0u32; 10];

    for n_done in 0..iterations {
        let iteration_seed = input_template.base_seed.wrapping_add(n_done as u64);
        let result = simulate_trial(shared, &input_template, iteration_seed, TraceMode::Off);
        if result.winner_by_round_limit {
            stalls += 1;
        } else if result.attacker_won {
            wins += 1;
            let b = rounds_bucket(result.rounds_simulated);
            *rounds_hist.entry(b).or_insert(0) += 1;
            let hf = hull_fraction_on_win(&input_template, &result, iteration_seed);
            hull_bins[hull_bin_index(hf)] += 1;
        } else {
            losses += 1;
        }
    }

    let rounds_vec: Vec<(u32, u32)> = (1..=ROUNDS_BUCKETS)
        .map(|r| (r, *rounds_hist.get(&r).unwrap_or(&0)))
        .collect();

    let proc_rates = if proc_sample_trials > 0 {
        Some(collect_proc_counts(
            shared,
            candidate,
            base_seed,
            proc_sample_trials,
        ))
    } else {
        None
    };

    CompareCrewDistribution {
        captain: candidate.captain.clone(),
        trials: iterations as u32,
        wins,
        stalls,
        losses,
        rounds_histogram: rounds_vec,
        hull_remaining_bins: hull_bins,
        proc_rates,
    }
}

/// Run Monte Carlo for 2–5 crews and return histograms for rounds (clean wins), hull remaining (clean wins), and optional proc rates from traced subsample.
#[allow(clippy::too_many_arguments)] // registry-driven MC entry; options are scenario + sampling knobs
pub fn compare_crews_monte_carlo_with_registry(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    candidates: &[CrewCandidate],
    iterations: usize,
    base_seed: u64,
    profile_id: Option<&str>,
    proc_sample_trials: u32,
    support_buffs: Option<&[String]>,
    defender_opponent: DefenderOpponent,
) -> CompareCrewsOutcome {
    let shared = build_shared_scenario_data_from_registry(
        registry,
        ship,
        hostile,
        ship_tier,
        ship_level,
        profile_id,
        support_buffs,
        defender_opponent,
        None,
    );
    let placeholder = shared.using_placeholder_combatants;
    let crews: Vec<CompareCrewDistribution> = candidates
        .par_iter()
        .enumerate()
        .map(|(i, c)| {
            let seed_i = base_seed.wrapping_add(i as u64 * 1_000_003);
            distribution_for_crew(&shared, c, iterations, seed_i, proc_sample_trials)
        })
        .collect();

    CompareCrewsOutcome {
        crews,
        using_placeholder_combatants: placeholder,
    }
}
