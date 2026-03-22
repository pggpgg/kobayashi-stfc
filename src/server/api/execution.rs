//! Execution layer: run optimize, job store, and response types.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::OwnedSemaphorePermit;

use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::{
    expand_crews, load_seed_file, BelowDecksStrategy, DEFAULT_HEURISTICS_DIR,
};
use crate::optimizer::crew_generator::{CrewCandidate, BELOW_DECKS_SLOTS};
use crate::optimizer::monte_carlo::{
    run_monte_carlo_parallel_with_registry,
    scenario::build_shared_scenario_data_from_registry,
    SimulationResult,
};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use crate::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizerStrategy,
};

use super::requests::{
    parse_below_decks_strategy, parse_strategy, OptimizePayloadError, OptimizeRequest,
    DEFAULT_SIMS,
};

#[derive(Debug, Clone, Serialize)]
pub struct CrewRecommendation {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub win_rate: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub avg_hull_remaining: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioSummary {
    pub ship: String,
    pub hostile: String,
    pub sims: u32,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeResponse {
    pub status: &'static str,
    pub engine: &'static str,
    pub scenario: ScenarioSummary,
    pub recommendations: Vec<CrewRecommendation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub notes: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Load heuristics seeds and expand them into CrewCandidates.
pub fn load_heuristics_candidates(
    registry: &DataRegistry,
    seed_names: &[String],
    bd_strategy: BelowDecksStrategy,
) -> Vec<CrewCandidate> {
    let canonical_names: Vec<String> = registry.officers().iter().map(|o| o.name.clone()).collect();
    seed_names
        .iter()
        .flat_map(|name| {
            let parsed = load_seed_file(name, DEFAULT_HEURISTICS_DIR, Some(&canonical_names));
            let candidates = expand_crews(parsed, BELOW_DECKS_SLOTS, bd_strategy);
            candidates.into_iter().map(|c| CrewCandidate {
                captain: c.captain,
                bridge: c.bridge,
                below_decks: c.below_decks,
            })
        })
        .collect()
}

/// Metadata from the shared optimize gather path (sync + async jobs).
#[derive(Clone, Copy)]
struct OptimizeGatherMeta {
    strategy: OptimizerStrategy,
    is_seeded_genetic: bool,
    heuristics_only: bool,
    heuristics_seeds_nonempty: bool,
    using_placeholder_combatants: bool,
}

/// Progress / cancellation hooks for optimize. Sync path uses [`OptimizeProgressSink::None`].
enum OptimizeProgressSink {
    None,
    Job {
        job_id: String,
        cancel: Arc<AtomicBool>,
        heuristics_seeds_nonempty: bool,
        /// Filled by [`gather_optimize_simulation_results`] once candidates are loaded.
        is_seeded_genetic: bool,
    },
}

impl OptimizeProgressSink {
    fn on_heuristics_start(&self, h_total: u32) {
        let Self::Job { job_id, .. } = self else {
            return;
        };
        if let Ok(mut map) = optimize_jobs().lock() {
            if let Some(state) = map.get_mut(job_id) {
                state.total_crews = h_total;
            }
        }
    }

    fn on_heuristics_complete(&self, heuristics_only: bool, h_total: u32) {
        let Self::Job { job_id, .. } = self else {
            return;
        };
        if let Ok(mut map) = optimize_jobs().lock() {
            if let Some(state) = map.get_mut(job_id) {
                state.crews_done = h_total;
                state.progress = if heuristics_only { 100 } else { 10 };
            }
        }
    }

    fn on_optimize_progress(&mut self, crews_done: u32, total_crews: u32) -> bool {
        match self {
            Self::None => true,
            Self::Job {
                job_id,
                cancel,
                heuristics_seeds_nonempty,
                is_seeded_genetic,
            } => {
                if cancel.load(Ordering::Relaxed) {
                    return false;
                }
                let base_progress = if *heuristics_seeds_nonempty && !*is_seeded_genetic {
                    10u8
                } else {
                    0u8
                };
                let progress = if total_crews == 0 {
                    base_progress
                } else {
                    let pct = (crews_done as f64 / total_crews as f64)
                        * (100.0 - base_progress as f64);
                    (base_progress as f64 + pct).round().min(100.0) as u8
                };
                if let Ok(mut map) = optimize_jobs().lock() {
                    if let Some(state) = map.get_mut(job_id) {
                        state.progress = progress;
                        state.crews_done = crews_done;
                        state.total_crews = total_crews;
                    }
                }
                true
            }
        }
    }

    fn job_cancelled(&self) -> bool {
        match self {
            Self::None => false,
            Self::Job { cancel, .. } => cancel.load(Ordering::Relaxed),
        }
    }
}

fn ranked_crew_to_simulation_result(r: RankedCrewResult) -> SimulationResult {
    SimulationResult {
        candidate: CrewCandidate {
            captain: r.captain,
            bridge: r.bridge,
            below_decks: r.below_decks,
        },
        win_rate: r.win_rate,
        stall_rate: r.stall_rate,
        loss_rate: r.loss_rate,
        avg_hull_remaining: r.avg_hull_remaining,
    }
}

/// Shared Monte Carlo + optimizer scenario execution. Sync and background jobs use the same logic.
fn gather_optimize_simulation_results(
    registry: &DataRegistry,
    request: &OptimizeRequest,
    profile_id: Option<&str>,
    sink: &mut OptimizeProgressSink,
) -> Result<(Vec<SimulationResult>, OptimizeGatherMeta), ()> {
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    let seed = request.seed.unwrap_or(0);
    let strategy = parse_strategy(request.strategy.as_ref());
    let heuristics_only = request.heuristics_only.unwrap_or(false);
    let bd_strategy = parse_below_decks_strategy(request.below_decks_strategy.as_ref());
    let heuristics_seeds = request.heuristics_seeds.as_deref().unwrap_or(&[]);
    let heuristics_seeds_nonempty = !heuristics_seeds.is_empty();

    let h_candidates = if heuristics_seeds_nonempty {
        load_heuristics_candidates(registry, heuristics_seeds, bd_strategy)
    } else {
        Vec::new()
    };
    let is_seeded_genetic =
        strategy == OptimizerStrategy::Genetic && !h_candidates.is_empty();

    if let OptimizeProgressSink::Job {
        is_seeded_genetic: sink_sg,
        ..
    } = sink
    {
        *sink_sg = is_seeded_genetic;
    }

    let using_placeholder_combatants = build_shared_scenario_data_from_registry(
        registry,
        &request.ship,
        &request.hostile,
        request.ship_tier,
        request.ship_level,
        profile_id,
    )
    .using_placeholder_combatants;

    let meta = OptimizeGatherMeta {
        strategy,
        is_seeded_genetic,
        heuristics_only,
        heuristics_seeds_nonempty,
        using_placeholder_combatants,
    };

    let mut all_results: Vec<SimulationResult> =
        if heuristics_seeds_nonempty && !is_seeded_genetic {
            let h_total = h_candidates.len() as u32;
            sink.on_heuristics_start(h_total);
            let (results, _) = run_monte_carlo_parallel_with_registry(
                registry,
                &request.ship,
                &request.hostile,
                request.ship_tier,
                request.ship_level,
                &h_candidates,
                sims as usize,
                seed,
                profile_id,
            );
            sink.on_heuristics_complete(heuristics_only, h_total);
            results
        } else {
            Vec::new()
        };

    if !heuristics_only {
        let scenario = OptimizationScenario {
            ship: &request.ship,
            hostile: &request.hostile,
            ship_tier: request.ship_tier,
            ship_level: request.ship_level,
            simulation_count: sims as usize,
            seed,
            max_candidates: request.max_candidates.map(|n| n as usize),
            strategy,
            only_below_decks_with_ability: request.prioritize_below_decks_ability.unwrap_or(false),
            seed_population: if is_seeded_genetic {
                h_candidates.clone()
            } else {
                Vec::new()
            },
            profile_id,
            tiered_scout_sims: None,
            tiered_top_k: None,
        };
        let normal_results = optimize_scenario_with_progress_with_registry(
            registry,
            &scenario,
            |crews_done, total_crews| sink.on_optimize_progress(crews_done, total_crews),
        );
        if sink.job_cancelled() {
            return Err(());
        }
        all_results.extend(
            normal_results
                .into_iter()
                .map(ranked_crew_to_simulation_result),
        );
    }

    Ok((all_results, meta))
}

fn build_optimize_response(
    request: &OptimizeRequest,
    all_results: Vec<SimulationResult>,
    duration_ms: u64,
    meta: &OptimizeGatherMeta,
) -> OptimizeResponse {
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    let seed = request.seed.unwrap_or(0);
    let ranked_results = rank_results(all_results);

    let engine = if meta.heuristics_only {
        "heuristics"
    } else if meta.is_seeded_genetic {
        "seeded_genetic"
    } else {
        match meta.strategy {
            OptimizerStrategy::Exhaustive => "optimizer_v1",
            OptimizerStrategy::Genetic => "genetic",
            OptimizerStrategy::Tiered => "tiered",
        }
    };
    let mut notes =
        vec!["Results are deterministic for the same ship, hostile, simulation count, and seed."];
    if meta.is_seeded_genetic {
        notes.insert(0, "GA population seeded with heuristics crews.");
    } else if meta.heuristics_seeds_nonempty {
        notes.insert(0, "Heuristics crews were evaluated first.");
    }

    let mut warnings = Vec::new();
    if meta.using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats. Results do not reflect real ship/hostile values."
                .to_string(),
        );
    }

    OptimizeResponse {
        status: "ok",
        engine,
        scenario: ScenarioSummary {
            ship: request.ship.clone(),
            hostile: request.hostile.clone(),
            sims,
            seed,
        },
        recommendations: ranked_results
            .into_iter()
            .map(|result| CrewRecommendation {
                captain: result.captain,
                bridge: result.bridge,
                below_decks: result.below_decks,
                win_rate: result.win_rate,
                stall_rate: result.stall_rate,
                loss_rate: result.loss_rate,
                avg_hull_remaining: result.avg_hull_remaining,
            })
            .collect(),
        duration_ms: Some(duration_ms),
        notes,
        warnings,
    }
}

/// Run optimization (assumes request already validated). Returns response or serialization error.
pub fn run_optimize(
    registry: &DataRegistry,
    request: &OptimizeRequest,
    profile_id: Option<&str>,
) -> Result<OptimizeResponse, OptimizePayloadError> {
    let start = Instant::now();
    let mut sink = OptimizeProgressSink::None;
    let (all_results, meta) =
        gather_optimize_simulation_results(registry, request, profile_id, &mut sink)
            .expect("sync optimize does not cancel");
    let duration_ms = start.elapsed().as_millis() as u64;
    Ok(build_optimize_response(request, all_results, duration_ms, &meta))
}

// --- Optimize job store (for progress polling) ---

#[derive(Debug, Clone)]
pub enum OptimizeJobStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct OptimizeJobState {
    pub status: OptimizeJobStatus,
    pub progress: u8,
    pub crews_done: u32,
    pub total_crews: u32,
    pub result: Option<OptimizeResponse>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeStartResponse {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeStatusResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crews_done: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_crews: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<OptimizeResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Cap on stored job records (running + finished). Oldest **completed** jobs are dropped first
/// when over limit so the global map cannot grow without bound.
const MAX_OPTIMIZE_JOBS_RETAINED: usize = 128;

static OPTIMIZE_JOB_COUNTER: OnceLock<AtomicU64> = OnceLock::new();
static OPTIMIZE_JOBS: OnceLock<Mutex<HashMap<String, OptimizeJobState>>> = OnceLock::new();
static OPTIMIZE_CANCEL_FLAGS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn optimize_jobs() -> &'static Mutex<HashMap<String, OptimizeJobState>> {
    OPTIMIZE_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn optimize_cancel_flags() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    OPTIMIZE_CANCEL_FLAGS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_job_id() -> String {
    let counter = OPTIMIZE_JOB_COUNTER.get_or_init(|| AtomicU64::new(0));
    let n = counter.fetch_add(1, Ordering::Relaxed);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("opt_{}_{}", ms, n)
}

/// Parse `opt_<millis>_<counter>` for eviction ordering (unknown shape → 0 = evicted first among ties).
fn parse_optimize_job_timestamp_ms(job_id: &str) -> u128 {
    job_id
        .strip_prefix("opt_")
        .and_then(|rest| rest.split('_').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Drop oldest finished jobs until `map.len() <= max_entries`. Running jobs are never removed.
fn prune_completed_optimize_jobs_over_cap(
    map: &mut HashMap<String, OptimizeJobState>,
    cancel_flags: &mut HashMap<String, Arc<AtomicBool>>,
    max_entries: usize,
) {
    while map.len() > max_entries {
        let Some(oldest_id) = map
            .iter()
            .filter(|(_, st)| {
                matches!(
                    st.status,
                    OptimizeJobStatus::Done | OptimizeJobStatus::Error
                )
            })
            .map(|(id, _)| (parse_optimize_job_timestamp_ms(id), id.clone()))
            .min_by(|(a_ts, a_id), (b_ts, b_id)| a_ts.cmp(b_ts).then_with(|| a_id.cmp(b_id)))
            .map(|(_, id)| id)
        else {
            break;
        };
        map.remove(&oldest_id);
        cancel_flags.remove(&oldest_id);
    }
}

#[derive(Debug)]
pub enum OptimizeStatusError {
    NotFound,
    Serialize(serde_json::Error),
}

impl std::fmt::Display for OptimizeStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Job not found"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OptimizeStatusError {}

/// Start an optimize job in the background; returns job_id. Caller must have validated request.
/// `cpu_permit` is held until the job thread finishes so `/api/optimize/start` shares the same
/// CPU concurrency budget as `/api/optimize` and `/api/simulate`.
pub fn start_optimize_job(
    registry: Arc<DataRegistry>,
    request: OptimizeRequest,
    profile_id: Option<&str>,
    cpu_permit: OwnedSemaphorePermit,
) -> Result<OptimizeStartResponse, OptimizePayloadError> {
    let job_id = next_job_id();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let heuristics_seeds_nonempty = request
        .heuristics_seeds
        .as_ref()
        .map_or(false, |s| !s.is_empty());

    {
        let mut map = optimize_jobs().lock().unwrap();
        map.insert(
            job_id.clone(),
            OptimizeJobState {
                status: OptimizeJobStatus::Running,
                progress: 0,
                crews_done: 0,
                total_crews: 0,
                result: None,
                error: None,
            },
        );
        let mut cancel_flags = optimize_cancel_flags().lock().unwrap();
        cancel_flags.insert(job_id.clone(), cancel_flag.clone());
        prune_completed_optimize_jobs_over_cap(
            &mut map,
            &mut cancel_flags,
            MAX_OPTIMIZE_JOBS_RETAINED,
        );
    }

    let job_id_thread = job_id.clone();
    let profile_owned = profile_id.map(String::from);

    std::thread::spawn(move || {
        let _cpu_permit = cpu_permit;
        let start = Instant::now();
        let mut sink = OptimizeProgressSink::Job {
            job_id: job_id_thread.clone(),
            cancel: cancel_flag.clone(),
            heuristics_seeds_nonempty,
            is_seeded_genetic: false,
        };
        let gather = gather_optimize_simulation_results(
            registry.as_ref(),
            &request,
            profile_owned.as_deref(),
            &mut sink,
        );

        match gather {
            Ok((all_results, meta)) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let response = build_optimize_response(&request, all_results, duration_ms, &meta);
                if let Ok(mut map) = optimize_jobs().lock() {
                    if let Some(state) = map.get_mut(&job_id_thread) {
                        state.status = OptimizeJobStatus::Done;
                        state.progress = 100;
                        state.result = Some(response);
                    }
                }
            }
            Err(()) => {
                if let Ok(mut map) = optimize_jobs().lock() {
                    if let Some(state) = map.get_mut(&job_id_thread) {
                        state.status = OptimizeJobStatus::Error;
                        state.error = Some("Cancelled".to_string());
                    }
                }
            }
        }
        optimize_cancel_flags()
            .lock()
            .unwrap()
            .remove(&job_id_thread);
    });

    Ok(OptimizeStartResponse { job_id })
}

pub fn get_job_status(job_id: &str) -> Result<OptimizeStatusResponse, OptimizeStatusError> {
    let map = optimize_jobs().lock().unwrap();
    let state = map.get(job_id).ok_or(OptimizeStatusError::NotFound)?;
    let status = match &state.status {
        OptimizeJobStatus::Running => "running",
        OptimizeJobStatus::Done => "done",
        OptimizeJobStatus::Error => "error",
    };
    Ok(OptimizeStatusResponse {
        status: status.to_string(),
        progress: Some(state.progress),
        crews_done: Some(state.crews_done),
        total_crews: Some(state.total_crews),
        result: state.result.clone(),
        error: state.error.clone(),
    })
}

pub fn cancel_job(job_id: &str) -> Result<(), OptimizeStatusError> {
    let flag = {
        let flags = optimize_cancel_flags().lock().unwrap();
        flags.get(job_id).cloned().ok_or(OptimizeStatusError::NotFound)?
    };
    flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod optimize_job_store_tests {
    use super::*;

    fn done_state() -> OptimizeJobState {
        OptimizeJobState {
            status: OptimizeJobStatus::Done,
            progress: 100,
            crews_done: 1,
            total_crews: 1,
            result: None,
            error: None,
        }
    }

    #[test]
    fn parse_job_timestamp_reads_opt_prefix() {
        assert_eq!(parse_optimize_job_timestamp_ms("opt_1700000000123_0"), 1700000000123);
        assert_eq!(parse_optimize_job_timestamp_ms("opt_99_7"), 99);
        assert_eq!(parse_optimize_job_timestamp_ms("bad"), 0);
    }

    #[test]
    fn prune_drops_oldest_completed_first() {
        let mut map = HashMap::new();
        let mut flags = HashMap::new();
        map.insert("opt_100_0".to_string(), done_state());
        map.insert("opt_200_1".to_string(), done_state());
        map.insert("opt_300_2".to_string(), done_state());
        map.insert(
            "opt_400_run".to_string(),
            OptimizeJobState {
                status: OptimizeJobStatus::Running,
                progress: 0,
                crews_done: 0,
                total_crews: 0,
                result: None,
                error: None,
            },
        );
        flags.insert("opt_100_0".to_string(), Arc::new(AtomicBool::new(false)));
        flags.insert("opt_200_1".to_string(), Arc::new(AtomicBool::new(false)));
        flags.insert("opt_300_2".to_string(), Arc::new(AtomicBool::new(false)));
        flags.insert("opt_400_run".to_string(), Arc::new(AtomicBool::new(false)));

        prune_completed_optimize_jobs_over_cap(&mut map, &mut flags, 2);
        assert_eq!(map.len(), 2);
        assert!(!map.contains_key("opt_100_0"));
        assert!(!map.contains_key("opt_200_1"));
        assert!(map.contains_key("opt_300_2"));
        assert!(map.contains_key("opt_400_run"));
    }
}
