//! Generic in-memory async-job registry shared by the optimize and sensitivity routes.
//!
//! Both kinds of jobs need the same plumbing:
//! - a global `HashMap<String, JobState>` keyed by job id,
//! - a parallel map of `Arc<AtomicBool>` cancel flags,
//! - oldest-finished eviction when the cap is exceeded (running jobs never evicted),
//! - mutex-poison recovery so a panicked worker thread doesn't bring the server down,
//! - deterministic `<prefix>_<unix_ms>_<counter>` ids for sortable lookup.
//!
//! The state shape and id prefix differ per job kind. Variations are parameterised via
//! the [`JobState`] trait (which exposes `started_at_ms` + `is_terminal`) and per-call
//! arguments (`prefix`, `max_retained`); everything else is shared.
//!
//! ## Why a shared module
//!
//! `src/server/api/execution.rs` (optimize jobs) and `src/server/sensitivity_jobs.rs`
//! (PR #192) each carried ~80 lines of this plumbing. Two copies were tolerable; a
//! third copy was the trigger to consolidate (#194).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Per-kind shape that the registry needs to know about the state it stores.
pub trait JobState: Clone + Send + 'static {
    /// Unix-milliseconds timestamp set when the job was inserted. Used to evict the
    /// oldest *finished* jobs when the registry exceeds its retention cap. Storing this
    /// on the state itself (instead of re-parsing from the job id) keeps the registry
    /// id-format-agnostic.
    fn started_at_ms(&self) -> u128;

    /// `true` once the job is finished (success or failure) and can be safely evicted.
    /// Running jobs are never evicted regardless of cap.
    fn is_terminal(&self) -> bool;
}

/// Generic registry. Construct as a `static` and call into it from the per-kind module
/// (see `sensitivity_jobs.rs` and `api/execution.rs` for live uses).
pub struct JobRegistry<S: JobState> {
    states: OnceLock<Mutex<HashMap<String, S>>>,
    cancel_flags: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    counter: OnceLock<AtomicU64>,
}

impl<S: JobState> Default for JobRegistry<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: JobState> JobRegistry<S> {
    pub const fn new() -> Self {
        Self {
            states: OnceLock::new(),
            cancel_flags: OnceLock::new(),
            counter: OnceLock::new(),
        }
    }

    fn states_lock(&self) -> MutexGuard<'_, HashMap<String, S>> {
        self.states
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn cancel_lock(&self) -> MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.cancel_flags
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Mint a job id of shape `<prefix>_<unix_ms>_<counter>`. The counter is per-registry
    /// (not per-prefix), which is fine because different prefixes can never collide.
    pub fn next_id(&self, prefix: &str) -> String {
        let counter = self.counter.get_or_init(|| AtomicU64::new(0));
        let n = counter.fetch_add(1, Ordering::Relaxed);
        let ms = now_ms();
        format!("{prefix}_{ms}_{n}")
    }

    /// Insert a freshly started job, register its cancel flag, and prune oldest finished
    /// jobs if the total count exceeds `max_retained`.
    pub fn insert(&self, id: String, state: S, cancel: Arc<AtomicBool>, max_retained: usize) {
        let mut states = self.states_lock();
        states.insert(id.clone(), state);
        let mut flags = self.cancel_lock();
        flags.insert(id, cancel);
        prune_completed(&mut states, &mut flags, max_retained);
    }

    /// Snapshot the job state by id, cloning out so callers don't hold the lock.
    pub fn get(&self, id: &str) -> Option<S> {
        self.states_lock().get(id).cloned()
    }

    /// Run `f` with mutable access to the job state, if it exists. Returns `f`'s output
    /// wrapped in `Some`, or `None` when the id isn't in the registry.
    pub fn with_state_mut<F, R>(&self, id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&mut S) -> R,
    {
        let mut states = self.states_lock();
        states.get_mut(id).map(f)
    }

    /// Set the cancel flag on the given job. Returns `true` if the job was found.
    /// Idempotent — calling on an already-cancelled job is a no-op success.
    pub fn cancel(&self, id: &str) -> bool {
        let flag = self.cancel_lock().get(id).cloned();
        match flag {
            Some(f) => {
                f.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Remove the cancel flag entry. Called by the worker thread at the end of its
    /// run; the job state itself is left behind so status / SSE polls still work.
    pub fn remove_cancel(&self, id: &str) {
        self.cancel_lock().remove(id);
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn prune_completed<S: JobState>(
    states: &mut HashMap<String, S>,
    flags: &mut HashMap<String, Arc<AtomicBool>>,
    max_retained: usize,
) {
    while states.len() > max_retained {
        let Some(oldest_id) = states
            .iter()
            .filter(|(_, s)| s.is_terminal())
            .map(|(id, s)| (s.started_at_ms(), id.clone()))
            .min_by(|(a_ts, a_id), (b_ts, b_id)| a_ts.cmp(b_ts).then_with(|| a_id.cmp(b_id)))
            .map(|(_, id)| id)
        else {
            // Only running jobs left; nothing safe to evict. Cap is soft in that case.
            break;
        };
        states.remove(&oldest_id);
        flags.remove(&oldest_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestState {
        started: u128,
        terminal: bool,
    }
    impl JobState for TestState {
        fn started_at_ms(&self) -> u128 {
            self.started
        }
        fn is_terminal(&self) -> bool {
            self.terminal
        }
    }

    #[test]
    fn insert_and_get_round_trip() {
        let reg: JobRegistry<TestState> = JobRegistry::new();
        let id = reg.next_id("test");
        reg.insert(
            id.clone(),
            TestState {
                started: 1,
                terminal: false,
            },
            Arc::new(AtomicBool::new(false)),
            10,
        );
        let got = reg.get(&id).expect("present");
        assert_eq!(got.started, 1);
    }

    #[test]
    fn next_id_uses_prefix_and_is_monotonic() {
        let reg: JobRegistry<TestState> = JobRegistry::new();
        let a = reg.next_id("foo");
        let b = reg.next_id("foo");
        assert!(a.starts_with("foo_"));
        assert!(b.starts_with("foo_"));
        assert_ne!(a, b);
    }

    #[test]
    fn cancel_sets_flag_and_returns_true() {
        let reg: JobRegistry<TestState> = JobRegistry::new();
        let id = reg.next_id("c");
        let flag = Arc::new(AtomicBool::new(false));
        reg.insert(
            id.clone(),
            TestState {
                started: 1,
                terminal: false,
            },
            Arc::clone(&flag),
            10,
        );
        assert!(reg.cancel(&id));
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn cancel_unknown_returns_false() {
        let reg: JobRegistry<TestState> = JobRegistry::new();
        assert!(!reg.cancel("does-not-exist"));
    }

    #[test]
    fn prune_evicts_oldest_finished_first() {
        let reg: JobRegistry<TestState> = JobRegistry::new();
        // Insert 5 finished jobs (started_at_ms 1..=5) then 1 running, max_retained=3.
        // Expect: 3 newest finished (3, 4, 5) plus the running job → 4 total survive,
        // since running jobs are never evicted. (Cap is soft when only running jobs are
        // candidates for removal.)
        for i in 1..=5 {
            reg.insert(
                reg.next_id("p"),
                TestState {
                    started: i as u128,
                    terminal: true,
                },
                Arc::new(AtomicBool::new(false)),
                3,
            );
        }
        let running_id = reg.next_id("p");
        reg.insert(
            running_id.clone(),
            TestState {
                started: 99,
                terminal: false,
            },
            Arc::new(AtomicBool::new(false)),
            3,
        );
        // After the last insert (which also prunes), we expect ≤ 3 finished + 1 running.
        let live: Vec<TestState> = (1..=5)
            .map(|i| TestState {
                started: i as u128,
                terminal: true,
            })
            .filter(|_| false)
            .collect();
        let _ = live; // unused — assertions below test the registry directly.
                      // Easier assertion: the oldest-finished (started=1) is gone.
        let states = reg.states_lock();
        assert!(states.values().all(|s| s.started >= 3 || !s.terminal));
        assert!(states.contains_key(&running_id));
    }
}
