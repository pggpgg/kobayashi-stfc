use kobayashi::server::api::{
    CrewRecommendation, OptimizeResponse, OptimizeStatusResponse, ScenarioSummary,
};
use schemars::{schema_for, JsonSchema};
use serde_json::Value;

fn spec() -> Value {
    serde_yaml::from_str(kobayashi::server::openapi::OPENAPI_YAML).unwrap()
}

fn assert_shape<T: JsonSchema>(name: &str) {
    let root = serde_json::to_value(schema_for!(T)).unwrap();
    let bundled = &spec()["components"]["schemas"][name];
    let mut expected = root["required"].as_array().cloned().unwrap_or_default();
    let mut actual = bundled["required"].as_array().cloned().unwrap_or_default();
    expected.sort_by_key(Value::to_string);
    actual.sort_by_key(Value::to_string);
    assert_eq!(actual, expected, "{name} required fields");
    let mut actual: Vec<_> = bundled["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut expected: Vec<_> = root["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "{name} properties");
}

#[test]
fn optimizer_results_and_progress_have_typed_schemas() {
    let spec = spec();
    let schemas = &spec["components"]["schemas"];
    assert_eq!(
        schemas["OptimizeStatusResponse"]["properties"]["result"]["$ref"],
        "#/components/schemas/OptimizeResponse"
    );
    assert_eq!(
        schemas["OptimizeStatusResponse"]["properties"]["progress_preview"]["items"]["$ref"],
        "#/components/schemas/CrewRecommendation"
    );
    assert_eq!(
        schemas["OptimizeResponse"]["properties"]["recommendations"]["items"]["$ref"],
        "#/components/schemas/CrewRecommendation"
    );
    assert_eq!(
        schemas["OptimizeResponse"]["properties"]["scenario"]["$ref"],
        "#/components/schemas/ScenarioSummary"
    );
}

#[test]
fn optimizer_schema_fields_and_requiredness_match_the_rust_payloads() {
    assert_shape::<OptimizeResponse>("OptimizeResponse");
    assert_shape::<OptimizeStatusResponse>("OptimizeStatusResponse");
    assert_shape::<CrewRecommendation>("CrewRecommendation");
    assert_shape::<ScenarioSummary>("ScenarioSummary");
}
