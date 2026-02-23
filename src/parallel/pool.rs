//! Rayon thread pool configuration for simulation workloads.
//!
//! Use [WorkerPool::install] to run parallel simulation (e.g. Monte Carlo) with a
//! fixed number of threads, or rely on Rayon's default (all CPU cores).

use rayon::ThreadPoolBuilder;

/// Configures how many worker threads are used for parallel batch execution.
#[derive(Debug, Clone, Copy)]
pub struct WorkerPool {
    /// Number of worker threads. If 0, use Rayon default (num_cpus).
    pub workers: usize,
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self {
            workers: 0, // Rayon default
        }
    }
}

impl WorkerPool {
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
