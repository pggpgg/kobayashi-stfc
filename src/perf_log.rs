//! Optional wall-clock logging for optimizer hot paths (set `KOBAYASHI_PERF_LOG=1`).

use std::time::Instant;

#[inline]
pub(crate) fn perf_start() -> Option<Instant> {
    if std::env::var("KOBAYASHI_PERF_LOG")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        Some(Instant::now())
    } else {
        None
    }
}

pub(crate) fn log_duration(label: &str, start: Option<Instant>) {
    if let Some(t0) = start {
        eprintln!("[kobayashi-perf] {label}: {:?}", t0.elapsed());
    }
}
