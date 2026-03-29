//! Bundled OpenAPI 3.0 document for heavy JSON payloads (`docs/openapi/kobayashi-heavy-payloads.yaml`).

/// Raw OpenAPI YAML (audit task 7).
pub const OPENAPI_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/openapi/kobayashi-heavy-payloads.yaml"
));

/// Same document as JSON (for clients that prefer `application/json`).
pub fn openapi_json_string() -> Result<String, String> {
    let v: serde_yaml::Value = serde_yaml::from_str(OPENAPI_YAML)
        .map_err(|e| format!("invalid bundled OpenAPI YAML: {e}"))?;
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_openapi_yaml_parses() {
        let v: serde_yaml::Value =
            serde_yaml::from_str(OPENAPI_YAML).expect("bundled OpenAPI YAML must parse");
        assert_eq!(
            v.get("openapi")
                .and_then(|x| x.as_str())
                .unwrap_or_default(),
            "3.0.3"
        );
    }

    #[test]
    fn openapi_json_roundtrip() {
        openapi_json_string().expect("JSON serialization");
    }
}
