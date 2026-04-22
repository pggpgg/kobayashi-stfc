//! Contract checks for `docs/openapi/kobayashi-openapi.yaml` (task 14).

use kobayashi::server::api::{
    CompareCrewsRequest, OptimizeRequest, ReplaySeedRequest, SimulateRequest,
};
use kobayashi::server::openapi::OPENAPI_YAML;

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
        "ship": "Saladin",
        "hostile": "2918121098",
        "crew": { "captain": "officer_id" }
    }))
    .expect("SimulateRequest");

    let _opt: OptimizeRequest = serde_json::from_value(serde_json::json!({
        "ship": "Saladin",
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
        "ship": "Saladin",
        "hostile": "2918121098",
        "crews": [
            { "captain": "a", "bridge": [], "below_deck": [] },
            { "captain": "b", "bridge": [], "below_deck": [] }
        ]
    }))
    .expect("CompareCrewsRequest");

    let _replay: ReplaySeedRequest = serde_json::from_value(serde_json::json!({
        "ship": "Saladin",
        "hostile": "2918121098",
        "sim_index": 0,
        "crew": { "captain": "officer_id" }
    }))
    .expect("ReplaySeedRequest");
}
