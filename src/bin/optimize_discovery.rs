//! Matchup discovery: find winning crews and measure time-to-first-win.
//!
//! Usage (repo root):
//!   cargo run --release --bin optimize_discovery
//!
//! Env (defaults target Gorn Eviscerator vs hostile 447012258):
//!   KOBAYASHI_DISCOVERY_SHIP=gorn_eviscerator
//!   KOBAYASHI_DISCOVERY_SHIP_TIER=10
//!   KOBAYASHI_DISCOVERY_SHIP_LEVEL=50
//!   KOBAYASHI_DISCOVERY_HOSTILE=447012258
//!   KOBAYASHI_DISCOVERY_PROFILE=demo
//!   KOBAYASHI_DISCOVERY_SCOUT_SIMS=800
//!   KOBAYASHI_DISCOVERY_CONFIRM_SIMS=5000
//!   KOBAYASHI_DISCOVERY_MAX_CANDIDATES=2500
//!   KOBAYASHI_DISCOVERY_GENETIC=1

use std::collections::HashSet;
use std::time::Instant;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::heuristics::BelowDecksPoolMode;
use kobayashi::data::heuristics::{
    expand_crews, list_heuristics_seeds, load_seed_file, BelowDecksStrategy, DEFAULT_HEURISTICS_DIR,
};
use kobayashi::optimizer::crew_generator::{
    build_officer_pools_with_constraints_from_registry, resolve_below_decks_slots_for_ship,
    CrewCandidate, OfficerPools,
};
use kobayashi::optimizer::genetic::{run_genetic_optimizer_ranked_with_stats, GeneticConfig};
use kobayashi::optimizer::monte_carlo::DefenderOpponent;
use kobayashi::optimizer::ranking::RankedCrewResult;
use kobayashi::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizerStrategy,
};
use kobayashi::parallel::init_from_env;

fn env_str(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref() {
        Some("1") | Some("true") | Some("yes") => true,
        Some("0") | Some("false") | Some("no") => false,
        Some(_) => default,
        None => default,
    }
}

fn crew_label(r: &RankedCrewResult) -> String {
    format!(
        "captain={} bridge={:?} below={:?} ({} BD)",
        r.captain,
        r.bridge,
        r.below_decks,
        r.below_decks.len()
    )
}

fn is_winning(r: &RankedCrewResult) -> bool {
    r.win_rate > 0.0 || r.win_rate_ci_low > 0.0
}

fn print_winners(title: &str, ranked: &[RankedCrewResult], limit: usize, expected_bd: usize) {
    let winners: Vec<_> = ranked
        .iter()
        .filter(|r| is_winning(r))
        .take(limit)
        .collect();
    println!("\n{title} ({} with win_rate > 0):", winners.len());
    if winners.is_empty() {
        println!("  (none)");
        return;
    }
    for (i, r) in winners.iter().enumerate() {
        let bd_note = if r.below_decks.len() < expected_bd {
            format!(" ⚠ partial BD ({}/{expected_bd})", r.below_decks.len())
        } else {
            String::new()
        };
        println!(
            "  #{i} win_rate={:.4} ci=[{:.4},{:.4}] hull={:.4} r1_kill={:.4} {}{bd_note}",
            r.win_rate,
            r.win_rate_ci_low,
            r.win_rate_ci_high,
            r.avg_hull_remaining,
            r.r1_kill_rate,
            crew_label(r),
        );
    }
}

/// Fill empty below-decks slots from the combat BD pool (deterministic order).
fn pad_below_decks(crew: &mut CrewCandidate, pools: &OfficerPools, slots: usize) {
    let mut used: HashSet<String> = HashSet::new();
    used.insert(crew.captain.clone());
    for b in &crew.bridge {
        used.insert(b.clone());
    }
    for bd in &crew.below_decks {
        used.insert(bd.clone());
    }
    for id in &pools.below_decks {
        if crew.below_decks.len() >= slots {
            break;
        }
        if !used.contains(id) {
            crew.below_decks.push(id.clone());
            used.insert(id.clone());
        }
    }
    crew.below_decks.truncate(slots);
}

fn load_heuristic_warm_start(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
) -> Vec<CrewCandidate> {
    let canonical_names: Vec<String> = registry.officers().iter().map(|o| o.name.clone()).collect();
    let pools = build_officer_pools_with_constraints_from_registry(
        registry,
        BelowDecksPoolMode::Strict,
        below_decks_slots,
        profile_id,
        None,
    )
    .expect("officer pools");
    let seeds = list_heuristics_seeds(DEFAULT_HEURISTICS_DIR);
    let mut out = Vec::new();
    for name in seeds {
        let parsed = load_seed_file(&name, DEFAULT_HEURISTICS_DIR, Some(&canonical_names));
        let expanded = expand_crews(parsed, below_decks_slots, BelowDecksStrategy::Ordered);
        for c in expanded {
            let mut crew = CrewCandidate {
                captain: c.captain,
                bridge: c.bridge,
                below_decks: c.below_decks,
            };
            pad_below_decks(&mut crew, &pools, below_decks_slots);
            out.push(crew);
        }
    }
    out
}

fn dedupe_warm_start(crews: Vec<CrewCandidate>) -> Vec<CrewCandidate> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in crews {
        let key = format!(
            "{}|{}|{}|{}",
            c.captain,
            c.bridge.first().map(String::as_str).unwrap_or(""),
            c.bridge.get(1).map(String::as_str).unwrap_or(""),
            c.below_decks.join(",")
        );
        if seen.insert(key) {
            out.push(c);
        }
    }
    out
}

fn main() {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        let _ = std::env::set_current_dir(m);
    }
    std::env::set_var("KOBAYASHI_OFFICER_SOURCE", "lcars");
    init_from_env();

    let ship = env_str("KOBAYASHI_DISCOVERY_SHIP", "gorn_eviscerator");
    let ship_tier = env_u32("KOBAYASHI_DISCOVERY_SHIP_TIER", 10);
    let ship_level = env_u32("KOBAYASHI_DISCOVERY_SHIP_LEVEL", 50);
    let hostile = env_str("KOBAYASHI_DISCOVERY_HOSTILE", "447012258");
    let profile_id = env_str("KOBAYASHI_DISCOVERY_PROFILE", "demo");
    let scout_sims = env_usize("KOBAYASHI_DISCOVERY_SCOUT_SIMS", 800);
    let confirm_sims = env_usize("KOBAYASHI_DISCOVERY_CONFIRM_SIMS", 5000);
    let max_candidates = env_usize("KOBAYASHI_DISCOVERY_MAX_CANDIDATES", 2500);
    let run_genetic = env_bool("KOBAYASHI_DISCOVERY_GENETIC", true);
    let seed = env_u64("KOBAYASHI_DISCOVERY_SEED", 4242);

    let below_decks_slots =
        resolve_below_decks_slots_for_ship(&ship, Some(ship_tier), Some(ship_level), None);

    let registry = DataRegistry::load().expect("DataRegistry::load from repo root");
    let warm_start = dedupe_warm_start(load_heuristic_warm_start(
        &registry,
        Some(profile_id.as_str()),
        below_decks_slots,
    ));

    println!("── Discovery optimize ────────────────────────────────────");
    println!("ship            : {ship} T{ship_tier} L{ship_level}");
    println!("hostile         : {hostile}");
    println!("profile         : {profile_id}");
    println!("below_decks     : {below_decks_slots} slots (from ship schedule + level)");
    println!("warm_start crews: {}", warm_start.len());
    println!(
        "tiered          : scout={scout_sims} confirm={confirm_sims} max_candidates={max_candidates}"
    );

    let t_run = Instant::now();
    let mut first_win_secs: Option<f64> = None;
    let mut first_win_crew: Option<String> = None;
    let mut first_win_phase: Option<&'static str> = None;

    let scenario = OptimizationScenario {
        ship: &ship,
        hostile: &hostile,
        ship_tier: Some(ship_tier),
        ship_level: Some(ship_level),
        simulation_count: confirm_sims,
        seed,
        max_candidates: Some(max_candidates),
        strategy: OptimizerStrategy::Tiered,
        below_decks_pool_mode: kobayashi::data::heuristics::BelowDecksPoolMode::Strict,
        seed_population: Vec::new(),
        profile_id: Some(profile_id.as_str()),
        tiered_scout_sims: Some(scout_sims),
        tiered_top_k: Some(32),
        tiered_scout_uniform: false,
        tiered_confirm_budget_cap_mult: None,
        tiered_scout_priority_queue: true,
        tiered_pq_minimal_scout: Some(150),
        tiered_pq_selection_mult: Some(4),
        tiered_pq_abandon_margin: Some(0.05),
        exhaustive_scout_sims: None,
        exhaustive_scout_top_keep: None,
        analytical_prefilter_keep: None,
        prune_analytical_hull_fraction: None,
        prune_static_gate_max_fraction: None,
        below_decks_slots,
        constraints: None,
        support_buffs: Vec::new(),
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        player_defender_officer_crew: None,
        pvp: None,
        warm_start,
        prior_reference_crews: Vec::new(),
        optimize_cache_key: None,
        enable_learned_pair_prior: true,
        learned_officer_scores: None,
    };

    let outcome = optimize_scenario_with_progress_with_registry(
        &registry,
        &scenario,
        |tick| {
            if first_win_secs.is_none() {
                if let Some(ref partial) = tick.partial_top {
                    for r in partial {
                        if is_winning(r) {
                            first_win_secs = Some(t_run.elapsed().as_secs_f64());
                            first_win_crew = Some(crew_label(r));
                            first_win_phase = Some(tick.phase);
                            eprintln!(
                                "FIRST WIN @ {:.1}s phase={} win_rate={:.4} {}",
                                first_win_secs.unwrap(),
                                tick.phase,
                                r.win_rate,
                                crew_label(r)
                            );
                            break;
                        }
                    }
                }
            }
            true
        },
        || true,
    );

    let tiered_elapsed = t_run.elapsed().as_secs_f64();
    print_winners("Tiered top winners", &outcome.ranked, 15, below_decks_slots);

    if let Some((n, scout, top_k)) = outcome.tiered_resolved {
        println!(
            "\nTiered stats: candidates={n} scout_sims={scout} top_k={top_k} elapsed={tiered_elapsed:.1}s"
        );
    }

    let tiered_has_winner = outcome.ranked.iter().any(is_winning);
    let mut genetic_ranked = Vec::new();
    let genetic_elapsed;

    if !tiered_has_winner && run_genetic {
        eprintln!("\nNo tiered winners — running genetic fallback...");
        let t_ga = Instant::now();
        let ga_config = GeneticConfig {
            population_size: 160,
            generations: 400,
            sims_per_eval: 300,
            mutation_rate: 0.35,
            adaptive_mutation: true,
            roster_profile_id: Some(profile_id.clone()),
            ship_tier: Some(ship_tier),
            ship_level: Some(ship_level),
            below_decks_slots,
            seed_population: scenario.warm_start.clone(),
            ..GeneticConfig::default()
        };
        let (results, stats) = run_genetic_optimizer_ranked_with_stats(
            &ship,
            &hostile,
            &ga_config,
            seed.wrapping_add(99),
            ga_config.sims_per_eval,
            Some(registry.as_ref()),
            |_, _, _| true,
            || true,
        );
        genetic_elapsed = t_ga.elapsed().as_secs_f64();
        genetic_ranked = results;
        print_winners(
            "Genetic fallback winners",
            &genetic_ranked,
            15,
            below_decks_slots,
        );
        println!(
            "Genetic: gens={} unique_crews={} elapsed={genetic_elapsed:.1}s",
            stats.generations_completed, stats.unique_crews_evaluated
        );
        if first_win_secs.is_none() {
            for r in &genetic_ranked {
                if is_winning(r) {
                    first_win_secs = Some(t_run.elapsed().as_secs_f64());
                    first_win_crew = Some(crew_label(r));
                    first_win_phase = Some("genetic");
                    break;
                }
            }
        }
    }

    let total_elapsed = t_run.elapsed().as_secs_f64();
    println!("\n── Time-to-discovery ─────────────────────────────────────");
    match first_win_secs {
        Some(s) => println!(
            "First winning crew @ {s:.1}s (phase={})",
            first_win_phase.unwrap_or("?")
        ),
        None => println!("No winning crew found in this run."),
    }
    if let Some(ref c) = first_win_crew {
        println!("  {c}");
    }
    println!("Total wall time: {total_elapsed:.1}s");

    if let Some(b) = outcome.ranked.first().or(genetic_ranked.first()) {
        println!("\nBest crew overall (by rank):");
        println!(
            "  win_rate={:.4} ci=[{:.4},{:.4}] hull={:.4} {}",
            b.win_rate,
            b.win_rate_ci_low,
            b.win_rate_ci_high,
            b.avg_hull_remaining,
            crew_label(b)
        );
    }
}
