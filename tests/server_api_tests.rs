use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::server::routes::build_router;
use std::net::SocketAddr;
use tower::ServiceExt;

struct TestResponse {
    status_code: u16,
    content_type: String,
    body: String,
}

async fn route_request(method: &str, path: &str, body: &str) -> TestResponse {
    route_request_ex(method, path, body, None, &[]).await
}

async fn route_request_ex(
    method: &str,
    path: &str,
    body: &str,
    peer: Option<SocketAddr>,
    extra_headers: &[(&str, &str)],
) -> TestResponse {
    let registry = DataRegistry::load().expect("data registry required for server tests");
    let app = build_router(registry);
    let m = match method {
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        _ => Method::GET,
    };
    let addr = peer.unwrap_or_else(|| "127.0.0.1:12345".parse().expect("loopback"));
    let mut builder = Request::builder()
        .method(m)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    let resp = app.oneshot(req).await.unwrap();
    let status_code = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    TestResponse {
        status_code,
        content_type,
        body,
    }
}

#[serial_test::serial]
#[tokio::test]
async fn health_endpoint_returns_ok_json() {
    let response = route_request("GET", "/api/health", "").await;
    assert_eq!(response.status_code, 200);
    assert_eq!(response.content_type, "application/json");
    let p: serde_json::Value = serde_json::from_str(&response.body).expect("health json");
    assert_eq!(p["status"], "ok");
    assert_eq!(p["service"], "kobayashi-api");
    assert!(p["build"]["cargo_pkg_version"].as_str().is_some());
    assert!(p["server"]["started_at_utc"].as_str().is_some());
    assert_eq!(p["server"]["max_concurrent_cpu_jobs"], 1);
    assert_eq!(p["server"]["max_concurrent_cpu_jobs_from_env"], false);
    assert!(p["data"]["officer_count"].as_u64().is_some());
    assert!(p["data"]["hostile_index_loaded"].is_boolean());
    assert!(p["data"]["ship_index_loaded"].is_boolean());
}

#[serial_test::serial]
#[tokio::test]
async fn openapi_yaml_and_json_served() {
    let yaml = route_request("GET", "/api/openapi.yaml", "").await;
    assert_eq!(yaml.status_code, 200);
    assert!(
        yaml.content_type.contains("yaml"),
        "unexpected content-type: {}",
        yaml.content_type
    );
    assert!(yaml.body.contains("openapi: 3.0.3"));
    assert!(yaml.body.contains("/api/simulate:"));

    let json = route_request("GET", "/api/openapi.json", "").await;
    assert_eq!(json.status_code, 200);
    assert!(
        json.content_type.contains("json"),
        "unexpected content-type: {}",
        json.content_type
    );
    let p: serde_json::Value = serde_json::from_str(&json.body).expect("openapi json");
    assert_eq!(p["openapi"], "3.0.3");
    assert!(p["paths"].is_object());
}

#[serial_test::serial]
#[tokio::test]
async fn mechanics_coverage_returns_tier_counts() {
    let response = route_request("GET", "/api/mechanics/coverage", "").await;
    assert_eq!(response.status_code, 200, "{}", response.body);
    let p: serde_json::Value = serde_json::from_str(&response.body).expect("coverage json");
    assert_eq!(p["status"], "ok");
    assert!(p["lcars_effects"]["implemented"].as_u64().is_some());
    assert!(p["ship_hull_abilities"].is_object());
    assert!(p["hostile_catalog_entries"].is_object());
    let backlog = p["fidelity_backlog"]
        .as_array()
        .expect("fidelity_backlog array");
    assert!(
        !backlog.is_empty(),
        "expected non-empty fidelity_backlog from bundled data"
    );
    let mut last_rank = 0u64;
    for row in backlog {
        let rank = row["rank"].as_u64().expect("backlog rank");
        assert!(rank > last_rank, "ranks should increase");
        last_rank = rank;
        assert!(row["area"].as_str().is_some());
        assert!(row["summary"].as_str().is_some());
    }
}

#[serial_test::serial]
#[tokio::test]
async fn profile_buildings_summary_returns_json() {
    let response = route_request("GET", "/api/profile/buildings-summary", "").await;
    assert_eq!(response.status_code, 200);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("buildings-summary json");
    assert!(payload["profile_id"].as_str().is_some());
    assert!(payload["synced_building_count"].is_number());
}

#[serial_test::serial]
#[tokio::test]
async fn profile_research_summary_returns_json() {
    let response = route_request("GET", "/api/profile/research-summary", "").await;
    assert_eq!(response.status_code, 200);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("research-summary json");
    assert!(payload["profile_id"].as_str().is_some());
    assert!(payload["synced_research_count"].is_number());
    assert!(payload["unmapped_rids"].is_array());
    assert!(payload["research"].is_array());
}

#[serial_test::serial]
#[tokio::test]
async fn simulate_rejects_body_over_cpu_json_limit_with_413() {
    const LIMIT: usize = 2 * 1024 * 1024;
    let registry = DataRegistry::load().expect("data registry required for server tests");
    let app = build_router(registry);
    let oversized = vec![b'{'; LIMIT + 1];
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/api/simulate")
        .header("content-type", "application/json")
        .body(Body::from(oversized))
        .unwrap();
    let addr: SocketAddr = "127.0.0.1:12345".parse().expect("loopback");
    req.extensions_mut().insert(ConnectInfo(addr));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_returns_ranked_recommendations() {
    let body =
        r#"{"ship":"saladin","hostile":"2918121098","sims":2000,"seed":7,"max_candidates":64}"#;
    let response = route_request("POST", "/api/optimize", body).await;

    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");

    assert_eq!(payload["engine"], "optimizer_v1");
    assert_eq!(payload["scenario"]["ship"], "saladin");
    assert_eq!(payload["scenario"]["hostile"], "2918121098");
    assert_eq!(payload["scenario"]["sims"], 2000);
    assert_eq!(payload["scenario"]["seed"], 7);

    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(
        !recommendations.is_empty(),
        "recommendations should not be empty"
    );

    let first = &recommendations[0];
    assert!(first["captain"].as_str().is_some());
    assert!(
        first["bridge"].as_array().is_some(),
        "bridge should be an array"
    );
    assert!(
        first["below_decks"].as_array().is_some(),
        "below_decks should be an array"
    );
    assert!(first["win_rate"].as_f64().is_some());
    assert!(first["avg_hull_remaining"].as_f64().is_some());
    let wr = first["win_rate"].as_f64().expect("win_rate");
    let wr_lo = first["win_rate_ci_low"].as_f64().expect("win_rate_ci_low");
    let wr_hi = first["win_rate_ci_high"]
        .as_f64()
        .expect("win_rate_ci_high");
    const CI_EPS: f64 = 1e-5;
    assert!(
        wr_lo - CI_EPS <= wr && wr <= wr_hi + CI_EPS,
        "win rate {wr} should lie in Wilson CI [{wr_lo}, {wr_hi}]"
    );
    assert!(first["r1_kill_rate"].as_f64().is_some());
    assert!(first["r1_kill_rate_ci_low"].as_f64().is_some());

    let mut prior_score: Option<f64> = None;
    let mut saw_non_trivial_metric = false;
    for recommendation in recommendations {
        let score = recommendation["win_rate"].as_f64().unwrap_or(0.0) * 0.8
            + recommendation["avg_hull_remaining"].as_f64().unwrap_or(0.0) * 0.2;
        let win_rate = recommendation["win_rate"].as_f64().unwrap_or(0.0);
        let avg_hull_remaining = recommendation["avg_hull_remaining"].as_f64().unwrap_or(0.0);
        if (0.0..1.0).contains(&win_rate) || (0.0..1.0).contains(&avg_hull_remaining) {
            saw_non_trivial_metric = true;
        }

        if let Some(previous) = prior_score {
            assert!(
                previous >= score,
                "recommendations should be ranked by descending score"
            );
        }
        prior_score = Some(score);
    }

    assert!(
        saw_non_trivial_metric,
        "combat-backed metrics should include non-trivial values"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_changes_with_seed() {
    let response_a = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"2918121098","sims":1000,"seed":7,"max_candidates":32}"#,
    )
    .await;
    let response_b = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"2918121098","sims":1000,"seed":8,"max_candidates":32}"#,
    )
    .await;

    assert_eq!(response_a.status_code, 200);
    assert_eq!(response_b.status_code, 200);
    assert_ne!(response_a.body, response_b.body);
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_is_deterministic_for_fixed_seed() {
    let body =
        r#"{"ship":"saladin","hostile":"2918121098","sims":2000,"seed":77,"max_candidates":64}"#;

    let response_a = route_request("POST", "/api/optimize", body).await;
    let response_b = route_request("POST", "/api/optimize", body).await;

    assert_eq!(response_a.status_code, 200);
    assert_eq!(response_b.status_code, 200);

    let payload_a: serde_json::Value =
        serde_json::from_str(&response_a.body).expect("response A should be valid json");
    let payload_b: serde_json::Value =
        serde_json::from_str(&response_b.body).expect("response B should be valid json");
    assert_eq!(payload_a["scenario"], payload_b["scenario"]);
    assert_eq!(payload_a["recommendations"], payload_b["recommendations"]);
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_invalid_payload() {
    let response = route_request("POST", "/api/optimize", "{bad json}").await;
    assert_eq!(response.status_code, 400);
    assert!(response.body.contains("Invalid request body"));
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_empty_ship_and_hostile() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"","hostile":"   ","sims":100}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");

    assert_eq!(payload["status"], "error");
    assert_eq!(payload["message"], "Validation failed");

    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(
        errors.iter().any(|error| {
            error["field"] == "ship"
                && error["messages"]
                    .as_array()
                    .is_some_and(|messages| !messages.is_empty())
        }),
        "ship validation error should be present"
    );
    assert!(
        errors.iter().any(|error| {
            error["field"] == "hostile"
                && error["messages"]
                    .as_array()
                    .is_some_and(|messages| !messages.is_empty())
        }),
        "hostile validation error should be present"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_zero_sims() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"2918121098","sims":0}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(errors.iter().any(|error| error["field"] == "sims"));
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_very_large_sims() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"2918121098","sims":5000000}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");

    let sims_error = errors
        .iter()
        .find(|error| error["field"] == "sims")
        .expect("sims validation error should be present");
    assert!(
        sims_error["messages"]
            .as_array()
            .is_some_and(|messages| !messages.is_empty()),
        "sims error should contain at least one message"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_excessive_max_candidates() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"2918121098","sims":1000,"max_candidates":3000000}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(
        errors.iter().any(|e| e["field"] == "max_candidates"),
        "max_candidates validation error should be present"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_zero_analytical_prefilter_keep() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"2918121098","sims":100,"analytical_prefilter_keep":0}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(
        errors
            .iter()
            .any(|e| e["field"] == "analytical_prefilter_keep"),
        "analytical_prefilter_keep validation error should be present"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_reports_analytical_prefilter_when_truncating() {
    let body = r#"{"ship":"saladin","hostile":"2918121098","sims":800,"seed":1,"max_candidates":80,"analytical_prefilter_keep":4}"#;
    let response = route_request("POST", "/api/optimize", body).await;
    assert_eq!(response.status_code, 200, "body: {}", response.body);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["scenario"]["analytical_prefilter_keep"], 4);
    assert_eq!(payload["scenario"]["analytical_prefilter_kept"], 4);
    assert!(
        payload["scenario"]["analytical_prefilter_from"]
            .as_u64()
            .unwrap_or(0)
            > 4
    );
    let notes = payload["approximate_notes"]
        .as_array()
        .expect("approximate_notes should be an array");
    assert!(
        notes.iter().any(|n| {
            n.as_str()
                .is_some_and(|s| s.contains("pre-filter") && s.contains("Monte Carlo"))
        }),
        "approximate_notes should describe analytical pre-filter: {:?}",
        notes
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_validation_error_has_expected_schema() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"","hostile":"2918121098","sims":0}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["status"], "error");
    assert_eq!(payload["message"], "Validation failed");

    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    for error in errors {
        assert!(
            error["field"].as_str().is_some(),
            "field should be a string"
        );
        let messages = error["messages"]
            .as_array()
            .expect("messages should be an array");
        assert!(
            messages.iter().all(|message| message.as_str().is_some()),
            "messages should contain strings"
        );
    }
}

#[serial_test::serial]
#[tokio::test]
async fn async_optimize_start_poll_completes_with_recommendations() {
    let body =
        r#"{"ship":"saladin","hostile":"2918121098","sims":1000,"seed":42,"max_candidates":16}"#;
    let start = route_request("POST", "/api/optimize/start", body).await;
    assert_eq!(start.status_code, 200, "body: {}", start.body);
    let payload: serde_json::Value =
        serde_json::from_str(&start.body).expect("start response json");
    let job_id = payload["job_id"].as_str().expect("job_id string");

    for _ in 0..400 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let status = route_request("GET", &format!("/api/optimize/status/{job_id}"), "").await;
        assert_eq!(status.status_code, 200, "status body: {}", status.body);
        let s: serde_json::Value = serde_json::from_str(&status.body).expect("status json");
        if s["status"] == "done" {
            let result = s["result"].as_object().expect("result object");
            let recs = result["recommendations"]
                .as_array()
                .expect("recommendations array");
            assert!(
                !recs.is_empty(),
                "async optimize should return recommendations"
            );
            return;
        }
        assert_ne!(
            s["status"], "error",
            "unexpected job error: {:?}",
            s["error"]
        );
    }
    panic!("async optimize did not complete within timeout");
}

#[serial_test::serial]
#[tokio::test]
async fn async_optimize_cancel_unknown_job_returns_404() {
    let response = route_request("POST", "/api/optimize/jobs/opt_nonexistent_0/cancel", "").await;
    assert_eq!(response.status_code, 404);
}

#[serial_test::serial]
#[tokio::test]
async fn async_optimize_cancel_after_done_is_idempotent_ok() {
    let body =
        r#"{"ship":"saladin","hostile":"2918121098","sims":500,"seed":1,"max_candidates":8}"#;
    let start = route_request("POST", "/api/optimize/start", body).await;
    assert_eq!(start.status_code, 200);
    let payload: serde_json::Value = serde_json::from_str(&start.body).expect("start json");
    let job_id = payload["job_id"].as_str().expect("job_id");

    let mut finished = false;
    for _ in 0..400 {
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
        let status = route_request("GET", &format!("/api/optimize/status/{job_id}"), "").await;
        let s: serde_json::Value = serde_json::from_str(&status.body).expect("status json");
        if s["status"] == "done" {
            finished = true;
            break;
        }
        assert_ne!(s["status"], "error", "{:?}", s["error"]);
    }
    assert!(finished, "job should complete");

    let cancel = route_request("POST", &format!("/api/optimize/jobs/{job_id}/cancel"), "").await;
    assert_eq!(cancel.status_code, 200, "{}", cancel.body);
    let c: serde_json::Value = serde_json::from_str(&cancel.body).expect("cancel json");
    assert_eq!(c["status"], "ok");
    assert!(
        c["message"]
            .as_str()
            .is_some_and(|m| m.contains("finished")),
        "expected idempotent cancel message, got {:?}",
        c["message"]
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_replay_seed_returns_trace_and_is_deterministic() {
    let body = r#"{"ship":"saladin","hostile":"2918121098","seed":77,"sim_index":12,"max_trace_events":50,"crew":{"captain":"718-0-2509d7","bridge":[null,null],"below_deck":[null,null,null]}}"#;
    let a = route_request("POST", "/api/optimize/replay-seed", body).await;
    let b = route_request("POST", "/api/optimize/replay-seed", body).await;
    assert_eq!(a.status_code, 200, "{}", a.body);
    assert_eq!(b.status_code, 200);
    assert_eq!(a.body, b.body);

    let p: serde_json::Value = serde_json::from_str(&a.body).expect("replay json");
    assert_eq!(p["status"], "ok");
    let sc = &p["scenario"];
    assert_eq!(sc["scenario_seed"], 77);
    assert_eq!(sc["sim_index"], 12);
    let base = sc["base_seed"].as_u64().expect("base_seed");
    let iteration = sc["iteration_seed"].as_u64().expect("iteration_seed");
    assert_eq!(iteration, base.wrapping_add(12));

    let tr = &p["trace"];
    assert!(
        tr["event_count"].as_u64().is_some_and(|n| n > 0),
        "expected combat trace events"
    );
    let events = tr["events"].as_array().expect("events array");
    assert!(events.len() <= 50);
    assert!(p["summary"]["attacker_won"].is_boolean());
}

#[serial_test::serial]
#[tokio::test]
async fn compare_crews_returns_distribution_payload() {
    let body = r#"{"ship":"saladin","hostile":"2918121098","num_sims":400,"seed":3,"crews":[
        {"captain":"718-0-2509d7","bridge":[null,null],"below_deck":[null,null,null]},
        {"captain":"718-0-2509d7","bridge":[null,null],"below_deck":[null,null,null]}
    ]}"#;
    let response = route_request("POST", "/api/compare/crews", body).await;
    assert_eq!(response.status_code, 200, "{}", response.body);
    let p: serde_json::Value = serde_json::from_str(&response.body).expect("compare json");
    assert_eq!(p["status"], "ok");
    assert_eq!(p["seed"], 3);
    let crews = p["crews"].as_array().expect("crews array");
    assert_eq!(crews.len(), 2);
    for c in crews {
        assert_eq!(c["trials"], 400);
        let rh = c["rounds_histogram"].as_array().expect("rounds_histogram");
        assert_eq!(rh.len(), 20);
        let hb = c["hull_remaining_bins"]
            .as_array()
            .expect("hull_remaining_bins");
        assert_eq!(hb.len(), 10);
    }
}

#[serial_test::serial]
#[tokio::test]
async fn api_key_required_for_non_loopback_when_configured() {
    std::env::set_var("KOBAYASHI_API_KEY", "unit-test-secret");
    std::env::set_var("KOBAYASHI_API_KEY_TRUST_LOOPBACK", "false");
    let body =
        r#"{"ship":"saladin","hostile":"2918121098","sims":2000,"seed":7,"max_candidates":64}"#;
    let lan: SocketAddr = "192.168.1.10:5555".parse().expect("lan");
    let response = route_request_ex("POST", "/api/optimize", body, Some(lan), &[]).await;
    assert_eq!(response.status_code, 401, "{}", response.body);
    std::env::remove_var("KOBAYASHI_API_KEY");
    std::env::remove_var("KOBAYASHI_API_KEY_TRUST_LOOPBACK");
}

#[serial_test::serial]
#[tokio::test]
async fn api_key_bearer_allows_non_loopback_when_configured() {
    std::env::set_var("KOBAYASHI_API_KEY", "unit-test-secret");
    std::env::set_var("KOBAYASHI_API_KEY_TRUST_LOOPBACK", "false");
    let body =
        r#"{"ship":"saladin","hostile":"2918121098","sims":2000,"seed":7,"max_candidates":64}"#;
    let lan: SocketAddr = "192.168.1.10:5555".parse().expect("lan");
    let response = route_request_ex(
        "POST",
        "/api/optimize",
        body,
        Some(lan),
        &[("authorization", "Bearer unit-test-secret")],
    )
    .await;
    assert_eq!(response.status_code, 200, "{}", response.body);
    std::env::remove_var("KOBAYASHI_API_KEY");
    std::env::remove_var("KOBAYASHI_API_KEY_TRUST_LOOPBACK");
}
