//! Default [`tracing`] subscriber. Set `RUST_LOG` for fine-grained levels (e.g. `RUST_LOG=debug`).
//!
//! If `RUST_LOG` is unset, uses `warn` for dependencies and `info` for this crate so server and
//! sync lines remain visible without noisy third-party logs.

use tracing_subscriber::EnvFilter;

/// Install a fmt subscriber once per process; later calls are no-ops.
pub fn init() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,kobayashi=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}
