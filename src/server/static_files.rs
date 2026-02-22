//! Serve static files from frontend/dist (SPA). Used when frontend is built and dist exists.

use std::fs;

use super::routes::HttpResponse;

/// Try to serve a static file or SPA index. Returns None if static serving is not available
/// or path is an API path.
pub fn try_serve_static(method: &str, path: &str) -> Option<HttpResponse> {
    if method != "GET" {
        return None;
    }
    if path.starts_with("/api") {
        return None;
    }

    let path = path.split('?').next().unwrap_or(path).trim_start_matches('/');
    if path.contains("..") {
        return None;
    }

    let base = std::env::current_dir().ok()?;
    let dist = base.join("frontend/dist").canonicalize().ok().or_else(|| {
        let d = base.join("dist");
        d.canonicalize().ok()
    })?;

    let (file_path, content_type) = if path.is_empty() || path == "index.html" {
        let p = dist.join("index.html");
        if p.is_file() {
            (p, "text/html; charset=utf-8")
        } else {
            return None;
        }
    } else {
        let p = dist.join(path);
        if !p.starts_with(&dist) {
            return None;
        }
        if p.is_file() {
            let ct = content_type_for_path(path);
            (p, ct)
        } else {
            let index = dist.join("index.html");
            if index.is_file() {
                (index, "text/html; charset=utf-8")
            } else {
                return None;
            }
        }
    };

    if !is_text_content_type(content_type) {
        return None;
    }
    let body = fs::read_to_string(&file_path).ok()?;

    Some(HttpResponse {
        status_code: 200,
        status_text: "OK",
        content_type,
        body,
    })
}

fn content_type_for_path(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

fn is_text_content_type(ct: &str) -> bool {
    ct.starts_with("text/") || ct.starts_with("application/javascript") || ct.starts_with("application/json")
}
