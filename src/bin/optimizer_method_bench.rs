//! Compare optimizer methods under reproducible scenario/budget settings.
//!
//! Usage:
//!   cargo run --release --bin optimizer_method_bench -- --case saladin_numeric
//!   cargo run --release --bin optimizer_method_bench -- --methods tiered,random_stratified

use std::collections::HashSet;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use kobayashi::combat::EnemyType;
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::heuristics::{
    expand_crews, list_heuristics_seeds, load_seed_file, BelowDecksPoolMode, BelowDecksStrategy,
    DEFAULT_HEURISTICS_DIR,
};
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::optimizer::crew_generator::{
    build_officer_pools_from_registry, build_officer_pools_with_constraints_from_registry,
    resolve_below_decks_slots_for_ship, CrewCandidate, OfficerPools, BRIDGE_SLOTS,
};
use kobayashi::optimizer::genetic::{run_genetic_optimizer_ranked_with_stats, GeneticConfig};
use kobayashi::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_parallel_with_registry, DefenderOpponent,
};
use kobayashi::optimizer::ranking::{rank_results, RankedCrewResult};
use kobayashi::optimizer::{
    enforce_candidate_optimization_eligibility_with_registry,
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizeCandidateFunnel,
    OptimizerStrategy,
};
use kobayashi::parallel::init_from_env;
use serde::Serialize;

#[derive(Debug, Parser)]
struct Args {
    /// Profile id used for roster/profile-specific pools.
    #[arg(long, default_value = DEMO_PROFILE_ID)]
    profile: String,
    /// Optional case label filter. Omit to run all built-in cases.
    #[arg(long)]
    case: Option<String>,
    /// Optional comma-separated seed panel. Omit to use each built-in case's default seed.
    #[arg(long, value_delimiter = ',')]
    seed_panel: Vec<u64>,
    /// Comma-separated methods: tiered,genetic,linear_eval,warm_start_tiered,random_stratified,all.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "tiered,genetic,linear_eval,warm_start_tiered,random_stratified"
    )]
    methods: Vec<String>,
    /// Full confirmation sims for MC-backed lanes.
    #[arg(long, default_value_t = 600)]
    sims: usize,
    /// Tiered scout sims per crew.
    #[arg(long, default_value_t = 80)]
    scout_sims: usize,
    /// Tiered top-K confirmations.
    #[arg(long, default_value_t = 12)]
    top_k: usize,
    /// Generated candidates for tiered/linear-eval lanes.
    #[arg(long, default_value_t = 128)]
    max_candidates: usize,
    /// Candidate count for the benchmark-only stratified random control.
    #[arg(long, default_value_t = 128)]
    random_candidates: usize,
    /// Monte Carlo sims per random-control candidate.
    #[arg(long, default_value_t = 600)]
    random_sims: usize,
    /// Cap heuristic warm-start crews used by the warm-start tiered lane.
    #[arg(long, default_value_t = 64)]
    warm_start_limit: usize,
    /// Genetic population size for this benchmark harness.
    #[arg(long, default_value_t = 64)]
    ga_population: usize,
    /// Genetic generations for this benchmark harness.
    #[arg(long, default_value_t = 20)]
    ga_generations: usize,
    /// Genetic sims per population evaluation.
    #[arg(long, default_value_t = 120)]
    ga_sims_per_eval: usize,
    /// Leading rows used for finalist diversity metrics.
    #[arg(long, default_value_t = 10)]
    diversity_top_k: usize,
    /// Pretty-print a JSON array instead of JSON lines.
    #[arg(long)]
    pretty: bool,
}

#[derive(Debug, Clone)]
struct BenchCase {
    label: &'static str,
    ship: &'static str,
    hostile: &'static str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    seed: u64,
    enemy_type: EnemyType,
}

const CASES: &[BenchCase] = &[
    BenchCase {
        label: "saladin_numeric",
        ship: "saladin",
        hostile: "2918121098",
        ship_tier: None,
        ship_level: None,
        seed: 7,
        enemy_type: EnemyType::RedMovingSpace,
    },
    BenchCase {
        label: "enterprise_d_numeric",
        ship: "uss_enterprise_d",
        hostile: "2918121098",
        ship_tier: None,
        ship_level: None,
        seed: 42,
        enemy_type: EnemyType::RedMovingSpace,
    },
];

#[derive(Debug, Clone, Serialize)]
struct BenchFunnel {
    generated_candidates: Option<usize>,
    warm_start_candidates: usize,
    after_warm_start_dedupe: Option<usize>,
    after_constraints: Option<usize>,
    analytical_prefilter_from: Option<usize>,
    analytical_prefilter_kept: Option<usize>,
    scout_candidates: Option<usize>,
    confirmed_candidates: Option<usize>,
}

impl From<OptimizeCandidateFunnel> for BenchFunnel {
    fn from(funnel: OptimizeCandidateFunnel) -> Self {
        Self {
            generated_candidates: funnel.generated_candidates,
            warm_start_candidates: funnel.warm_start_candidates,
            after_warm_start_dedupe: funnel.after_warm_start_dedupe,
            after_constraints: funnel.after_constraints,
            analytical_prefilter_from: funnel.analytical_prefilter_from,
            analytical_prefilter_kept: funnel.analytical_prefilter_kept,
            scout_candidates: funnel.scout_candidates,
            confirmed_candidates: funnel.confirmed_candidates,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct BenchRecord {
    schema_version: u8,
    ts_ms: u128,
    simulator_version: &'static str,
    case: String,
    method: String,
    profile: String,
    ship: String,
    hostile: String,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    below_decks_slots: usize,
    seed: u64,
    sims: usize,
    candidate_budget: usize,
    elapsed_ms: u128,
    ranked_count: usize,
    trials_run_total: usize,
    candidate_funnel: Option<BenchFunnel>,
    tiered_candidates: Option<usize>,
    tiered_scout_sims: Option<usize>,
    tiered_top_k: Option<usize>,
    tiered_scout_trials_final: Option<u64>,
    tiered_confirm_trials_total: Option<u64>,
    genetic_generations_completed: Option<usize>,
    genetic_unique_crews_evaluated: Option<usize>,
    diversity_top_k: usize,
    unique_captains_top_k: usize,
    unique_bridge_officers_top_k: usize,
    unique_below_decks_officers_top_k: usize,
    avg_pairwise_material_jaccard_distance_top_k: Option<f64>,
    best_win_rate: Option<f64>,
    best_win_rate_ci_low: Option<f64>,
    best_score: Option<f32>,
    best_crew_hash: Option<u64>,
    best_captain: Option<String>,
    best_bridge: Option<Vec<String>>,
    best_below_decks: Option<Vec<String>>,
    discovered_best_win_rate: Option<f64>,
    win_rate_regret: Option<f64>,
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn index(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() as usize) % n
        }
    }
}

fn method_enabled(args: &Args, method: &str) -> bool {
    args.methods
        .iter()
        .any(|m| m == "all" || m.eq_ignore_ascii_case(method))
}

fn scenario<'a>(
    case: &'a BenchCase,
    profile: Option<&'a str>,
    args: &Args,
    strategy: OptimizerStrategy,
    below_decks_slots: usize,
    warm_start: Vec<CrewCandidate>,
) -> OptimizationScenario<'a> {
    OptimizationScenario {
        ship: case.ship,
        hostile: case.hostile,
        ship_tier: case.ship_tier,
        ship_level: case.ship_level,
        simulation_count: args.sims,
        seed: case.seed,
        max_candidates: Some(args.max_candidates),
        strategy,
        below_decks_pool_mode: BelowDecksPoolMode::Strict,
        seed_population: warm_start.clone(),
        profile_id: profile,
        tiered_scout_sims: Some(args.scout_sims),
        tiered_top_k: Some(args.top_k),
        tiered_scout_uniform: false,
        tiered_confirm_budget_cap_mult: None,
        tiered_scout_priority_queue: false,
        tiered_pq_minimal_scout: None,
        tiered_pq_selection_mult: None,
        tiered_pq_abandon_margin: None,
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
        enemy_type: case.enemy_type,
        warm_start,
        prior_reference_crews: Vec::new(),
        optimize_cache_key: None,
        enable_learned_pair_prior: true,
        learned_officer_scores: None,
    }
}

fn choose_distinct(pool: &[String], used: &mut HashSet<String>, rng: &mut Lcg) -> Option<String> {
    for _ in 0..32 {
        let value = pool.get(rng.index(pool.len()))?;
        if used.insert(value.to_ascii_lowercase()) {
            return Some(value.clone());
        }
    }
    for value in pool {
        if used.insert(value.to_ascii_lowercase()) {
            return Some(value.clone());
        }
    }
    None
}

fn stratified_random_crews(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: Option<&str>,
    below_decks_slots: usize,
    count: usize,
) -> Vec<CrewCandidate> {
    let Some(pools) = build_officer_pools_from_registry(
        registry,
        BelowDecksPoolMode::Strict,
        Some(case.enemy_type),
        profile,
        below_decks_slots,
        None,
    ) else {
        return Vec::new();
    };
    let mut rng = Lcg::new(case.seed.wrapping_add(0x5151_5151));
    let mut out = Vec::with_capacity(count);
    let mut seen = HashSet::new();
    let max_attempts = count.saturating_mul(64).max(256);
    for attempt in 0..max_attempts {
        if out.len() >= count {
            break;
        }
        let Some(captain) = pools
            .captains
            .get((attempt + rng.index(pools.captains.len())) % pools.captains.len())
            .cloned()
        else {
            break;
        };
        let mut used = HashSet::new();
        used.insert(captain.to_ascii_lowercase());
        let mut bridge = Vec::with_capacity(BRIDGE_SLOTS);
        for _ in 0..BRIDGE_SLOTS {
            let Some(value) = choose_distinct(&pools.bridge, &mut used, &mut rng) else {
                bridge.clear();
                break;
            };
            bridge.push(value);
        }
        if bridge.len() != BRIDGE_SLOTS {
            continue;
        }
        let mut below_decks = Vec::with_capacity(below_decks_slots);
        for _ in 0..below_decks_slots {
            let Some(value) = choose_distinct(&pools.below_decks, &mut used, &mut rng) else {
                below_decks.clear();
                break;
            };
            below_decks.push(value);
        }
        if below_decks.len() != below_decks_slots {
            continue;
        }
        let crew = CrewCandidate {
            captain,
            bridge,
            below_decks,
        };
        if seen.insert(crew_candidate_stable_hash(&crew)) {
            out.push(crew);
        }
    }
    out
}

fn pad_below_decks(crew: &mut CrewCandidate, pools: &OfficerPools, slots: usize) {
    let mut used: HashSet<String> = HashSet::new();
    used.insert(crew.captain.to_ascii_lowercase());
    for b in &crew.bridge {
        used.insert(b.to_ascii_lowercase());
    }
    for bd in &crew.below_decks {
        used.insert(bd.to_ascii_lowercase());
    }
    for name in &pools.below_decks {
        if crew.below_decks.len() >= slots {
            break;
        }
        if used.insert(name.to_ascii_lowercase()) {
            crew.below_decks.push(name.clone());
        }
    }
    crew.below_decks.truncate(slots);
}

fn heuristic_warm_start(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: Option<&str>,
    below_decks_slots: usize,
    limit: usize,
) -> Vec<CrewCandidate> {
    if limit == 0 {
        return Vec::new();
    }
    let Some(pools) = build_officer_pools_with_constraints_from_registry(
        registry,
        BelowDecksPoolMode::Strict,
        false,
        below_decks_slots,
        profile,
        None,
    ) else {
        return Vec::new();
    };
    let canonical_names: Vec<String> = registry.officers().iter().map(|o| o.name.clone()).collect();
    let mut crews = Vec::new();
    for name in list_heuristics_seeds(DEFAULT_HEURISTICS_DIR) {
        let parsed = load_seed_file(&name, DEFAULT_HEURISTICS_DIR, Some(&canonical_names));
        let expanded = expand_crews(parsed, below_decks_slots, BelowDecksStrategy::Ordered);
        for c in expanded {
            let mut crew = CrewCandidate {
                captain: c.captain,
                bridge: c.bridge,
                below_decks: c.below_decks,
            };
            pad_below_decks(&mut crew, &pools, below_decks_slots);
            crews.push(crew);
        }
    }
    let (legal, _) = enforce_candidate_optimization_eligibility_with_registry(
        registry,
        profile,
        below_decks_slots,
        case.enemy_type,
        crews,
    );
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for crew in legal {
        if seen.insert(crew_candidate_stable_hash(&crew)) {
            out.push(crew);
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn summarize_record(
    case: &BenchCase,
    method: &str,
    profile: &str,
    below_decks_slots: usize,
    sims: usize,
    candidate_budget: usize,
    elapsed_ms: u128,
    ranked: &[RankedCrewResult],
    diversity_top_k: usize,
) -> BenchRecord {
    let best = ranked.first();
    let diversity = diversity_summary(ranked, diversity_top_k);
    BenchRecord {
        schema_version: 1,
        ts_ms: now_ms(),
        simulator_version: env!("CARGO_PKG_VERSION"),
        case: case.label.to_string(),
        method: method.to_string(),
        profile: profile.to_string(),
        ship: case.ship.to_string(),
        hostile: case.hostile.to_string(),
        ship_tier: case.ship_tier,
        ship_level: case.ship_level,
        below_decks_slots,
        seed: case.seed,
        sims,
        candidate_budget,
        elapsed_ms,
        ranked_count: ranked.len(),
        trials_run_total: ranked.iter().map(|r| r.trials_run).sum(),
        candidate_funnel: None,
        tiered_candidates: None,
        tiered_scout_sims: None,
        tiered_top_k: None,
        tiered_scout_trials_final: None,
        tiered_confirm_trials_total: None,
        genetic_generations_completed: None,
        genetic_unique_crews_evaluated: None,
        diversity_top_k: diversity.rows_considered,
        unique_captains_top_k: diversity.unique_captains,
        unique_bridge_officers_top_k: diversity.unique_bridge_officers,
        unique_below_decks_officers_top_k: diversity.unique_below_decks_officers,
        avg_pairwise_material_jaccard_distance_top_k: diversity.avg_pairwise_jaccard_distance,
        best_win_rate: best.map(|r| r.win_rate),
        best_win_rate_ci_low: best.map(|r| r.win_rate_ci_low),
        best_score: best.map(|r| r.score.value),
        best_crew_hash: best.map(|r| crew_candidate_stable_hash(&ranked_to_candidate(r))),
        best_captain: best.map(|r| r.captain.clone()),
        best_bridge: best.map(|r| r.bridge.clone()),
        best_below_decks: best.map(|r| r.below_decks.clone()),
        discovered_best_win_rate: None,
        win_rate_regret: None,
    }
}

struct DiversitySummary {
    rows_considered: usize,
    unique_captains: usize,
    unique_bridge_officers: usize,
    unique_below_decks_officers: usize,
    avg_pairwise_jaccard_distance: Option<f64>,
}

fn normalized_material(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn material_set(r: &RankedCrewResult) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(normalized_material(&r.captain));
    for value in r.bridge.iter().chain(r.below_decks.iter()) {
        set.insert(normalized_material(value));
    }
    set
}

fn diversity_summary(ranked: &[RankedCrewResult], top_k: usize) -> DiversitySummary {
    let rows: Vec<&RankedCrewResult> = ranked.iter().take(top_k).collect();
    let mut captains = HashSet::new();
    let mut bridge = HashSet::new();
    let mut below_decks = HashSet::new();
    for row in &rows {
        captains.insert(normalized_material(&row.captain));
        for value in &row.bridge {
            bridge.insert(normalized_material(value));
        }
        for value in &row.below_decks {
            below_decks.insert(normalized_material(value));
        }
    }
    let material_sets: Vec<HashSet<String>> = rows.iter().map(|row| material_set(row)).collect();
    let mut pair_count = 0usize;
    let mut distance_sum = 0.0;
    for i in 0..material_sets.len() {
        for j in (i + 1)..material_sets.len() {
            let a = &material_sets[i];
            let b = &material_sets[j];
            let intersection = a.intersection(b).count();
            let union = a.union(b).count();
            let similarity = if union == 0 {
                0.0
            } else {
                intersection as f64 / union as f64
            };
            distance_sum += 1.0 - similarity;
            pair_count += 1;
        }
    }
    DiversitySummary {
        rows_considered: rows.len(),
        unique_captains: captains.len(),
        unique_bridge_officers: bridge.len(),
        unique_below_decks_officers: below_decks.len(),
        avg_pairwise_jaccard_distance: (pair_count > 0).then_some(distance_sum / pair_count as f64),
    }
}

fn ranked_to_candidate(r: &RankedCrewResult) -> CrewCandidate {
    CrewCandidate {
        captain: r.captain.clone(),
        bridge: r.bridge.clone(),
        below_decks: r.below_decks.clone(),
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[allow(clippy::too_many_arguments)]
fn run_optimizer_lane(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: &str,
    args: &Args,
    method: &str,
    strategy: OptimizerStrategy,
    below_decks_slots: usize,
    warm_start: Vec<CrewCandidate>,
) -> BenchRecord {
    let scenario = scenario(
        case,
        Some(profile),
        args,
        strategy,
        below_decks_slots,
        warm_start,
    );
    let started = Instant::now();
    let outcome =
        optimize_scenario_with_progress_with_registry(registry, &scenario, |_| true, || true);
    let elapsed_ms = started.elapsed().as_millis();
    let mut record = summarize_record(
        case,
        method,
        profile,
        below_decks_slots,
        args.sims,
        args.max_candidates,
        elapsed_ms,
        &outcome.ranked,
        args.diversity_top_k,
    );
    record.candidate_funnel = Some(outcome.candidate_funnel.into());
    if let Some((n, scout, top_k)) = outcome.tiered_resolved {
        record.tiered_candidates = Some(n);
        record.tiered_scout_sims = Some(scout);
        record.tiered_top_k = Some(top_k);
    }
    if let Some(b) = outcome.tiered_scout_budget {
        record.tiered_scout_trials_final = Some(b.scout_trials_final);
        record.tiered_confirm_trials_total = Some(b.confirm_trials_total);
    }
    record
}

fn run_genetic_lane(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: &str,
    args: &Args,
    below_decks_slots: usize,
) -> BenchRecord {
    let config = GeneticConfig {
        population_size: args.ga_population,
        generations: args.ga_generations,
        sims_per_eval: args.ga_sims_per_eval,
        below_decks_pool_mode: BelowDecksPoolMode::Strict,
        below_decks_slots,
        roster_profile_id: Some(profile.to_string()),
        ship_tier: case.ship_tier,
        ship_level: case.ship_level,
        ..GeneticConfig::default()
    };
    let started = Instant::now();
    let (ranked, stats) = run_genetic_optimizer_ranked_with_stats(
        case.ship,
        case.hostile,
        &config,
        case.seed,
        args.sims,
        Some(registry),
        |_, _, _| true,
        || true,
    );
    let elapsed_ms = started.elapsed().as_millis();
    let mut record = summarize_record(
        case,
        "genetic",
        profile,
        below_decks_slots,
        args.sims,
        args.ga_population.saturating_mul(args.ga_generations),
        elapsed_ms,
        &ranked,
        args.diversity_top_k,
    );
    record.genetic_generations_completed = Some(stats.generations_completed);
    record.genetic_unique_crews_evaluated = Some(stats.unique_crews_evaluated);
    record
}

fn run_random_lane(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: &str,
    args: &Args,
    below_decks_slots: usize,
) -> BenchRecord {
    let candidates = stratified_random_crews(
        registry,
        case,
        Some(profile),
        below_decks_slots,
        args.random_candidates,
    );
    let started = Instant::now();
    let (results, _) = run_monte_carlo_parallel_with_registry(
        registry,
        case.ship,
        case.hostile,
        case.ship_tier,
        case.ship_level,
        &candidates,
        args.random_sims,
        case.seed,
        Some(profile),
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    let ranked = rank_results(results);
    summarize_record(
        case,
        "random_stratified",
        profile,
        below_decks_slots,
        args.random_sims,
        candidates.len(),
        started.elapsed().as_millis(),
        &ranked,
        args.diversity_top_k,
    )
}

fn annotate_case_regret(records: &mut [BenchRecord]) {
    let best = records
        .iter()
        .filter_map(|r| r.best_win_rate)
        .max_by(f64::total_cmp);
    if let Some(best) = best {
        for r in records {
            r.discovered_best_win_rate = Some(best);
            r.win_rate_regret = r.best_win_rate.map(|wr| (best - wr).max(0.0));
        }
    }
}

fn main() {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        let _ = std::env::set_current_dir(m);
    }
    init_from_env();
    let args = Args::parse();
    let registry = DataRegistry::load().expect("DataRegistry::load from repo root");
    let mut all_records = Vec::new();

    for case in CASES {
        if args
            .case
            .as_ref()
            .is_some_and(|wanted| wanted != case.label)
        {
            continue;
        }
        let seeds = if args.seed_panel.is_empty() {
            vec![case.seed]
        } else {
            args.seed_panel.clone()
        };
        for seed in seeds {
            let case = BenchCase {
                seed,
                ..case.clone()
            };
            run_case(&registry, &args, &case, &mut all_records);
        }
    }

    if args.pretty {
        println!(
            "{}",
            serde_json::to_string_pretty(&all_records).expect("serialize bench records")
        );
    } else {
        for record in all_records {
            println!(
                "{}",
                serde_json::to_string(&record).expect("serialize bench record")
            );
        }
    }
}

fn run_case(
    registry: &DataRegistry,
    args: &Args,
    case: &BenchCase,
    all_records: &mut Vec<BenchRecord>,
) {
    let profile = args.profile.as_str();
    let below_decks_slots =
        resolve_below_decks_slots_for_ship(case.ship, case.ship_tier, case.ship_level, None);
    let mut records = Vec::new();
    if method_enabled(args, "tiered") {
        records.push(run_optimizer_lane(
            registry,
            case,
            profile,
            args,
            "tiered",
            OptimizerStrategy::Tiered,
            below_decks_slots,
            Vec::new(),
        ));
    }
    if method_enabled(args, "genetic") {
        records.push(run_genetic_lane(
            registry,
            case,
            profile,
            args,
            below_decks_slots,
        ));
    }
    if method_enabled(args, "linear_eval") {
        records.push(run_optimizer_lane(
            registry,
            case,
            profile,
            args,
            "linear_eval",
            OptimizerStrategy::LinearEval,
            below_decks_slots,
            Vec::new(),
        ));
    }
    if method_enabled(args, "warm_start_tiered") {
        let warm_start = heuristic_warm_start(
            registry,
            case,
            Some(profile),
            below_decks_slots,
            args.warm_start_limit,
        );
        records.push(run_optimizer_lane(
            registry,
            case,
            profile,
            args,
            "warm_start_tiered",
            OptimizerStrategy::Tiered,
            below_decks_slots,
            warm_start,
        ));
    }
    if method_enabled(args, "random_stratified") {
        records.push(run_random_lane(
            registry,
            case,
            profile,
            args,
            below_decks_slots,
        ));
    }
    annotate_case_regret(&mut records);
    all_records.extend(records);
}
