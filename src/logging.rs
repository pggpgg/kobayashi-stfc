//! Default [`tracing`] subscriber for CLI and server.
//!
//! - `RUST_LOG` has highest precedence and accepts a full [`tracing_subscriber::EnvFilter`] value.
//! - `KOBAYASHI_LOG` is an alias for convenience in run scripts. It accepts either:
//!   - a plain level (`info`, `debug`, ...), mapped to `warn,kobayashi=<level>`
//!   - or a full `EnvFilter` expression (`kobayashi=debug,tower_http=info`)
//! - If neither is set, default is `warn,kobayashi=info`.
//!
//! Logs are emitted as newline-delimited JSON so request and optimizer fields are machine-readable.

use tracing_subscriber::EnvFilter;

fn env_filter_from_env() -> EnvFilter {
    if let Ok(raw) = std::env::var("RUST_LOG") {
        if let Ok(filter) = EnvFilter::try_new(raw.trim()) {
            return filter;
        }
    }

    if let Ok(raw) = std::env::var("KOBAYASHI_LOG") {
        let raw = raw.trim();
        if !raw.is_empty() {
            // Accept either a full EnvFilter expression or a simple level shorthand.
            let candidate = if raw.contains('=') || raw.contains(',') {
                raw.to_string()
            } else {
                format!("warn,kobayashi={raw}")
            };
            if let Ok(filter) = EnvFilter::try_new(candidate) {
                return filter;
            }
        }
    }

    EnvFilter::new("warn,kobayashi=info")
}

/// Install a fmt subscriber once per process; later calls are no-ops.
pub fn init() {
    let filter = env_filter_from_env();
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_target(true)
        .with_current_span(true)
        .with_span_list(true)
        .try_init();
}
