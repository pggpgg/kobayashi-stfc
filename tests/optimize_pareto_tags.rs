//! Pareto tags and recommendation reasons on the optimize response (roadmap §1.2).
//!
//! Asserted over HTTP because the point is the *contract*: what a client can rely on across a whole
//! result set — that named views are unique, that reasons accompany tags, and above all that
//! tagging never reorders the table. Which crew wins each view depends on the bundled catalog, so
//! these check structural invariants rather than officer names.

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;
use kobayashi::optimizer::pareto::PARETO_MAX_ROWS_CONSIDERED;
use kobayashi::server::routes::build_router;
use serde_json::Value;
use std::net::SocketAddr;
use tower::ServiceExt;

/// Views that describe a single best row; `pareto_optimal` is front membership and may repeat.
const SINGLETON_TAGS: &[&str] = &["safest", "fastest_farming", "best_chain", "most_different"];

async fn optimize(body: &str) -> Value {
    let registry = DataRegistry::load().expect("data registry required for server tests");
    let app = build_router(registry);
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/optimize")
        .header("content-type", "application/json")
        .header("x-profile-id", NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)
        .body(Body::from(body.to_string()))
        .expect("request");
    let addr: SocketAddr = "127.0.0.1:12345".parse().expect("loopback");
    let response = app
        .oneshot({
            let mut r = request;
            r.extensions_mut().insert(ConnectInfo(addr));
            r
        })
        .await
        .expect("router response");
    assert_eq!(response.status(), 200, "optimize should succeed");
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("optimize response json")
}

fn recommendations(payload: &Value) -> &Vec<Value> {
    payload["recommendations"]
        .as_array()
        .expect("recommendations array")
}

fn tags(row: &Value) -> Vec<String> {
    row.get("pareto_tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|t| t.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[serial_test::serial]
#[tokio::test]
async fn optimize_tags_a_front_and_explains_every_tagged_row() {
    let payload = optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":80,"seed":7,"max_candidates":60}"#,
    )
    .await;
    let rows = recommendations(&payload);
    assert!(rows.len() > 1, "need a set to compare rows within");

    let tagged: Vec<&Value> = rows.iter().filter(|r| !tags(r).is_empty()).collect();
    assert!(
        !tagged.is_empty(),
        "a multi-row Monte Carlo result set must put at least one crew on the front"
    );

    for row in &tagged {
        let reason = row
            .get("recommendation_reason")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(
            !reason.trim().is_empty(),
            "a tagged row must say why it is tagged: {row}"
        );
    }
    for row in rows.iter().filter(|r| tags(r).is_empty()) {
        assert!(
            row.get("recommendation_reason").is_none(),
            "an untagged row must not carry a reason: {row}"
        );
    }

    for name in SINGLETON_TAGS {
        let count = rows
            .iter()
            .filter(|r| tags(r).iter().any(|t| t == name))
            .count();
        assert!(
            count <= 1,
            "{name} must name at most one crew, found {count}"
        );
    }

    for row in rows.iter().skip(PARETO_MAX_ROWS_CONSIDERED) {
        assert!(
            tags(row).is_empty(),
            "rows past the considered head must stay untagged: {row}"
        );
    }
}

/// Tags annotate; they must never reorder. Same seed, same order, with or without reading them.
#[serial_test::serial]
#[tokio::test]
async fn tagging_leaves_the_ranking_order_untouched() {
    let body =
        r#"{"ship":"uss_saladin","hostile":"2918121098","sims":80,"seed":7,"max_candidates":60}"#;
    let rows = recommendations(&optimize(body).await).clone();

    let crew_of = |row: &Value| {
        format!(
            "{}|{}|{}",
            row["captain"], row["bridge"], row["below_decks"]
        )
    };
    let order: Vec<String> = rows.iter().map(crew_of).collect();
    let repeat = recommendations(&optimize(body).await).clone();
    let repeat_order: Vec<String> = repeat.iter().map(crew_of).collect();
    assert_eq!(order, repeat_order, "same seed must give the same order");

    // The front is not the head of the table: rows are still sorted by the scalar score, so a
    // tagged row is free to sit below an untagged one.
    let mut previous = f64::INFINITY;
    for row in &rows {
        let win = row["win_rate"].as_f64().expect("win_rate");
        let hull = row["avg_hull_remaining"].as_f64().expect("hull");
        let score = win * 0.8 + hull * 0.2;
        assert!(
            score <= previous + 1e-6,
            "rows must stay in descending ranking-score order"
        );
        previous = score;
    }
}

/// Closed-form rows have no simulated rates to trade off, so the pass declines the whole run.
#[serial_test::serial]
#[tokio::test]
async fn linear_eval_rows_carry_no_tags() {
    let payload = optimize(
        r#"{"ship":"uss_saladin","hostile":"2918121098","strategy":"linear_eval","max_candidates":40}"#,
    )
    .await;
    let rows = recommendations(&payload);
    assert!(!rows.is_empty(), "linear eval should still rank crews");
    for row in rows {
        assert!(
            tags(row).is_empty(),
            "linear eval row must be untagged: {row}"
        );
        assert!(row.get("recommendation_reason").is_none());
    }
}
