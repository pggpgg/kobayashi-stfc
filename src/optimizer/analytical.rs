//! Closed-form expected damage calculator for analytical pre-filtering.
//! Described in DESIGN.md; not yet wired into the main optimization flow in `optimizer::mod`.
//! When implemented, can prune obviously weak combos before any Monte Carlo runs.

/// Placeholder for analytical expected damage. When implemented, will return a closed-form
/// estimate so callers can skip simulation for clearly suboptimal crews.
pub fn expected_damage() -> f32 {
    0.0
}
