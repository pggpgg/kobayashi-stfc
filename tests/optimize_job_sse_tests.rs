//! SSE integration tests for `GET /api/optimize/jobs/:job_id/stream`.
//!
//! Verifies clients receive progress JSON payloads, terminal done/error events, and that the
//! stream closes cleanly. Uses seeded jobs for fast paths and one real async optimize for E2E.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;
use kobayashi::server::api::{
    patch_optimize_job_for_tests, seed_optimize_job_for_tests, OptimizeJobState, OptimizeJobStatus,
};
use kobayashi::server::routes::build_router;
use std::net::SocketAddr;
use tower::ServiceExt;

const TEST_PROFILE_HEADERS: &[(&str, &str)] =
    &[("x-profile-id", NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)];

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn parse_sse_json_events(body: &str) -> Vec<serde_json::Value> {
    body.split("\n\n")
        .filter_map(|block| {
            block
                .lines()
                .find(|line| line.starts_with("data: "))
                .and_then(|line| line.strip_prefix("data: "))
                .and_then(|json| serde_json::from_str(json).ok())
        })
        .collect()
}

async fn collect_optimize_sse_events(
    path: &str,
    extra_headers: &[(&str, &str)],
    timeout: Duration,
) -> Vec<serde_json::Value> {
    let registry = DataRegistry::load().expect("data registry required for SSE tests");
    let app = build_router(registry);
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(path)
        .header("accept", "text/event-stream");
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder.body(Body::empty()).expect("request");
    req.extensions_mut()
        .insert(ConnectInfo("127.0.0.1:12345".parse::<SocketAddr>().unwrap()));

    let resp = app.oneshot(req).await.expect("router response");
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/event-stream"),
        "unexpected content-type: {content_type}"
    );

    let body = resp.into_body();
    let bytes = tokio::time::timeout(timeout, axum::body::to_bytes(body, 512 * 1024))
        .await
        .unwrap_or_else(|_| panic!("SSE stream timed out after {timeout:?} for {path}"))
        .expect("read SSE body");
    parse_sse_json_events(&String::from_utf8_lossy(&bytes))
}

async fn post_json(path: &str, body: &str, extra_headers: &[(&str, &str)]) -> String {
    let registry = DataRegistry::load().expect("data registry required for SSE tests");
    let app = build_router(registry);
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder
        .body(Body::from(body.to_string()))
        .expect("request");
    req.extensions_mut()
        .insert(ConnectInfo("127.0.0.1:12345".parse::<SocketAddr>().unwrap()));
    let resp = app.oneshot(req).await.expect("router response");
    assert_eq!(resp.status(), StatusCode::OK, "POST {path} failed");
    let bytes = axum::body::to_bytes(resp.into_body(), 256 * 1024)
        .await
        .expect("read POST body");
    String::from_utf8_lossy(&bytes).into_owned()
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_job_stream_unknown_job_emits_error_and_closes() {
    let events = collect_optimize_sse_events(
        "/api/optimize/jobs/opt_sse_missing_0/stream",
        &[],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(events.len(), 1, "expected single terminal event: {events:?}");
    assert_eq!(events[0]["status"], "error");
    assert_eq!(events[0]["error"], "Job not found");
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_job_stream_terminal_error_job_emits_error_payload() {
    let job_id = "opt_sse_test_error_terminal";
    seed_optimize_job_for_tests(
        job_id,
        OptimizeJobState {
            status: OptimizeJobStatus::Error,
            progress: 0,
            crews_done: 0,
            total_crews: 0,
            phase: None,
            progress_preview: None,
            result: None,
            error: Some("validation failed".to_string()),
            started_at_ms: now_ms(),
        },
    );

    let path = format!("/api/optimize/jobs/{job_id}/stream");
    let events = collect_optimize_sse_events(&path, &[], Duration::from_secs(2)).await;
    assert_eq!(events.len(), 1, "terminal error should emit once: {events:?}");
    assert_eq!(events[0]["status"], "error");
    assert_eq!(events[0]["error"], "validation failed");
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_job_stream_emits_running_updates_then_done() {
    let job_id = "opt_sse_test_running_done";
    seed_optimize_job_for_tests(
        job_id,
        OptimizeJobState {
            status: OptimizeJobStatus::Running,
            progress: 25,
            crews_done: 5,
            total_crews: 20,
            phase: Some("monte_carlo".to_string()),
            progress_preview: None,
            result: None,
            error: None,
            started_at_ms: now_ms(),
        },
    );

    let job_id_owned = job_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(patch_optimize_job_for_tests(&job_id_owned, |state| {
            state.progress = 60;
            state.crews_done = 12;
        }));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(patch_optimize_job_for_tests(&job_id_owned, |state| {
            state.status = OptimizeJobStatus::Done;
            state.progress = 100;
            state.phase = None;
        }));
    });

    let path = format!("/api/optimize/jobs/{job_id}/stream");
    let events = collect_optimize_sse_events(&path, &[], Duration::from_secs(5)).await;

    assert!(!events.is_empty(), "expected at least one SSE payload");
    assert_eq!(events[0]["status"], "running");
    assert_eq!(events[0]["progress"], 25);
    assert_eq!(events[0]["phase"], "monte_carlo");

    let last = events.last().expect("terminal event");
    assert_eq!(last["status"], "done");
    assert_eq!(last["progress"], 100);
    assert!(events.len() >= 2, "expected progress + terminal events: {events:?}");
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_start_sse_stream_delivers_done_with_recommendations() {
    let body =
        r#"{"ship":"saladin","hostile":"2918121098","sims":200,"seed":1,"max_candidates":8}"#;
    let start_body = post_json("/api/optimize/start", body, TEST_PROFILE_HEADERS).await;
    let payload: serde_json::Value =
        serde_json::from_str(&start_body).expect("start response json");
    let job_id = payload["job_id"].as_str().expect("job_id");

    let path = format!("/api/optimize/jobs/{job_id}/stream");
    let events = collect_optimize_sse_events(&path, &[], Duration::from_secs(30)).await;

    let last = events.last().expect("terminal SSE event");
    assert_eq!(last["status"], "done", "unexpected SSE terminal: {last:?}");
    let recs = last["result"]["recommendations"]
        .as_array()
        .expect("recommendations in final SSE payload");
    assert!(
        !recs.is_empty(),
        "async optimize SSE should include recommendations"
    );
}
