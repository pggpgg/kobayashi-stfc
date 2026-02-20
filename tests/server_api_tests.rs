use kobayashi::server::routes::route_request;

#[test]
fn health_endpoint_returns_ok_json() {
    let response = route_request("GET", "/api/health", "");
    assert_eq!(response.status_code, 200);
    assert_eq!(response.content_type, "application/json");
    assert!(response.body.contains("\"status\": \"ok\""));
}

#[test]
fn optimize_endpoint_accepts_json_payload() {
    let body = r#"{"ship":"saladin","hostile":"explorer_30","sims":2000}"#;
    let response = route_request("POST", "/api/optimize", body);

    assert_eq!(response.status_code, 200);
    assert!(response.body.contains("optimizer_stub"));
    assert!(response.body.contains("saladin"));
    assert!(response.body.contains("explorer_30"));
}

#[test]
fn optimize_endpoint_rejects_invalid_payload() {
    let response = route_request("POST", "/api/optimize", "{bad json}");
    assert_eq!(response.status_code, 400);
    assert!(response.body.contains("Invalid request body"));
}
