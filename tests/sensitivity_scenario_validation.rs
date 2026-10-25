use axum::body::{to_bytes, Body};
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::server::api::{
    sensitivity_morris_payload, sensitivity_payload, sensitivity_sobol_payload,
};
use kobayashi::server::routes::build_router;
use tower::ServiceExt;

#[test]
fn synchronous_sensitivity_methods_reject_unresolved_scenario_ids() {
    let registry = DataRegistry::load().expect("load registry");
    let body = r#"{"ship":"missing-ship-for-sensitivity-test","hostile":"missing-hostile-for-sensitivity-test","bridge":[],"num_sims":2,"r_trajectories":2,"n_samples":8,"rounds":1,"deltas":{"weapon_damage":0.0}}"#;

    assert!(sensitivity_payload(&registry, body, None).is_err());
    assert!(sensitivity_morris_payload(&registry, body, None).is_err());
    assert!(sensitivity_sobol_payload(&registry, body, None).is_err());
}

#[tokio::test]
async fn async_sensitivity_start_rejects_unresolved_scenario_before_enqueue() {
    let app = build_router(DataRegistry::load().expect("load registry"));
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/api/sensitivity/start")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"ship":"missing-ship-for-sensitivity-test","hostile":"missing-hostile-for-sensitivity-test","bridge":[],"num_sims":2,"rounds":1,"deltas":{"weapon_damage":0.0}}"#,
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:12345".parse::<std::net::SocketAddr>().unwrap(),
    ));

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = String::from_utf8_lossy(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
        .into_owned();
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "invalid sensitivity scenario must be rejected before a job is enqueued: {content_type} {body}"
    );
}
