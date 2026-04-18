pub mod api;
pub mod api_key;
pub(crate) mod cpu_admission;
pub mod openapi;
pub mod profile_backup;
pub mod routes;
pub mod static_files;
pub mod sync;

use std::net::SocketAddr;
use tracing::info;

/// Max concurrent CPU-heavy API jobs (`spawn_blocking`), from `KOBAYASHI_MAX_CONCURRENT_CPU_JOBS` (default 1).
pub(crate) fn max_concurrent_cpu_jobs_from_env() -> usize {
    std::env::var("KOBAYASHI_MAX_CONCURRENT_CPU_JOBS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1)
}

/// Start the Axum HTTP server and block until it shuts down.
///
/// This function is `async` and must be called from a tokio runtime.
/// `main.rs` builds the runtime explicitly for the `serve` command so that
/// all other CLI sub-commands remain synchronous.
pub async fn run_server_async(bind_addr: &str) -> std::io::Result<()> {
    crate::parallel::init_from_env();

    let addr: SocketAddr = bind_addr
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    // Validate all data files before accepting any connections.
    // This catches corrupt or missing records immediately rather than surfacing
    // mid-simulation after the user has already waited minutes.
    info!("validating data files before accepting connections");
    crate::data::validate::validate_all_startup_data()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    crate::data::profile_index::ensure_profile_index_bootstrap()
        .map_err(|e| std::io::Error::other(format!("Profile index bootstrap failed: {e}")))?;
    crate::data::profile_index::prune_ephemeral_scenario_test_profiles()
        .map_err(|e| std::io::Error::other(format!("Ephemeral profile prune failed: {e}")))?;
    crate::data::profile_index::sync_profile_index_with_disk()
        .map_err(|e| std::io::Error::other(format!("Profile index sync failed: {e}")))?;

    let registry = crate::data::data_registry::DataRegistry::load().map_err(|e| {
        std::io::Error::other(
            format!("Failed to load data registry: {e}. Ensure data/officers/officers.canonical.json exists."),
        )
    })?;

    let app = routes::build_router(registry);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%bind_addr, "kobayashi server listening");
    info!("sync ingress uses token-based routing (per-profile stfc-sync-token)");
    if std::env::var("KOBAYASHI_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        info!(
            "mutating /api routes require Authorization: Bearer or X-Api-Key \
             (loopback trusted by default; set KOBAYASHI_API_KEY_TRUST_LOOPBACK=0 to require the key everywhere); \
             see docs/DEPLOYMENT_SECURITY.md"
        );
    }
    if static_files::static_files_available() {
        info!("serving SPA from frontend/dist");
    } else {
        info!(
            "SPA not found (API-only); build the UI with: cd frontend && npm install && npm run build"
        );
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    eprintln!("kobayashi: server stopped.");
    Ok(())
}

/// Wait for Ctrl+C (all platforms) or SIGTERM (Unix) so `axum::serve` can shut down cleanly.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sig = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        sig.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

/// Synchronous entry point: creates a tokio runtime and drives the async server.
///
/// Called from `main.rs` and `cli.rs` for the `serve` sub-command.
pub fn run_server(bind_addr: &str) -> std::io::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(std::io::Error::other)?
        .block_on(run_server_async(bind_addr))
}
