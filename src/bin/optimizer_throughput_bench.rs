//! Real-catalog optimizer workloads with complete result/ranking bit identities.
//! Run from an isolated public runtime with KOBAYASHI_HOME equal to CWD.
//! `--list`; `--parity [--case NAME] --threads 1`; `--case NAME --iterations 8 --threads 8`.
//! Run separate processes sequentially for 1/2/8 threads. Registry loading, validation,
//! warmup, hashing and canaries are untimed. Scenario/candidate compilation is timed.
//! Three same-mode preflight runs must agree exactly before any workload is timed.
//! The two `_empty_profile` MC cases require an explicit isolated empty profile;
//! they still resolve real catalog ship, hostile and complete LCARS officer definitions.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::import::load_imported_roster_ids_unlocked_only;
use kobayashi::data::optimize_fingerprint::{
    data_catalog_fingerprint, engine_canary_digest, engine_fingerprint, profile_inputs_fingerprint,
};
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::optimizer::crew_generator::{
    build_officer_pools_with_constraints_from_registry, CrewCandidate,
};
use kobayashi::optimizer::genetic::{
    run_genetic_optimizer_ranked_with_stats, GeneticConfig, GeneticRunStats,
};
use kobayashi::optimizer::monte_carlo::{
    crew_candidate_ordered_hash, run_monte_carlo_parallel_with_registry,
    run_monte_carlo_with_registry, unresolved_officers_for_candidate, DefenderOpponent,
    SimulationResult,
};
use kobayashi::optimizer::ranking::{rank_results, RankedCrewResult};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::hint::black_box;
use std::time::{Duration, Instant};

const SHIP: &str = "uss_enterprise_d";
const HOSTILE: &str = "563945657";
const PROFILE: &str = "demo";
const EMPTY_PROFILE: &str = "optimizer_bench_empty";
const EMPTY_PROFILE_JSON: &[u8] = br#"{"bonuses":{}}"#;
const SEED: u64 = 42;
const PREFLIGHT_RUNS: usize = 3;
const CASES: [&str; 6] = [
    "mc_compile_1",
    "mc_refine_64",
    "mc_compile_1_empty_profile",
    "mc_refine_64_empty_profile",
    "ga_compile_1",
    "ga_refine_256",
];
const BRIDGE_POOL: [&str; 8] = [
    "pike-1e7d0d",
    "moreau-23ddb4",
    "chen-cdb1ca",
    "t-laan-c4627b",
    "kirk-1323b6",
    "spock-c04738",
    "khan-3f1d1e",
    "jadzia-dax-736698",
];
const BELOW: [&str; 3] = [
    "harry-kim-a79fdf",
    "tom-paris-3640cc",
    "beverly-crusher-74b2d7",
];

#[derive(Default)]
struct Args {
    case: Option<String>,
    iterations: usize,
    threads: usize,
    parity: bool,
    list: bool,
}
fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut out = Args {
        iterations: 1,
        threads: 1,
        ..Args::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--list" => out.list = true,
            "--parity" => out.parity = true,
            "--case" => out.case = Some(args.next().ok_or("--case requires a name")?),
            "--iterations" => {
                out.iterations = args
                    .next()
                    .ok_or("--iterations requires a count")?
                    .parse()?
            }
            "--threads" => {
                out.threads = args.next().ok_or("--threads requires a count")?.parse()?
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    if !(1..=1000).contains(&out.iterations) || ![1, 2, 8].contains(&out.threads) {
        return Err("iterations must be 1..1000; threads must be 1, 2 or 8".into());
    }
    if out
        .case
        .as_deref()
        .is_some_and(|name| !CASES.contains(&name))
    {
        return Err("unknown case; use --list".into());
    }
    if !out.list && !out.parity && out.case.is_none() {
        return Err("timing requires --case; --parity may cover all cases".into());
    }
    Ok(out)
}

fn crew_json(c: &CrewCandidate) -> Value {
    json!({"captain": c.captain, "bridge": c.bridge, "below_decks": c.below_decks})
}
fn panel() -> Vec<CrewCandidate> {
    let mut out = Vec::new();
    for (captain, captain_id) in BRIDGE_POOL.iter().enumerate() {
        for offset in 1..BRIDGE_POOL.len() {
            let first = (captain + offset) % BRIDGE_POOL.len();
            let mut second = (first + 1) % BRIDGE_POOL.len();
            if second == captain {
                second = (second + 1) % BRIDGE_POOL.len();
            }
            out.push(CrewCandidate {
                captain: (*captain_id).into(),
                bridge: vec![BRIDGE_POOL[first].into(), BRIDGE_POOL[second].into()],
                below_decks: BELOW.iter().map(|id| (*id).into()).collect(),
            });
        }
    }
    out
}
fn ga_config(case: &str) -> GeneticConfig {
    GeneticConfig {
        population_size: 32,
        generations: if case == "ga_refine_256" { 2 } else { 8 },
        sims_per_eval: if case == "ga_refine_256" { 256 } else { 1 },
        mutation_rate: 0.4,
        adaptive_mutation: false,
        incremental_fitness: false,
        offspring_reduced_budget_mul: None,
        stagnation_limit: None,
        roster_profile_id: Some(PROFILE.into()),
        ship_tier: Some(12),
        ship_level: Some(60),
        ..GeneticConfig::default()
    }
}
fn final_trials(case: &str) -> usize {
    if case == "ga_refine_256" {
        256
    } else {
        8
    }
}
fn mc_trials(case: &str) -> usize {
    if matches!(case, "mc_refine_64" | "mc_refine_64_empty_profile") {
        64
    } else {
        1
    }
}

fn combat_profile_id(case: &str) -> &'static str {
    match case {
        "mc_compile_1_empty_profile" | "mc_refine_64_empty_profile" => EMPTY_PROFILE,
        _ => PROFILE,
    }
}

fn combat_profile_metadata(case: &str) -> Result<Value, Box<dyn Error>> {
    let id = combat_profile_id(case);
    if id != EMPTY_PROFILE {
        return Ok(
            json!({"kind": "bundled_demo", "api_profile_id": id, "effective_profile_id": id,
            "profile_inputs_fingerprint": format!("{:016x}", profile_inputs_fingerprint(Some(id))),
            "demo_roster_role": "ID validation and production imported roster in combat"}),
        );
    }

    // A missing profile or None would silently use default-profile behavior. Require
    // a real, explicit empty file and forbid every imported sidecar instead.
    let directory = std::path::Path::new("profiles").join(id);
    let entries = std::fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
    if entries.len() != 1
        || entries[0].file_name() != "profile.json"
        || !entries[0].file_type()?.is_file()
    {
        return Err(format!("{id} must contain only the explicit empty profile.json; imported sidecars are forbidden").into());
    }
    let bytes = std::fs::read(directory.join("profile.json"))?;
    if bytes != EMPTY_PROFILE_JSON {
        return Err(format!("{id}/profile.json must contain exactly the UTF-8 bytes {{\"bonuses\":{{}}}} without a BOM or newline").into());
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    Ok(
        json!({"kind": "empty_external_profile", "api_profile_id": id, "effective_profile_id": id,
        "profile_inputs_fingerprint": format!("{:016x}", profile_inputs_fingerprint(Some(id))),
        "profile_json_sha256": digest, "expected_profile_json_sha256": format!("{:x}", Sha256::digest(EMPTY_PROFILE_JSON)),
        "profile_json_semantics": {"bonuses": {}}, "imported_sidecars_present": false,
        "demo_roster_role": "ID validation only; not used by this combat workload",
        "officer_resolution": "Full catalog LCARS definitions with default rank/level resolution; no imported roster or officer stats",
        "scope": "Separate catalog workload with an explicitly empty external profile; not a complete real-player profile"}),
    )
}

fn run_mc(
    registry: &DataRegistry,
    crews: &[CrewCandidate],
    trials: usize,
    parallel: bool,
    profile_id: &str,
) -> Vec<SimulationResult> {
    let run = if parallel {
        run_monte_carlo_parallel_with_registry
    } else {
        run_monte_carlo_with_registry
    };
    let (rows, placeholder) = run(
        registry,
        SHIP,
        HOSTILE,
        Some(12),
        Some(60),
        crews,
        trials,
        SEED,
        Some(profile_id),
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert!(!placeholder, "synthetic combatants are forbidden");
    assert_eq!(rows.len(), crews.len());
    for (row, crew) in rows.iter().zip(crews) {
        assert_eq!(&row.candidate, crew, "candidate order changed");
        assert_eq!(
            row.trials_run, trials,
            "requested MC budget was not executed"
        );
    }
    rows
}
enum RawResult {
    MonteCarlo(Vec<SimulationResult>),
    Genetic(
        Vec<RankedCrewResult>,
        GeneticRunStats,
        Vec<(usize, usize, u32)>,
    ),
}
fn run_case(
    case: &str,
    registry: &DataRegistry,
    crews: &[CrewCandidate],
    parallel: bool,
) -> RawResult {
    if case.starts_with("mc_") {
        return RawResult::MonteCarlo(run_mc(
            registry,
            crews,
            mc_trials(case),
            parallel,
            combat_profile_id(case),
        ));
    }
    let config = ga_config(case);
    let mut progress = Vec::new();
    let (rows, stats) = run_genetic_optimizer_ranked_with_stats(
        SHIP,
        HOSTILE,
        &config,
        SEED,
        final_trials(case),
        Some(registry),
        |generation, total, fitness| {
            progress.push((generation, total, fitness.to_bits()));
            true
        },
        || true,
    );
    assert!(!rows.is_empty(), "GA produced no ranked crews");
    assert_eq!(stats.generations_completed, config.generations);
    assert!(stats.ran_final_full_mc);
    assert!(rows.iter().all(|row| row.trials_run == final_trials(case)));
    RawResult::Genetic(rows, stats, progress)
}

/// Sort object keys and replace each floating number by its exact binary64 bits.
/// Serialized f32 scores widen losslessly. Array and ranking order are retained.
fn exact_json(value: Value) -> Value {
    match value {
        Value::Number(n) if n.is_f64() => {
            json!({"f64_bits": format!("{:016x}", n.as_f64().unwrap().to_bits())})
        }
        Value::Array(rows) => Value::Array(rows.into_iter().map(exact_json).collect()),
        Value::Object(map) => {
            let sorted: BTreeMap<_, _> = map.into_iter().map(|(k, v)| (k, exact_json(v))).collect();
            serde_json::to_value(sorted).unwrap()
        }
        other => other,
    }
}
fn ranked_json(rows: &[RankedCrewResult]) -> Value {
    for r in rows {
        assert!(r.chain.is_none() && r.expected_hull_damage.is_none() && r.score.value.is_finite());
        assert!(
            [
                r.win_rate,
                r.win_rate_ci_low,
                r.win_rate_ci_high,
                r.stall_rate,
                r.stall_rate_ci_low,
                r.stall_rate_ci_high,
                r.loss_rate,
                r.loss_rate_ci_low,
                r.loss_rate_ci_high,
                r.r1_kill_rate,
                r.r1_kill_rate_ci_low,
                r.r1_kill_rate_ci_high,
                r.avg_hull_remaining,
                r.avg_hull_remaining_ci_low,
                r.avg_hull_remaining_ci_high,
                r.avg_defender_hull_remaining,
                r.avg_defender_hull_remaining_ci_low,
                r.avg_defender_hull_remaining_ci_high,
            ]
            .iter()
            .all(|v| v.is_finite()),
            "non-finite result cannot be hashed losslessly"
        );
    }
    exact_json(serde_json::to_value(rows).unwrap())
}
fn result_identity(raw: RawResult) -> Value {
    match raw {
        RawResult::MonteCarlo(rows) => {
            let actual_trials: usize = rows.iter().map(|r| r.trials_run).sum();
            let ordered: Vec<_> = rows
                .iter()
                .cloned()
                .map(|r| rank_results(vec![r]).remove(0))
                .collect();
            json!({"ordered_results": ranked_json(&ordered), "ranked_results": ranked_json(&rank_results(rows)), "actual_distinct_candidates": ordered.len(), "actual_simulations": actual_trials})
        }
        RawResult::Genetic(rows, stats, progress) => {
            let mut seen = HashSet::new();
            let actual_final_trials: usize = rows
                .iter()
                .filter(|r| {
                    seen.insert(crew_candidate_ordered_hash(&CrewCandidate {
                        captain: r.captain.clone(),
                        bridge: r.bridge.clone(),
                        below_decks: r.below_decks.clone(),
                    }))
                })
                .map(|r| r.trials_run)
                .sum();
            json!({"ranked_results": ranked_json(&rows), "unique_crew_compositions_visited": stats.unique_crews_evaluated,
                "generations_completed": stats.generations_completed, "ran_final_full_mc": stats.ran_final_full_mc,
                "actual_final_simulations": actual_final_trials, "total_generation_simulations": null,
                "intermediate_crew_identity_exposed": false, "generation_best_fitness_f32_bits": progress,
                "count_limit": "Public GA stats expose unique compositions, not per-generation deduplicated evaluations; no total sims/sec claim."})
        }
    }
}
fn sha(value: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}

/// Bound structural diagnostics: a missing array/object must not dump the full result.
fn small_value(value: &Value) -> Value {
    match value {
        Value::Array(rows) => json!({"type": "array", "length": rows.len()}),
        Value::Object(fields) => json!({"type": "object", "length": fields.len()}),
        Value::String(text) if text.chars().count() > 128 => {
            json!({"type": "string", "prefix": text.chars().take(128).collect::<String>(), "characters": text.chars().count()})
        }
        other => other.clone(),
    }
}

fn first_difference(expected: &Value, actual: &Value, path: &str) -> Option<Value> {
    if expected == actual {
        return None;
    }
    match (expected, actual) {
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (a, b)) in left.iter().zip(right).enumerate() {
                if let Some(diff) = first_difference(a, b, &format!("{path}/{index}")) {
                    return Some(diff);
                }
            }
        }
        (Value::Object(left), Value::Object(right)) => {
            // Exact identities already sort object keys. JSON Pointer escapes keep paths unambiguous.
            for (key, a) in left {
                let pointer_key = key.replace('~', "~0").replace('/', "~1");
                let child = format!("{path}/{pointer_key}");
                let Some(b) = right.get(key) else {
                    return Some(json!({"path": child, "reason": "missing_actual_key"}));
                };
                if let Some(diff) = first_difference(a, b, &child) {
                    return Some(diff);
                }
            }
            if let Some(key) = right.keys().find(|key| !left.contains_key(*key)) {
                let pointer_key = key.replace('~', "~0").replace('/', "~1");
                return Some(
                    json!({"path": format!("{path}/{pointer_key}"), "reason": "unexpected_actual_key"}),
                );
            }
        }
        _ => {}
    }
    Some(json!({"path": path, "expected": small_value(expected), "actual": small_value(actual)}))
}

fn identity_comparison(expected: &Value, actual: &Value) -> Value {
    json!({"identical": expected == actual, "expected_sha256": sha(expected),
        "actual_sha256": sha(actual), "first_difference": first_difference(expected, actual, "")})
}

fn require_identity(
    case: &str,
    workload: &Value,
    stage: &str,
    repetition: usize,
    expected: &Value,
    actual: &Value,
) -> Result<(), Box<dyn Error>> {
    if expected != actual {
        eprintln!(
            "{}",
            json!({"schema": "optimizer-throughput-failure-v2", "case": case,
            "stage": stage, "repetition": repetition, "workload_sha256": sha(workload),
            "timing_valid": false, "comparison": identity_comparison(expected, actual)})
        );
        return Err(format!(
            "{case}: exact result identity failed during {stage}; no valid timing reported"
        )
        .into());
    }
    Ok(())
}

fn metadata(registry: &DataRegistry, crews: &[CrewCandidate]) -> Result<Value, Box<dyn Error>> {
    let mut demo_files = BTreeMap::new();
    for entry in std::fs::read_dir("profiles/demo")? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            demo_files.insert(
                entry.file_name().to_string_lossy().into_owned(),
                format!("{:x}", Sha256::digest(std::fs::read(entry.path())?)),
            );
        }
    }
    let demo_files = serde_json::to_value(demo_files)?;
    let ship = registry
        .resolve_ship_with_tier_level(SHIP, Some(12), Some(60))
        .ok_or("ship is missing")?;
    let hostile = registry
        .resolve_hostile(HOSTILE)
        .ok_or("hostile is missing")?;
    assert!(ship.hull_health > 0.0 && hostile.hull_health > 0.0);
    let lcars = registry
        .lcars_officers()
        .ok_or("LCARS catalog failed to load")?;
    let by_name: HashMap<_, _> = lcars
        .iter()
        .map(|o| (o.name.as_str(), o.id.as_str()))
        .collect();
    let ids: HashSet<_> = lcars.iter().map(|o| o.id.as_str()).collect();
    let roster = load_imported_roster_ids_unlocked_only("profiles/demo/roster.imported.json")
        .ok_or("demo roster is missing or invalid")?;
    for id in BRIDGE_POOL.iter().chain(&BELOW) {
        assert!(
            ids.contains(id) && roster.contains(*id),
            "panel officer missing/locked: {id}"
        );
    }
    for crew in crews {
        assert!(unresolved_officers_for_candidate(registry, crew).is_empty());
        let all: HashSet<_> = std::iter::once(&crew.captain)
            .chain(&crew.bridge)
            .chain(&crew.below_decks)
            .collect();
        assert_eq!(all.len(), 6, "duplicate occupied officer");
    }
    assert_eq!(
        crews
            .iter()
            .map(crew_candidate_ordered_hash)
            .collect::<HashSet<_>>()
            .len(),
        crews.len()
    );
    let config = ga_config("ga_compile_1");
    let pools = build_officer_pools_with_constraints_from_registry(
        registry,
        config.below_decks_pool_mode,
        config.pvp_mode,
        config.below_decks_slots,
        Some(PROFILE),
        None,
    )
    .ok_or("real GA pools could not be built")?;
    let canonical = |names: &[String]| {
        names
            .iter()
            .map(|name| {
                let id = *by_name
                    .get(name.as_str())
                    .expect("GA pool officer lacks LCARS ID");
                assert!(
                    roster.contains(id),
                    "GA pool includes locked/unknown officer"
                );
                json!({"name": name, "id": id})
            })
            .collect::<Vec<_>>()
    };
    Ok(
        json!({"schema": "optimizer-throughput-v3", "ship": SHIP, "ship_tier": 12, "ship_level": 60,
        "hostile": HOSTILE, "demo_roster_validation_profile": PROFILE, "scenario_seed": SEED.to_string(),
        "combat_profile_selection": "Explicit per-case combat_profile metadata; only the two named MC empty-profile cases omit demo combat inputs. GA always uses demo.",
        "registry_load_timed": false, "scenario_compilation_timed": true, "candidate_compilation_timed": true,
        "same_mode_preflight_runs": PREFLIGHT_RUNS, "timed_api_mode": "parallel_public_api",
        "sequential_parallel_comparison": "Diagnostic only; modes compile independent scenario inputs. Exact same-mode repeat identity gates timing, and matching result/workload hashes across revisions remain required.",
        "seed_policy": "Production candidate-ordered hash + scenario seed; trial index added by MC. GA generation uses seed+generation.",
        "engine_canary_digest": engine_canary_digest(), "engine_fingerprint": engine_fingerprint(),
        "data_catalog_fingerprint": format!("{:016x}", data_catalog_fingerprint(registry)),
        "demo_profile_fingerprint": format!("{:016x}", profile_inputs_fingerprint(Some(PROFILE))),
        "demo_snapshot_sha256": sha(&demo_files), "demo_file_sha256": demo_files,
        "harness_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("optimizer_throughput_bench.rs"))),
        "mc_panel": crews.iter().map(crew_json).collect::<Vec<_>>(),
        "ga_pool": {"captains": canonical(&pools.captains), "bridge": canonical(&pools.bridge), "below_decks": canonical(&pools.below_decks)},
        "mc_early_stop": false, "genetic_early_stop": false,
        "normalization": "Sorted object keys; exact floating result binary64 bits (f32 scores widen losslessly); all result fields and array/ranking/crew order retained. Timing/thread metadata is outside identity."}),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    if args.list {
        println!(
            "{}",
            json!({"cases": CASES, "iterations_unit": "complete fixed-budget workloads", "threads": [1,2,8]})
        );
        return Ok(());
    }
    let cwd = std::env::current_dir()?.canonicalize()?;
    let home = std::path::PathBuf::from(
        std::env::var_os("KOBAYASHI_HOME")
            .ok_or("set KOBAYASHI_HOME to the isolated public runtime")?,
    )
    .canonicalize()?;
    if cwd != home || kobayashi::runtime_paths::asset_root().canonicalize()? != cwd {
        return Err("CWD, KOBAYASHI_HOME and resolved asset root must match".into());
    }
    if !cwd.join("profiles/demo/profile.json").is_file() {
        return Err("public demo profile is required".into());
    }
    // Process-local controls, applied before Rayon initialization.
    std::env::set_var("KOBAYASHI_RAYON_THREADS", args.threads.to_string());
    std::env::set_var("KOBAYASHI_FULLMC_EARLY_STOP", "0");
    std::env::set_var("KOBAYASHI_GENETIC_EARLY_STOP", "0");
    kobayashi::parallel::init_from_env();
    assert_eq!(rayon::current_num_threads(), args.threads);
    let registry = DataRegistry::load()?;
    let crews = panel();
    let common = metadata(&registry, &crews)?;
    let selected: Vec<_> = args
        .case
        .as_deref()
        .map_or_else(|| CASES.to_vec(), |case| vec![case]);
    for case in selected {
        let ga = ga_config(case);
        let workload = json!({"common": common, "case": case,
            "combat_profile": combat_profile_metadata(case)?,
            "mc_trials_per_candidate": case.starts_with("mc_").then(|| mc_trials(case)),
            "ga_config": case.starts_with("ga_").then(|| format!("{ga:?}")),
            "ga_final_trials": case.starts_with("ga_").then(|| final_trials(case))});
        let expected = result_identity(run_case(case, &registry, &crews, true));
        for repetition in 2..=PREFLIGHT_RUNS {
            let actual = result_identity(run_case(case, &registry, &crews, true));
            require_identity(
                case,
                &workload,
                "untimed_same_mode_preflight",
                repetition,
                &expected,
                &actual,
            )?;
        }
        // A comparison of independently compiled modes cannot attribute a difference to
        // parallel execution. Preserve it as evidence separately from same-mode stability.
        let sequential_parallel = case.starts_with("mc_").then(|| {
            identity_comparison(
                &expected,
                &result_identity(run_case(case, &registry, &crews, false)),
            )
        });
        let mut elapsed = Duration::ZERO;
        let repeats = if args.parity { 0 } else { args.iterations };
        for repetition in 1..=repeats {
            let start = Instant::now();
            let raw = black_box(run_case(case, &registry, &crews, true));
            elapsed += start.elapsed();
            require_identity(
                case,
                &workload,
                "measured_same_mode_repeat",
                repetition,
                &expected,
                &result_identity(raw),
            )?;
        }
        let mut output = json!({"case": case, "workload": workload, "workload_sha256": sha(&workload), "result": expected,
            "result_sha256": sha(&expected), "actual_rayon_threads": rayon::current_num_threads(),
            "same_mode_preflight_runs_verified": PREFLIGHT_RUNS, "measured_repeats_verified": repeats,
            "sequential_parallel_comparison": sequential_parallel});
        if !args.parity {
            output["elapsed_ns"] = json!(elapsed.as_nanos().to_string());
            output["ns_per_workload"] = json!(elapsed.as_secs_f64() * 1e9 / repeats as f64);
        }
        println!("{output}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_diagnostic_preserves_exact_float_bits_and_pointer_path() {
        let left = json!({"rows/one": [{"~hull": {"f64_bits": "3fe959a5f3b8cf2b"}}]});
        let right = json!({"rows/one": [{"~hull": {"f64_bits": "3fe959a5f3b8cf27"}}]});
        let comparison = identity_comparison(&left, &right);
        assert_eq!(comparison["identical"], false);
        assert_ne!(comparison["expected_sha256"], comparison["actual_sha256"]);
        assert_eq!(
            comparison["first_difference"]["path"],
            "/rows~1one/0/~0hull/f64_bits"
        );
        assert_eq!(comparison["first_difference"]["actual"], "3fe959a5f3b8cf27");
        assert_eq!(
            identity_comparison(&left, &left)["first_difference"],
            Value::Null
        );
    }

    #[test]
    fn structural_identity_failure_is_bounded_and_blocks_timing() {
        let left = json!({"rows": vec!["large result"; 1000]});
        let right = json!({"rows": []});
        let comparison = identity_comparison(&left, &right);
        assert!(serde_json::to_string(&comparison).unwrap().len() < 500);
        assert_eq!(comparison["first_difference"]["expected"]["length"], 1000);
        assert!(require_identity(
            "test",
            &json!({}),
            "untimed_same_mode_preflight",
            2,
            &left,
            &right
        )
        .is_err());
        assert!(require_identity(
            "test",
            &json!({}),
            "untimed_same_mode_preflight",
            2,
            &left,
            &left
        )
        .is_ok());
    }
}
