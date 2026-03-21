//! Rayon thread pool configuration for simulation workloads.
//!
//! Call [init_from_env] once at process startup (before any `par_iter`) so
//! `KOBAYASHI_RAYON_THREADS` caps the **global** Rayon pool used by Monte Carlo.
//!
//! Use [WorkerPool::install] to run work on a separate fixed-size pool (e.g. helpers in
//! [crate::parallel::batch]); when `workers` is 0, that uses the same global pool.

use rayon::ThreadPoolBuilder;
use std::sync::Once;

static INIT_PARALLEL_RUNTIME: Once = Once::new();

/// Applies `KOBAYASHI_RAYON_THREADS` and (on Windows) `KOBAYASHI_LOW_PRIORITY` once per process.
///
/// Safe to call from `main`, `serve`, and tests; subsequent calls are no-ops.
/// If Rayon’s global pool was already initialized (e.g. another test used `par_iter` first),
/// a custom thread count cannot be applied and a note is printed to stderr.
pub fn init_from_env() {
    INIT_PARALLEL_RUNTIME.call_once(|| {
        init_rayon_global_pool_from_env();
        #[cfg(windows)]
        init_low_priority_on_windows_from_env();
    });
}

fn init_rayon_global_pool_from_env() {
    let workers = std::env::var("KOBAYASHI_RAYON_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    if workers == 0 {
        return;
    }
    let threads = workers.max(1);
    match ThreadPoolBuilder::new().num_threads(threads).build_global() {
        Ok(_) => {}
        Err(e) => {
            eprintln!(
                "kobayashi: KOBAYASHI_RAYON_THREADS={threads} not applied (Rayon global pool already initialized): {e}"
            );
        }
    }
}

#[cfg(windows)]
fn init_low_priority_on_windows_from_env() {
    let on = std::env::var("KOBAYASHI_LOW_PRIORITY")
        .ok()
        .map(|s| {
            let t = s.to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false);
    if !on {
        return;
    }
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, SetPriorityClass, BELOW_NORMAL_PRIORITY_CLASS,
        };
        SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS);
    }
}

/// Configures how many worker threads are used for parallel batch execution.
#[derive(Debug, Clone, Copy)]
pub struct WorkerPool {
    /// Number of worker threads. If 0, use Rayon default (num_cpus).
    pub workers: usize,
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::from_env_or_default()
    }
}

impl WorkerPool {
    /// Thread count from `KOBAYASHI_RAYON_THREADS` (empty or invalid → Rayon default / all cores).
    pub fn from_env_or_default() -> Self {
        let workers = std::env::var("KOBAYASHI_RAYON_THREADS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        Self { workers }
    }

    /// Use all available CPU cores (Rayon default).
    pub fn default_workers() -> Self {
        Self::default()
    }

    /// Use exactly `n` worker threads.
    pub fn with_workers(n: usize) -> Self {
        Self { workers: n }
    }

    /// Run a closure on a thread pool with this worker count. If [workers](WorkerPool::workers) is 0,
    /// uses the global Rayon pool (all cores). Otherwise builds a temporary pool with that many threads.
    pub fn install<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        if self.workers == 0 {
            f()
        } else {
            let pool = ThreadPoolBuilder::new()
                .num_threads(self.workers)
                .build()
                .expect("Rayon thread pool");
            pool.install(f)
        }
    }
}
