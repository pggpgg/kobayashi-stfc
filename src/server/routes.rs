//! Axum router definition and handler functions.
//!
//! Each handler calls the corresponding `api::*_payload` function (which is
//! synchronous and may do I/O or CPU work).  Heavy operations (optimize,
//! simulate) are offloaded to a blocking thread pool via
//! `tokio::task::spawn_blocking` so that the async runtime stays responsive.

use axum::{
    Router,
    extract::OriginalUri,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::data::data_registry::DataRegistry;
use crate::server::api;
use crate::server::sync;

/// Application state shared by all handlers.
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<DataRegistry>,
}

// ---------------------------------------------------------------------------
// Shared JSON response helpers
// ---------------------------------------------------------------------------

struct JsonResponse {
    status: StatusCode,
    body: String,
}

impl IntoResponse for JsonResponse {
    fn into_response(self) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        (self.status, headers, self.body).into_response()
    }
}

fn ok_json(body: String) -> JsonResponse {
    JsonResponse { status: StatusCode::OK, body }
}

fn error_json(status: StatusCode, message: &str) -> JsonResponse {
    let body = format!(
        "{{\n  \"status\": \"error\",\n  \"message\": {}\n}}",
        serde_json::to_string(message).unwrap_or_else(|_| "\"Unknown error\"".to_string())
    );
    JsonResponse { status, body }
}

fn validation_json(payload: api::ValidationErrorResponse) -> JsonResponse {
    let fallback =
        "{\n  \"status\": \"error\",\n  \"message\": \"Validation failed\"\n}".to_string();
    JsonResponse {
        status: StatusCode::BAD_REQUEST,
        body: serde_json::to_string_pretty(&payload).unwrap_or(fallback),
    }
}

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

pub fn build_router(registry: Arc<DataRegistry>) -> Router {
    let state = AppState { registry };

    let api_routes = Router::new()
        // Health
        .route("/api/health", get(handle_health))
        // Officers
        .route("/api/officers", get(handle_officers))
        .route("/api/officers/import", post(handle_officers_import))
        // Ships / hostiles
        .route("/api/ships", get(handle_ships))
        .route("/api/hostiles", get(handle_hostiles))
        // Data version
        .route("/api/data/version", get(handle_data_version))
        // Profile
        .route("/api/profile", get(handle_profile_get))
        .route("/api/profile", put(handle_profile_put))
        // Presets
        .route("/api/presets", get(handle_presets_list))
        .route("/api/presets", post(handle_preset_post))
        .route("/api/presets/:id", get(handle_preset_get))
        // Simulate (CPU-bound, blocking pool)
        .route("/api/simulate", post(handle_simulate))
        // Optimize synchronous (long-running, blocking pool)
        .route("/api/optimize", post(handle_optimize))
        // Heuristics seed list
        .route("/api/heuristics", get(handle_heuristics))
        // Optimize estimate (lightweight GET with query params)
        .route("/api/optimize/estimate", get(handle_optimize_estimate))
        // Optimize async job
        .route("/api/optimize/start", post(handle_optimize_start))
        .route("/api/optimize/status/:job_id", get(handle_optimize_status))
        // Sync ingress
        .route("/api/sync/status", get(handle_sync_status))
        .route("/api/sync/ingress", post(handle_sync_ingress))
        .with_state(state);

    // Wire the SPA or legacy console fallback depending on whether the dist
    // directory exists at startup time.
    //
    // When dist exists:
    //   - Requests for files that exist on disk (JS bundles, CSS, images, etc.)
    //     are served directly by tower-http's ServeDir.
    //   - All other non-API paths fall back to index.html so that React Router
    //     deep-links (e.g. /ships, /optimize) work when navigated to directly.
    //
    // When dist does not exist:
    //   - "/" serves the legacy API console HTML.
    //   - All other paths return 404.
    match locate_dist_dir() {
        Some(_dir) => {
            // Fallback handler: serve static files from dist; if the path doesn't
            // exist, serve index.html (200) so React Router deep-links work.
            api_routes.fallback(serve_spa_static_fallback)
        }
        None => {
            // No built SPA — serve the legacy API console on "/" only and 404
            // everywhere else.
            api_routes.fallback(handle_no_spa_fallback)
        }
    }
}

fn locate_dist_dir() -> Option<std::path::PathBuf> {
    let base = std::env::current_dir().ok()?;
    [base.join("frontend/dist"), base.join("dist")]
        .into_iter()
        .find(|p| p.is_dir())
}

// ---------------------------------------------------------------------------
// SPA static fallback (when dist exists): serve files from dist or index.html
// ---------------------------------------------------------------------------

async fn serve_spa_static_fallback(OriginalUri(uri): OriginalUri) -> Response {
    let dir = match locate_dist_dir() {
        Some(d) => d,
        None => return error_json(StatusCode::NOT_FOUND, "Not found").into_response(),
    };
    let path = uri.path();
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return serve_index_html(&dir);
    }
    let path = PathBuf::from(path);
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return error_json(StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }
    let full = dir.join(&path);
    match tokio::fs::metadata(&full).await {
        Ok(meta) if meta.is_file() => {
            match tokio::fs::read(&full).await {
                Ok(body) => {
                    let ct = content_type_for_path(path.as_path());
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, ct)],
                        body,
                    )
                        .into_response()
                }
                Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Read error").into_response(),
            }
        }
        _ => serve_index_html(&dir),
    }
}

fn serve_index_html(dir: &std::path::Path) -> Response {
    let index = dir.join("index.html");
    match std::fs::read(&index) {
        Ok(body) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "index.html not found").into_response(),
    }
}

fn content_type_for_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("mjs") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// Fallback when no SPA dist is present
// ---------------------------------------------------------------------------

async fn handle_no_spa_fallback(OriginalUri(uri): OriginalUri) -> Response {
    if uri.path() == "/" {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            legacy_console_html(),
        )
            .into_response()
    } else {
        error_json(StatusCode::NOT_FOUND, "Not found").into_response()
    }
}

// ---------------------------------------------------------------------------
// API handler implementations
// ---------------------------------------------------------------------------

async fn handle_health() -> impl IntoResponse {
    match api::health_payload() {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_officers(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let owned_only = params.get("owned_only").map(String::as_str).unwrap_or("");
    let path = if owned_only == "1" || owned_only.eq_ignore_ascii_case("true") {
        "/api/officers?owned_only=1".to_string()
    } else {
        "/api/officers".to_string()
    };
    match api::officers_payload(state.registry.as_ref(), &path) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_ships(State(state): State<AppState>) -> impl IntoResponse {
    match api::ships_payload(state.registry.as_ref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_hostiles(State(state): State<AppState>) -> impl IntoResponse {
    match api::hostiles_payload(state.registry.as_ref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_heuristics() -> impl IntoResponse {
    match api::heuristics_list_payload() {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_data_version(State(state): State<AppState>) -> impl IntoResponse {
    match api::data_version_payload(state.registry.as_ref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profile_get() -> impl IntoResponse {
    match api::profile_get_payload() {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profile_put(body: String) -> impl IntoResponse {
    match api::profile_put_payload(&body) {
        Ok(response) => ok_json(response).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

async fn handle_presets_list() -> impl IntoResponse {
    match api::presets_list_payload() {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_preset_get(Path(id): Path<String>) -> impl IntoResponse {
    match api::preset_get_payload(&id) {
        Ok(body) => ok_json(body).into_response(),
        Err(api::PresetError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Preset not found").into_response()
        }
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_preset_post(body: String) -> impl IntoResponse {
    match api::preset_post_payload(&body) {
        Ok(response) => ok_json(response).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

async fn handle_officers_import(body: String) -> impl IntoResponse {
    match api::officers_import_payload(&body) {
        Ok(response) => ok_json(response).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

/// POST /api/simulate — CPU-bound, offloaded to blocking pool.
async fn handle_simulate(State(state): State<AppState>, body: String) -> impl IntoResponse {
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || api::simulate_payload(registry.as_ref(), &body)).await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::SimulateError::Parse(e))) => {
            error_json(StatusCode::BAD_REQUEST, &format!("Invalid request body: {e}"))
                .into_response()
        }
        Ok(Err(api::SimulateError::Validation(msg))) => {
            error_json(StatusCode::BAD_REQUEST, &msg).into_response()
        }
        Err(e) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Task panicked: {e}"),
        )
        .into_response(),
    }
}

/// POST /api/optimize — long-running synchronous optimization; runs on blocking pool.
async fn handle_optimize(State(state): State<AppState>, body: String) -> impl IntoResponse {
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || api::optimize_payload(registry.as_ref(), &body)).await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::OptimizePayloadError::Parse(e))) => {
            error_json(StatusCode::BAD_REQUEST, &format!("Invalid request body: {e}"))
                .into_response()
        }
        Ok(Err(api::OptimizePayloadError::Validation(v))) => validation_json(v).into_response(),
        Err(e) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Task panicked: {e}"),
        )
        .into_response(),
    }
}

/// GET /api/optimize/estimate?ship=...&hostile=...&sims=...
async fn handle_optimize_estimate(
    State(state): State<AppState>,
    Query(raw): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let query: String = raw
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");
    let path = format!("/api/optimize/estimate?{}", query);
    match api::optimize_estimate_payload(state.registry.as_ref(), &path) {
        Ok(payload) => ok_json(payload).into_response(),
        Err(api::OptimizePayloadError::Parse(e)) => {
            error_json(StatusCode::BAD_REQUEST, &format!("Invalid request: {e}")).into_response()
        }
        Err(api::OptimizePayloadError::Validation(v)) => validation_json(v).into_response(),
    }
}

/// POST /api/optimize/start — spawns a background std::thread, returns job_id immediately.
async fn handle_optimize_start(State(state): State<AppState>, body: String) -> impl IntoResponse {
    match api::optimize_start_payload(state.registry.clone(), &body) {
        Ok(payload) => ok_json(payload).into_response(),
        Err(api::OptimizePayloadError::Parse(e)) => {
            error_json(StatusCode::BAD_REQUEST, &format!("Invalid request body: {e}"))
                .into_response()
        }
        Err(api::OptimizePayloadError::Validation(v)) => validation_json(v).into_response(),
    }
}

/// GET /api/optimize/status/:job_id
async fn handle_optimize_status(Path(job_id): Path<String>) -> impl IntoResponse {
    match api::optimize_status_payload(&job_id) {
        Ok(payload) => ok_json(payload).into_response(),
        Err(api::OptimizeStatusError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Job not found").into_response()
        }
        Err(api::OptimizeStatusError::Serialize(e)) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Sync handlers
// ---------------------------------------------------------------------------

async fn handle_sync_status() -> impl IntoResponse {
    let (status, body) = sync::sync_status_payload();
    JsonResponse { status, body }.into_response()
}

async fn handle_sync_ingress(headers: HeaderMap, body: String) -> impl IntoResponse {
    let token = headers
        .get("stfc-sync-token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let (status, response_body) = sync::ingress_payload(&body, token.as_deref());
    JsonResponse { status, body: response_body }.into_response()
}

// ---------------------------------------------------------------------------
// Legacy API console HTML (served when no SPA build is present)
// ---------------------------------------------------------------------------

fn legacy_console_html() -> String {
    r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width,initial-scale=1" />
  <title>Kobayashi API Console</title>
  <style>
    body { font-family: Arial, sans-serif; max-width: 900px; margin: 24px auto; padding: 0 12px; }
    h1 { margin-bottom: 8px; }
    .card { border: 1px solid #ddd; border-radius: 8px; padding: 14px; margin: 14px 0; }
    label { display:block; margin: 8px 0 4px; font-weight: 600; }
    input { width: 100%; padding: 8px; box-sizing: border-box; }
    button { margin-top: 12px; padding: 8px 14px; }
    pre { background: #111; color: #aef2ae; padding: 12px; overflow: auto; border-radius: 6px; min-height: 180px; }
  </style>
</head>
<body>
  <h1>Kobayashi Local API</h1>
  <p>Infrastructure scaffold for browser-based access to optimization endpoints.</p>

  <div class="card">
    <strong>Health</strong>
    <div><button id="health-btn">GET /api/health</button></div>
  </div>

  <div class="card">
    <strong>Optimize</strong>
    <label for="ship">Ship</label>
    <input id="ship" value="Saladin" />
    <label for="hostile">Hostile</label>
    <input id="hostile" value="Explorer_30" />
    <label for="sims">Fight iterations per crew</label>
    <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;">
      <input id="sims" type="number" min="1" max="100000" value="5000" style="width:90px" />
      <button type="button" class="sims-preset" data-sims="1000">1k</button>
      <button type="button" class="sims-preset" data-sims="5000">5k</button>
      <button type="button" class="sims-preset" data-sims="10000">10k</button>
      <button type="button" class="sims-preset" data-sims="50000">50k</button>
    </div>
    <p id="estimate-msg" style="margin:8px 0 0;font-size:0.9rem;color:#666;"></p>
    <div><button id="optimize-btn">POST /api/optimize</button></div>
  </div>

  <pre id="output">Ready.</pre>

  <script>
    const output = document.getElementById('output');
    const shipEl = document.getElementById('ship');
    const hostileEl = document.getElementById('hostile');
    const simsEl = document.getElementById('sims');
    const estimateEl = document.getElementById('estimate-msg');

    let estimateTimer = null;
    function fetchEstimate() {
      const ship = shipEl.value.trim();
      const hostile = hostileEl.value.trim();
      const sims = Math.max(1, Math.min(100000, Number(simsEl.value) || 5000));
      if (!ship || !hostile) { estimateEl.textContent = ''; return; }
      const url = '/api/optimize/estimate?ship=' + encodeURIComponent(ship) + '&hostile=' + encodeURIComponent(hostile) + '&sims=' + sims;
      fetch(url).then(r => r.ok ? r.json() : null).then(data => {
        if (data) estimateEl.textContent = 'Estimated time: ~' + (data.estimated_seconds < 1 ? '<1' : data.estimated_seconds.toFixed(1)) + ' s (' + data.estimated_candidates + ' crews)';
        else estimateEl.textContent = '';
      }).catch(() => { estimateEl.textContent = ''; });
    }
    function scheduleEstimate() {
      if (estimateTimer) clearTimeout(estimateTimer);
      estimateTimer = setTimeout(fetchEstimate, 300);
    }
    shipEl.addEventListener('input', scheduleEstimate);
    hostileEl.addEventListener('input', scheduleEstimate);
    simsEl.addEventListener('input', scheduleEstimate);
    fetchEstimate();

    document.querySelectorAll('.sims-preset').forEach(btn => {
      btn.addEventListener('click', () => { simsEl.value = btn.dataset.sims; scheduleEstimate(); });
    });

    async function request(path, options) {
      output.textContent = 'Loading\u2026';
      const response = await fetch(path, options);
      const text = await response.text();
      let display = 'HTTP ' + response.status + '\n' + text;
      if (options && options.method === 'POST' && path === '/api/optimize') {
        try {
          const j = JSON.parse(text);
          if (j.duration_ms != null) display = 'Completed in ' + (j.duration_ms / 1000).toFixed(1) + ' s\n\n' + display;
        } catch (e) {}
      }
      output.textContent = display;
    }

    document.getElementById('health-btn').addEventListener('click', () => {
      request('/api/health', { method: 'GET' });
    });

    document.getElementById('optimize-btn').addEventListener('click', () => {
      const payload = {
        ship: shipEl.value,
        hostile: hostileEl.value,
        sims: Math.max(1, Math.min(100000, Number(simsEl.value) || 5000)),
      };
      request('/api/optimize', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
    });
  </script>
</body>
</html>
"#
    .to_string()
}
