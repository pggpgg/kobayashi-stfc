//! Axum router definition and handler functions.
//!
//! Each handler calls the corresponding `api::*_payload` function (which is
//! synchronous and may do I/O or CPU work).  Heavy operations (optimize,
//! simulate) are offloaded to a blocking thread pool via
//! `tokio::task::spawn_blocking` so that the async runtime stays responsive.
//! `/api/simulate` and synchronous `/api/optimize` share a semaphore
//! (`KOBAYASHI_MAX_CONCURRENT_CPU_JOBS`, default 1). Optional bounded queue wait:
//! `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS` (>0) returns HTTP 503 with `code: cpu_busy` when saturated.

use axum::{
    body::Bytes,
    extract::DefaultBodyLimit,
    extract::MatchedPath,
    extract::OriginalUri,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware,
    response::sse::{Event, Sse},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_stream::wrappers::ReceiverStream;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::Span;

use crate::data::data_registry::DataRegistry;
use crate::mechanics::coverage::mechanics_coverage_json;
use crate::server::api;
use crate::server::api::load_hull_id_registry;
use crate::server::api_key;
use crate::server::cpu_admission;
use crate::server::openapi;
use crate::server::profile_backup;
use crate::server::sensitivity_jobs;
use crate::server::sync;

/// Application state shared by all handlers.
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<DataRegistry>,
    /// Limits concurrent CPU-heavy `spawn_blocking` tasks (`/api/simulate`, `/api/optimize`).
    pub cpu_jobs: Arc<Semaphore>,
    /// Bounded wait for a CPU permit (from `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS` at router build).
    pub cpu_job_queue_wait: Option<Duration>,
    /// Whether `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS` was set at router build (including `0`).
    pub cpu_job_queue_wait_env_present: bool,
    /// Wall-clock time when this server process built router state (after data validation).
    pub started_at_utc: chrono::DateTime<chrono::Utc>,
    /// Mapping from upstream hull IDs to Kobayashi ship IDs, loaded once at startup.
    pub hull_id_registry: Arc<HashMap<i64, String>>,
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
    JsonResponse {
        status: StatusCode::OK,
        body,
    }
}

fn error_json(status: StatusCode, message: &str) -> JsonResponse {
    let body = format!(
        "{{\n  \"status\": \"error\",\n  \"message\": {}\n}}",
        serde_json::to_string(message).unwrap_or_else(|_| "\"Unknown error\"".to_string())
    );
    JsonResponse { status, body }
}

fn cpu_busy_response(retry_after_ms: u64) -> Response {
    let body = serde_json::json!({
        "status": "error",
        "code": "cpu_busy",
        "message": "Server CPU capacity is saturated; retry later.",
        "retry_after_ms": retry_after_ms,
    });
    let body_str = serde_json::to_string(&body)
        .unwrap_or_else(|_| "{\"status\":\"error\",\"code\":\"cpu_busy\"}".to_string());
    let secs = retry_after_ms.div_ceil(1000).max(1);
    let retry =
        HeaderValue::from_str(&secs.to_string()).unwrap_or_else(|_| HeaderValue::from_static("1"));
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::RETRY_AFTER, retry);
    (StatusCode::SERVICE_UNAVAILABLE, headers, body_str).into_response()
}

async fn acquire_cpu_or_response(state: &AppState) -> Result<OwnedSemaphorePermit, Response> {
    match cpu_admission::acquire_cpu_permit(Arc::clone(&state.cpu_jobs), state.cpu_job_queue_wait)
        .await
    {
        Ok(p) => Ok(p),
        Err(cpu_admission::AcquireCpuPermitError::SemaphoreClosed) => Err(error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CPU job semaphore closed",
        )
        .into_response()),
        Err(cpu_admission::AcquireCpuPermitError::QueueTimeout { retry_after_ms }) => {
            Err(cpu_busy_response(retry_after_ms))
        }
    }
}

/// Extract profile id from X-Profile-Id header or ?profile= query.
fn profile_id_from_request(headers: &HeaderMap, query: &HashMap<String, String>) -> Option<String> {
    headers
        .get("x-profile-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| query.get("profile").cloned().filter(|s| !s.is_empty()))
}

fn validation_json(payload: api::ValidationErrorResponse) -> JsonResponse {
    let fallback =
        "{\n  \"status\": \"error\",\n  \"message\": \"Validation failed\"\n}".to_string();
    JsonResponse {
        status: StatusCode::BAD_REQUEST,
        body: serde_json::to_string(&payload).unwrap_or(fallback),
    }
}

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

/// Large mod/game sync batches and roster / Spock’s JSON imports.
const BODY_LIMIT_LARGE_INGEST: usize = 16 * 1024 * 1024;
/// Full `PlayerProfile` JSON on PUT.
const BODY_LIMIT_PROFILE_PUT: usize = 8 * 1024 * 1024;
/// Optimize / simulate / compare / replay / async-start JSON payloads.
const BODY_LIMIT_CPU_JSON: usize = 2 * 1024 * 1024;
/// Preset create, profile create, optimize cancel, and other small JSON bodies.
const BODY_LIMIT_SMALL_JSON: usize = 512 * 1024;
/// Full `profiles/` tree as a zip (backup restore).
const BODY_LIMIT_PROFILE_BACKUP: usize = 32 * 1024 * 1024;

pub fn build_router(registry: Arc<DataRegistry>) -> Router {
    let (cpu_job_queue_wait, cpu_job_queue_wait_env_present) =
        cpu_admission::cpu_job_queue_wait_config_from_env();
    let hull_id_registry = Arc::new(load_hull_id_registry());
    let state = AppState {
        registry,
        cpu_jobs: Arc::new(Semaphore::new(
            crate::server::max_concurrent_cpu_jobs_from_env(),
        )),
        cpu_job_queue_wait,
        cpu_job_queue_wait_env_present,
        started_at_utc: chrono::Utc::now(),
        hull_id_registry,
    };

    let api_read = Router::new()
        .route("/api/openapi.yaml", get(handle_openapi_yaml))
        .route("/api/openapi.json", get(handle_openapi_json))
        .route("/api/health", get(handle_health))
        .route("/api/mechanics/coverage", get(handle_mechanics_coverage))
        .route(
            "/api/debug/combat-effect-spec/officers/:id",
            get(handle_combat_effect_spec_debug_officer),
        )
        .route("/api/officers", get(handle_officers))
        .route("/api/officers/:id/resolved", get(handle_officer_resolved))
        .route("/api/ships", get(handle_ships))
        .route("/api/ships/:id/tiers-levels", get(handle_ship_tiers_levels))
        .route("/api/hostiles", get(handle_hostiles))
        .route("/api/data/version", get(handle_data_version))
        .route("/api/forbidden-tech", get(handle_forbidden_tech))
        .route("/api/profile", get(handle_profile_get))
        .route(
            "/api/profile/buildings-summary",
            get(handle_profile_buildings_summary),
        )
        .route(
            "/api/profile/research-summary",
            get(handle_profile_research_summary),
        )
        .route(
            "/api/profile/forbidden-tech-imported",
            get(handle_profile_forbidden_tech_imported),
        )
        .route("/api/profiles", get(handle_profiles_list))
        .route("/api/profiles/export", get(handle_profiles_export))
        .route("/api/profiles/:id", delete(handle_profiles_delete))
        .route("/api/presets", get(handle_presets_list))
        .route("/api/presets/:id", get(handle_preset_get))
        .route("/api/heuristics", get(handle_heuristics))
        .route("/api/optimize/estimate", get(handle_optimize_estimate))
        .route("/api/optimize/status/:job_id", get(handle_optimize_status))
        .route(
            "/api/optimize/jobs/:job_id/stream",
            get(handle_optimize_job_stream),
        )
        .route("/api/sync/status", get(handle_sync_status))
        .route(
            "/api/sensitivity/defaults",
            get(handle_sensitivity_defaults),
        )
        .route(
            "/api/sensitivity/morris/defaults",
            get(handle_sensitivity_morris_defaults),
        )
        .route(
            "/api/sensitivity/sobol/defaults",
            get(handle_sensitivity_sobol_defaults),
        )
        .route(
            "/api/sensitivity/jobs/:job_id/status",
            get(handle_sensitivity_job_status),
        )
        .route(
            "/api/sensitivity/jobs/:job_id/stream",
            get(handle_sensitivity_job_stream),
        )
        .with_state(state.clone());

    let api_large_ingest = Router::new()
        .route("/api/sync/ingress", post(handle_sync_ingress))
        .route("/api/officers/import", post(handle_officers_import))
        .layer(DefaultBodyLimit::max(BODY_LIMIT_LARGE_INGEST))
        .with_state(state.clone());

    let api_profile_put = Router::new()
        .route("/api/profile", put(handle_profile_put))
        .layer(DefaultBodyLimit::max(BODY_LIMIT_PROFILE_PUT))
        .with_state(state.clone());

    let api_cpu_json = Router::new()
        .route("/api/simulate", post(handle_simulate))
        .route("/api/compare/crews", post(handle_compare_crews))
        .route("/api/optimize", post(handle_optimize))
        .route(
            "/api/optimize/replay-seed",
            post(handle_optimize_replay_seed),
        )
        .route("/api/optimize/start", post(handle_optimize_start))
        .route("/api/sensitivity", post(handle_sensitivity))
        .route("/api/sensitivity/morris", post(handle_sensitivity_morris))
        .route("/api/sensitivity/sobol", post(handle_sensitivity_sobol))
        .route("/api/sensitivity/start", post(handle_sensitivity_oat_start))
        .route(
            "/api/sensitivity/morris/start",
            post(handle_sensitivity_morris_start),
        )
        .route(
            "/api/sensitivity/sobol/start",
            post(handle_sensitivity_sobol_start),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT_CPU_JSON))
        .with_state(state.clone());

    let api_small_json = Router::new()
        .route("/api/presets", post(handle_preset_post))
        .route("/api/profiles", post(handle_profiles_create))
        .route(
            "/api/optimize/jobs/:job_id/cancel",
            post(handle_optimize_job_cancel),
        )
        .route(
            "/api/sensitivity/jobs/:job_id/cancel",
            post(handle_sensitivity_job_cancel),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT_SMALL_JSON))
        .with_state(state.clone());

    let api_profile_backup = Router::new()
        .route("/api/profiles/import", post(handle_profiles_import))
        .layer(DefaultBodyLimit::max(BODY_LIMIT_PROFILE_BACKUP))
        .with_state(state.clone());

    let api_routes = Router::new()
        .merge(api_read)
        .merge(api_large_ingest)
        .merge(api_profile_put)
        .merge(api_cpu_json)
        .merge(api_small_json)
        .merge(api_profile_backup)
        .layer(middleware::from_fn(api_key::middleware))
        .with_state(state);

    // Wire the SPA or legacy console fallback depending on whether the dist
    // directory exists at startup time.
    //
    // When dist exists:
    //   - Requests for files that exist on disk (JS bundles, CSS, images, etc.)
    //     are served by `serve_spa_static_fallback` with cache headers; gzip/br via CompressionLayer.
    //   - All other non-API paths fall back to index.html so that React Router
    //     deep-links (e.g. /ships, /optimize) work when navigated to directly.
    //
    // When dist does not exist:
    //   - "/" serves the legacy API console HTML.
    //   - All other paths return 404.
    let app = match locate_dist_dir() {
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
    };

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request<_>| {
            let matched_path = request
                .extensions()
                .get::<MatchedPath>()
                .map(MatchedPath::as_str)
                .unwrap_or_else(|| request.uri().path())
                .to_string();
            let has_profile_hint = request.headers().contains_key("x-profile-id")
                || request
                    .uri()
                    .query()
                    .is_some_and(|q| q.contains("profile="));
            tracing::info_span!(
                "http_request",
                method = %request.method(),
                uri = %request.uri(),
                matched_path = %matched_path,
                has_profile_hint
            )
        })
        .on_response(|response: &Response<_>, latency: Duration, span: &Span| {
            tracing::info!(
                parent: span,
                status = response.status().as_u16(),
                latency_ms = latency.as_millis() as u64,
                "request_completed"
            );
        })
        .on_failure(
            |failure_class: tower_http::classify::ServerErrorsFailureClass,
             latency: Duration,
             span: &Span| {
                tracing::warn!(
                    parent: span,
                    failure_class = %failure_class,
                    latency_ms = latency.as_millis() as u64,
                    "request_failed"
                );
            },
        );

    // Gzip/Brotli for compressible responses (audit task 13). Default predicate skips
    // SSE (`text/event-stream`), images, and tiny bodies.
    app.layer(CompressionLayer::new()).layer(trace_layer)
}

fn locate_dist_dir() -> Option<std::path::PathBuf> {
    crate::server::static_files::locate_dist_dir()
}

// ---------------------------------------------------------------------------
// SPA static fallback (when dist exists): serve files from dist or index.html
// ---------------------------------------------------------------------------

/// `Cache-Control` for files under `frontend/dist` (Vite hashes live under `assets/`).
fn spa_asset_cache_control(rel: &str) -> &'static str {
    if rel.is_empty() || rel.eq_ignore_ascii_case("index.html") {
        "no-cache"
    } else if rel.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=86400"
    }
}

async fn serve_spa_static_fallback(OriginalUri(uri): OriginalUri) -> Response {
    let dir = match locate_dist_dir() {
        Some(d) => d,
        None => return error_json(StatusCode::NOT_FOUND, "Not found").into_response(),
    };
    let path = uri.path();
    let rel = path.trim_start_matches('/');
    if rel.is_empty() {
        return serve_index_html(&dir);
    }
    let path = PathBuf::from(rel);
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return error_json(StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }
    let full = dir.join(&path);
    match tokio::fs::metadata(&full).await {
        Ok(meta) if meta.is_file() => match tokio::fs::read(&full).await {
            Ok(body) => {
                let ct = content_type_for_path(path.as_path());
                let cc = spa_asset_cache_control(rel);
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, HeaderValue::from_static(ct)),
                        (header::CACHE_CONTROL, HeaderValue::from_static(cc)),
                    ],
                    body,
                )
                    .into_response()
            }
            Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Read error").into_response(),
        },
        _ => spa_fallback_for_missing(rel, &dir),
    }
}

/// React Router deep-links fall back to `index.html`; missing Vite chunks must 404 instead
/// (serving HTML for `/assets/*.js` makes the browser fail module load → blank UI).
fn spa_fallback_for_missing(rel: &str, dir: &std::path::Path) -> Response {
    if rel.starts_with("assets/") {
        return error_json(StatusCode::NOT_FOUND, "Asset not found").into_response();
    }
    serve_index_html(dir)
}

fn serve_index_html(dir: &std::path::Path) -> Response {
    let index = dir.join("index.html");
    match std::fs::read(&index) {
        Ok(body) => (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
            ],
            body,
        )
            .into_response(),
        Err(_) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, "index.html not found").into_response()
        }
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

async fn handle_openapi_yaml() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/yaml; charset=utf-8"),
        )],
        openapi::OPENAPI_YAML,
    )
}

async fn handle_openapi_json() -> impl IntoResponse {
    match openapi::openapi_json_string() {
        Ok(s) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            )],
            s,
        )
            .into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
    }
}

async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    match api::health_payload(
        state.registry.as_ref(),
        state.started_at_utc,
        state.cpu_jobs.as_ref(),
        state.cpu_job_queue_wait,
        state.cpu_job_queue_wait_env_present,
    ) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_mechanics_coverage(State(state): State<AppState>) -> impl IntoResponse {
    match mechanics_coverage_json(state.registry.as_ref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_officers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let owned_only = params.get("owned_only").map(String::as_str).unwrap_or("");
    let path = if owned_only == "1" || owned_only.eq_ignore_ascii_case("true") {
        "/api/officers?owned_only=1".to_string()
    } else {
        "/api/officers".to_string()
    };
    let profile_id = profile_id_from_request(&headers, &params);
    match api::officers_payload(state.registry.as_ref(), &path, profile_id.as_deref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_ships(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let owned_only = params
        .get("owned_only")
        .map(|s| s.as_str())
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let profile_id = profile_id_from_request(&headers, &params);
    match api::ships_payload(
        state.registry.as_ref(),
        owned_only,
        profile_id.as_deref(),
        &state.hull_id_registry,
    ) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_ship_tiers_levels(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match api::ship_tiers_levels_payload(&id, state.registry.as_ref()) {
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

async fn handle_forbidden_tech(State(state): State<AppState>) -> impl IntoResponse {
    match api::forbidden_tech_catalog_payload(state.registry.as_ref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profile_get(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::profile_get_payload(profile_id.as_deref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profile_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::profile_put_payload(&body, profile_id.as_deref(), state.registry.as_ref()) {
        Ok(response) => ok_json(response).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

async fn handle_profile_forbidden_tech_imported(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::profile_forbidden_tech_imported_payload(profile_id.as_deref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profile_buildings_summary(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::profile_buildings_summary_payload(profile_id.as_deref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profile_research_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    let ship_id = params.get("ship_id").map(String::as_str);
    let hostile_id = params.get("hostile_id").map(String::as_str);
    match api::profile_research_summary_payload(
        state.registry.as_ref(),
        profile_id.as_deref(),
        ship_id,
        hostile_id,
    ) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profiles_list() -> impl IntoResponse {
    match api::profiles_list_payload() {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_profiles_create(body: String) -> impl IntoResponse {
    match api::profiles_create_payload(&body) {
        Ok(resp) => ok_json(resp).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

async fn handle_profiles_delete(Path(id): Path<String>) -> impl IntoResponse {
    match api::profiles_delete_payload(&id) {
        Ok(()) => ok_json(serde_json::json!({ "status": "ok" }).to_string()).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e).into_response(),
    }
}

async fn handle_profiles_export() -> impl IntoResponse {
    match tokio::task::spawn_blocking(profile_backup::export_profiles_zip).await {
        Ok(Ok(bytes)) => {
            let filename = format!(
                "kobayashi-profiles-{}.zip",
                chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
            );
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/zip"),
            );
            if let Ok(cd) = HeaderValue::try_from(format!("attachment; filename=\"{}\"", filename))
            {
                headers.insert(header::CONTENT_DISPOSITION, cd);
            }
            (StatusCode::OK, headers, bytes).into_response()
        }
        Ok(Err(e)) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
        Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "export failed").into_response(),
    }
}

async fn handle_profiles_import(body: Bytes) -> impl IntoResponse {
    let body = body.to_vec();
    match tokio::task::spawn_blocking(move || profile_backup::import_profiles_zip(&body)).await {
        Ok(Ok(())) => ok_json(serde_json::json!({ "status": "ok" }).to_string()).into_response(),
        Ok(Err(e)) => error_json(StatusCode::BAD_REQUEST, &e).into_response(),
        Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "import failed").into_response(),
    }
}

async fn handle_presets_list(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::presets_list_payload(profile_id.as_deref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_preset_get(
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::preset_get_payload(&id, profile_id.as_deref()) {
        Ok(body) => ok_json(body).into_response(),
        Err(api::PresetError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Preset not found").into_response()
        }
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

async fn handle_preset_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::preset_post_payload(&body, profile_id.as_deref(), state.registry.as_ref()) {
        Ok(response) => ok_json(response).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

async fn handle_officers_import(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    match api::officers_import_payload(&body, profile_id.as_deref()) {
        Ok(response) => ok_json(response).into_response(),
        Err(e) => error_json(StatusCode::BAD_REQUEST, &e.to_string()).into_response(),
    }
}

async fn handle_officer_resolved(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match api::officer_resolved_payload(state.registry.as_ref(), &id) {
        Ok(body) => ok_json(body).into_response(),
        Err(api::OfficerResolveError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Officer not found").into_response()
        }
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

/// `GET /api/debug/combat-effect-spec/officers/:id` — optional LCARS → [`CombatEffectSpec`] dump when
/// `KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG=1`. Returns **404** when disabled or officer missing.
async fn handle_combat_effect_spec_debug_officer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match api::combat_effect_spec_debug_officer_payload(state.registry.as_ref(), &id) {
        Ok(body) => ok_json(body).into_response(),
        Err(api::CombatEffectSpecDebugError::Disabled) => error_json(
            StatusCode::NOT_FOUND,
            "CombatEffectSpec HTTP debug disabled (set KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG=1)",
        )
        .into_response(),
        Err(api::CombatEffectSpecDebugError::LcarsOfficersNotLoaded) => error_json(
            StatusCode::NOT_FOUND,
            "LCARS officers not loaded (set KOBAYASHI_OFFICER_SOURCE=lcars before starting the server)",
        )
        .into_response(),
        Err(api::CombatEffectSpecDebugError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Officer not found").into_response()
        }
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

/// POST /api/simulate — CPU-bound, offloaded to blocking pool.
async fn handle_simulate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        api::simulate_payload(registry.as_ref(), &body, profile_id.as_deref())
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::SimulateError::Parse(e))) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
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

/// POST /api/compare/crews — Monte Carlo distributions for 2–5 crews (blocking pool).
async fn handle_compare_crews(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        api::compare_crews_payload(registry.as_ref(), &body, profile_id.as_deref())
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::CompareCrewsError::Parse(e))) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
        Ok(Err(api::CompareCrewsError::Validation(msg))) => {
            error_json(StatusCode::BAD_REQUEST, &msg).into_response()
        }
        Err(e) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Task panicked: {e}"),
        )
        .into_response(),
    }
}

/// POST /api/sensitivity — stat-level Δ-on-outcome analysis (paired CRN Monte Carlo).
/// Synchronous v1; gated by the CPU admission semaphore.
async fn handle_sensitivity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        api::sensitivity_payload(registry.as_ref(), &body, profile_id.as_deref())
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::SensitivityError::Parse(e))) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
        Ok(Err(api::SensitivityError::Validation(msg))) => {
            error_json(StatusCode::BAD_REQUEST, &msg).into_response()
        }
        Ok(Err(api::SensitivityError::Run(msg))) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, &msg).into_response()
        }
        Err(e) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Task panicked: {e}"),
        )
        .into_response(),
    }
}

/// GET /api/sensitivity/defaults — per-stat default delta catalog.
async fn handle_sensitivity_defaults(State(_state): State<AppState>) -> impl IntoResponse {
    ok_json(api::sensitivity_defaults_payload())
}

/// POST /api/sensitivity/morris — Morris-method screening (random trajectories, μ\*/σ).
/// Synchronous; gated by the CPU admission semaphore.
async fn handle_sensitivity_morris(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        api::sensitivity_morris_payload(registry.as_ref(), &body, profile_id.as_deref())
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::SensitivityError::Parse(e))) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
        Ok(Err(api::SensitivityError::Validation(msg))) => {
            error_json(StatusCode::BAD_REQUEST, &msg).into_response()
        }
        Ok(Err(api::SensitivityError::Run(msg))) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, &msg).into_response()
        }
        Err(e) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Task panicked: {e}"),
        )
        .into_response(),
    }
}

/// GET /api/sensitivity/morris/defaults — Morris defaults (δ catalog + r/sims defaults & caps).
async fn handle_sensitivity_morris_defaults(State(_state): State<AppState>) -> impl IntoResponse {
    ok_json(api::sensitivity_morris_defaults_payload())
}

/// POST /api/sensitivity/sobol — Sobol variance-based sensitivity (Saltelli design,
/// Jansen estimators). Synchronous; gated by the CPU admission semaphore.
async fn handle_sensitivity_sobol(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        api::sensitivity_sobol_payload(registry.as_ref(), &body, profile_id.as_deref())
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::SensitivityError::Parse(e))) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
        Ok(Err(api::SensitivityError::Validation(msg))) => {
            error_json(StatusCode::BAD_REQUEST, &msg).into_response()
        }
        Ok(Err(api::SensitivityError::Run(msg))) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, &msg).into_response()
        }
        Err(e) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Task panicked: {e}"),
        )
        .into_response(),
    }
}

/// GET /api/sensitivity/sobol/defaults — Sobol defaults (δ catalog + N defaults & caps).
async fn handle_sensitivity_sobol_defaults(State(_state): State<AppState>) -> impl IntoResponse {
    ok_json(api::sensitivity_sobol_defaults_payload())
}

/// POST /api/optimize — long-running synchronous optimization; runs on blocking pool.
async fn handle_optimize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        api::optimize_payload(registry.as_ref(), &body, profile_id.as_deref())
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::OptimizePayloadError::Parse(e))) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
        Ok(Err(api::OptimizePayloadError::Validation(v))) => validation_json(v).into_response(),
        Err(e) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Task panicked: {e}"),
        )
        .into_response(),
    }
}

/// POST /api/optimize/replay-seed — deterministic replay of one MC draw with combat trace.
async fn handle_optimize_replay_seed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    let registry = state.registry.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        api::replay_optimize_seed_payload(registry.as_ref(), &body, profile_id.as_deref())
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok_json(payload).into_response(),
        Ok(Err(api::ReplaySeedError::Parse(e))) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
        Ok(Err(api::ReplaySeedError::Validation(msg))) => {
            error_json(StatusCode::BAD_REQUEST, &msg).into_response()
        }
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
    headers: HeaderMap,
    Query(raw): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &raw);
    let query: String = raw
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");
    let path = format!("/api/optimize/estimate?{}", query);
    match api::optimize_estimate_payload(state.registry.as_ref(), &path, profile_id.as_deref()) {
        Ok(payload) => ok_json(payload).into_response(),
        Err(api::OptimizePayloadError::Parse(e)) => {
            error_json(StatusCode::BAD_REQUEST, &format!("Invalid request: {e}")).into_response()
        }
        Err(api::OptimizePayloadError::Validation(v)) => validation_json(v).into_response(),
    }
}

/// POST /api/optimize/start — background job on a std::thread; holds a CPU permit like sync optimize.
async fn handle_optimize_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let profile_id = profile_id_from_request(&headers, &params);
    match api::optimize_start_payload(permit, state.registry.clone(), &body, profile_id.as_deref())
    {
        Ok(payload) => ok_json(payload).into_response(),
        Err(api::OptimizePayloadError::Parse(e)) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Invalid request body: {e}"),
        )
        .into_response(),
        Err(api::OptimizePayloadError::Validation(v)) => validation_json(v).into_response(),
    }
}

/// GET /api/optimize/status/:job_id
async fn handle_optimize_status(Path(job_id): Path<String>) -> impl IntoResponse {
    match api::get_job_status(&job_id) {
        Ok(response) => match serde_json::to_string(&response) {
            Ok(payload) => ok_json(payload).into_response(),
            Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
        },
        Err(api::OptimizeStatusError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Job not found").into_response()
        }
        Err(api::OptimizeStatusError::Serialize(e)) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response()
        }
    }
}

/// GET /api/optimize/jobs/:job_id/stream — SSE stream of optimize job progress until done or error.
async fn handle_optimize_job_stream(Path(job_id): Path<String>) -> impl IntoResponse {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);
    tokio::spawn(async move {
        let job_id = job_id.clone();
        let mut last_payload = String::new();
        loop {
            match api::get_job_status(&job_id) {
                Ok(response) => {
                    let done = response.status == "done" || response.status == "error";
                    let payload = match serde_json::to_string(&response) {
                        Ok(s) => s,
                        Err(_) => {
                            let event = Event::default()
                                .data(r#"{"status":"error","error":"Serialization error"}"#);
                            let _ = tx.send(Ok(event)).await;
                            break;
                        }
                    };
                    // Skip sending if nothing changed — for long-running optimize
                    // jobs this avoids ~95% of serde passes and network writes.
                    // Always send terminal (done/error) events so the client
                    // receives the final state even when polled at start.
                    if !done && payload == last_payload {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        continue;
                    }
                    last_payload = payload.clone();
                    let event = Event::default().data(payload);
                    if tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                    if done {
                        break;
                    }
                }
                Err(api::OptimizeStatusError::NotFound) => {
                    let event =
                        Event::default().data(r#"{"status":"error","error":"Job not found"}"#);
                    let _ = tx.send(Ok(event)).await;
                    break;
                }
                Err(api::OptimizeStatusError::Serialize(_)) => {
                    let event = Event::default()
                        .data(r#"{"status":"error","error":"Serialization error"}"#);
                    let _ = tx.send(Ok(event)).await;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    });
    let stream = ReceiverStream::new(rx);
    Sse::new(stream)
}

/// POST /api/optimize/jobs/:job_id/cancel — request cancellation of a running optimize job.
async fn handle_optimize_job_cancel(Path(job_id): Path<String>) -> impl IntoResponse {
    match api::optimize_cancel_payload(&job_id) {
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
// Sensitivity async-job handlers (start / status / stream / cancel)
// ---------------------------------------------------------------------------

/// Shared body of the three `*_start` handlers: parse the JSON request, acquire a CPU
/// permit, kick off the worker thread via [`sensitivity_jobs::start_sensitivity_job`].
async fn handle_sensitivity_start_inner<T, F>(
    state: AppState,
    body: String,
    kind: sensitivity_jobs::SensitivityJobKind,
    request_factory: F,
) -> Response
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(T) -> sensitivity_jobs::SensitivityJobRequest,
{
    let req: T = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return error_json(
                StatusCode::BAD_REQUEST,
                &format!("Invalid request body: {e}"),
            )
            .into_response();
        }
    };
    let permit = match acquire_cpu_or_response(&state).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let response = sensitivity_jobs::start_sensitivity_job(
        state.registry.clone(),
        kind,
        request_factory(req),
        permit,
    );
    match serde_json::to_string(&response) {
        Ok(s) => ok_json(s).into_response(),
        Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }
}

/// POST /api/sensitivity/start — async OAT sensitivity job.
async fn handle_sensitivity_oat_start(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    handle_sensitivity_start_inner::<crate::optimizer::sensitivity::SensitivityRequest, _>(
        state,
        body,
        sensitivity_jobs::SensitivityJobKind::Oat,
        sensitivity_jobs::SensitivityJobRequest::Oat,
    )
    .await
}

/// POST /api/sensitivity/morris/start — async Morris-screening job.
async fn handle_sensitivity_morris_start(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    handle_sensitivity_start_inner::<crate::optimizer::sensitivity_morris::MorrisRequest, _>(
        state,
        body,
        sensitivity_jobs::SensitivityJobKind::Morris,
        sensitivity_jobs::SensitivityJobRequest::Morris,
    )
    .await
}

/// POST /api/sensitivity/sobol/start — async Sobol variance-decomposition job.
async fn handle_sensitivity_sobol_start(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    handle_sensitivity_start_inner::<crate::optimizer::sensitivity_sobol::SobolRequest, _>(
        state,
        body,
        sensitivity_jobs::SensitivityJobKind::Sobol,
        sensitivity_jobs::SensitivityJobRequest::Sobol,
    )
    .await
}

/// GET /api/sensitivity/jobs/:job_id/status — one-shot status snapshot.
async fn handle_sensitivity_job_status(Path(job_id): Path<String>) -> impl IntoResponse {
    match sensitivity_jobs::get_job_status(&job_id) {
        Ok(response) => match serde_json::to_string(&response) {
            Ok(payload) => ok_json(payload).into_response(),
            Err(e) => error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
        },
        Err(sensitivity_jobs::SensitivityJobError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Job not found").into_response()
        }
        Err(sensitivity_jobs::SensitivityJobError::Serialize(e)) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response()
        }
    }
}

/// GET /api/sensitivity/jobs/:job_id/stream — SSE stream of job progress until done/error.
async fn handle_sensitivity_job_stream(Path(job_id): Path<String>) -> impl IntoResponse {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);
    tokio::spawn(async move {
        let job_id = job_id.clone();
        let mut last_payload = String::new();
        loop {
            match sensitivity_jobs::get_job_status(&job_id) {
                Ok(response) => {
                    let done = response.status == "done" || response.status == "error";
                    let payload = match serde_json::to_string(&response) {
                        Ok(s) => s,
                        Err(_) => {
                            let event = Event::default()
                                .data(r#"{"status":"error","error":"Serialization error"}"#);
                            let _ = tx.send(Ok(event)).await;
                            break;
                        }
                    };
                    if !done && payload == last_payload {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        continue;
                    }
                    last_payload = payload.clone();
                    let event = Event::default().data(payload);
                    if tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                    if done {
                        break;
                    }
                }
                Err(sensitivity_jobs::SensitivityJobError::NotFound) => {
                    let event =
                        Event::default().data(r#"{"status":"error","error":"Job not found"}"#);
                    let _ = tx.send(Ok(event)).await;
                    break;
                }
                Err(sensitivity_jobs::SensitivityJobError::Serialize(_)) => {
                    let event = Event::default()
                        .data(r#"{"status":"error","error":"Serialization error"}"#);
                    let _ = tx.send(Ok(event)).await;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    });
    let stream = ReceiverStream::new(rx);
    Sse::new(stream)
}

/// POST /api/sensitivity/jobs/:job_id/cancel — request cancellation of a running sensitivity job.
async fn handle_sensitivity_job_cancel(Path(job_id): Path<String>) -> impl IntoResponse {
    match sensitivity_jobs::cancel_job(&job_id) {
        Ok(()) => ok_json(r#"{"status":"ok","message":"Cancelled"}"#.to_string()).into_response(),
        Err(sensitivity_jobs::SensitivityJobError::NotFound) => {
            error_json(StatusCode::NOT_FOUND, "Job not found").into_response()
        }
        Err(sensitivity_jobs::SensitivityJobError::Serialize(e)) => {
            error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Sync handlers
// ---------------------------------------------------------------------------

async fn handle_sync_status(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let profile_id = profile_id_from_request(&headers, &params);
    let (status, body) = sync::sync_status_payload(profile_id.as_deref());
    JsonResponse { status, body }.into_response()
}

async fn handle_sync_ingress(headers: HeaderMap, body: String) -> impl IntoResponse {
    let token = headers
        .get("stfc-sync-token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let (status, response_body) = sync::ingress_payload(&body, token.as_deref());
    JsonResponse {
        status,
        body: response_body,
    }
    .into_response()
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
    <input id="hostile" value="2918121098" />
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

#[cfg(test)]
mod spa_cache_control_tests {
    use super::{spa_asset_cache_control, spa_fallback_for_missing};
    use axum::http::StatusCode;

    #[test]
    fn missing_vite_asset_returns_404_not_index_html() {
        let dir = std::env::temp_dir().join(format!(
            "kobayashi_spa_fallback_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("index.html"),
            "<!DOCTYPE html><html><body>ok</body></html>",
        )
        .unwrap();

        let resp = spa_fallback_for_missing("assets/missing-chunk.js", &dir);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let resp = spa_fallback_for_missing("workspace/deep-link", &dir);
        assert_eq!(resp.status(), StatusCode::OK);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_and_root_are_no_cache() {
        assert_eq!(spa_asset_cache_control(""), "no-cache");
        assert_eq!(spa_asset_cache_control("index.html"), "no-cache");
        assert_eq!(spa_asset_cache_control("INDEX.HTML"), "no-cache");
    }

    #[test]
    fn vite_assets_are_immutable_long_cache() {
        assert_eq!(
            spa_asset_cache_control("assets/index-abc123.js"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            spa_asset_cache_control("assets/index-abc123.css"),
            "public, max-age=31536000, immutable"
        );
    }

    #[test]
    fn other_root_files_short_cache() {
        assert_eq!(spa_asset_cache_control("vite.svg"), "public, max-age=86400");
    }
}
