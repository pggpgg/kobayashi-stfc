//! Binary-only requested-allocation counters. Never used for timing reports.

use std::alloc::System;

pub const AVAILABLE: bool = cfg!(feature = "bench-alloc-count");

#[cfg(not(feature = "bench-alloc-count"))]
#[global_allocator]
static ALLOCATOR: System = System;

#[derive(serde::Serialize)]
pub struct Counts {
    pub alloc_calls: u64,
    pub alloc_zeroed_calls: u64,
    pub realloc_calls: u64,
    pub dealloc_calls: u64,
    pub alloc_requested_bytes: u64,
    pub alloc_zeroed_requested_bytes: u64,
    pub realloc_new_requested_bytes: u64,
    pub dealloc_layout_bytes: u64,
    pub scope: &'static str,
}

#[cfg(feature = "bench-alloc-count")]
mod enabled {
    use super::{Counts, System};
    use std::alloc::{GlobalAlloc, Layout};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    static ENABLED: AtomicBool = AtomicBool::new(false);
    static ALLOC: AtomicU64 = AtomicU64::new(0);
    static ZEROED: AtomicU64 = AtomicU64::new(0);
    static REALLOC: AtomicU64 = AtomicU64::new(0);
    static DEALLOC: AtomicU64 = AtomicU64::new(0);
    static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static ZEROED_BYTES: AtomicU64 = AtomicU64::new(0);
    static REALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

    struct CountingSystem;

    // SAFETY: every operation delegates to System with the original pointer and
    // layout. Counter operations allocate no memory and never touch allocation
    // contents. Counts describe requests, not successful/live/peak heap bytes.
    unsafe impl GlobalAlloc for CountingSystem {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if ENABLED.load(Ordering::Relaxed) {
                ALLOC.fetch_add(1, Ordering::Relaxed);
                ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            // SAFETY: uphold the caller's GlobalAlloc contract through System.
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if ENABLED.load(Ordering::Relaxed) {
                ZEROED.fetch_add(1, Ordering::Relaxed);
                ZEROED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            // SAFETY: uphold the caller's GlobalAlloc contract through System.
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if ENABLED.load(Ordering::Relaxed) {
                REALLOC.fetch_add(1, Ordering::Relaxed);
                REALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            }
            // SAFETY: pointer, original layout, and new size are delegated unchanged.
            unsafe { System.realloc(pointer, layout, new_size) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            if ENABLED.load(Ordering::Relaxed) {
                DEALLOC.fetch_add(1, Ordering::Relaxed);
                DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            // SAFETY: pointer and allocation layout are delegated unchanged.
            unsafe { System.dealloc(pointer, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingSystem = CountingSystem;

    pub struct Window;

    impl Window {
        pub fn finish(self) -> Counts {
            ENABLED.store(false, Ordering::SeqCst);
            Counts {
                alloc_calls: ALLOC.load(Ordering::Relaxed),
                alloc_zeroed_calls: ZEROED.load(Ordering::Relaxed),
                realloc_calls: REALLOC.load(Ordering::Relaxed),
                dealloc_calls: DEALLOC.load(Ordering::Relaxed),
                alloc_requested_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
                alloc_zeroed_requested_bytes: ZEROED_BYTES.load(Ordering::Relaxed),
                realloc_new_requested_bytes: REALLOC_BYTES.load(Ordering::Relaxed),
                dealloc_layout_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
                scope: "GlobalAlloc requests during the selected fixed TraceOff loop only; initial preparation/probes/output excluded; per-combat setup included only for the wrapper lane; not live or peak bytes and not timing data",
            }
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            ENABLED.store(false, Ordering::SeqCst);
        }
    }

    pub fn start() -> Result<Window, String> {
        if ENABLED.load(Ordering::SeqCst) {
            return Err("Allocation counter is already active".into());
        }
        for counter in [
            &ALLOC,
            &ZEROED,
            &REALLOC,
            &DEALLOC,
            &ALLOC_BYTES,
            &ZEROED_BYTES,
            &REALLOC_BYTES,
            &DEALLOC_BYTES,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
        ENABLED.store(true, Ordering::SeqCst);
        Ok(Window)
    }
}

#[cfg(feature = "bench-alloc-count")]
pub use enabled::start;

#[cfg(not(feature = "bench-alloc-count"))]
pub struct Window;

#[cfg(not(feature = "bench-alloc-count"))]
impl Window {
    pub fn finish(self) -> Counts {
        unreachable!("allocation mode is rejected when the feature is absent")
    }
}

#[cfg(not(feature = "bench-alloc-count"))]
pub fn start() -> Result<Window, String> {
    Err("Allocation counters require --features bench-alloc-count".into())
}
