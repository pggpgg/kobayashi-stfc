//! OpenAPI ↔ Rust handler DTO field parity (roadmap task 9).

#[test]
fn bundled_openapi_component_schemas_match_rust_dto_fields() {
    kobayashi::server::openapi_parity::verify_openapi_rust_field_parity()
        .expect("OpenAPI component schemas must match Rust DTO property names");
}
