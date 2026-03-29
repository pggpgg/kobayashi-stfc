//! Utility for detecting whether the built frontend SPA is available.
//!
//! The SPA is served from `frontend/dist` by `routes::serve_spa_static_fallback` (not `ServeDir`).
//! Responses use `Cache-Control` (immutable for `assets/*`, `no-cache` for `index.html`) and
//! `tower_http::compression::CompressionLayer` (gzip/br) on the router.

/// Returns true if `frontend/dist` (or `dist`) exists in the current working
/// directory so the SPA can be served.
pub fn static_files_available() -> bool {
    let base = match std::env::current_dir() {
        Ok(b) => b,
        Err(_) => return false,
    };
    base.join("frontend/dist").is_dir() || base.join("dist").is_dir()
}
