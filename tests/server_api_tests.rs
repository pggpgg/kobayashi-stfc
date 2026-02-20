use kobayashi::server::routes::route_request;

#[test]
fn health_endpoint_returns_ok_json() {
    let response = route_request("GET", "/api/health", "");
    assert_eq!(response.status_code, 200);
    assert_eq!(response.content_type, "application/json");
    assert!(response.body.contains("\"status\": \"ok\""));
}

#[test]
fn optimize_endpoint_returns_ranked_recommendations() {
    let body = r#"{"ship":"saladin","hostile":"explorer_30","sims":2000,"seed":7}"#;
    let response = route_request("POST", "/api/optimize", body);

    assert_eq!(response.status_code, 200);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");

    assert_eq!(payload["engine"], "optimizer_v1");
    assert_eq!(payload["scenario"]["ship"], "saladin");
    assert_eq!(payload["scenario"]["hostile"], "explorer_30");
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
    assert!(first["bridge"].as_str().is_some());
    assert!(first["below_decks"].as_str().is_some());
    assert!(first["win_rate"].as_f64().is_some());
    assert!(first["avg_hull_remaining"].as_f64().is_some());

    let mut prior_score: Option<f64> = None;
    for recommendation in recommendations {
        let score = recommendation["win_rate"].as_f64().unwrap_or(0.0) * 0.8
            + recommendation["avg_hull_remaining"].as_f64().unwrap_or(0.0) * 0.2;
        if let Some(previous) = prior_score {
            assert!(
                previous >= score,
                "recommendations should be ranked by descending score"
            );
        }
        prior_score = Some(score);
    }
}

#[test]
fn optimize_endpoint_is_deterministic_for_fixed_seed() {
    let body = r#"{"ship":"saladin","hostile":"explorer_30","sims":2000,"seed":77}"#;

    let response_a = route_request("POST", "/api/optimize", body);
    let response_b = route_request("POST", "/api/optimize", body);

    assert_eq!(response_a.status_code, 200);
    assert_eq!(response_b.status_code, 200);
    assert_eq!(response_a.body, response_b.body);
}

#[test]
fn optimize_endpoint_rejects_invalid_payload() {
    let response = route_request("POST", "/api/optimize", "{bad json}");
    assert_eq!(response.status_code, 400);
    assert!(response.body.contains("Invalid request body"));
}
