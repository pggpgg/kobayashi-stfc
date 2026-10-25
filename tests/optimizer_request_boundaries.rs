use kobayashi::data::data_registry::DataRegistry;
use kobayashi::server::api::{parse_optimize_request_body, validate_request, OptimizePayloadError};

fn request_with(overrides: &str) -> String {
    format!(r#"{{"ship":"uss_saladin","hostile":"2918121098"{overrides}}}"#)
}

#[test]
fn optimize_explicit_strategy_accepts_supported_values_and_rejects_unknown_values() {
    let registry = DataRegistry::load().expect("test registry should load");

    for strategy in [
        "exhaustive",
        "genetic",
        "tiered",
        "linear_eval",
        "random_stratified",
        " ExHaUsTiVe ",
    ] {
        let body = request_with(&format!(
            ",\"strategy\":{}",
            serde_json::to_string(strategy).unwrap()
        ));
        let request = parse_optimize_request_body(&body).unwrap();
        assert!(
            validate_request(registry.as_ref(), &request, 100).is_ok(),
            "supported strategy {strategy:?} should validate"
        );
    }
    let omitted = parse_optimize_request_body(&request_with("")).unwrap();
    assert!(validate_request(registry.as_ref(), &omitted, 100).is_ok());

    for strategy in ["auto", "teired", "", "   "] {
        let body = request_with(&format!(
            ",\"strategy\":{}",
            serde_json::to_string(strategy).unwrap()
        ));
        let request = parse_optimize_request_body(&body).unwrap();
        let error = validate_request(registry.as_ref(), &request, 100)
            .expect_err("unknown explicit strategies must not silently select exhaustive");
        match error {
            OptimizePayloadError::Validation(validation) => assert!(validation
                .errors
                .iter()
                .any(|issue| issue.field == "strategy")),
            other => panic!("expected request validation error, got {other:?}"),
        }
    }
}

#[test]
fn optimize_heuristic_seed_names_are_bounded_validated_and_case_deduplicated() {
    let registry = DataRegistry::load().expect("test registry should load");
    let oversized = (0..33)
        .map(|i| format!("\"seed-{i}\""))
        .collect::<Vec<_>>()
        .join(",");
    for (seeds, expected_field) in [
        (format!("[{}]", oversized), "heuristics_seeds"),
        (r#"["../sentinel"]"#.to_string(), "heuristics_seeds"),
        (
            format!("[{}]", serde_json::to_string(&"a".repeat(65)).unwrap()),
            "heuristics_seeds",
        ),
        (r#"["séed"]"#.to_string(), "heuristics_seeds"),
    ] {
        let body = request_with(&format!(",\"heuristics_seeds\":{seeds}"));
        let request = parse_optimize_request_body(&body).unwrap();
        let error = validate_request(registry.as_ref(), &request, 100)
            .expect_err("unsafe or oversized heuristic seed list must be rejected");
        match error {
            OptimizePayloadError::Validation(validation) => assert!(validation
                .errors
                .iter()
                .any(|issue| issue.field == expected_field)),
            other => panic!("expected request validation error, got {other:?}"),
        }
    }

    let max_length = request_with(&format!(
        ",\"heuristics_seeds\":[{}]",
        serde_json::to_string(&"a".repeat(64)).unwrap()
    ));
    let request = parse_optimize_request_body(&max_length).unwrap();
    assert!(validate_request(registry.as_ref(), &request, 100).is_ok());

    let duplicate_body = r#"{"ship":"uss_saladin","hostile":"2918121098","heuristics_seeds":["missing-seed","MISSING-SEED"]}"#;
    let duplicate = parse_optimize_request_body(duplicate_body).unwrap();
    assert!(validate_request(registry.as_ref(), &duplicate, 100).is_ok());
}
