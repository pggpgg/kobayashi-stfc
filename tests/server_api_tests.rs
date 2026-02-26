use kobayashi::server::routes::route_request;

#[test]
fn health_endpoint_returns_ok_json() {
    let response = route_request("GET", "/api/health", "", None);
    assert_eq!(response.status_code, 200);
    assert_eq!(response.content_type, "application/json");
    assert!(response.body.contains("\"status\": \"ok\""));
}

#[test]
fn optimize_endpoint_returns_ranked_recommendations() {
    let body = r#"{"ship":"saladin","hostile":"explorer_30","sims":2000,"seed":7,"max_candidates":64}"#;
    let response = route_request("POST", "/api/optimize", body, None);

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
    assert!(first["bridge"].as_array().is_some(), "bridge should be an array");
    assert!(first["below_decks"].as_array().is_some(), "below_decks should be an array");
    assert!(first["win_rate"].as_f64().is_some());
    assert!(first["avg_hull_remaining"].as_f64().is_some());

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

#[test]
fn optimize_endpoint_changes_with_seed() {
    let response_a = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"explorer_30","sims":1000,"seed":7,"max_candidates":32}"#,
        None,
    );
    let response_b = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"explorer_30","sims":1000,"seed":8,"max_candidates":32}"#,
        None,
    );

    assert_eq!(response_a.status_code, 200);
    assert_eq!(response_b.status_code, 200);
    assert_ne!(response_a.body, response_b.body);
}

#[test]
fn optimize_endpoint_is_deterministic_for_fixed_seed() {
    let body = r#"{"ship":"saladin","hostile":"explorer_30","sims":2000,"seed":77,"max_candidates":64}"#;

    let response_a = route_request("POST", "/api/optimize", body, None);
    let response_b = route_request("POST", "/api/optimize", body, None);

    assert_eq!(response_a.status_code, 200);
    assert_eq!(response_b.status_code, 200);

    let payload_a: serde_json::Value =
        serde_json::from_str(&response_a.body).expect("response A should be valid json");
    let payload_b: serde_json::Value =
        serde_json::from_str(&response_b.body).expect("response B should be valid json");
    assert_eq!(payload_a["scenario"], payload_b["scenario"]);
    assert_eq!(payload_a["recommendations"], payload_b["recommendations"]);
}

#[test]
fn optimize_endpoint_rejects_invalid_payload() {
    let response = route_request("POST", "/api/optimize", "{bad json}", None);
    assert_eq!(response.status_code, 400);
    assert!(response.body.contains("Invalid request body"));
}

#[test]
fn optimize_endpoint_rejects_empty_ship_and_hostile() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"","hostile":"   ","sims":100}"#,
        None,
    );

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

#[test]
fn optimize_endpoint_rejects_zero_sims() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"explorer_30","sims":0}"#,
        None,
    );

    assert_eq!(response.status_code, 400);

    let payload: serde_json::Value =
        serde_json::from_str(&response.body).expect("response should be valid json");
    let errors = payload["errors"]
        .as_array()
        .expect("errors should be array");
    assert!(errors.iter().any(|error| error["field"] == "sims"));
}

#[test]
fn optimize_endpoint_rejects_very_large_sims() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"explorer_30","sims":5000000}"#,
        None,
    );

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

#[test]
fn optimize_endpoint_rejects_excessive_max_candidates() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"saladin","hostile":"explorer_30","sims":1000,"max_candidates":3000000}"#,
        None,
    );

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

#[test]
fn optimize_validation_error_has_expected_schema() {
    let response = route_request(
        "POST",
        "/api/optimize",
        r#"{"ship":"","hostile":"explorer_30","sims":0}"#,
        None,
    );

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
