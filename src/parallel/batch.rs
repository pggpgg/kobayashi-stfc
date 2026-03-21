//! Batch distribution for parallel simulation.
//!
//! Splits work into batches for parallel execution or progress reporting.
//! The Monte Carlo runner uses one candidate per parallel task; this module
//! provides helpers for batch boundaries and optional chunked iteration.

/// Target batch count for tiered / progress-chunked Monte Carlo: balances Rayon parallelism vs
/// fewer `SharedScenarioData` clones and progress updates.
pub fn monte_carlo_batch_count_for_candidates(total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let threads = rayon::current_num_threads().max(1);
    // Roughly one batch per ~64 candidates, bounded by thread count and a small cap.
    let by_size = total.div_ceil(64);
    by_size
        .max(1)
        .min(total)
        .min(threads.saturating_mul(8).max(1))
        .min(40)
}

/// Split `total` items into up to `num_batches` ranges `[start, end)`.
/// Batches are as equal in size as possible; later batches may be smaller.
///
/// # Example
/// ```
/// # use kobayashi::parallel::batch_ranges;
/// let ranges = batch_ranges(100, 4);
/// assert_eq!(ranges, vec![(0, 25), (25, 50), (50, 75), (75, 100)]);
/// ```
pub fn batch_ranges(total: usize, num_batches: usize) -> Vec<(usize, usize)> {
    if total == 0 || num_batches == 0 {
        return Vec::new();
    }
    let num_batches = num_batches.min(total);
    let base = total / num_batches;
    let remainder = total % num_batches;
    let mut ranges = Vec::with_capacity(num_batches);
    let mut start = 0;
    for i in 0..num_batches {
        let size = base + if i < remainder { 1 } else { 0 };
        let end = start + size;
        ranges.push((start, end));
        start = end;
    }
    ranges
}

/// Run parallel Monte Carlo simulation distributed across workers.
/// This is a convenience that calls [crate::optimizer::monte_carlo::run_monte_carlo_parallel]
/// inside [crate::parallel::pool::WorkerPool::install] when a custom worker count is set.
pub fn run_simulation_batches(
    ship: &str,
    hostile: &str,
    candidates: &[crate::optimizer::crew_generator::CrewCandidate],
    iterations: usize,
    seed: u64,
    allow_duplicate_officers: bool,
    pool: &crate::parallel::pool::WorkerPool,
) -> Vec<crate::optimizer::monte_carlo::SimulationResult> {
    pool.install(|| {
        crate::optimizer::monte_carlo::run_monte_carlo_parallel(
            ship,
            hostile,
            candidates,
            iterations,
            seed,
            allow_duplicate_officers,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_ranges_even_split() {
        let r = batch_ranges(100, 4);
        assert_eq!(r, vec![(0, 25), (25, 50), (50, 75), (75, 100)]);
    }

    #[test]
    fn batch_ranges_with_remainder() {
        let r = batch_ranges(10, 3);
        assert_eq!(r, vec![(0, 4), (4, 7), (7, 10)]);
    }

    #[test]
    fn batch_ranges_more_batches_than_items() {
        let r = batch_ranges(3, 10);
        assert_eq!(r.len(), 3);
        assert_eq!(r, vec![(0, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn batch_ranges_empty() {
        assert!(batch_ranges(0, 5).is_empty());
        assert!(batch_ranges(10, 0).is_empty());
    }

    #[test]
    fn monte_carlo_batch_count_nonzero_for_work() {
        assert_eq!(super::monte_carlo_batch_count_for_candidates(0), 0);
        let n = super::monte_carlo_batch_count_for_candidates(500);
        assert!(n >= 1 && n <= 40);
    }
}
