use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::{profile_data_dir, DEMO_PROFILE_ID};
use kobayashi::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;
use kobayashi::server::routes::build_router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::OnceLock;
use tower::ServiceExt;

struct TestResponse {
    status_code: u16,
    content_type: String,
    body: String,
}

/// `enforce_candidate_legality_with_registry` rejects empty seats; bundled profiles like `demo`
/// and `default` ship a tiny `roster.imported.json`, so legality must not use those ids here.
const SERVER_API_TEST_PROFILE_HEADERS: &[(&str, &str)] =
    &[("x-profile-id", NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)];
/// `saladin` crew-slot schedule unlocks a third below-decks slot at ship level 20; lower levels
/// resolve to fewer slots and would pad with empty names (rejected as illegal).
const SERVER_API_TEST_SHIP_LEVEL_THREE_BELOW: &str = "25";
const SERVER_API_TEST_CREW_718_LEGAL_JSON: &str = r#"{"captain":"718-0-2509d7","bridge":["kirk-1323b6","spock-c04738"],"below_deck":["scotty-a83cb5","uhura-ea117c","kira-nerys-a5253a"]}"#;

fn test_registry() -> &'static Arc<DataRegistry> {
    static REGISTRY: OnceLock<Arc<DataRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| DataRegistry::load().expect("data registry required for server tests"))
}

async fn route_request(method: &str, path: &str, body: &str) -> TestResponse {
    route_request_ex(method, path, body, None, &[]).await
}

/// Optimize scenarios need enough roster breadth for legal crews; a user's mutable default profile
/// may not, so use the no-roster synthetic id (full canonical catalog) like replay-seed tests.
async fn route_request_optimize(body: &str) -> TestResponse {
    route_request_ex(
        "POST",
        "/api/optimize",
        body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await
}

async fn route_request_optimize_start(body: &str) -> TestResponse {
    route_request_ex(
        "POST",
        "/api/optimize/start",
        body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await
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
    assert_eq!(p["server"]["cpu_job_permits_total"], 1);
    assert_eq!(p["server"]["cpu_job_permits_available"], 1);
    assert!(p["server"]["cpu_job_queue_wait_ms"].is_null());
    assert_eq!(p["server"]["cpu_job_queue_wait_ms_from_env"], false);
    assert!(p["data"]["officer_count"].as_u64().is_some());
    assert!(p["data"]["hostile_index_loaded"].is_boolean());
    assert!(p["data"]["ship_index_loaded"].is_boolean());
}

#[serial_test::serial]
#[tokio::test]
async fn health_reports_cpu_job_queue_wait_when_env_set() {
    std::env::set_var("KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS", "250");
    let response = route_request("GET", "/api/health", "").await;
    std::env::remove_var("KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS");
    assert_eq!(response.status_code, 200);
    let p: serde_json::Value = serde_json::from_str(&response.body).expect("health json");
    assert_eq!(p["server"]["cpu_job_queue_wait_ms"], 250);
    assert_eq!(p["server"]["cpu_job_queue_wait_ms_from_env"], true);
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
    assert!(yaml.body.contains("openapi: 3.1.0"));
    assert!(yaml.body.contains("/api/simulate:"));
    assert!(yaml
        .body
        .contains("/api/debug/combat-effect-spec/officers/{id}:"));

    let json = route_request("GET", "/api/openapi.json", "").await;
    assert_eq!(json.status_code, 200);
    assert!(
        json.content_type.contains("json"),
        "unexpected content-type: {}",
        json.content_type
    );
    let p: serde_json::Value = serde_json::from_str(&json.body).expect("openapi json");
    assert_eq!(p["openapi"], "3.1.0");
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
    if let Some(rows) = payload["research"].as_array() {
        if let Some(first) = rows.first() {
            assert!(first["combat_kind"].is_string());
        }
    }
}

#[serial_test::serial]
#[tokio::test]
async fn profile_put_rejects_unknown_bonus_key() {
    let profile_id = "profile_validation_unknown_bonus";
    let _ = std::fs::remove_dir_all(profile_data_dir(profile_id));
    let response = route_request(
        "PUT",
        &format!("/api/profile?profile={profile_id}"),
        r#"{"bonuses":{"warp_speed":1.0}}"#,
    )
    .await;
    let _ = std::fs::remove_dir_all(profile_data_dir(profile_id));

    assert_eq!(response.status_code, 400);
    assert!(response.body.contains("not a supported combat stat"));
}

#[serial_test::serial]
#[tokio::test]
async fn profile_put_rejects_invalid_optional_fields() {
    let profile_id = "profile_validation_invalid_options";
    let _ = std::fs::remove_dir_all(profile_data_dir(profile_id));
    let response = route_request(
        "PUT",
        &format!("/api/profile?profile={profile_id}"),
        r#"{"bonuses":{"weapon_damage":0.1},"ops_level":0,"forbidden_tech_override":[123,123]}"#,
    )
    .await;
    let _ = std::fs::remove_dir_all(profile_data_dir(profile_id));

    assert_eq!(response.status_code, 400);
    assert!(response.body.contains("ops_level"));
    assert!(response.body.contains("duplicate id"));
}

#[serial_test::serial]
#[tokio::test]
async fn profile_put_rejects_malformed_json() {
    let response = route_request(
        "PUT",
        "/api/profile?profile=profile_validation_malformed",
        "{",
    )
    .await;
    assert_eq!(response.status_code, 400);
}

#[serial_test::serial]
#[tokio::test]
async fn profile_put_accepts_and_canonicalizes_valid_payload() {
    let profile_id = "profile_validation_valid_payload";
    let _ = std::fs::remove_dir_all(profile_data_dir(profile_id));
    let response = route_request(
        "PUT",
        &format!("/api/profile?profile={profile_id}"),
        r#"{"bonuses":{"weapon_damage":0.1,"armor_pierce":5.0},"support_buffs":["cerritos_support","defiant_reinforce","cerritos_support"],"ops_level":40,"equipped_forbidden_fid":115391048,"equipped_chaos_fid":2766262625}"#,
    )
    .await;
    assert_eq!(response.status_code, 200, "{}", response.body);

    let get_response =
        route_request("GET", &format!("/api/profile?profile={profile_id}"), "").await;
    let _ = std::fs::remove_dir_all(profile_data_dir(profile_id));

    assert_eq!(get_response.status_code, 200, "{}", get_response.body);
    let payload: serde_json::Value =
        serde_json::from_str(&get_response.body).expect("profile json");
    assert_eq!(payload["bonuses"]["weapon_damage"], 0.1);
    assert_eq!(payload["bonuses"]["pierce"], 5.0);
    assert!(payload["bonuses"]["armor_pierce"].is_null());
    assert_eq!(
        payload["support_buffs"],
        serde_json::json!(["cerritos_support", "defiant_reinforce"])
    );
    assert_eq!(payload["ops_level"], 40);
}

#[serial_test::serial]
#[tokio::test]
async fn simulate_rejects_body_over_cpu_json_limit_with_413() {
    const LIMIT: usize = 2 * 1024 * 1024;
    let app = build_router(test_registry().clone());
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
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":7,"max_candidates":64}"#;
    let response = route_request_optimize(body).await;

    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");

    assert_eq!(payload["engine"], "optimizer_v1");
    assert_eq!(payload["scenario"]["ship"], "uss_saladin");
    assert_eq!(payload["scenario"]["hostile"], "2918121098");
    assert_eq!(payload["scenario"]["sims"], 500);
    assert_eq!(payload["scenario"]["seed"], 7);
    assert_eq!(payload["scenario"]["effective_strategy"], "exhaustive");
    assert_eq!(payload["scenario"]["strategy_auto"], true);
    let funnel = payload["scenario"]["optimizer_funnel"]
        .as_object()
        .expect("optimizer_funnel should be an object");
    assert!(
        funnel["generated_candidates"].as_u64().unwrap_or(0) > 0,
        "optimizer_funnel should include generated candidate count"
    );
    assert!(
        funnel["raw_role_pool"]["captains"].as_u64().unwrap_or(0) > 0,
        "optimizer_funnel should include raw captain pool size"
    );
    assert!(
        funnel["banned_role_pool"]["bridge"].as_u64().unwrap_or(0) > 0,
        "optimizer_funnel should include ban-list-filtered bridge pool size"
    );
    assert!(
        funnel["eligible_role_pool"]["bridge"].as_u64().unwrap_or(0) > 0,
        "optimizer_funnel should include eligibility-filtered bridge pool size"
    );
    assert!(
        funnel["roster_role_pool"]["captains"].as_u64().unwrap_or(0) > 0,
        "optimizer_funnel should include roster-filtered captain pool size"
    );
    assert!(
        funnel["final_role_pool"]["below_decks"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "optimizer_funnel should include final below-decks pool size"
    );
    assert!(
        funnel["after_constraints"].as_u64().unwrap_or(0) > 0,
        "optimizer_funnel should include post-constraints count"
    );
    assert!(
        funnel["phase_durations_ms"]["total"].as_u64().unwrap_or(0) > 0,
        "optimizer_funnel should include total phase timing"
    );
    assert!(
        payload["scenario"].get("requested_strategy").is_none()
            || payload["scenario"]["requested_strategy"].is_null()
    );

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
    assert!(
        first["method_provenance"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "recommendations should include method provenance"
    );
    assert_eq!(
        funnel["final_result_count"].as_u64(),
        Some(recommendations.len() as u64)
    );
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
        let wr_lo = recommendation["win_rate_ci_low"]
            .as_f64()
            .unwrap_or(win_rate);
        let wr_hi = recommendation["win_rate_ci_high"]
            .as_f64()
            .unwrap_or(win_rate);
        // Interior rates are clearly combat-backed; 0% / 100% wins are common but Wilson CI span
        // still shows Monte Carlo ran (strict (0,1) misses all-wins / all-losses outcomes).
        if (0.0..1.0).contains(&win_rate)
            || (0.0..1.0).contains(&avg_hull_remaining)
            || (wr_hi - wr_lo) > 1e-6
        {
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
async fn optimize_linear_eval_returns_expected_hull_damage_without_monte_carlo() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":7,"max_candidates":16,"strategy":"linear_eval"}"#;
    let response = route_request_optimize(body).await;

    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");

    assert_eq!(payload["engine"], "linear_eval");
    assert_eq!(payload["scenario"]["effective_strategy"], "linear_eval");
    assert_eq!(payload["scenario"]["strategy_auto"], false);
    assert_eq!(payload["scenario"]["requested_strategy"], "linear_eval");

    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(
        !recommendations.is_empty(),
        "recommendations should not be empty"
    );

    let first = &recommendations[0];
    assert!(
        first["expected_hull_damage"].as_f64().is_some(),
        "linear_eval rows should include expected_hull_damage"
    );
    assert_eq!(first["method_provenance"].as_str(), Some("linear_eval"));
    assert_eq!(first["win_rate"].as_f64(), Some(0.0));

    let approximate = payload["approximate_notes"]
        .as_array()
        .expect("approximate_notes should be present");
    assert!(
        approximate
            .iter()
            .any(|n| n.as_str().unwrap_or("").contains("Linear eval")),
        "approximate_notes should mention linear eval"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_linear_eval_rejects_chain_grind() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","strategy":"linear_eval","chain":{"enabled":true,"kills_target":3}}"#;
    let response = route_request_optimize(body).await;
    assert_eq!(response.status_code, 400);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("validation error json");
    let errors = payload["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| e["field"] == "chain"),
        "chain field should be rejected for linear_eval"
    );
}

/// Auto tiered vs exhaustive must use post-constraint candidate count (not raw generation).
#[serial_test::serial]
#[tokio::test]
async fn optimize_auto_strategy_respects_constraints_on_effective_candidate_count() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":80,"seed":42,"max_candidates":600,"constraints":{"must_include":["___kobayashi_nonexistent_officer___"]}}"#;
    let response = route_request("POST", "/api/optimize", body).await;
    assert_eq!(response.status_code, 200, "body: {}", response.body);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["scenario"]["effective_strategy"], "exhaustive");
    assert_eq!(payload["scenario"]["strategy_auto"], true);
    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(
        recommendations.is_empty(),
        "impossible must_include should yield no crews"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_changes_with_seed() {
    let response_a = route_request_optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":7,"max_candidates":32}"#,
    )
    .await;
    let response_b = route_request_optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":8,"max_candidates":32}"#,
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
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":77,"max_candidates":64}"#;

    let response_a = route_request("POST", "/api/optimize", body).await;
    let response_b = route_request("POST", "/api/optimize", body).await;

    assert_eq!(response_a.status_code, 200);
    assert_eq!(response_b.status_code, 200);

    let payload_a: serde_json::Value =
        serde_json::from_str(&response_a.body).expect("response A should be valid json");
    let payload_b: serde_json::Value =
        serde_json::from_str(&response_b.body).expect("response B should be valid json");
    let mut scenario_a = payload_a["scenario"].clone();
    let mut scenario_b = payload_b["scenario"].clone();
    if let Some(funnel) = scenario_a
        .get_mut("optimizer_funnel")
        .and_then(|v| v.as_object_mut())
    {
        funnel.remove("phase_durations_ms");
    }
    if let Some(funnel) = scenario_b
        .get_mut("optimizer_funnel")
        .and_then(|v| v.as_object_mut())
    {
        funnel.remove("phase_durations_ms");
    }
    assert_eq!(scenario_a, scenario_b);
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

/// An id that does not resolve must be rejected, not simulated.
///
/// Before this was enforced, an unresolvable ship or hostile reached the engine's synthetic
/// fallback — a hash-derived toy fight with ~260–540 defender hull — and came back HTTP 200 with
/// `win_rate: 1.0` and `r1_kill_rate: 1.0`. A typo produced a confident answer about a fight that
/// never existed.
#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_a_ship_id_that_does_not_resolve() {
    // "saladin" is not a ship id; "uss_saladin" is. The near-miss is the realistic typo.
    let response = route_request_optimize(
        r#"{"ship":"saladin","hostile":"2918121098","sims":20,"max_candidates":4}"#,
    )
    .await;

    assert_eq!(response.status_code, 400, "body: {}", response.body);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["status"], "error");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    let ship_messages = errors
        .iter()
        .find(|e| e["field"] == "ship")
        .and_then(|e| e["messages"].as_array())
        .expect("a ship validation error");
    assert!(
        ship_messages
            .iter()
            .any(|m| m.as_str().is_some_and(|m| m.contains("saladin"))),
        "the error should name the offending id: {ship_messages:?}"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_a_hostile_id_that_does_not_resolve() {
    let response = route_request_optimize(
        r#"{"ship":"uss_saladin","hostile":"definitely_not_a_hostile","sims":20,"max_candidates":4}"#,
    )
    .await;

    assert_eq!(response.status_code, 400, "body: {}", response.body);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    let hostile_messages = errors
        .iter()
        .find(|e| e["field"] == "hostile")
        .and_then(|e| e["messages"].as_array())
        .expect("a hostile validation error");
    assert!(
        hostile_messages.iter().any(|m| m
            .as_str()
            .is_some_and(|m| m.contains("definitely_not_a_hostile"))),
        "the error should name the offending id: {hostile_messages:?}"
    );
}

/// A tier the ship does not have resolves to `None` exactly like an unknown id does, so it reaches
/// the same synthetic fallback and needs the same rejection.
#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_a_ship_tier_the_ship_does_not_have() {
    let response = route_request_optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","ship_tier":99,"sims":20,"max_candidates":4}"#,
    )
    .await;

    assert_eq!(response.status_code, 400, "body: {}", response.body);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let messages = payload["errors"]
        .as_array()
        .and_then(|errors| errors.iter().find(|e| e["field"] == "ship"))
        .and_then(|e| e["messages"].as_array())
        .expect("a ship validation error");
    assert!(
        messages
            .iter()
            .any(|m| m.as_str().is_some_and(|m| m.contains("tier"))),
        "the error should say the tier is the problem: {messages:?}"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_accepts_a_scenario_whose_ids_resolve() {
    let response = route_request_optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":20,"seed":7,"max_candidates":8}"#,
    )
    .await;

    assert_eq!(response.status_code, 200, "body: {}", response.body);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert!(payload["recommendations"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty()));
}

#[serial_test::serial]
#[tokio::test]
async fn simulate_endpoint_rejects_ids_that_do_not_resolve() {
    let crew = SERVER_API_TEST_CREW_718_LEGAL_JSON;
    for (label, body) in [
        (
            "ship",
            format!(
                r#"{{"ship":"saladin","hostile":"2918121098","ship_level":{SERVER_API_TEST_SHIP_LEVEL_THREE_BELOW},"crew":{crew},"num_sims":5}}"#
            ),
        ),
        (
            "hostile",
            format!(
                r#"{{"ship":"uss_saladin","hostile":"definitely_not_a_hostile","ship_level":{SERVER_API_TEST_SHIP_LEVEL_THREE_BELOW},"crew":{crew},"num_sims":5}}"#
            ),
        ),
    ] {
        let response = route_request_ex(
            "POST",
            "/api/simulate",
            &body,
            None,
            SERVER_API_TEST_PROFILE_HEADERS,
        )
        .await;
        assert_eq!(
            response.status_code, 400,
            "unresolvable {label} should be rejected, got: {}",
            response.body
        );
    }
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_start_rejects_ids_that_do_not_resolve() {
    let response = route_request_optimize_start(
        r#"{"ship":"saladin","hostile":"2918121098","sims":20,"max_candidates":4}"#,
    )
    .await;
    assert_eq!(response.status_code, 400, "body: {}", response.body);
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_random_stratified_strategy_labels_all_rows() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":300,"seed":9,"max_candidates":24,"strategy":"random_stratified"}"#;
    let response = route_request_optimize(body).await;

    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");

    assert_eq!(payload["engine"], "random_stratified");
    assert_eq!(
        payload["scenario"]["effective_strategy"],
        "random_stratified"
    );
    assert_eq!(payload["scenario"]["strategy_auto"], false);
    assert_eq!(
        payload["scenario"]["requested_strategy"],
        "random_stratified"
    );

    let funnel = payload["scenario"]["optimizer_funnel"]
        .as_object()
        .expect("optimizer_funnel should be an object");
    let random_candidates = funnel["random_exploration_candidates"]
        .as_u64()
        .expect("funnel should report random_exploration_candidates");
    assert!(
        random_candidates > 0 && random_candidates <= 24,
        "random candidate count should be positive and capped by max_candidates, got {random_candidates}"
    );

    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(!recommendations.is_empty());
    for recommendation in recommendations {
        assert_eq!(
            recommendation["method_provenance"].as_str(),
            Some("random_stratified"),
            "every random_stratified row should carry the lane's provenance label"
        );
    }
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_random_stratified_is_deterministic_for_fixed_seed() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":200,"seed":21,"max_candidates":12,"strategy":"random_stratified"}"#;
    let first = route_request_optimize(body).await;
    let second = route_request_optimize(body).await;
    assert_eq!(first.status_code, 200);
    assert_eq!(second.status_code, 200);
    let a: serde_json::Value = serde_json::from_str(&first.body).expect("json");
    let b: serde_json::Value = serde_json::from_str(&second.body).expect("json");
    assert_eq!(
        a["recommendations"], b["recommendations"],
        "same seed should reproduce the same random-lane recommendations"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_tiered_random_exploration_slice_reports_funnel() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":300,"seed":5,"max_candidates":64,"strategy":"tiered","tiered_scout_sims":50,"tiered_top_k":4,"tiered_random_exploration_pct":0.25}"#;
    let response = route_request_optimize(body).await;

    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["scenario"]["effective_strategy"], "tiered");

    let funnel = payload["scenario"]["optimizer_funnel"]
        .as_object()
        .expect("optimizer_funnel should be an object");
    let injected = funnel["random_exploration_candidates"]
        .as_u64()
        .expect("funnel should report random_exploration_candidates for the slice");
    assert!(
        injected > 0,
        "exploration slice should inject at least one random crew"
    );
    let scout = funnel["scout_candidates"].as_u64().unwrap_or(0);
    assert!(
        injected <= scout.div_ceil(2).max(1),
        "budget-neutral slice: injected ({injected}) stays within half the scout set ({scout})"
    );

    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    for recommendation in recommendations {
        let provenance = recommendation["method_provenance"].as_str().unwrap_or("");
        assert!(
            !provenance.is_empty(),
            "tiered rows keep a provenance label with the slice active"
        );
    }
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_tiered_local_refinement_labels_refined_rows() {
    // Hostile 1127817100 (level 31) on purpose: refinement can only accept a neighbour that
    // measurably improves on a finalist, and this test profile carries no research or building
    // buffs, so most matchups collapse to every crew winning at score 1.0 or every crew losing at
    // 0.0 — neither leaves anything to improve. At level 31 this ship wins with real spread
    // (~60 distinct scores across the legal crews), which is what gives the pass headroom.
    let body = r#"{"ship":"uss_saladin","hostile":"1127817100","sims":150,"seed":11,"max_candidates":24,"strategy":"tiered","tiered_scout_sims":30,"tiered_top_k":3,"local_refinement":true}"#;
    let response = route_request_optimize(body).await;

    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["scenario"]["effective_strategy"], "tiered");

    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(!recommendations.is_empty());

    const REFINEMENT_LABELS: &[&str] = &[
        "local_swap",
        "local_captain_swap",
        "large_neighborhood_repair",
    ];
    let mut saw_refinement_label = false;
    for recommendation in recommendations {
        let provenance = recommendation["method_provenance"].as_str().unwrap_or("");
        assert!(
            !provenance.is_empty(),
            "every row should carry a non-empty method_provenance label"
        );
        if REFINEMENT_LABELS.contains(&provenance) {
            saw_refinement_label = true;
        }
    }
    // Deterministic for this fixed seed/body (SplitMix64, same seed -> same fight outcomes):
    // the refinement pass finds and confirms at least one improving neighbor, so at least one
    // row must carry one of the three refinement labels rather than falling back to
    // "tiered_confirmed" — this is what actually exercises the hash-agreement between
    // `ranked_crew_hash` and the refinement module's canonical crew hash.
    assert!(
        saw_refinement_label,
        "expected at least one row labeled with a local-refinement method, got: {:?}",
        recommendations
            .iter()
            .map(|r| r["method_provenance"].as_str().unwrap_or(""))
            .collect::<Vec<_>>()
    );
}

const REFINEMENT_METHOD_LABELS: &[&str] = &[
    "local_swap",
    "local_captain_swap",
    "large_neighborhood_repair",
];

/// Assert every refined row's `refinement` detail is self-consistent, and return how many carried
/// one. Shared by the tiered and genetic exposure tests so both hold the payload to one contract.
fn assert_refinement_details_are_coherent(recommendations: &[serde_json::Value]) -> usize {
    let mut refined = 0usize;
    for row in recommendations {
        let label = row["method_provenance"].as_str().unwrap_or("");
        let is_refined = REFINEMENT_METHOD_LABELS.contains(&label);
        let detail = row.get("refinement");
        if !is_refined {
            assert!(
                detail.is_none(),
                "row labeled {label} should not carry a refinement detail: {row}"
            );
            continue;
        }
        refined += 1;
        let detail = detail.unwrap_or_else(|| panic!("row labeled {label} needs a detail: {row}"));
        assert_eq!(
            detail["kind"].as_str(),
            Some(label),
            "detail kind must agree with method_provenance: {row}"
        );

        let baseline = detail["baseline_score"].as_f64().expect("baseline_score");
        let refined_score = detail["refined_score"].as_f64().expect("refined_score");
        let gain = detail["gain"].as_f64().expect("gain");
        assert!(
            (gain - (refined_score - baseline)).abs() < 1e-9,
            "gain must be refined_score - baseline_score: {row}"
        );
        // Refinement only keeps neighbors that beat the source at confirmation depth, so a
        // non-positive gain would mean an accepted move that measured no improvement.
        assert!(gain > 0.0, "accepted refinement must gain score: {row}");

        let changed = detail["changed_slots"]
            .as_array()
            .expect("changed_slots array");
        assert!(
            !changed.is_empty(),
            "a refined row differs from its source in at least one seat: {row}"
        );
        // The kind is derived from the diff, so the two must agree about how much moved.
        match label {
            "large_neighborhood_repair" => assert!(
                changed.len() >= 2,
                "destroy-repair changes two or more seats: {row}"
            ),
            _ => assert_eq!(changed.len(), 1, "single-slot kinds change one seat: {row}"),
        }
        for change in changed {
            let slot = change["slot"].as_str().expect("slot label");
            assert!(
                matches!(slot, "captain" | "bridge" | "below_decks"),
                "unexpected slot group {slot}: {row}"
            );
            assert_eq!(
                change.get("index").is_some(),
                slot != "captain",
                "index is present for seat groups and absent for the captain: {change}"
            );
            let from = change["from"].as_str().expect("from");
            let to = change["to"].as_str().expect("to");
            assert_ne!(from, to, "a changed seat must change officer: {change}");
            if label == "local_captain_swap" {
                assert_eq!(
                    slot, "captain",
                    "captain-swap kind changes the captain: {row}"
                );
            }
        }
        // The source crew must be a different crew, not a reordering of this one.
        let source = (
            detail["source_captain"].as_str().unwrap_or(""),
            sorted_names(&detail["source_bridge"]),
            sorted_names(&detail["source_below_decks"]),
        );
        let row_crew = (
            row["captain"].as_str().unwrap_or(""),
            sorted_names(&row["bridge"]),
            sorted_names(&row["below_decks"]),
        );
        assert_ne!(
            source, row_crew,
            "refined row must differ from its source crew: {row}"
        );
    }
    refined
}

fn sorted_names(value: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = value
        .as_array()
        .map(|a| {
            a.iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_tiered_local_refinement_exposes_provenance_detail() {
    // Same scenario as `optimize_tiered_local_refinement_labels_refined_rows`, which pins that this
    // seed produces at least one refined row; this asserts what that row now reports about itself.
    // Hostile 1127817100 (level 31) on purpose: refinement can only accept a neighbour that
    // measurably improves on a finalist, and this test profile carries no research or building
    // buffs, so most matchups collapse to every crew winning at score 1.0 or every crew losing at
    // 0.0 — neither leaves anything to improve. At level 31 this ship wins with real spread
    // (~60 distinct scores across the legal crews), which is what gives the pass headroom.
    let body = r#"{"ship":"uss_saladin","hostile":"1127817100","sims":150,"seed":11,"max_candidates":24,"strategy":"tiered","tiered_scout_sims":30,"tiered_top_k":3,"local_refinement":true}"#;
    let response = route_request_optimize(body).await;
    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");

    let refined = assert_refinement_details_are_coherent(recommendations);
    assert!(
        refined > 0,
        "expected at least one refined row to expose a refinement detail, got: {:?}",
        recommendations
            .iter()
            .map(|r| r["method_provenance"].as_str().unwrap_or(""))
            .collect::<Vec<_>>()
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_genetic_accepts_local_refinement() {
    // Refinement was tiered-only when it shipped; genetic produces the same kind of ranked finalist
    // list, so the pass applies there too. This asserts the endpoint accepts the flag on the
    // genetic lane and that anything it contributes is coherent. That the pass *runs* is asserted
    // in `tests/local_refinement_lanes.rs`, which can read the pass's own stats — over HTTP an
    // accepted improvement is not guaranteed, because the genetic lane's finalists reach a perfect
    // score against the bundled catalog and leave no headroom to climb into.
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":150,"seed":11,"max_candidates":24,"strategy":"genetic","local_refinement":true}"#;
    let response = route_request_optimize(body).await;
    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["scenario"]["effective_strategy"], "genetic");
    let recommendations = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(!recommendations.is_empty());

    assert_refinement_details_are_coherent(recommendations);
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_recommendations_report_trials_run() {
    // Exhaustive Monte Carlo runs the requested depth on every crew, so `trials_run` pins to
    // `sims`. This is the reading that gives the field meaning elsewhere: it is the count of trials
    // actually run, not an echo of the request.
    let exhaustive = route_request_optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":150,"seed":5,"max_candidates":24,"strategy":"exhaustive"}"#,
    )
    .await;
    assert_eq!(exhaustive.status_code, 200);
    let payload: serde_json::Value =
        serde_json::from_str(&exhaustive.body).expect("response should be valid json");
    let rows = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(!rows.is_empty());
    for row in rows {
        assert_eq!(
            row["trials_run"].as_u64(),
            Some(150),
            "exhaustive runs the requested depth on every crew: {row}"
        );
    }

    // Tiered allocates confirmation depth adaptively, so its rows report whatever budget they
    // actually received rather than the requested `sims`.
    let tiered = route_request_optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":200,"seed":5,"max_candidates":24,"strategy":"tiered","tiered_scout_sims":25,"tiered_top_k":2}"#,
    )
    .await;
    assert_eq!(tiered.status_code, 200);
    let payload: serde_json::Value =
        serde_json::from_str(&tiered.body).expect("response should be valid json");
    let rows = payload["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(!rows.is_empty());
    for row in rows {
        let trials = row["trials_run"]
            .as_u64()
            .unwrap_or_else(|| panic!("every row reports trials_run: {row}"));
        assert!(trials > 0, "a simulated row ran trials: {row}");
    }
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_estimate_scales_with_chain_kills_target() {
    // Sized so both estimates land inside the 0.1s–1h display clamp: below the floor every
    // scenario reports 0.1s and the scaling would be invisible.
    let base_path =
        "/api/optimize/estimate?ship=saladin&hostile=2918121098&sims=100000&max_candidates=20000";
    let single = route_request("GET", base_path, "").await;
    assert_eq!(single.status_code, 200);
    let single: serde_json::Value = serde_json::from_str(&single.body).expect("json");
    assert!(
        single.get("chain_kills_target").is_none(),
        "single-fight estimates stay silent about chain: {single}"
    );

    let chained = route_request("GET", &format!("{base_path}&chain_kills_target=5"), "").await;
    assert_eq!(chained.status_code, 200);
    let chained: serde_json::Value = serde_json::from_str(&chained.body).expect("json");
    assert_eq!(chained["chain_kills_target"], 5);
    assert_eq!(chained["chain_fights_per_trial_upper_bound"], 5);
    assert_eq!(
        chained["estimated_candidates"], single["estimated_candidates"],
        "chain changes cost per trial, not the candidate count"
    );

    let single_seconds = single["estimated_seconds"].as_f64().expect("seconds");
    let chained_seconds = chained["estimated_seconds"].as_f64().expect("seconds");
    assert!(
        single_seconds > 0.1 && chained_seconds < 3600.0,
        "both estimates must be inside the display clamp for the ratio below to mean anything \
         ({single_seconds}, {chained_seconds})"
    );
    // Five fights per trial instead of one. Tolerance covers the 0.1s rounding on both sides only.
    assert!(
        (chained_seconds - single_seconds * 5.0).abs() < 0.3,
        "a 5-kill chain should estimate ~5x one fight ({chained_seconds} vs {single_seconds})"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_out_of_range_local_refinement_seeds() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":1000,"strategy":"tiered","local_refinement":true,"local_refinement_seeds":99}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(errors
        .iter()
        .any(|error| error["field"] == "local_refinement_seeds"));
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_out_of_range_random_exploration_pct() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":1000,"tiered_random_exploration_pct":0.9}"#,
    )
    .await;

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(errors
        .iter()
        .any(|error| error["field"] == "tiered_random_exploration_pct"));
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_rejects_zero_sims() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":0}"#,
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
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":5000000}"#,
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
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":1000,"max_candidates":3000000}"#,
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
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":100,"analytical_prefilter_keep":0}"#,
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
async fn optimize_endpoint_rejects_zero_novelty_lambda() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":100,"novelty_lambda":0}"#,
    )
    .await;
    assert_eq!(response.status_code, 400);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(
        errors.iter().any(|e| e["field"] == "novelty_lambda"),
        "expected novelty_lambda validation error"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_requires_novelty_lambda_when_novelty_pool_set() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":100,"novelty_pool":120}"#,
    )
    .await;
    assert_eq!(response.status_code, 400);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(
        errors.iter().any(|e| e["field"] == "novelty_lambda"),
        "expected novelty_lambda required when novelty_pool set"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_requires_novelty_lambda_when_novelty_history_anchors_true() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":100,"novelty_history_anchors":true}"#,
    )
    .await;
    assert_eq!(response.status_code, 400);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(
        errors.iter().any(|e| e["field"] == "novelty_lambda"),
        "expected novelty_lambda required when novelty_history_anchors is true"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_endpoint_reports_analytical_prefilter_when_truncating() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":1,"max_candidates":80,"analytical_prefilter_keep":4}"#;
    let response = route_request_optimize(body).await;
    assert_eq!(response.status_code, 200, "body: {}", response.body);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["scenario"]["analytical_prefilter_keep"], 4);
    assert_eq!(payload["scenario"]["analytical_prefilter_kept"], 4);
    assert_eq!(
        payload["scenario"]["optimizer_funnel"]["analytical_prefilter_kept"],
        4
    );
    assert!(
        payload["scenario"]["analytical_prefilter_from"]
            .as_u64()
            .unwrap_or(0)
            > 4
    );
    assert!(
        payload["scenario"]["optimizer_funnel"]["analytical_prefilter_from"]
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
async fn optimize_rejects_fast_discovery_without_heuristic_seeds() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":100,"seed":1,"max_candidates":32,"strategy":"tiered","fast_discovery":true}"#;
    let response = route_request("POST", "/api/optimize", body).await;
    assert_eq!(response.status_code, 400, "{}", response.body);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let fields: Vec<&str> = payload["errors"]
        .as_array()
        .expect("errors array")
        .iter()
        .filter_map(|e| e["field"].as_str())
        .collect();
    assert!(
        fields.contains(&"fast_discovery"),
        "expected fast_discovery validation issue: {:?}",
        payload
    );
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_fast_discovery_echoes_in_scenario_and_notes() {
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":400,"seed":3,"max_candidates":48,"strategy":"tiered","heuristics_seeds":["heuristics-seed"],"fast_discovery":true}"#;
    let response = route_request_optimize(body).await;
    assert_eq!(response.status_code, 200, "{}", response.body);
    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    assert_eq!(payload["scenario"]["fast_discovery"], true);
    let notes = payload["notes"].as_array().expect("notes array");
    assert!(
        notes.iter().any(|n| {
            n.as_str()
                .is_some_and(|s| s.contains("Fast discovery") && s.contains("warm-start"))
        }),
        "expected fast discovery note: {:?}",
        notes
    );
    let approx = payload["approximate_notes"]
        .as_array()
        .expect("approximate_notes array");
    assert!(
        approx
            .iter()
            .any(|n| n.as_str().is_some_and(|s| s.contains("fast_discovery"))),
        "expected fast_discovery approximate note: {:?}",
        approx
    );
}

#[serial_test::serial]
#[tokio::test]
async fn async_optimize_start_poll_completes_with_recommendations() {
    let body =
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":42,"max_candidates":16}"#;
    let start = route_request_optimize_start(body).await;
    assert_eq!(start.status_code, 200, "body: {}", start.body);
    let payload: serde_json::Value =
        serde_json::from_str(&start.body).expect("start response json");
    let job_id = payload["job_id"].as_str().expect("job_id string");

    for _ in 0..200 {
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
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":200,"seed":1,"max_candidates":8}"#;
    let start = route_request_optimize_start(body).await;
    assert_eq!(start.status_code, 200);
    let payload: serde_json::Value = serde_json::from_str(&start.body).expect("start json");
    let job_id = payload["job_id"].as_str().expect("job_id");

    let mut finished = false;
    for _ in 0..200 {
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
    let body = format!(
        r#"{{"ship":"uss_saladin","hostile":"2918121098","ship_level":{lvl},"seed":77,"sim_index":12,"max_trace_events":50,"crew":{crew}}}"#,
        lvl = SERVER_API_TEST_SHIP_LEVEL_THREE_BELOW,
        crew = SERVER_API_TEST_CREW_718_LEGAL_JSON
    );
    let a = route_request_ex(
        "POST",
        "/api/optimize/replay-seed",
        &body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
    let b = route_request_ex(
        "POST",
        "/api/optimize/replay-seed",
        &body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
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
async fn optimize_replay_seed_trace_reports_applied_support_buffs() {
    let body = format!(
        r#"{{"ship":"uss_saladin","hostile":"2918121098","ship_level":{lvl},"seed":77,"sim_index":12,"max_trace_events":1,"crew":{crew},"support_buffs":["cerritos_support","not_a_real_support_buff_id"]}}"#,
        lvl = SERVER_API_TEST_SHIP_LEVEL_THREE_BELOW,
        crew = SERVER_API_TEST_CREW_718_LEGAL_JSON
    );
    let response = route_request_ex(
        "POST",
        "/api/optimize/replay-seed",
        &body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
    assert_eq!(response.status_code, 200, "{}", response.body);

    let p: serde_json::Value = serde_json::from_str(&response.body).expect("replay json");
    let support = &p["trace"]["external_buffs"]["support_buffs"];
    assert_eq!(
        support["resolved_ids"]
            .as_array()
            .expect("resolved support ids"),
        &[serde_json::json!("cerritos_support")]
    );
    assert_eq!(
        support["unknown_ids"]
            .as_array()
            .expect("unknown support ids"),
        &[serde_json::json!("not_a_real_support_buff_id")]
    );
    let aggregate_weapon_damage = support["aggregate_static_bonuses"]["weapon_damage"]
        .as_f64()
        .expect("weapon_damage aggregate");
    assert!(
        aggregate_weapon_damage >= 1.25,
        "expected Cerritos static weapon_damage plus any support-gated research, got {aggregate_weapon_damage}"
    );

    let applied = support["applied"]
        .as_array()
        .expect("applied support buffs");
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["id"], "cerritos_support");
    assert_eq!(applied[0]["display_name"], "Cerritos Support");
    assert_eq!(applied[0]["static_bonuses"]["weapon_damage"], 1.25);
    assert!(
        p["warnings"].as_array().expect("warnings").iter().any(|w| w
            .as_str()
            .is_some_and(|s| s.contains("not_a_real_support_buff_id"))),
        "expected unknown support buff warning"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn compare_crews_returns_distribution_payload() {
    let crew = SERVER_API_TEST_CREW_718_LEGAL_JSON;
    let body = format!(
        r#"{{"ship":"uss_saladin","hostile":"2918121098","num_sims":400,"seed":3,"below_decks_slots":3,"crews":[{crew},{crew}]}}"#,
        crew = crew
    );
    let response = route_request_ex(
        "POST",
        "/api/compare/crews",
        &body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
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
async fn simulate_unknown_support_buff_emits_warning() {
    let body = format!(
        r#"{{"ship":"uss_saladin","hostile":"2918121098","num_sims":100,"seed":1,"below_decks_slots":3,"crew":{crew},"support_buffs":["not_a_real_support_buff_id"]}}"#,
        crew = SERVER_API_TEST_CREW_718_LEGAL_JSON
    );
    let response = route_request_ex(
        "POST",
        "/api/simulate",
        &body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
    assert_eq!(response.status_code, 200, "{}", response.body);
    let p: serde_json::Value = serde_json::from_str(&response.body).expect("simulate json");
    let w = p["warnings"].as_array().expect("warnings array");
    assert!(
        w.iter().any(|x| {
            x.as_str()
                .is_some_and(|s| s.contains("not_a_real_support_buff_id"))
        }),
        "expected unknown support_buff warning, got {:?}",
        w
    );
}

#[serial_test::serial]
#[tokio::test]
async fn simulate_support_buff_request_succeeds_without_warnings() {
    let with_buff = format!(
        r#"{{"ship":"uss_saladin","hostile":"2918121098","num_sims":800,"seed":9001,"below_decks_slots":3,"crew":{crew},"support_buffs":["cerritos_support"]}}"#,
        crew = SERVER_API_TEST_CREW_718_LEGAL_JSON
    );
    let response = route_request_ex(
        "POST",
        "/api/simulate",
        &with_buff,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
    assert_eq!(response.status_code, 200, "{}", response.body);
    let payload: serde_json::Value = serde_json::from_str(&response.body).expect("simulate json");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["stats"]["n"], 800);
    let warnings = payload
        .get("warnings")
        .and_then(|warnings| warnings.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings.iter().all(|warning| warning
            .as_str()
            .is_none_or(|message| !message.contains("support_buff"))),
        "known support buff should not emit support-buff warnings: {}",
        response.body
    );
}

#[serial_test::serial]
#[tokio::test]
async fn compare_crews_accepts_support_buffs() {
    let crew = SERVER_API_TEST_CREW_718_LEGAL_JSON;
    let body = format!(
        r#"{{"ship":"uss_saladin","hostile":"2918121098","num_sims":200,"seed":5,"below_decks_slots":3,"support_buffs":["cerritos_support"],"crews":[{crew},{crew}]}}"#,
        crew = crew
    );
    let response = route_request_ex(
        "POST",
        "/api/compare/crews",
        &body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
    assert_eq!(response.status_code, 200, "{}", response.body);
}

#[serial_test::serial]
#[tokio::test]
async fn api_key_required_for_non_loopback_when_configured() {
    std::env::set_var("KOBAYASHI_API_KEY", "unit-test-secret");
    std::env::set_var("KOBAYASHI_API_KEY_TRUST_LOOPBACK", "false");
    let body =
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":7,"max_candidates":64}"#;
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
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":500,"seed":7,"max_candidates":64}"#;
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

struct CombatEffectSpecDebugEnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl CombatEffectSpecDebugEnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for CombatEffectSpecDebugEnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            None => std::env::remove_var(self.key),
            Some(v) => std::env::set_var(self.key, v),
        }
    }
}

#[serial_test::serial]
#[tokio::test]
async fn combat_effect_spec_debug_respects_env_gates() {
    let path = "/api/debug/combat-effect-spec/officers/718-0-2509d7";
    let disabled = route_request("GET", path, "").await;
    assert_eq!(disabled.status_code, 404);
    assert!(
        disabled.body.contains("KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG"),
        "{}",
        disabled.body
    );

    // One test: `DataRegistry::load()` reads `KOBAYASHI_OFFICER_SOURCE` at load time; splitting this
    // across two tests left the "enabled" case flaky when the sibling ran first in the same binary.
    let _g_debug = CombatEffectSpecDebugEnvGuard::set("KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG", "1");
    let _g_lcars = CombatEffectSpecDebugEnvGuard::set("KOBAYASHI_OFFICER_SOURCE", "lcars");
    let enabled = route_request("GET", path, "").await;
    assert_eq!(enabled.status_code, 200, "{}", enabled.body);
    assert!(
        enabled.content_type.contains("json"),
        "{}",
        enabled.content_type
    );
    let v: serde_json::Value = serde_json::from_str(&enabled.body).expect("json");
    assert_eq!(v["officer_id"], "718-0-2509d7");
    assert!(v["abilities"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(v["combat_effect_spec_enabled"].is_boolean());
}

#[serial_test::serial]
#[tokio::test]
async fn simulate_partial_crew_captain_only_is_allowed() {
    // Partial crew (captain only, null bridge slots) should succeed — legality check must allow
    // empty/unset officer slots. This is the exact scenario exercised by the E2E workspace test.
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","num_sims":50,"seed":1,"ship_tier":1,"ship_level":50,"crew":{"captain":"annorax-830d35","bridge":[null,null],"below_deck":[]}}"#;
    let response = route_request_ex(
        "POST",
        "/api/simulate",
        body,
        None,
        SERVER_API_TEST_PROFILE_HEADERS,
    )
    .await;
    assert_eq!(
        response.status_code, 200,
        "partial crew should succeed: {}",
        response.body
    );
    let payload: serde_json::Value = serde_json::from_str(&response.body).expect("json");
    assert_eq!(payload["status"], "ok");
}

#[serial_test::serial]
#[tokio::test]
async fn simulate_partial_crew_with_demo_profile() {
    // Same as above but explicitly using the bundled demo profile. Omitting the header would make
    // this test depend on the user's mutable profiles/index.json default.
    let body = r#"{"ship":"uss_saladin","hostile":"2918121098","num_sims":50,"seed":1,"ship_tier":1,"ship_level":50,"crew":{"captain":"annorax-830d35","bridge":[null,null],"below_deck":[]}}"#;
    let response = route_request_ex(
        "POST",
        "/api/simulate",
        body,
        None,
        &[("x-profile-id", DEMO_PROFILE_ID)],
    )
    .await;
    assert_eq!(
        response.status_code, 200,
        "demo profile partial crew should succeed: {}",
        response.body
    );
    let payload: serde_json::Value = serde_json::from_str(&response.body).expect("json");
    assert_eq!(payload["status"], "ok");
}
