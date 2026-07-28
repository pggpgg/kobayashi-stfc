//! Contract checks for `docs/openapi/kobayashi-openapi.yaml` (task 14).

use kobayashi::server::api::{
    CompareCrewsRequest, OptimizeRequest, ReplaySeedRequest, SimulateRequest,
};
use kobayashi::server::openapi::OPENAPI_YAML;
use serde_yaml::Value;

fn component_schema<'a>(spec: &'a Value, name: &str) -> &'a Value {
    spec.get("components")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.get(name))
        .unwrap_or_else(|| panic!("missing OpenAPI component schema {name}"))
}

fn assert_support_buffs_array_property(schema: &Value, schema_name: &str) {
    let support_buffs = schema
        .get("properties")
        .and_then(|v| v.get("support_buffs"))
        .unwrap_or_else(|| panic!("{schema_name} must document support_buffs"));
    assert_eq!(
        support_buffs.get("type").and_then(|v| v.as_str()),
        Some("array"),
        "{schema_name}.support_buffs must be an array"
    );
    assert_eq!(
        support_buffs
            .get("items")
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("string"),
        "{schema_name}.support_buffs items must be strings"
    );
}

#[test]
fn bundled_openapi_is_valid_yaml_with_expected_version() {
    let v: serde_yaml::Value = serde_yaml::from_str(OPENAPI_YAML).expect("OpenAPI YAML must parse");
    assert_eq!(v.get("openapi").and_then(|x| x.as_str()), Some("3.1.0"));
    assert!(v.get("paths").is_some());
    assert!(v.get("components").is_some());
}

/// Minimal JSON bodies documented in OpenAPI must deserialize into the same DTOs the server uses.
#[test]
fn heavy_payload_dtos_accept_documented_minimal_shapes() {
    let _sim: SimulateRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "2918121098",
        "crew": { "captain": "officer_id" }
    }))
    .expect("SimulateRequest");

    let _opt: OptimizeRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "2918121098",
        "sims": 1000,
        "fast_discovery": true,
        "constraints": {
            "must_include": [],
            "exclude": [],
            "groups": []
        }
    }))
    .expect("OptimizeRequest");

    let _cmp: CompareCrewsRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "2918121098",
        "crews": [
            { "captain": "a", "bridge": [], "below_deck": [] },
            { "captain": "b", "bridge": [], "below_deck": [] }
        ]
    }))
    .expect("CompareCrewsRequest");

    let _replay: ReplaySeedRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "2918121098",
        "sim_index": 0,
        "crew": { "captain": "officer_id" }
    }))
    .expect("ReplaySeedRequest");
}

/// Support-buff request payloads are part of the public simulate/optimize contract.
#[test]
fn simulate_and_optimize_contracts_accept_support_buffs() {
    let spec: Value = serde_yaml::from_str(OPENAPI_YAML).expect("OpenAPI YAML must parse");
    assert_support_buffs_array_property(
        component_schema(&spec, "SimulateRequest"),
        "SimulateRequest",
    );
    assert_support_buffs_array_property(
        component_schema(&spec, "OptimizeRequest"),
        "OptimizeRequest",
    );

    let sim: SimulateRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "2918121098",
        "crew": { "captain": "officer_id", "bridge": [], "below_deck": [] },
        "support_buffs": ["cerritos_support", "defiant_reinforce"]
    }))
    .expect("SimulateRequest with support_buffs");
    assert_eq!(
        sim.support_buffs.as_deref(),
        Some(
            &[
                "cerritos_support".to_string(),
                "defiant_reinforce".to_string()
            ][..]
        )
    );

    let opt: OptimizeRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "2918121098",
        "sims": 1000,
        "max_candidates": 16,
        "support_buffs": ["cerritos_support", "titan_a_max_fortification"]
    }))
    .expect("OptimizeRequest with support_buffs");
    assert_eq!(
        opt.support_buffs.as_deref(),
        Some(
            &[
                "cerritos_support".to_string(),
                "titan_a_max_fortification".to_string()
            ][..]
        )
    );
}
