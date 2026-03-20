//! Execution layer: run optimize, job store, and response types.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::{
    expand_crews, load_seed_file, BelowDecksStrategy, DEFAULT_HEURISTICS_DIR,
};
use crate::optimizer::crew_generator::{CrewCandidate, BELOW_DECKS_SLOTS};
use crate::optimizer::monte_carlo::{
    run_monte_carlo_parallel_with_registry, SimulationResult,
};
use crate::optimizer::ranking::rank_results;
use crate::optimizer::{
    optimize_scenario_with_progress_with_registry, optimize_scenario_with_registry,
    OptimizationScenario, OptimizerStrategy,
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

/// Run optimization (assumes request already validated). Returns response or serialization error.
pub fn run_optimize(
    registry: &DataRegistry,
    request: &OptimizeRequest,
    profile_id: Option<&str>,
) -> Result<OptimizeResponse, OptimizePayloadError> {
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    let seed = request.seed.unwrap_or(0);
    let strategy = parse_strategy(request.strategy.as_ref());
    let heuristics_only = request.heuristics_only.unwrap_or(false);
    let bd_strategy = parse_below_decks_strategy(request.below_decks_strategy.as_ref());
    let heuristics_seeds = request.heuristics_seeds.as_deref().unwrap_or(&[]);

    let start = Instant::now();

    let h_candidates = if !heuristics_seeds.is_empty() {
        load_heuristics_candidates(registry, heuristics_seeds, bd_strategy)
    } else {
        Vec::new()
    };
    let is_seeded_genetic = strategy == OptimizerStrategy::Genetic && !h_candidates.is_empty();

    let mut all_results: Vec<SimulationResult> = if !h_candidates.is_empty() && !is_seeded_genetic {
        run_monte_carlo_parallel_with_registry(
            registry,
            &request.ship,
            &request.hostile,
            request.ship_tier,
            request.ship_level,
            &h_candidates,
            sims as usize,
            seed,
            profile_id,
        )
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
            allow_duplicate_officers: request.allow_duplicate_officers.unwrap_or(false),
        };
        all_results.extend(
            optimize_scenario_with_registry(registry, &scenario)
                .into_iter()
                .map(|r| SimulationResult {
                    candidate: CrewCandidate {
                        captain: r.captain,
                        bridge: r.bridge,
                        below_decks: r.below_decks,
                    },
                    win_rate: r.win_rate,
                    stall_rate: r.stall_rate,
                    loss_rate: r.loss_rate,
                    avg_hull_remaining: r.avg_hull_remaining,
                }),
        );
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let ranked_results = rank_results(all_results);

    let engine = if heuristics_only {
        "heuristics"
    } else if is_seeded_genetic {
        "seeded_genetic"
    } else {
        match strategy {
            OptimizerStrategy::Exhaustive => "optimizer_v1",
            OptimizerStrategy::Genetic => "genetic",
            OptimizerStrategy::Tiered => "tiered",
        }
    };
    let mut notes = vec!["Results are deterministic for the same ship, hostile, simulation count, and seed."];
    if is_seeded_genetic {
        let note = "GA population seeded with heuristics crews.";
        notes.insert(0, note);
    } else if !heuristics_seeds.is_empty() {
        notes.insert(0, "Heuristics crews were evaluated first.");
    }

    Ok(OptimizeResponse {
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
        warnings: Vec::new(),
    })
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
pub fn start_optimize_job(
    registry: Arc<DataRegistry>,
    request: OptimizeRequest,
    profile_id: Option<&str>,
) -> Result<OptimizeStartResponse, OptimizePayloadError> {
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    let seed = request.seed.unwrap_or(0);

    let job_id = next_job_id();
    let cancel_flag = Arc::new(AtomicBool::new(false));
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
        optimize_cancel_flags().lock().unwrap().insert(job_id.clone(), cancel_flag.clone());
    }

    let ship = request.ship.clone();
    let hostile = request.hostile.clone();
    let ship_tier = request.ship_tier;
    let ship_level = request.ship_level;
    let job_id_clone = job_id.clone();
    let max_candidates = request.max_candidates.map(|n| n as usize);
    let strategy = parse_strategy(request.strategy.as_ref());
    let prioritize_below_decks_ability = request.prioritize_below_decks_ability.unwrap_or(false);
    let heuristics_only = request.heuristics_only.unwrap_or(false);
    let bd_strategy = parse_below_decks_strategy(request.below_decks_strategy.as_ref());
    let heuristics_seeds = request.heuristics_seeds.clone().unwrap_or_default();
    let allow_duplicate_officers = request.allow_duplicate_officers.unwrap_or(false);
    let profile_id_owned = profile_id.map(String::from);
    let cancel_flag_clone = cancel_flag.clone();

    std::thread::spawn(move || {
        let start = Instant::now();
        let registry_ref = registry.as_ref();

        let h_candidates = if !heuristics_seeds.is_empty() {
            load_heuristics_candidates(registry_ref, &heuristics_seeds, bd_strategy)
        } else {
            Vec::new()
        };
        let is_seeded_genetic = strategy == OptimizerStrategy::Genetic && !h_candidates.is_empty();

        let mut all_results: Vec<SimulationResult> =
            if !h_candidates.is_empty() && !is_seeded_genetic {
                let h_total = h_candidates.len() as u32;
                if let Ok(mut map) = optimize_jobs().lock() {
                    if let Some(state) = map.get_mut(&job_id_clone) {
                        state.total_crews = h_total;
                    }
                }
                let results = run_monte_carlo_parallel_with_registry(
                    registry_ref,
                    &ship,
                    &hostile,
                    ship_tier,
                    ship_level,
                    &h_candidates,
                    sims as usize,
                    seed,
                    profile_id_owned.as_deref(),
                );
                if let Ok(mut map) = optimize_jobs().lock() {
                    if let Some(state) = map.get_mut(&job_id_clone) {
                        state.crews_done = h_total;
                        state.progress = if heuristics_only { 100 } else { 10 };
                    }
                }
                results
            } else {
                Vec::new()
            };

        if !heuristics_only {
            let scenario = OptimizationScenario {
                ship: &ship,
                hostile: &hostile,
                ship_tier,
                ship_level,
                simulation_count: sims as usize,
                seed,
                max_candidates,
                strategy,
                only_below_decks_with_ability: prioritize_below_decks_ability,
                seed_population: if is_seeded_genetic {
                    h_candidates.clone()
                } else {
                    Vec::new()
                },
                profile_id: profile_id_owned.as_deref(),
                tiered_scout_sims: None,
                tiered_top_k: None,
                allow_duplicate_officers,
            };
            let normal_results = optimize_scenario_with_progress_with_registry(
                registry_ref,
                &scenario,
                |crews_done, total_crews| {
                    if cancel_flag_clone.load(Ordering::Relaxed) {
                        return false;
                    }
                    let base_progress = if !heuristics_seeds.is_empty() && !is_seeded_genetic {
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
                        if let Some(state) = map.get_mut(&job_id_clone) {
                            state.progress = progress;
                            state.crews_done = crews_done;
                            state.total_crews = total_crews;
                        }
                    }
                    true
                },
            );
            if cancel_flag_clone.load(Ordering::Relaxed) {
                if let Ok(mut map) = optimize_jobs().lock() {
                    if let Some(state) = map.get_mut(&job_id_clone) {
                        state.status = OptimizeJobStatus::Error;
                        state.error = Some("Cancelled".to_string());
                    }
                }
                optimize_cancel_flags().lock().unwrap().remove(&job_id_clone);
                return;
            }
            all_results.extend(
                normal_results
                    .into_iter()
                    .map(|r| SimulationResult {
                        candidate: CrewCandidate {
                            captain: r.captain,
                            bridge: r.bridge,
                            below_decks: r.below_decks,
                        },
                        win_rate: r.win_rate,
                        stall_rate: r.stall_rate,
                        loss_rate: r.loss_rate,
                        avg_hull_remaining: r.avg_hull_remaining,
                    }),
            );
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let ranked_results = rank_results(all_results);

        let engine = if heuristics_only {
            "heuristics"
        } else if is_seeded_genetic {
            "seeded_genetic"
        } else {
            match strategy {
                OptimizerStrategy::Exhaustive => "optimizer_v1",
                OptimizerStrategy::Genetic => "genetic",
                OptimizerStrategy::Tiered => "tiered",
            }
        };
        let mut notes = vec!["Results are deterministic for the same ship, hostile, simulation count, and seed."];
        if is_seeded_genetic {
            let note = "GA population seeded with heuristics crews.";
            notes.insert(0, note);
        } else if !heuristics_seeds.is_empty() {
            notes.insert(0, "Heuristics crews were evaluated first.");
        }

        let response = OptimizeResponse {
            status: "ok",
            engine,
            scenario: ScenarioSummary {
                ship: ship.clone(),
                hostile: hostile.clone(),
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
            warnings: Vec::new(),
        };

        if let Ok(mut map) = optimize_jobs().lock() {
            if let Some(state) = map.get_mut(&job_id_clone) {
                state.status = OptimizeJobStatus::Done;
                state.progress = 100;
                state.result = Some(response);
            }
        }
        optimize_cancel_flags().lock().unwrap().remove(&job_id_clone);
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
