//! Batch distribution for parallel simulation.
//!
//! Splits work into batches for parallel execution or progress reporting.
//! The Monte Carlo runner uses one candidate per parallel task; this module
//! provides helpers for batch boundaries and optional chunked iteration.

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
    pool: &crate::parallel::pool::WorkerPool,
) -> Vec<crate::optimizer::monte_carlo::SimulationResult> {
    pool.install(|| {
        crate::optimizer::monte_carlo::run_monte_carlo_parallel(
            ship, hostile, candidates, iterations, seed,
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
}
