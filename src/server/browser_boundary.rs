//! Browser boundary for the local application and native mod/API clients.
//!
//! Host validation also covers SPA responses. Origins are checked before API
//! authentication, including requests from trusted loopback peers. Native
//! clients without Origin remain supported; normal JSON routes require JSON.

use axum::body::Body;
use axum::http::{header, uri::Authority, HeaderMap, Method, Request, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::{IpAddr, SocketAddr};

fn authority_key(authority: &Authority, scheme: &str) -> (String, u16) {
    (
        authority.host().to_ascii_lowercase(),
        authority
            .port_u16()
            .unwrap_or(if scheme == "https" { 443 } else { 80 }),
    )
}

fn parse_origin(value: &str) -> Option<(String, Authority)> {
    let uri: Uri = value.parse().ok()?;
    let scheme = uri.scheme_str()?;
    if !matches!(scheme, "http" | "https") || uri.query().is_some() || uri.path() != "/" {
        return None;
    }
    let authority = uri.authority()?.clone();
    if authority.as_str().contains('@') {
        return None;
    }
    Some((scheme.to_string(), authority))
}

fn host_allowed(authority: &Authority) -> bool {
    if authority.as_str().contains('@')
        || (authority.port().is_some() && authority.port_u16().is_none())
    {
        return false;
    }
    let host = authority
        .host()
        .trim_start_matches('[')
        .trim_end_matches(']');
    let ip = host.parse::<IpAddr>().ok();
    if host.eq_ignore_ascii_case("localhost") || ip.is_some_and(|ip| ip.is_loopback()) {
        return true;
    }
    let bind = std::env::var("KOBAYASHI_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    if let Ok(bind) = bind.parse::<SocketAddr>() {
        if ip.is_some_and(|ip| bind.ip().is_unspecified() || ip == bind.ip()) {
            return true;
        }
    } else if let Ok(bind) = bind.parse::<Authority>() {
        if host.eq_ignore_ascii_case(bind.host()) {
            return true;
        }
    }
    std::env::var("KOBAYASHI_ALLOWED_HOSTS")
        .ok()
        .is_some_and(|hosts| {
            hosts.split(',').map(str::trim).any(|allowed| {
                allowed.parse::<Authority>().ok().is_some_and(|allowed| {
                    authority.host().eq_ignore_ascii_case(allowed.host())
                        && (allowed.port().is_none() || allowed.port_u16() == authority.port_u16())
                })
            })
        })
}

fn request_boundary(headers: &HeaderMap) -> Result<(), &'static str> {
    if headers.get_all(header::HOST).iter().count() > 1 {
        return Err("Multiple Host headers are not allowed.");
    }
    if headers.get_all(header::ORIGIN).iter().count() > 1 {
        return Err("Multiple Origin headers are not allowed.");
    }
    let host = match headers.get(header::HOST) {
        Some(value) => {
            let authority = value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<Authority>().ok())
                .ok_or("Invalid request Host header.")?;
            if !host_allowed(&authority) {
                return Err("Request host is not allowed. Use the configured server address.");
            }
            Some(authority)
        }
        None => None,
    };
    if let Some(origin) = headers.get(header::ORIGIN) {
        let (scheme, origin) = origin
            .to_str()
            .ok()
            .and_then(parse_origin)
            .ok_or("Cross-origin request is not allowed.")?;
        let same_origin = host
            .as_ref()
            .is_some_and(|host| authority_key(host, &scheme) == authority_key(&origin, &scheme));
        let allowed_origin =
            std::env::var("KOBAYASHI_ALLOWED_ORIGINS")
                .ok()
                .is_some_and(|allowed| {
                    allowed
                        .split(',')
                        .map(str::trim)
                        .filter_map(parse_origin)
                        .any(|(allowed_scheme, allowed)| {
                            allowed_scheme == scheme
                                && authority_key(&allowed, &scheme)
                                    == authority_key(&origin, &scheme)
                        })
                });
        if !same_origin && !allowed_origin {
            return Err("Cross-origin request is not allowed.");
        }
    }
    Ok(())
}

fn json_body_route(method: &Method, path: &str) -> bool {
    matches!(*method, Method::POST | Method::PUT | Method::PATCH)
        && path.starts_with("/api/")
        && !path.ends_with("/cancel")
        && !matches!(
            path,
            "/api/officers/import" | "/api/profiles/import" | "/api/sync/ingress"
        )
}

fn json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let mime = value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            mime == "application/json"
                || (mime.starts_with("application/") && mime.ends_with("+json"))
        })
}

fn rejection(status: StatusCode, message: &str) -> Response {
    (
        status,
        axum::Json(serde_json::json!({"status":"error", "message":message})),
    )
        .into_response()
}

pub async fn middleware(request: Request<Body>, next: Next) -> Response {
    if let Err(message) = request_boundary(request.headers()) {
        return rejection(StatusCode::FORBIDDEN, message);
    }
    if json_body_route(request.method(), request.uri().path())
        && !json_content_type(request.headers())
    {
        return rejection(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type must be application/json for this route.",
        );
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_parser_rejects_opaque_origins_paths_and_credentials() {
        for input in [
            "null",
            "file:///",
            "https://example.test/path",
            "https://example.test/?x=1",
            "https://user@example.test",
        ] {
            assert!(parse_origin(input).is_none(), "{input}");
        }
        assert!(parse_origin("http://localhost:5173").is_some());
        assert!(parse_origin("https://[::1]:3000").is_some());
    }

    #[test]
    fn native_sync_formats_remain_exempt_from_json_requirement() {
        for path in [
            "/api/sync/ingress",
            "/api/officers/import",
            "/api/profiles/import",
            "/api/optimize/jobs/id/cancel",
        ] {
            assert!(!json_body_route(&Method::POST, path));
        }
        assert!(json_body_route(&Method::POST, "/api/optimize/start"));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "Application/Problem+JSON; charset=utf-8".parse().unwrap(),
        );
        assert!(json_content_type(&headers));
    }
}
