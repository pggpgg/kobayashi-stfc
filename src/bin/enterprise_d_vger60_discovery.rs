//! Enterprise-D vs hostile 3931453197 — find winning crews and measure time-to-discovery.
//!
//! Usage (repo root):
//!   cargo run --release --bin enterprise_d_vger60_discovery
//!
//! Env:
//!   KOBAYASHI_OFFICER_SOURCE=lcars  (set automatically)
//!   KOBAYASHI_DISCOVERY_SCOUT_SIMS=800
//!   KOBAYASHI_DISCOVERY_CONFIRM_SIMS=5000
//!   KOBAYASHI_DISCOVERY_MAX_CANDIDATES=2500
//!   KOBAYASHI_DISCOVERY_GENETIC=1     — run GA fallback when tiered finds no wins

use std::time::Instant;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::heuristics::{
    expand_crews, list_heuristics_seeds, load_seed_file, BelowDecksStrategy, DEFAULT_HEURISTICS_DIR,
};
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::genetic::{run_genetic_optimizer_ranked_with_stats, GeneticConfig};
use kobayashi::optimizer::monte_carlo::DefenderOpponent;
use kobayashi::optimizer::ranking::RankedCrewResult;
use kobayashi::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizerStrategy,
};
use kobayashi::parallel::init_from_env;

const SHIP_ID: &str = "uss_enterprise_d";
const SHIP_TIER: u32 = 12;
const SHIP_LEVEL: u32 = 56;
const HOSTILE_ID: &str = "3931453197";
const SEED: u64 = 4242;

fn env_usize(name: &str, default: usize) -> usize {
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
        "captain={} bridge={:?} below={:?}",
        r.captain, r.bridge, r.below_decks
    )
}

fn is_winning(r: &RankedCrewResult) -> bool {
    r.win_rate > 0.0 || r.win_rate_ci_low > 0.0
}

fn print_winners(title: &str, ranked: &[RankedCrewResult], limit: usize) {
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
        println!(
            "  #{i} win_rate={:.4} ci=[{:.4},{:.4}] hull={:.4} r1_kill={:.4} {}",
            r.win_rate,
            r.win_rate_ci_low,
            r.win_rate_ci_high,
            r.avg_hull_remaining,
            r.r1_kill_rate,
            crew_label(r)
        );
    }
}

fn hurak_warm_start() -> CrewCandidate {
    CrewCandidate {
        captain: "annorax-830d35".to_string(),
        bridge: vec!["suder-d348a9".to_string(), "seska-848b5b".to_string()],
        below_decks: vec!["harry-kim-a79fdf".to_string()],
    }
}

fn load_heuristic_warm_start(registry: &DataRegistry) -> Vec<CrewCandidate> {
    let canonical_names: Vec<String> = registry.officers().iter().map(|o| o.name.clone()).collect();
    let seeds = list_heuristics_seeds(DEFAULT_HEURISTICS_DIR);
    let mut out = Vec::new();
    for name in seeds {
        let parsed = load_seed_file(&name, DEFAULT_HEURISTICS_DIR, Some(&canonical_names));
        let expanded = expand_crews(parsed, 1, BelowDecksStrategy::Ordered);
        for c in expanded {
            out.push(CrewCandidate {
                captain: c.captain,
                bridge: c.bridge,
                below_decks: c.below_decks,
            });
        }
    }
    out
}

fn dedupe_warm_start(crews: Vec<CrewCandidate>) -> Vec<CrewCandidate> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in crews {
        let key = format!(
            "{}|{}|{}|{}",
            c.captain,
            c.bridge.first().map(String::as_str).unwrap_or(""),
            c.bridge.get(1).map(String::as_str).unwrap_or(""),
            c.below_decks
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(",")
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

    let scout_sims = env_usize("KOBAYASHI_DISCOVERY_SCOUT_SIMS", 800);
    let confirm_sims = env_usize("KOBAYASHI_DISCOVERY_CONFIRM_SIMS", 5000);
    let max_candidates = env_usize("KOBAYASHI_DISCOVERY_MAX_CANDIDATES", 2500);
    let run_genetic = env_bool("KOBAYASHI_DISCOVERY_GENETIC", true);

    let registry = DataRegistry::load().expect("DataRegistry::load from repo root");
    let warm_start = dedupe_warm_start({
        let mut w = load_heuristic_warm_start(&registry);
        w.push(hurak_warm_start());
        w
    });

    println!("── Enterprise-D discovery run ─────────────────────────────");
    println!("ship            : {SHIP_ID} T{SHIP_TIER} L{SHIP_LEVEL}");
    println!("hostile         : {HOSTILE_ID} (level 60 V'Ger Honorguard Apex family)");
    println!("profile         : {DEMO_PROFILE_ID} (Quantum Slipstream T10 L49)");
    println!("warm_start crews: {}", warm_start.len());
    println!(
        "tiered          : scout={scout_sims} confirm={confirm_sims} max_candidates={max_candidates}"
    );

    let t_run = Instant::now();
    let mut first_win_secs: Option<f64> = None;
    let mut first_win_crew: Option<String> = None;
    let mut first_win_phase: Option<&'static str> = None;

    let scenario = OptimizationScenario {
        ship: SHIP_ID,
        hostile: HOSTILE_ID,
        ship_tier: Some(SHIP_TIER),
        ship_level: Some(SHIP_LEVEL),
        simulation_count: confirm_sims,
        seed: SEED,
        max_candidates: Some(max_candidates),
        strategy: OptimizerStrategy::Tiered,
        below_decks_pool_mode: kobayashi::data::heuristics::BelowDecksPoolMode::default(),
        seed_population: Vec::new(),
        profile_id: Some(DEMO_PROFILE_ID),
        tiered_scout_sims: Some(scout_sims),
        tiered_top_k: Some(32),
        tiered_scout_uniform: false,
        tiered_confirm_budget_cap_mult: None,
        tiered_scout_priority_queue: true,
        tiered_pq_minimal_scout: Some(150),
        tiered_pq_selection_mult: Some(4),
        tiered_pq_abandon_margin: Some(0.05),
        tiered_random_exploration_pct: None,
        exhaustive_scout_sims: None,
        exhaustive_scout_top_keep: None,
        analytical_prefilter_keep: None,
        prune_analytical_hull_fraction: None,
        prune_static_gate_max_fraction: None,
        below_decks_slots: 1,
        constraints: None,
        support_buffs: Vec::new(),
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        chain_grind: None,
        defender_opponent: DefenderOpponent::Hostile,
        player_defender_officer_crew: None,
        pvp: None,
        enemy_type: kobayashi::combat::EnemyType::RedMovingSpace,
        warm_start,
        prior_reference_crews: Vec::new(),
        optimize_cache_key: None,
        enable_learned_pair_prior: true,
        learned_officer_scores: None,

        local_refinement: None,
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
    print_winners("Tiered top winners", &outcome.ranked, 15);

    if let Some((n, scout, top_k)) = outcome.tiered_resolved {
        println!(
            "\nTiered stats: candidates={n} scout_sims={scout} top_k={top_k} elapsed={tiered_elapsed:.1}s"
        );
    }
    if let Some(b) = outcome.tiered_scout_budget {
        println!(
            "Scout trials: coarse={} refine={} final={} confirm={}",
            b.coarse_pass_trials,
            b.refine_pass_trials,
            b.scout_trials_final,
            b.confirm_trials_total
        );
    }

    let tiered_has_winner = outcome.ranked.iter().any(is_winning);
    let mut genetic_ranked = Vec::new();
    let mut genetic_elapsed = 0.0_f64;

    if !tiered_has_winner && run_genetic {
        eprintln!("\nNo tiered winners — running genetic fallback (~90s budget)...");
        let t_ga = Instant::now();
        let ga_config = GeneticConfig {
            population_size: 160,
            generations: 400,
            sims_per_eval: 300,
            mutation_rate: 0.35,
            adaptive_mutation: true,
            roster_profile_id: Some(DEMO_PROFILE_ID.to_string()),
            ship_tier: Some(SHIP_TIER),
            ship_level: Some(SHIP_LEVEL),
            seed_population: scenario.warm_start.clone(),
            ..GeneticConfig::default()
        };
        let (results, stats) = run_genetic_optimizer_ranked_with_stats(
            SHIP_ID,
            HOSTILE_ID,
            &ga_config,
            SEED.wrapping_add(99),
            ga_config.sims_per_eval,
            Some(registry.as_ref()),
            |_, _, _| true,
            || true,
        );
        genetic_elapsed = t_ga.elapsed().as_secs_f64();
        genetic_ranked = results;
        print_winners("Genetic fallback winners", &genetic_ranked, 15);
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
    println!("Total wall time: {total_elapsed:.1}s (tiered={tiered_elapsed:.1}s genetic={genetic_elapsed:.1}s)");

    let best = outcome.ranked.first().or(genetic_ranked.first());
    if let Some(b) = best {
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
