//! Integration tests for the async sensitivity-job runner. Drives the job lifecycle
//! directly through `start_sensitivity_job` + `get_job_status` (the routes are thin
//! wrappers — testing the engine-facing surface here keeps the test offline / no Axum).

use std::sync::Arc;
use std::time::{Duration, Instant};

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::sensitivity::OutcomeMetric;
use kobayashi::optimizer::sensitivity_sobol::SobolRequest;
use kobayashi::server::sensitivity_jobs::{
    cancel_job, get_job_status, start_sensitivity_job, SensitivityJobKind, SensitivityJobRequest,
    SensitivityJobResult,
};
use tokio::sync::Semaphore;

/// Minimal Sobol request for integration testing — same shape used elsewhere in the
/// sensitivity test suite. Low N keeps test time under ~1s.
fn small_sobol_request(include_pairwise: bool) -> SobolRequest {
    SobolRequest {
        ship: "uss_enterprise_d".into(),
        hostile: "kobayashi_theoretical_damage_sponge".into(),
        ship_tier: Some(5),
        ship_level: Some(7),
        captain: Some("ent-e-picard-556227".into()),
        bridge: vec!["ent-e-data-871245".into(), "five-of-eleven-d9aa11".into()],
        below_decks: vec!["harry-kim-a79fdf (T5)".into()],
        support_buffs: None,
        profile_id: Some(DEMO_PROFILE_ID.into()),
        n_samples: Some(16),
        seed: Some(77_001),
        rounds: Some(5),
        metric: Some(OutcomeMetric::HullRemaining),
        deltas: None,
        include_pairwise: Some(include_pairwise),
    }
}

/// Acquire a permit from a brand-new semaphore so the test runs in isolation from the
/// process-wide one (which `routes::AppState` owns).
fn test_permit() -> tokio::sync::OwnedSemaphorePermit {
    let sem = Arc::new(Semaphore::new(1));
    sem.try_acquire_owned().expect("semaphore permit")
}

fn block_until_terminal(job_id: &str, timeout: Duration) -> String {
    let start = Instant::now();
    loop {
        let status = get_job_status(job_id).expect("job exists");
        if status.status == "done" || status.status == "error" {
            return status.status;
        }
        if start.elapsed() > timeout {
            panic!(
                "job {job_id} did not finish within {:?} (status={})",
                timeout, status.status
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn sobol_job_completes_and_carries_result_payload() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = small_sobol_request(false);

    let resp = start_sensitivity_job(
        registry,
        SensitivityJobKind::Sobol,
        SensitivityJobRequest::Sobol(req),
        test_permit(),
    );
    assert!(
        resp.job_id.starts_with("sens_sobol_"),
        "job_id shape: {}",
        resp.job_id
    );

    let final_status = block_until_terminal(&resp.job_id, Duration::from_secs(30));
    assert_eq!(final_status, "done", "job should complete successfully");

    let status = get_job_status(&resp.job_id).expect("status");
    assert_eq!(status.progress, Some(100));
    assert!(status.sims_done.is_some_and(|s| s > 0));
    assert!(status.total_sims.is_some_and(|t| t > 0));
    assert!(status.error.is_none());
    match status.result {
        Some(SensitivityJobResult::Sobol(r)) => {
            assert_eq!(r.metric, "hull_remaining");
            assert_eq!(r.n_samples, 16);
            assert!(!r.rows.is_empty(), "Sobol rows must be populated");
        }
        other => panic!("expected Sobol result variant, got: {:?}", other),
    }
}

#[test]
fn sobol_pairwise_job_carries_pair_payload() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = small_sobol_request(true);

    let resp = start_sensitivity_job(
        registry,
        SensitivityJobKind::Sobol,
        SensitivityJobRequest::Sobol(req),
        test_permit(),
    );
    let final_status = block_until_terminal(&resp.job_id, Duration::from_secs(60));
    assert_eq!(final_status, "done");

    let status = get_job_status(&resp.job_id).expect("status");
    match status.result {
        Some(SensitivityJobResult::Sobol(r)) => {
            let pairs = r.pairs.as_ref().expect("pairwise run yields pairs");
            let k = r.k_stats as usize;
            assert_eq!(pairs.len(), k * (k - 1) / 2);
        }
        other => panic!("expected Sobol result variant, got: {:?}", other),
    }
}

#[test]
fn missing_job_returns_not_found() {
    let err = get_job_status("sens_sobol_doesnotexist_99").unwrap_err();
    assert!(
        matches!(
            err,
            kobayashi::server::sensitivity_jobs::SensitivityJobError::NotFound,
        ),
        "expected NotFound, got {err}"
    );
}

#[test]
fn cancel_of_unknown_job_returns_not_found() {
    let err = cancel_job("sens_sobol_doesnotexist_99").unwrap_err();
    assert!(matches!(
        err,
        kobayashi::server::sensitivity_jobs::SensitivityJobError::NotFound,
    ));
}
