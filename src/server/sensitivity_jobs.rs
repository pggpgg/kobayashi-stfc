//! Async job runner for sensitivity analysis (OAT, Morris, Sobol).
//!
//! Modeled on the optimize-job pattern in [`crate::server::api::execution`] — same
//! in-memory job registry, atomic cancel flag, std::thread worker, oldest-finished
//! eviction policy. The key difference is the work itself: instead of a single
//! `gather_optimize_simulation_results` path, this module dispatches to one of three
//! sensitivity engines based on the [`SensitivityJobKind`] enum.
//!
//! ## Progress reporting
//!
//! Each engine receives a [`SensitivityJobProgress`] handle that lets it
//! - atomically increment a `sims_done` counter from inside Rayon parallel maps
//!   ([`SensitivityJobProgress::record_sims`]), and
//! - set a phase string at phase boundaries
//!   ([`SensitivityJobProgress::set_phase`]).
//!
//! Cancellation is cooperative: engines call [`SensitivityJobProgress::cancelled`] at
//! phase boundaries (the same pattern optimize uses — Rayon doesn't natively support
//! early termination of in-flight parallel iterators).
//!
//! ## CPU admission
//!
//! Each `*_start` route acquires a permit from the shared
//! `KOBAYASHI_MAX_CONCURRENT_CPU_JOBS` semaphore before spawning the worker thread.
//! The permit is held by the spawned thread for the duration of the job (move-captured),
//! so the semaphore naturally serializes async sensitivity jobs with optimize/simulate.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::OwnedSemaphorePermit;
use tracing::{info, info_span, warn};

use crate::data::data_registry::DataRegistry;
use crate::optimizer::sensitivity::{
    run_sensitivity_with_progress, SensitivityRequest, SensitivityResponse,
};
use crate::optimizer::sensitivity_morris::{
    run_morris_with_progress, MorrisRequest, MorrisResponse,
};
use crate::optimizer::sensitivity_sobol::{run_sobol_with_progress, SobolRequest, SobolResponse};
use crate::server::job_registry::{JobRegistry, JobState};

/// Discriminator for the three sensitivity methods. Used as a job-id prefix and to
/// route the worker thread to the right engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitivityJobKind {
    Oat,
    Morris,
    Sobol,
}

impl SensitivityJobKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Oat => "oat",
            Self::Morris => "morris",
            Self::Sobol => "sobol",
        }
    }
}

/// One of the three concrete request payloads. The worker thread holds this until it
/// dispatches to the appropriate engine.
pub enum SensitivityJobRequest {
    Oat(SensitivityRequest),
    Morris(MorrisRequest),
    Sobol(SobolRequest),
}

/// Tagged result type so the SSE stream / status endpoint can serialize whichever engine
/// produced this job's output without forcing all three into a single response shape.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum SensitivityJobResult {
    Oat(SensitivityResponse),
    Morris(MorrisResponse),
    Sobol(SobolResponse),
}

#[derive(Debug, Clone)]
pub enum SensitivityJobStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct SensitivityJobState {
    pub status: SensitivityJobStatus,
    pub kind: SensitivityJobKind,
    pub progress: u8,
    pub sims_done: u64,
    pub total_sims: u64,
    pub phase: Option<String>,
    pub result: Option<SensitivityJobResult>,
    pub error: Option<String>,
    /// Unix-millis at insertion. Read by [`JobRegistry`] for oldest-finished eviction.
    pub started_at_ms: u128,
}

impl JobState for SensitivityJobState {
    fn started_at_ms(&self) -> u128 {
        self.started_at_ms
    }
    fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            SensitivityJobStatus::Done | SensitivityJobStatus::Error
        )
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SensitivityStartResponse {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SensitivityStatusResponse {
    pub status: String,
    /// Method discriminator (`oat`, `morris`, or `sobol`) — useful for clients that
    /// poll a job without tracking which method they started.
    pub method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sims_done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_sims: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// Best-effort sims/sec. Computed from `sims_done` and elapsed wall time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_sims_per_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<SensitivityJobResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum SensitivityJobError {
    NotFound,
    Serialize(serde_json::Error),
}

impl std::fmt::Display for SensitivityJobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Job not found"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SensitivityJobError {}

// --- Job registry (shared with optimize via [`crate::server::job_registry`]) ---

const MAX_SENSITIVITY_JOBS_RETAINED: usize = 64;

static REGISTRY: JobRegistry<SensitivityJobState> = JobRegistry::new();

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// --- Progress sink shared with the engines ---

/// Handle passed into a sensitivity engine to report progress and check cancellation.
///
/// Designed to be cheap to clone: every field is an `Arc` so engines can hand the sink to
/// nested Rayon scopes without lifetime contortions.
#[derive(Clone)]
pub struct SensitivityJobProgress {
    sims_done: Arc<AtomicU64>,
    total_sims: Arc<AtomicU64>,
    phase: Arc<Mutex<Option<&'static str>>>,
    cancel: Arc<AtomicBool>,
    /// Reference back to the job registry so engines can opportunistically flush phase /
    /// counter updates into [`SensitivityJobState`] for the next status poll. Set to
    /// `None` for the synchronous (no-job) path so the engine functions can be unit-tested
    /// without spinning up the global job map.
    job_id: Option<String>,
}

impl SensitivityJobProgress {
    /// Sink that does nothing — used by the existing sync `run_*` paths so they remain
    /// drop-in replacements for the pre-async API.
    pub fn no_op() -> Self {
        Self {
            sims_done: Arc::new(AtomicU64::new(0)),
            total_sims: Arc::new(AtomicU64::new(0)),
            phase: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
            job_id: None,
        }
    }

    /// Set the total sim budget once the engine has resolved its k / N / r parameters.
    /// Called once near the top of `run_*_with_progress`. The status endpoint uses this
    /// as the denominator for the progress %.
    pub fn set_total_sims(&self, total: u64) {
        self.total_sims.store(total, Ordering::Relaxed);
        if let Some(job_id) = self.job_id.as_deref() {
            REGISTRY.with_state_mut(job_id, |state| state.total_sims = total);
        }
    }

    /// Atomic increment of the `sims_done` counter (cheap; safe from inside Rayon maps).
    /// Throttles the per-call registry write to one in every 64 sims so high-throughput
    /// parallel maps (~µs per sim) don't end up contending the global lock.
    #[inline]
    pub fn record_sims(&self, count: u64) {
        if count == 0 {
            return;
        }
        let prev = self.sims_done.fetch_add(count, Ordering::Relaxed);
        let new = prev + count;
        // Only flush to the registry once we've crossed a 64-sim boundary (or hit
        // total_sims so the final %=100 lands). The status endpoint reads the atomics
        // for the % anyway via its own elapsed-time calc on the next poll.
        let total = self.total_sims.load(Ordering::Relaxed);
        let should_flush = (prev / 64) != (new / 64) || new >= total;
        if !should_flush {
            return;
        }
        if let Some(job_id) = self.job_id.as_deref() {
            REGISTRY.with_state_mut(job_id, |st| {
                st.sims_done = new;
                if let Some(pct) = (new * 100).checked_div(total) {
                    st.progress = pct.min(100) as u8;
                }
            });
        }
    }

    /// Set the current phase string. Called at phase boundaries (between Rayon scopes).
    pub fn set_phase(&self, phase: &'static str) {
        *self.phase.lock().unwrap_or_else(|e| e.into_inner()) = Some(phase);
        if let Some(job_id) = self.job_id.as_deref() {
            REGISTRY.with_state_mut(job_id, |st| st.phase = Some(phase.to_string()));
        }
    }

    /// Cancellation check. Engines call this between phases (e.g., after each
    /// `into_par_iter().collect()`) and return early on `true`.
    #[inline]
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

// --- Public API ---

/// Spawn a sensitivity job (OAT, Morris, or Sobol). Returns the job id immediately;
/// the engine runs on a dedicated std::thread that holds `cpu_permit` for the duration.
pub fn start_sensitivity_job(
    registry: Arc<DataRegistry>,
    kind: SensitivityJobKind,
    request: SensitivityJobRequest,
    cpu_permit: OwnedSemaphorePermit,
) -> SensitivityStartResponse {
    let job_id = REGISTRY.next_id(&format!("sens_{}", kind.as_str()));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    info!(job_id = %job_id, method = kind.as_str(), "sensitivity_job_started");

    REGISTRY.insert(
        job_id.clone(),
        SensitivityJobState {
            status: SensitivityJobStatus::Running,
            kind,
            progress: 0,
            sims_done: 0,
            total_sims: 0,
            phase: None,
            result: None,
            error: None,
            started_at_ms: now_ms(),
        },
        cancel_flag.clone(),
        MAX_SENSITIVITY_JOBS_RETAINED,
    );

    let progress = SensitivityJobProgress {
        sims_done: Arc::new(AtomicU64::new(0)),
        total_sims: Arc::new(AtomicU64::new(0)),
        phase: Arc::new(Mutex::new(None)),
        cancel: cancel_flag,
        job_id: Some(job_id.clone()),
    };

    let job_id_thread = job_id.clone();
    std::thread::spawn(move || {
        let job_span = info_span!(
            "sensitivity_job_run",
            job_id = %job_id_thread,
            method = kind.as_str(),
        );
        let _job_span = job_span.enter();
        let _cpu_permit = cpu_permit;
        let start = Instant::now();

        let outcome: Result<SensitivityJobResult, String> = match request {
            SensitivityJobRequest::Oat(req) => {
                run_sensitivity_with_progress(registry.as_ref(), &req, &progress)
                    .map(SensitivityJobResult::Oat)
                    .map_err(|e| e.to_string())
            }
            SensitivityJobRequest::Morris(req) => {
                run_morris_with_progress(registry.as_ref(), &req, &progress)
                    .map(SensitivityJobResult::Morris)
            }
            SensitivityJobRequest::Sobol(req) => {
                run_sobol_with_progress(registry.as_ref(), &req, &progress)
                    .map(SensitivityJobResult::Sobol)
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        match outcome {
            Ok(result) if progress.cancelled() => {
                warn!(job_id = %job_id_thread, duration_ms, "sensitivity_job_cancelled");
                drop(result);
                REGISTRY.with_state_mut(&job_id_thread, |state| {
                    state.status = SensitivityJobStatus::Error;
                    state.error = Some("Cancelled".to_string());
                });
            }
            Ok(result) => {
                info!(job_id = %job_id_thread, duration_ms, "sensitivity_job_completed");
                REGISTRY.with_state_mut(&job_id_thread, |state| {
                    state.status = SensitivityJobStatus::Done;
                    state.progress = 100;
                    state.phase = None;
                    state.result = Some(result);
                });
            }
            Err(err) => {
                warn!(job_id = %job_id_thread, error = %err, duration_ms, "sensitivity_job_failed");
                REGISTRY.with_state_mut(&job_id_thread, |state| {
                    state.status = SensitivityJobStatus::Error;
                    state.error = Some(err);
                });
            }
        }
        REGISTRY.remove_cancel(&job_id_thread);
    });

    SensitivityStartResponse { job_id }
}

pub fn get_job_status(job_id: &str) -> Result<SensitivityStatusResponse, SensitivityJobError> {
    let state = REGISTRY.get(job_id).ok_or(SensitivityJobError::NotFound)?;
    let status_str = match &state.status {
        SensitivityJobStatus::Running => "running",
        SensitivityJobStatus::Done => "done",
        SensitivityJobStatus::Error => "error",
    };
    let elapsed_s = ((now_ms().saturating_sub(state.started_at_ms)) as f64) / 1000.0;
    let (throughput, eta) = if matches!(state.status, SensitivityJobStatus::Running)
        && elapsed_s > 0.05
        && state.sims_done > 0
        && state.total_sims > state.sims_done
    {
        let tp = state.sims_done as f64 / elapsed_s;
        let remaining = (state.total_sims - state.sims_done) as f64;
        let eta = if tp > 1e-6 {
            Some((remaining / tp).ceil().max(0.0) as u64)
        } else {
            None
        };
        (Some(tp), eta)
    } else {
        (None, None)
    };
    Ok(SensitivityStatusResponse {
        status: status_str.to_string(),
        method: state.kind.as_str(),
        progress: Some(state.progress),
        sims_done: Some(state.sims_done),
        total_sims: Some(state.total_sims),
        phase: state.phase.clone(),
        throughput_sims_per_sec: throughput,
        eta_seconds: eta,
        result: state.result.clone(),
        error: state.error.clone(),
    })
}

pub fn cancel_job(job_id: &str) -> Result<(), SensitivityJobError> {
    if REGISTRY.cancel(job_id) {
        Ok(())
    } else {
        Err(SensitivityJobError::NotFound)
    }
}
