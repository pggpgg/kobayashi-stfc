//! Simulation orchestration: run_monte_carlo* and SimulationResult.

use rayon::prelude::*;
use std::collections::HashMap;

use crate::combat::{simulate_combat, SimulationConfig, TraceMode};
use crate::data::data_registry::DataRegistry;
use crate::data::loader::{resolve_hostile, resolve_ship};
use crate::data::officer::{load_canonical_officers, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::profile_index::{profile_path, FORBIDDEN_TECH_IMPORTED, PROFILE_JSON, ROSTER_IMPORTED};
use crate::data::{forbidden_chaos, import};
use crate::data::profile::{load_profile, merge_forbidden_tech_bonuses_into_profile};
use crate::lcars::{index_lcars_officers_by_id, load_lcars_dir};
use crate::optimizer::crew_generator::CrewCandidate;

use super::crew_resolution::{index_officers_by_name, normalize_lookup_key, seeded_variance};
use super::scenario::{
    build_shared_scenario_data_from_registry, scenario_to_combat_input_from_shared,
    LcarsOfficerData, SharedScenarioData,
};

const DEFAULT_LCARS_OFFICERS_DIR: &str = "data/officers";

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub candidate: CrewCandidate,
    pub win_rate: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub avg_hull_remaining: f64,
}

pub fn run_monte_carlo(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(ship, hostile, candidates, iterations, seed, false)
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
    run_monte_carlo_with_parallelism(ship, hostile, candidates, iterations, seed, true)
}

fn use_lcars_officer_source() -> bool {
    std::env::var("KOBAYASHI_OFFICER_SOURCE")
        .map(|v| v.eq_ignore_ascii_case("lcars"))
        .unwrap_or(false)
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
) -> Vec<SimulationResult> {
    let shared =
        build_shared_scenario_data_from_registry(registry, ship, hostile, ship_tier, ship_level, profile_id);
    run_monte_carlo_with_shared(shared, candidates, iterations, seed, true)
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
) -> Vec<SimulationResult> {
    let shared =
        build_shared_scenario_data_from_registry(registry, ship, hostile, ship_tier, ship_level, profile_id);
    run_monte_carlo_with_shared(shared, candidates, iterations, seed, false)
}

fn run_monte_carlo_with_parallelism(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
) -> Vec<SimulationResult> {
    let officer_index = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .ok()
        .map(index_officers_by_name)
        .unwrap_or_default();
    let pid = crate::data::profile_index::resolve_profile_id_for_api(None);
    let profile_path_str = profile_path(&pid, PROFILE_JSON).to_string_lossy().to_string();
    let ft_path = profile_path(&pid, FORBIDDEN_TECH_IMPORTED).to_string_lossy().to_string();
    let mut profile = load_profile(&profile_path_str);
    let ft_entries: Vec<import::ForbiddenTechEntry> = if profile
        .forbidden_tech_override
        .as_ref()
        .map_or(false, |v| !v.is_empty())
    {
        profile
            .forbidden_tech_override
            .as_ref()
            .unwrap()
            .iter()
            .map(|&fid| import::ForbiddenTechEntry {
                fid,
                tier: 1,
                level: 1,
                shard_count: 0,
            })
            .collect()
    } else {
        import::load_imported_forbidden_tech(&ft_path).unwrap_or_default()
    };
    if let (Some(catalog), true) = (
        forbidden_chaos::load_forbidden_chaos(forbidden_chaos::DEFAULT_FORBIDDEN_CHAOS_PATH),
        !ft_entries.is_empty(),
    ) {
        merge_forbidden_tech_bonuses_into_profile(&mut profile, &ft_entries, &catalog);
    }

    let lcars_data = if use_lcars_officer_source() {
        load_lcars_dir(DEFAULT_LCARS_OFFICERS_DIR)
            .ok()
            .map(|officers| {
                let by_id = index_lcars_officers_by_id(officers);
                let name_to_id: HashMap<String, String> = by_id
                    .values()
                    .map(|o| (normalize_lookup_key(&o.name), o.id.clone()))
                    .collect();
                LcarsOfficerData {
                    by_id,
                    name_to_id,
                }
            })
    } else {
        None
    };

    let roster_path = profile_path(
        &crate::data::profile_index::resolve_profile_id_for_api(None),
        ROSTER_IMPORTED,
    )
    .to_string_lossy()
    .to_string();
    let resolve_options = import::load_imported_roster(&roster_path)
        .map(|entries| {
            let officer_tiers: HashMap<String, u8> = entries
                .into_iter()
                .filter_map(|e| e.tier.map(|t| (e.canonical_officer_id, t)))
                .collect();
            crate::lcars::ResolveOptions {
                tier: None,
                officer_tiers: if officer_tiers.is_empty() {
                    None
                } else {
                    Some(officer_tiers)
                },
            }
        })
        .unwrap_or_default();

    let ship_rec = resolve_ship(ship);
    let hostile_rec = resolve_hostile(hostile);

    let (cached_defender, cached_rounds, cached_defender_hull, cached_pierce, cached_defender_mitigation) =
        if let (Some(ref ship_r), Some(ref hostile_r)) = (&ship_rec, &hostile_rec) {
            let attacker_stats = ship_r.to_attacker_stats();
            let defender_mitigation = crate::combat::mitigation_for_hostile(
                hostile_r.to_defender_stats(),
                attacker_stats,
                hostile_r.ship_type(),
                hostile_r.mystery_mitigation_factor.unwrap_or(0.0),
                hostile_r.mitigation_floor.unwrap_or(crate::combat::MITIGATION_FLOOR),
                hostile_r.mitigation_ceiling.unwrap_or(crate::combat::MITIGATION_CEILING),
            );
            let pierce = crate::combat::pierce_damage_through_bonus(
                hostile_r.to_defender_stats(),
                attacker_stats,
                hostile_r.ship_type(),
            );
            let defender = crate::combat::Combatant {
                id: hostile.to_string(),
                attack: 0.0,
                mitigation: defender_mitigation,
                pierce: 0.0,
                crit_chance: 0.0,
                crit_multiplier: 1.0,
                proc_chance: 0.0,
                proc_multiplier: 1.0,
                end_of_round_damage: 0.0,
                hull_health: hostile_r.hull_health,
                shield_health: hostile_r.shield_health,
                shield_mitigation: hostile_r.shield_mitigation.unwrap_or(0.8),
                apex_barrier: hostile_r.apex_barrier,
                apex_shred: 0.0,
                isolytic_damage: 0.0,
                isolytic_defense: hostile_r.isolytic_defense,
                weapons: vec![],
            };
            let rounds = 100u32.min(10u32.saturating_add(hostile_r.level as u32));
            (
                Some(defender),
                Some(rounds),
                Some(hostile_r.hull_health),
                Some(pierce),
                Some(defender_mitigation),
            )
        } else {
            (None, None, None, None, None)
        };

    let shared = SharedScenarioData {
        ship: ship.to_string(),
        hostile: hostile.to_string(),
        officer_index,
        profile,
        lcars_data,
        resolve_options,
        ship_rec,
        hostile_rec,
        cached_defender,
        cached_rounds,
        cached_defender_hull,
        cached_pierce,
        cached_defender_mitigation,
    };

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
    let run_one = |candidate: &CrewCandidate| {
        let input = scenario_to_combat_input_from_shared(&shared, candidate, seed);
        let mut wins = 0usize;
        let mut stalls = 0usize;
        let mut losses = 0usize;
        let mut surviving_hull_sum = 0.0;

        for iteration in 0..iterations {
            let iteration_seed = input.base_seed.wrapping_add(iteration as u64);
            let result = simulate_combat(
                &input.attacker,
                &input.defender,
                SimulationConfig {
                    rounds: input.rounds,
                    seed: iteration_seed,
                    trace_mode: TraceMode::Off,
                },
                &input.crew,
            );
            let effective_hull = input.defender_hull * seeded_variance(iteration_seed);

            if result.winner_by_round_limit {
                stalls += 1;
            } else if result.attacker_won {
                wins += 1;
            } else {
                losses += 1;
            }

            if result.attacker_won {
                let remaining = if result.winner_by_round_limit {
                    (result.attacker_hull_remaining / input.attacker.hull_health.max(1.0))
                        .clamp(0.0, 1.0)
                } else {
                    ((result.total_damage - effective_hull) / effective_hull).clamp(0.0, 1.0)
                };
                surviving_hull_sum += remaining;
            }
        }

        let n = iterations as f64;
        let win_rate = if iterations == 0 { 0.0 } else { wins as f64 / n };
        let stall_rate = if iterations == 0 { 0.0 } else { stalls as f64 / n };
        let loss_rate = if iterations == 0 { 0.0 } else { losses as f64 / n };
        let avg_hull_remaining = if iterations == 0 {
            0.0
        } else {
            surviving_hull_sum / n
        };

        SimulationResult {
            candidate: candidate.clone(),
            win_rate,
            stall_rate,
            loss_rate,
            avg_hull_remaining,
        }
    };

    if parallel {
        candidates.par_iter().map(run_one).collect()
    } else {
        candidates.iter().map(run_one).collect()
    }
}
