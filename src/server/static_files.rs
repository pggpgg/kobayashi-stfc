//! Utility for detecting whether the built frontend SPA is available.
//!
//! Actual static file serving is handled by `tower_http::services::ServeDir`
//! wired in `routes::build_router`.

/// Returns true if `frontend/dist` (or `dist`) exists in the current working
/// directory so the SPA can be served.
pub fn static_files_available() -> bool {
    let base = match std::env::current_dir() {
        Ok(b) => b,
        Err(_) => return false,
    };
    base.join("frontend/dist").is_dir() || base.join("dist").is_dir()
}
