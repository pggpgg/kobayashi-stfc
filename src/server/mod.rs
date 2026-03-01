pub mod api;
pub mod routes;
pub mod static_files;
pub mod sync;

use std::net::SocketAddr;

/// Start the Axum HTTP server and block until it shuts down.
///
/// This function is `async` and must be called from a tokio runtime.
/// `main.rs` builds the runtime explicitly for the `serve` command so that
/// all other CLI sub-commands remain synchronous.
pub async fn run_server_async(bind_addr: &str) -> std::io::Result<()> {
    let addr: SocketAddr = bind_addr
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    // Validate all data files before accepting any connections.
    // This catches corrupt or missing records immediately rather than surfacing
    // mid-simulation after the user has already waited minutes.
    println!("kobayashi: validating data files…");
    crate::data::validate::validate_all_startup_data().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    let registry = crate::data::data_registry::DataRegistry::load().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to load data registry: {e}. Ensure data/officers/officers.canonical.json exists."),
        )
    })?;

    let app = routes::build_router(registry);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("kobayashi server listening on http://{bind_addr}");
    if static_files::static_files_available() {
        println!("  SPA: serving frontend from frontend/dist");
    } else {
        println!(
            "  SPA: not found (API-only mode). \
             To use the MVP UI: cd frontend, run 'npm install' then 'npm run build', \
             then restart the server from the project root."
        );
    }

    axum::serve(listener, app).await?;
    Ok(())
}

/// Synchronous entry point: creates a tokio runtime and drives the async server.
///
/// Called from `main.rs` and `cli.rs` for the `serve` sub-command.
pub fn run_server(bind_addr: &str) -> std::io::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
        .block_on(run_server_async(bind_addr))
}
