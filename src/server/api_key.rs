//! Optional shared secret for mutating HTTP API routes when the server is reachable
//! from beyond a trusted loopback client. See `docs/DEPLOYMENT_SECURITY.md`.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::header;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::convert::Infallible;
use std::net::SocketAddr;

fn configured_key() -> Option<String> {
    std::env::var("KOBAYASHI_API_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
}

fn trust_loopback() -> bool {
    std::env::var("KOBAYASHI_API_KEY_TRUST_LOOPBACK")
        .map(|v| {
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(true)
}

fn peer_is_loopback(addr: &SocketAddr) -> bool {
    addr.ip().is_loopback()
}

/// True for `POST`/`PUT`/`DELETE`/`PATCH` under `/api/` except [`SYNC_INGRESS_PATH`], which uses
/// per-profile sync tokens instead (`docs/SYNC.md`).
pub(crate) fn is_mutating_api_route(method: &Method, path: &str) -> bool {
    if !path.starts_with("/api/") {
        return false;
    }
    if path == SYNC_INGRESS_PATH {
        return false;
    }
    matches!(
        *method,
        Method::POST | Method::PUT | Method::DELETE | Method::PATCH
    )
}

pub(crate) const SYNC_INGRESS_PATH: &str = "/api/sync/ingress";

/// When `KOBAYASHI_API_KEY` is set, mutating `/api/*` routes require a matching header unless the
/// peer is loopback and `KOBAYASHI_API_KEY_TRUST_LOOPBACK` is true (default).
pub fn should_enforce_request(method: &Method, path: &str, peer: &SocketAddr) -> bool {
    if configured_key().is_none() {
        return false;
    }
    if !is_mutating_api_route(method, path) {
        return false;
    }
    if trust_loopback() && peer_is_loopback(peer) {
        return false;
    }
    true
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    const PREFIX: &str = "Bearer ";
    if raw.len() <= PREFIX.len() || !raw[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return None;
    }
    Some(raw[PREFIX.len()..].trim())
}

fn x_api_key_header(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
}

fn bytes_eq_ct(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub(crate) fn headers_valid(headers: &HeaderMap, expected: &str) -> bool {
    let exp = expected.as_bytes();
    if let Some(t) = bearer_token(headers) {
        if bytes_eq_ct(t.as_bytes(), exp) {
            return true;
        }
    }
    if let Some(t) = x_api_key_header(headers) {
        if bytes_eq_ct(t.as_bytes(), exp) {
            return true;
        }
    }
    false
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json")],
        concat!(
            "{",
            "\"status\":\"error\",",
            "\"message\":\"Invalid or missing API key. ",
            "Use Authorization: Bearer <token> or X-Api-Key.\"",
            "}"
        ),
    )
        .into_response()
}

pub async fn middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    if !should_enforce_request(&method, &path, &addr) {
        return Ok(next.run(req).await);
    }
    let Some(key) = configured_key() else {
        return Ok(next.run(req).await);
    };
    if !headers_valid(req.headers(), &key) {
        return Ok(unauthorized_response());
    }
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn mutating_route_detection() {
        assert!(is_mutating_api_route(&Method::POST, "/api/simulate"));
        assert!(!is_mutating_api_route(&Method::POST, "/api/sync/ingress"));
        assert!(!is_mutating_api_route(&Method::GET, "/api/profile"));
    }

    #[test]
    fn headers_valid_accepts_bearer_and_x_api_key() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer my-secret"),
        );
        assert!(headers_valid(&headers, "my-secret"));

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("other"));
        assert!(headers_valid(&headers, "other"));
    }
}
