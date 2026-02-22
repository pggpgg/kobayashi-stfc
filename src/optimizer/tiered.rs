//! Tiered simulation: two-pass strategy (cheap scouting pass → expensive confirmation).
//! Described in DESIGN.md and README; not yet wired into the main optimization flow in `optimizer::mod`.
//! The active path is `CrewGenerator` → `run_monte_carlo` → `rank_results`.

/// Placeholder for tiered optimization. When implemented, will run a fast scouting pass
/// to prune candidates, then a full Monte Carlo confirmation on the short list.
pub fn run_tiered() {
    // Placeholder implementation.
}
