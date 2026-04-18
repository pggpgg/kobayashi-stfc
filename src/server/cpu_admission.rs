//! Bounded admission for CPU-heavy API handlers (`AppState.cpu_jobs` semaphore).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Max milliseconds to wait for a CPU job permit when `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS` is set
/// to a positive value. `0` or unset means wait indefinitely (backward compatible).
pub(crate) fn cpu_job_queue_wait_duration_from_env() -> Option<Duration> {
    std::env::var("KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&ms| ms > 0)
        .map(Duration::from_millis)
}

/// Whether `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS` was present in the environment (including `0`).
pub(crate) fn cpu_job_queue_wait_ms_env_present() -> bool {
    std::env::var("KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS").is_ok()
}

/// Snapshot queue-wait settings at process/router construction (matches other server env knobs).
pub(crate) fn cpu_job_queue_wait_config_from_env() -> (Option<Duration>, bool) {
    (
        cpu_job_queue_wait_duration_from_env(),
        cpu_job_queue_wait_ms_env_present(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcquireCpuPermitError {
    SemaphoreClosed,
    /// Permit not acquired within `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS`.
    QueueTimeout { retry_after_ms: u64 },
}

/// Acquire an owned permit from the shared CPU semaphore, optionally with a bounded wait.
pub(crate) async fn acquire_cpu_permit(
    sem: Arc<Semaphore>,
    queue_wait: Option<Duration>,
) -> Result<OwnedSemaphorePermit, AcquireCpuPermitError> {
    match queue_wait {
        None => sem
            .acquire_owned()
            .await
            .map_err(|_| AcquireCpuPermitError::SemaphoreClosed),
        Some(wait) => {
            let retry_after_ms = wait.as_millis() as u64;
            match tokio::time::timeout(wait, sem.acquire_owned()).await {
                Ok(Ok(p)) => Ok(p),
                Ok(Err(_)) => Err(AcquireCpuPermitError::SemaphoreClosed),
                Err(_) => Err(AcquireCpuPermitError::QueueTimeout { retry_after_ms }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Semaphore;

    #[tokio::test]
    async fn acquire_succeeds_when_permit_free() {
        let sem = Arc::new(Semaphore::new(1));
        let p = acquire_cpu_permit(sem.clone(), None)
            .await
            .expect("permit");
        drop(p);
        let _ = acquire_cpu_permit(sem, None).await.expect("second acquire");
    }

    #[tokio::test]
    async fn acquire_times_out_when_starved_and_wait_configured() {
        let sem = Arc::new(Semaphore::new(0));
        let err = acquire_cpu_permit(sem, Some(Duration::from_millis(50)))
            .await
            .expect_err("should time out");
        assert_eq!(
            err,
            AcquireCpuPermitError::QueueTimeout { retry_after_ms: 50 }
        );
    }
}
