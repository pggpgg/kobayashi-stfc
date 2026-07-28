//! Compare optimizer methods under reproducible scenario/budget settings.
//!
//! Usage:
//!   cargo run --release --bin optimizer_method_bench -- --case saladin_corvus
//!   cargo run --release --bin optimizer_method_bench -- --methods tiered,random_stratified
//!   cargo run --release --bin optimizer_method_bench -- --budget-mode equal-trials --trial-budget 40000
//!   cargo run --release --bin optimizer_method_bench -- --reference-sweep --prefilter-keep 64,128
//!   cargo run --release --bin optimizer_method_bench -- --ship uss_saladin --hostile 1140710508
//!
//! Output is JSON lines (or one pretty array with `--pretty`), tagged by `record_kind`:
//! `lane`, `reference`, `prefilter_false_negatives`, `stability`.
//!
//! Lanes are only comparable under an equal-budget mode, and only when the budget's breadth fits
//! inside the case's candidate space — see [CREW_OPTIMIZATION_METHODS.md §16](../../docs/CREW_OPTIMIZATION_METHODS.md).
//! `cargo xtask optimizer-bench-check` runs a fixed configuration and gates it against a baseline.

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
    build_officer_pools_with_constraints_from_registry, resolve_below_decks_slots_for_ship,
    CrewCandidate, OfficerPools,
};
use kobayashi::optimizer::genetic::{run_genetic_optimizer_ranked_with_stats, GeneticConfig};
use kobayashi::optimizer::method_bench::{
    aggregate_stability, confirmation_seed, plan_flat_equal_trials, plan_genetic_equal_trials,
    plan_tiered_equal_trials, run_reference_sweep, score_against_reference,
    score_analytical_prefilter, trial_budget_for_wall_clock, trial_budget_from_two_probes,
    BudgetMode, PrefilterFalseNegativeScore, ReferenceCrew, ReferenceScore, ReferenceSweep,
    ReferenceSweepParams, StabilityAggregate, StabilitySample,
};
use kobayashi::optimizer::monte_carlo::{
    crew_candidate_stable_hash, run_monte_carlo_parallel_with_registry, DefenderOpponent,
};
use kobayashi::optimizer::random_stratified::{
    sample_stratified_random_crews, StratifiedSampleParams,
};
use kobayashi::optimizer::ranking::{rank_results, RankedCrewResult};
use kobayashi::optimizer::{
    enforce_candidate_optimization_eligibility_with_registry,
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizeCandidateFunnel,
    OptimizerStrategy,
};
use kobayashi::parallel::init_from_env;
use serde::Serialize;

/// Bumped when record fields change shape. v2 added budget-mode, reference, and stability records.
const BENCH_SCHEMA_VERSION: u8 = 2;

#[derive(Debug, Parser)]
struct Args {
    /// Profile id used for roster/profile-specific pools.
    #[arg(long, default_value = DEMO_PROFILE_ID)]
    profile: String,
    /// Optional case label filter. Omit to run all built-in cases.
    #[arg(long)]
    case: Option<String>,
    /// Ad-hoc case: ship id. Requires --hostile; replaces the built-in case list.
    #[arg(long)]
    ship: Option<String>,
    /// Ad-hoc case: hostile id. Requires --ship.
    #[arg(long)]
    hostile: Option<String>,
    /// Ad-hoc case: ship tier.
    #[arg(long)]
    ship_tier: Option<u32>,
    /// Ad-hoc case: ship level.
    #[arg(long)]
    ship_level: Option<u32>,
    /// Ad-hoc case: seed when --seed-panel is empty.
    #[arg(long, default_value_t = 7)]
    ad_hoc_seed: u64,
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
    /// How lane budgets are normalized. `native` keeps each lane's own knobs (not comparable).
    #[arg(long, value_enum, default_value_t = BudgetMode::Native)]
    budget_mode: BudgetMode,
    /// Monte Carlo trials each lane may spend under `--budget-mode equal-trials`.
    #[arg(long, default_value_t = 100_000)]
    trial_budget: u64,
    /// Wall-clock target per lane under `--budget-mode equal-wall-clock`.
    #[arg(long, default_value_t = 20_000)]
    wall_clock_ms: u64,
    /// Trial budget of the probe run that measures each lane's trial rate for wall-clock mode.
    #[arg(long, default_value_t = 20_000)]
    wall_clock_probe_trials: u64,
    /// Run a deep reference sweep per case/seed and score every lane's recall and regret against it.
    #[arg(long)]
    reference_sweep: bool,
    /// Monte Carlo trials per crew in the reference sweep.
    #[arg(long, default_value_t = 1_500)]
    reference_sims: usize,
    /// Tractability cap on crews the reference sweep evaluates.
    #[arg(long, default_value_t = 512)]
    reference_max_crews: usize,
    /// K for reference top-K recall and prefilter false-negative scoring.
    #[arg(long, default_value_t = 10)]
    recall_top_k: usize,
    /// Score analytical-prefilter false negatives at these keep values. Requires --reference-sweep.
    #[arg(long, value_delimiter = ',')]
    prefilter_keep: Vec<usize>,
    /// Pretty-print a JSON array instead of JSON lines.
    #[arg(long)]
    pretty: bool,
}

#[derive(Debug, Clone)]
struct BenchCase {
    label: String,
    ship: String,
    hostile: String,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    seed: u64,
    enemy_type: EnemyType,
}

fn built_in_case(
    label: &str,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    seed: u64,
) -> BenchCase {
    BenchCase {
        label: label.to_string(),
        ship: ship.to_string(),
        hostile: hostile.to_string(),
        ship_tier,
        ship_level,
        seed,
        enemy_type: EnemyType::RedMovingSpace,
    }
}

/// Cases to run: the ad-hoc case when `--ship`/`--hostile` are given, otherwise the built-ins.
fn cases(args: &Args) -> Vec<BenchCase> {
    match (args.ship.as_deref(), args.hostile.as_deref()) {
        (Some(ship), Some(hostile)) => vec![BenchCase {
            label: args
                .case
                .clone()
                .unwrap_or_else(|| format!("ad_hoc_{ship}_{hostile}")),
            ship: ship.to_string(),
            hostile: hostile.to_string(),
            ship_tier: args.ship_tier,
            ship_level: args.ship_level,
            seed: args.ad_hoc_seed,
            enemy_type: EnemyType::RedMovingSpace,
        }],
        (Some(_), None) | (None, Some(_)) => {
            eprintln!(
                "--ship and --hostile must be given together; falling back to built-in cases"
            );
            built_in_cases()
        }
        (None, None) => built_in_cases(),
    }
}

fn built_in_cases() -> Vec<BenchCase> {
    vec![
        built_in_case("saladin_corvus", "uss_saladin", "1140710508", None, None, 7),
        built_in_case(
            "enterprise_d_numeric",
            "uss_enterprise_d",
            "2918121098",
            None,
            None,
            42,
        ),
    ]
}

/// Refuse to benchmark a matchup whose ship or hostile does not resolve.
///
/// An unresolved id does not stop the run downstream — it produces a fight every crew wins in
/// round one, which looks like a benchmark result and measures nothing. The `saladin_numeric` case
/// this harness shipped with was exactly that: `saladin` is not a ship id (`uss_saladin` is).
fn validate_case(registry: &DataRegistry, case: &BenchCase) -> Result<(), String> {
    if registry.resolve_ship(&case.ship).is_none() {
        return Err(format!(
            "case {}: ship {:?} does not resolve — every fight would be a phantom the crew wins in round 1",
            case.label, case.ship
        ));
    }
    if registry.resolve_hostile(&case.hostile).is_none() {
        return Err(format!(
            "case {}: hostile {:?} does not resolve — every fight would be a phantom the crew wins in round 1",
            case.label, case.hostile
        ));
    }
    Ok(())
}

/// One comparable search method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Tiered,
    Genetic,
    LinearEval,
    WarmStartTiered,
    RandomStratified,
}

impl Lane {
    const ALL: [Lane; 5] = [
        Lane::Tiered,
        Lane::Genetic,
        Lane::LinearEval,
        Lane::WarmStartTiered,
        Lane::RandomStratified,
    ];

    fn label(self) -> &'static str {
        match self {
            Lane::Tiered => "tiered",
            Lane::Genetic => "genetic",
            Lane::LinearEval => "linear_eval",
            Lane::WarmStartTiered => "warm_start_tiered",
            Lane::RandomStratified => "random_stratified",
        }
    }

    /// False for lanes that never run a Monte Carlo trial, which a trial budget cannot size.
    fn uses_monte_carlo(self) -> bool {
        self != Lane::LinearEval
    }
}

/// Knobs a lane runs with. Each lane reads the fields that apply to it.
#[derive(Debug, Clone, Copy)]
struct LaneBudget {
    /// Candidate budget for the optimizer-backed lanes (`--max-candidates`).
    candidates: usize,
    /// Candidate budget for the stratified random control, which has its own knob.
    random_candidates: usize,
    scout_sims: usize,
    confirm_sims: usize,
    top_k: usize,
    ga_population: usize,
    ga_generations: usize,
    ga_sims_per_eval: usize,
    random_sims: usize,
}

impl LaneBudget {
    fn native(args: &Args) -> Self {
        Self {
            candidates: args.max_candidates,
            random_candidates: args.random_candidates,
            scout_sims: args.scout_sims,
            confirm_sims: args.sims,
            top_k: args.top_k,
            ga_population: args.ga_population,
            ga_generations: args.ga_generations,
            ga_sims_per_eval: args.ga_sims_per_eval,
            random_sims: args.random_sims,
        }
    }
}

/// How this lane's budget was chosen, carried into the record so a comparison can be audited.
#[derive(Debug, Clone, Serialize)]
struct BudgetPlan {
    mode: &'static str,
    /// Trial budget the lane was sized to, when the mode sets one.
    trial_budget: Option<u64>,
    /// Trials the plan expects to spend (an upper bound for the genetic lane).
    projected_trials: Option<u64>,
    /// False when the mode could not be applied to this lane; `note` says why.
    applied: bool,
    note: Option<String>,
    wall_clock_target_ms: Option<u64>,
    wall_clock_probe_ms: Option<u128>,
    wall_clock_probe_trials: Option<u64>,
}

impl BudgetPlan {
    fn native() -> Self {
        Self {
            mode: BudgetMode::Native.label(),
            trial_budget: None,
            projected_trials: None,
            applied: true,
            note: None,
            wall_clock_target_ms: None,
            wall_clock_probe_ms: None,
            wall_clock_probe_trials: None,
        }
    }
}

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
    /// Trials the lane actually spent, from lane-reported totals where they exist.
    realized_trials: u64,
    /// True when `realized_trials` is derived rather than counted (the GA has no trial counter).
    realized_trials_estimated: bool,
    budget: BudgetPlan,
    /// `realized_trials / projected_trials`. Well under 1.0 means the lane could not spend its
    /// budget — usually the candidate space is smaller than the budget's breadth, which makes an
    /// equal-budget comparison degenerate into "every lane searched everything".
    budget_utilization: Option<f64>,
    /// Realized wall clock minus the target, as a share of the target. Wall-clock mode only.
    wall_clock_error_pct: Option<f64>,
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
    /// Best win rate any lane in this case/seed found (cross-lane control, always present).
    discovered_best_win_rate: Option<f64>,
    win_rate_regret: Option<f64>,
    /// Recall and regret against the deep reference sweep, when `--reference-sweep` ran.
    reference: Option<ReferenceScore>,
}

/// A reference sweep, reported so recall numbers can be read with their coverage.
#[derive(Debug, Clone, Serialize)]
struct ReferenceRecord {
    schema_version: u8,
    ts_ms: u128,
    simulator_version: &'static str,
    case: String,
    profile: String,
    ship: String,
    hostile: String,
    seed: u64,
    sims_per_crew: usize,
    crews_generated: usize,
    crews_evaluated: usize,
    /// True when `--reference-max-crews` did not bind. Not a claim of exhaustiveness over the
    /// legal space: the generator narrows officer pools before enumerating.
    covers_generator_space: bool,
    elapsed_ms: u128,
    recall_top_k: usize,
    best_win_rate: Option<f64>,
    top_crews: Vec<ReferenceCrew>,
}

#[derive(Debug, Clone, Serialize)]
struct PrefilterRecord {
    schema_version: u8,
    ts_ms: u128,
    simulator_version: &'static str,
    case: String,
    profile: String,
    seed: u64,
    enable_learned_pair_prior: bool,
    #[serde(flatten)]
    score: PrefilterFalseNegativeScore,
}

#[derive(Debug, Clone, Serialize)]
struct StabilityRecord {
    schema_version: u8,
    ts_ms: u128,
    simulator_version: &'static str,
    profile: String,
    #[serde(flatten)]
    aggregate: StabilityAggregate,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "record_kind", rename_all = "snake_case")]
enum BenchOutput {
    Lane(Box<BenchRecord>),
    Reference(Box<ReferenceRecord>),
    PrefilterFalseNegatives(Box<PrefilterRecord>),
    Stability(Box<StabilityRecord>),
}

fn method_enabled(args: &Args, method: &str) -> bool {
    args.methods
        .iter()
        .any(|m| m == "all" || m.eq_ignore_ascii_case(method))
}

fn scenario<'a>(
    case: &'a BenchCase,
    profile: Option<&'a str>,
    budget: &LaneBudget,
    strategy: OptimizerStrategy,
    below_decks_slots: usize,
    warm_start: Vec<CrewCandidate>,
) -> OptimizationScenario<'a> {
    OptimizationScenario {
        ship: case.ship.as_str(),
        hostile: case.hostile.as_str(),
        ship_tier: case.ship_tier,
        ship_level: case.ship_level,
        simulation_count: budget.confirm_sims,
        seed: case.seed,
        max_candidates: Some(budget.candidates),
        strategy,
        below_decks_pool_mode: BelowDecksPoolMode::Strict,
        seed_population: warm_start.clone(),
        profile_id: profile,
        tiered_scout_sims: Some(budget.scout_sims),
        tiered_top_k: Some(budget.top_k),
        tiered_scout_uniform: false,
        tiered_confirm_budget_cap_mult: None,
        optimize_history_confirm_cap_mult: None,
        tiered_scout_priority_queue: false,
        tiered_pq_minimal_scout: None,
        tiered_pq_selection_mult: None,
        tiered_pq_abandon_margin: None,
        tiered_random_exploration_pct: None,
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
        reuse_fingerprint: None,
        enable_learned_pair_prior: true,
        learned_officer_scores: None,

        local_refinement: None,
    }
}

/// Random-control candidates via the production stratified sampler
/// (`kobayashi::optimizer::random_stratified`), so the benchmark control and the
/// production `random_stratified` lane exercise identical sampling code.
fn stratified_random_crews(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: Option<&str>,
    below_decks_slots: usize,
    count: usize,
) -> Vec<CrewCandidate> {
    sample_stratified_random_crews(
        registry,
        &StratifiedSampleParams {
            count,
            seed: case.seed,
            below_decks_slots,
            below_decks_pool_mode: BelowDecksPoolMode::Strict,
            enemy_type: case.enemy_type,
            profile_id: profile,
            constraints: None,
            exclude_hashes: None,
        },
    )
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
    budget: BudgetPlan,
) -> BenchRecord {
    let best = ranked.first();
    let diversity = diversity_summary(ranked, diversity_top_k);
    let trials_run_total: usize = ranked.iter().map(|r| r.trials_run).sum();
    BenchRecord {
        schema_version: BENCH_SCHEMA_VERSION,
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
        trials_run_total,
        realized_trials: trials_run_total as u64,
        realized_trials_estimated: false,
        budget,
        budget_utilization: None,
        wall_clock_error_pct: None,
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
        reference: None,
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

/// A lane's record plus the ranked rows it produced. Recall scoring needs the rows, which the
/// record does not retain.
struct LaneRun {
    record: BenchRecord,
    ranked: Vec<RankedCrewResult>,
}

#[allow(clippy::too_many_arguments)]
fn run_optimizer_lane(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: &str,
    method: &str,
    strategy: OptimizerStrategy,
    below_decks_slots: usize,
    warm_start: Vec<CrewCandidate>,
    budget: &LaneBudget,
    plan: BudgetPlan,
    diversity_top_k: usize,
) -> LaneRun {
    let scenario = scenario(
        case,
        Some(profile),
        budget,
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
        budget.confirm_sims,
        budget.candidates,
        elapsed_ms,
        &outcome.ranked,
        diversity_top_k,
        plan,
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
        // Tiered counts every trial it spent, including crews that never reached the ranked rows.
        record.realized_trials = b.scout_trials_final.saturating_add(b.confirm_trials_total);
    }
    LaneRun {
        record,
        ranked: outcome.ranked,
    }
}

fn run_genetic_lane(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: &str,
    below_decks_slots: usize,
    budget: &LaneBudget,
    plan: BudgetPlan,
    diversity_top_k: usize,
) -> LaneRun {
    let config = GeneticConfig {
        population_size: budget.ga_population,
        generations: budget.ga_generations,
        sims_per_eval: budget.ga_sims_per_eval,
        below_decks_pool_mode: BelowDecksPoolMode::Strict,
        below_decks_slots,
        roster_profile_id: Some(profile.to_string()),
        ship_tier: case.ship_tier,
        ship_level: case.ship_level,
        ..GeneticConfig::default()
    };
    let started = Instant::now();
    let (ranked, stats) = run_genetic_optimizer_ranked_with_stats(
        case.ship.as_str(),
        case.hostile.as_str(),
        &config,
        case.seed,
        budget.confirm_sims,
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
        budget.confirm_sims,
        budget.ga_population.saturating_mul(budget.ga_generations),
        elapsed_ms,
        &ranked,
        diversity_top_k,
        plan,
    );
    record.genetic_generations_completed = Some(stats.generations_completed);
    record.genetic_unique_crews_evaluated = Some(stats.unique_crews_evaluated);
    // The GA keeps no trial counter, so cost is the scouting pass over distinct crews plus the
    // trials the ranked rows report from the final full-sim pass.
    record.realized_trials = (stats.unique_crews_evaluated as u64)
        .saturating_mul(budget.ga_sims_per_eval as u64)
        .saturating_add(record.trials_run_total as u64);
    record.realized_trials_estimated = true;
    LaneRun { record, ranked }
}

fn run_random_lane(
    registry: &DataRegistry,
    case: &BenchCase,
    profile: &str,
    below_decks_slots: usize,
    budget: &LaneBudget,
    plan: BudgetPlan,
    diversity_top_k: usize,
) -> LaneRun {
    let candidates = stratified_random_crews(
        registry,
        case,
        Some(profile),
        below_decks_slots,
        budget.random_candidates,
    );
    let started = Instant::now();
    let (results, _) = run_monte_carlo_parallel_with_registry(
        registry,
        case.ship.as_str(),
        case.hostile.as_str(),
        case.ship_tier,
        case.ship_level,
        &candidates,
        budget.random_sims,
        case.seed,
        Some(profile),
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    let ranked = rank_results(results);
    let record = summarize_record(
        case,
        "random_stratified",
        profile,
        below_decks_slots,
        budget.random_sims,
        candidates.len(),
        started.elapsed().as_millis(),
        &ranked,
        diversity_top_k,
        plan,
    );
    LaneRun { record, ranked }
}

/// Run one lane with an already-resolved budget.
fn run_lane(
    registry: &DataRegistry,
    case: &BenchCase,
    args: &Args,
    lane: Lane,
    below_decks_slots: usize,
    budget: &LaneBudget,
    plan: BudgetPlan,
) -> LaneRun {
    let profile = args.profile.as_str();
    match lane {
        Lane::Tiered => run_optimizer_lane(
            registry,
            case,
            profile,
            lane.label(),
            OptimizerStrategy::Tiered,
            below_decks_slots,
            Vec::new(),
            budget,
            plan,
            args.diversity_top_k,
        ),
        Lane::LinearEval => run_optimizer_lane(
            registry,
            case,
            profile,
            lane.label(),
            OptimizerStrategy::LinearEval,
            below_decks_slots,
            Vec::new(),
            budget,
            plan,
            args.diversity_top_k,
        ),
        Lane::WarmStartTiered => {
            let warm_start = heuristic_warm_start(
                registry,
                case,
                Some(profile),
                below_decks_slots,
                args.warm_start_limit,
            );
            run_optimizer_lane(
                registry,
                case,
                profile,
                lane.label(),
                OptimizerStrategy::Tiered,
                below_decks_slots,
                warm_start,
                budget,
                plan,
                args.diversity_top_k,
            )
        }
        Lane::Genetic => run_genetic_lane(
            registry,
            case,
            profile,
            below_decks_slots,
            budget,
            plan,
            args.diversity_top_k,
        ),
        Lane::RandomStratified => run_random_lane(
            registry,
            case,
            profile,
            below_decks_slots,
            budget,
            plan,
            args.diversity_top_k,
        ),
    }
}

/// Size a lane to `trial_budget`, holding per-crew depth fixed and buying breadth.
fn equal_trial_budget(args: &Args, lane: Lane, trial_budget: u64) -> (LaneBudget, BudgetPlan) {
    let mut budget = LaneBudget::native(args);
    let mut plan = BudgetPlan {
        mode: BudgetMode::EqualTrials.label(),
        trial_budget: Some(trial_budget),
        projected_trials: None,
        applied: true,
        note: None,
        wall_clock_target_ms: None,
        wall_clock_probe_ms: None,
        wall_clock_probe_trials: None,
    };
    match lane {
        Lane::Tiered | Lane::WarmStartTiered => {
            let t = plan_tiered_equal_trials(trial_budget, args.scout_sims, args.sims, args.top_k);
            budget.candidates = t.candidates;
            budget.scout_sims = t.scout_sims;
            budget.confirm_sims = t.confirm_sims;
            budget.top_k = t.top_k;
            plan.projected_trials = Some(t.projected_trials);
        }
        Lane::Genetic => {
            let g =
                plan_genetic_equal_trials(trial_budget, args.ga_generations, args.ga_sims_per_eval);
            budget.ga_population = g.population;
            budget.ga_generations = g.generations;
            budget.ga_sims_per_eval = g.sims_per_eval;
            plan.projected_trials = Some(g.projected_trials);
        }
        Lane::RandomStratified => {
            budget.random_candidates = plan_flat_equal_trials(trial_budget, args.random_sims);
            plan.projected_trials = Some(budget.random_candidates as u64 * args.random_sims as u64);
        }
        Lane::LinearEval => {
            plan.applied = false;
            plan.note = Some(
                "linear_eval runs no Monte Carlo trials; candidate count left at --max-candidates"
                    .to_string(),
            );
        }
    }
    (budget, plan)
}

/// Size a lane to a wall-clock target by measuring its trial rate on a short probe run.
///
/// The probe is a real run of the same lane at a small trial budget, so the rate reflects that
/// lane's own per-trial cost rather than a shared assumption. When the probe cannot produce a
/// usable rate the lane falls back to its native knobs and says so.
fn wall_clock_budget(
    registry: &DataRegistry,
    case: &BenchCase,
    args: &Args,
    lane: Lane,
    below_decks_slots: usize,
) -> (LaneBudget, BudgetPlan) {
    let fallback = |note: String| {
        let mut plan = BudgetPlan::native();
        plan.mode = BudgetMode::EqualWallClock.label();
        plan.applied = false;
        plan.wall_clock_target_ms = Some(args.wall_clock_ms);
        plan.note = Some(note);
        (LaneBudget::native(args), plan)
    };
    if !lane.uses_monte_carlo() {
        return fallback(
            "linear_eval runs no Monte Carlo trials; no trial rate to size a wall-clock budget"
                .to_string(),
        );
    }
    // Two probe sizes, so the fit can separate the lane's fixed setup from its per-trial cost.
    let probe = |trials: u64| {
        let (budget, plan) = equal_trial_budget(args, lane, trials);
        run_lane(registry, case, args, lane, below_decks_slots, &budget, plan).record
    };
    let small = probe(args.wall_clock_probe_trials);
    let large = probe(args.wall_clock_probe_trials.saturating_mul(2));
    let target_ms = args.wall_clock_ms as u128;
    let fitted = trial_budget_from_two_probes(
        (small.realized_trials, small.elapsed_ms),
        (large.realized_trials, large.elapsed_ms),
        target_ms,
    );
    // Fall back to the single-point rate when the two points do not support a fit; it undershoots
    // by whatever the lane's setup costs, which is still better than not applying the mode.
    let single = || trial_budget_for_wall_clock(large.realized_trials, large.elapsed_ms, target_ms);
    let Some(trial_budget) = fitted.or_else(single) else {
        return fallback(format!(
            "probes measured {} trials in {} ms and {} in {} ms — too little to derive a rate",
            small.realized_trials, small.elapsed_ms, large.realized_trials, large.elapsed_ms
        ));
    };
    let probe = large;
    let (budget, mut plan) = equal_trial_budget(args, lane, trial_budget);
    plan.mode = BudgetMode::EqualWallClock.label();
    plan.wall_clock_target_ms = Some(args.wall_clock_ms);
    plan.wall_clock_probe_ms = Some(probe.elapsed_ms);
    plan.wall_clock_probe_trials = Some(probe.realized_trials);
    let mut notes: Vec<&str> = Vec::new();
    if fitted.is_none() {
        notes.push("two-point fit failed; single-point rate undershoots by the lane's setup cost");
    }
    if probe.realized_trials_estimated {
        notes.push("probe trial count is estimated, so the derived budget is too");
    }
    if !notes.is_empty() {
        plan.note = Some(notes.join("; "));
    }
    (budget, plan)
}

fn resolve_budget(
    registry: &DataRegistry,
    case: &BenchCase,
    args: &Args,
    lane: Lane,
    below_decks_slots: usize,
) -> (LaneBudget, BudgetPlan) {
    match args.budget_mode {
        BudgetMode::Native => (LaneBudget::native(args), BudgetPlan::native()),
        BudgetMode::EqualTrials => equal_trial_budget(args, lane, args.trial_budget),
        BudgetMode::EqualWallClock => {
            wall_clock_budget(registry, case, args, lane, below_decks_slots)
        }
    }
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
    let mut output: Vec<BenchOutput> = Vec::new();
    let mut samples: Vec<StabilitySample> = Vec::new();

    for case in cases(&args) {
        if args
            .case
            .as_ref()
            .is_some_and(|wanted| *wanted != case.label)
        {
            continue;
        }
        if let Err(err) = validate_case(&registry, &case) {
            eprintln!("{err}");
            std::process::exit(2);
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
            run_case(&registry, &args, &case, &mut output, &mut samples);
        }
    }

    for aggregate in aggregate_stability(&samples) {
        output.push(BenchOutput::Stability(Box::new(StabilityRecord {
            schema_version: BENCH_SCHEMA_VERSION,
            ts_ms: now_ms(),
            simulator_version: env!("CARGO_PKG_VERSION"),
            profile: args.profile.clone(),
            aggregate,
        })));
    }

    if args.pretty {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).expect("serialize bench records")
        );
    } else {
        for record in output {
            println!(
                "{}",
                serde_json::to_string(&record).expect("serialize bench record")
            );
        }
    }
}

fn reference_params<'a>(
    case: &'a BenchCase,
    args: &'a Args,
    below_decks_slots: usize,
) -> ReferenceSweepParams<'a> {
    ReferenceSweepParams {
        ship: case.ship.as_str(),
        hostile: case.hostile.as_str(),
        ship_tier: case.ship_tier,
        ship_level: case.ship_level,
        profile_id: Some(args.profile.as_str()),
        enemy_type: case.enemy_type,
        below_decks_slots,
        below_decks_pool_mode: BelowDecksPoolMode::Strict,
        seed: case.seed,
        sims_per_crew: args.reference_sims,
        max_crews: args.reference_max_crews,
    }
}

fn reference_record(case: &BenchCase, args: &Args, sweep: &ReferenceSweep) -> ReferenceRecord {
    ReferenceRecord {
        schema_version: BENCH_SCHEMA_VERSION,
        ts_ms: now_ms(),
        simulator_version: env!("CARGO_PKG_VERSION"),
        case: case.label.to_string(),
        profile: args.profile.clone(),
        ship: case.ship.to_string(),
        hostile: case.hostile.to_string(),
        seed: case.seed,
        sims_per_crew: sweep.sims_per_crew,
        crews_generated: sweep.crews_generated,
        crews_evaluated: sweep.crews_evaluated,
        covers_generator_space: sweep.covers_generator_space,
        elapsed_ms: sweep.elapsed_ms,
        recall_top_k: args.recall_top_k,
        best_win_rate: sweep.best_win_rate(),
        top_crews: sweep
            .ranked
            .iter()
            .take(args.recall_top_k)
            .cloned()
            .collect(),
    }
}

fn run_case(
    registry: &DataRegistry,
    args: &Args,
    case: &BenchCase,
    output: &mut Vec<BenchOutput>,
    samples: &mut Vec<StabilitySample>,
) {
    let below_decks_slots =
        resolve_below_decks_slots_for_ship(&case.ship, case.ship_tier, case.ship_level, None);

    let mut reference = args.reference_sweep.then(|| {
        let params = reference_params(case, args, below_decks_slots);
        let sweep = run_reference_sweep(registry, &params);
        (params, sweep)
    });
    if let Some((params, sweep)) = reference.as_ref() {
        output.push(BenchOutput::Reference(Box::new(reference_record(
            case, args, sweep,
        ))));
        for keep in &args.prefilter_keep {
            let score =
                score_analytical_prefilter(registry, params, sweep, *keep, args.recall_top_k, true);
            output.push(BenchOutput::PrefilterFalseNegatives(Box::new(
                PrefilterRecord {
                    schema_version: BENCH_SCHEMA_VERSION,
                    ts_ms: now_ms(),
                    simulator_version: env!("CARGO_PKG_VERSION"),
                    case: case.label.to_string(),
                    profile: args.profile.clone(),
                    seed: case.seed,
                    enable_learned_pair_prior: true,
                    score,
                },
            )));
        }
    }
    if !args.prefilter_keep.is_empty() && reference.is_none() {
        eprintln!("--prefilter-keep ignored: it scores against the reference sweep, which needs --reference-sweep");
    }

    let mut runs = Vec::new();
    for lane in Lane::ALL {
        if !method_enabled(args, lane.label()) {
            continue;
        }
        let (budget, plan) = resolve_budget(registry, case, args, lane, below_decks_slots);
        let target_ms = plan.wall_clock_target_ms;
        let run = run_lane(registry, case, args, lane, below_decks_slots, &budget, plan);
        let mut record = run.record;
        record.wall_clock_error_pct = target_ms
            .filter(|ms| *ms > 0)
            .map(|ms| (record.elapsed_ms as f64 - ms as f64) / ms as f64);
        record.budget_utilization = record
            .budget
            .projected_trials
            .filter(|p| *p > 0)
            .map(|p| record.realized_trials as f64 / p as f64);
        runs.push(LaneRun {
            record,
            ranked: run.ranked,
        });
    }

    // Confirm the reference's leading crews and every lane's winner together, on a seed none of
    // them was selected on. Regret measured on the selection seed rewards whichever search looked
    // at the most crews, because the luckiest draw wins.
    if let Some((params, sweep)) = reference.as_mut() {
        let winners: Vec<CrewCandidate> = runs
            .iter()
            .filter_map(|r| r.ranked.first().map(ranked_to_candidate))
            .collect();
        sweep.confirm(
            registry,
            params,
            confirmation_seed(case.seed),
            args.recall_top_k,
            &winners,
        );
        for run in &mut runs {
            run.record.reference = Some(score_against_reference(
                &run.ranked,
                sweep,
                args.recall_top_k,
            ));
        }
    }

    let mut records: Vec<BenchRecord> = runs.into_iter().map(|r| r.record).collect();
    annotate_case_regret(&mut records);
    for record in &records {
        samples.push(StabilitySample {
            case: record.case.clone(),
            method: record.method.clone(),
            seed: record.seed,
            best_win_rate: record.best_win_rate,
            best_crew_hash: record.best_crew_hash,
            top_k_recall: record.reference.as_ref().and_then(|r| r.top_k_recall),
            win_rate_regret: record
                .reference
                .as_ref()
                .and_then(|r| r.win_rate_regret_vs_reference)
                .or(record.win_rate_regret),
            score_regret: record
                .reference
                .as_ref()
                .and_then(|r| r.score_regret_vs_reference),
            elapsed_ms: record.elapsed_ms,
            trials_run_total: record.realized_trials,
        });
    }
    output.extend(records.into_iter().map(|r| BenchOutput::Lane(Box::new(r))));
}
