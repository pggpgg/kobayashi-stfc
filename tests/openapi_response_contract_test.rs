//! Validate live JSON responses from the Axum router against bundled OpenAPI 3.1 component schemas.

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::server::openapi::OPENAPI_YAML;
use kobayashi::server::routes::build_router;
use serde_json::Value;
use std::net::SocketAddr;
use tower::ServiceExt;

fn openapi_spec_json() -> Value {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(OPENAPI_YAML).expect("OpenAPI YAML must parse");
    serde_json::to_value(yaml).expect("OpenAPI YAML → JSON")
}

fn rewrite_component_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get_mut("$ref") {
                if let Some(rest) = s.strip_prefix("#/components/schemas/") {
                    *s = format!("#/$defs/{rest}");
                }
            }
            for v in map.values_mut() {
                rewrite_component_refs(v);
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                rewrite_component_refs(v);
            }
        }
        _ => {}
    }
}

fn validator_for_schema(spec: &Value, schema_name: &str) -> jsonschema::Validator {
    let mut defs = spec
        .pointer("/components/schemas")
        .unwrap_or_else(|| panic!("OpenAPI missing components.schemas"))
        .clone();
    rewrite_component_refs(&mut defs);
    let bundle = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": defs,
        "$ref": format!("#/$defs/{schema_name}")
    });
    jsonschema::validator_for(&bundle).unwrap_or_else(|e| {
        panic!("compile JSON Schema for {schema_name}: {e}");
    })
}

async fn get_json_200(path: &str) -> Value {
    let registry = DataRegistry::load().expect("data registry");
    let app = build_router(registry);
    let addr: SocketAddr = "127.0.0.1:12345".parse().expect("loopback");
    let mut req = Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Body::empty())
        .expect("request");
    req.extensions_mut().insert(ConnectInfo(addr));
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK, "GET {path} expected 200");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&body).unwrap_or_else(|e| {
        panic!(
            "GET {path} invalid JSON: {e}; body={}",
            String::from_utf8_lossy(&body)
        )
    })
}

fn assert_valid(spec: &Value, schema_name: &str, instance: &Value) {
    let validator = validator_for_schema(spec, schema_name);
    if let Err(e) = validator.validate(instance) {
        panic!("{schema_name} validation failed: {e}\ninstance={instance}");
    }
}

#[serial_test::serial]
#[tokio::test]
async fn read_only_get_json_matches_openapi_schemas() {
    let spec = openapi_spec_json();

    let health = get_json_200("/api/health").await;
    assert_valid(&spec, "HealthResponse", &health);

    let coverage = get_json_200("/api/mechanics/coverage").await;
    assert_valid(&spec, "MechanicsCoverageResponse", &coverage);

    let data_version = get_json_200("/api/data/version").await;
    assert_valid(&spec, "DataVersionResponse", &data_version);

    let officers = get_json_200("/api/officers").await;
    assert_valid(&spec, "OfficersListResponse", &officers);

    let ships = get_json_200("/api/ships").await;
    assert_valid(&spec, "ShipsListResponse", &ships);

    let hostiles = get_json_200("/api/hostiles").await;
    assert_valid(&spec, "HostilesListResponse", &hostiles);

    let heuristics = get_json_200("/api/heuristics").await;
    assert_valid(&spec, "HeuristicsListResponse", &heuristics);

    let forbidden = get_json_200("/api/forbidden-tech").await;
    assert_valid(&spec, "ForbiddenTechCatalogResponse", &forbidden);

    let profiles = get_json_200("/api/profiles").await;
    assert_valid(&spec, "ProfilesListResponse", &profiles);

    let tiers = get_json_200("/api/ships/Saladin/tiers-levels").await;
    assert_valid(&spec, "ShipTiersLevelsResponse", &tiers);

    let estimate =
        get_json_200("/api/optimize/estimate?ship=Saladin&hostile=2918121098&sims=100").await;
    assert_valid(&spec, "OptimizeEstimateResponse", &estimate);
}
