use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::server::routes::build_router;
use tower::ServiceExt;

fn registry() -> Arc<DataRegistry> {
    static REGISTRY: OnceLock<Arc<DataRegistry>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| DataRegistry::load().unwrap())
        .clone()
}

async fn request(host: &str, origin: Option<&str>, content_type: &str) -> StatusCode {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/optimize/start")
        .header("host", host)
        .header("content-type", content_type);
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    // Invalid JSON reaches the handler without ever writing a real player profile.
    let mut request = builder.body(Body::from("{not-json")).unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
    ));
    build_router(registry())
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn hostile_browser_origin_is_rejected_before_the_json_handler() {
    assert_eq!(
        request(
            "127.0.0.1:3000",
            Some("https://untrusted.invalid"),
            "text/plain"
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn rebinding_host_is_rejected_even_without_an_origin() {
    assert_eq!(
        request("rebinding.invalid:3000", None, "application/json").await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn valid_same_origin_and_native_json_requests_reach_validation() {
    for origin in [Some("http://127.0.0.1:3000"), None] {
        assert_eq!(
            request("127.0.0.1:3000", origin, "application/json").await,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn json_routes_reject_simple_text_plain_bodies() {
    assert_eq!(
        request("127.0.0.1:3000", None, "text/plain").await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
}
