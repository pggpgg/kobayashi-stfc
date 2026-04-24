//! Optional per-profile knobs for Monte Carlo budget heuristics (`profiles/<id>/optimizer_budget_hints.json`).

use serde::Deserialize;

use crate::data::profile_index::{profile_path, OPTIMIZER_BUDGET_HINTS_JSON};

/// Optional JSON overrides; missing file or parse error ⇒ treat as no hints.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OptimizerBudgetHints {
    /// Multiplier on tiered **adaptive coarse** scout iterations (applied after `scout_coarse_sims_from_cap`, before refine).
    #[serde(default)]
    pub tiered_scout_coarse_mult: Option<f64>,
}

pub fn load_for_profile(profile_id: &str) -> Option<OptimizerBudgetHints> {
    let path = profile_path(profile_id, OPTIMIZER_BUDGET_HINTS_JSON);
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Apply `hints.tiered_scout_coarse_mult` to coarse scout cap (clamped to `[1, scout_cap - 1]` when `scout_cap > 1`).
pub fn apply_tiered_coarse_hint(coarse_sims: usize, scout_cap: usize, hints: Option<&OptimizerBudgetHints>) -> usize {
    let Some(h) = hints else {
        return coarse_sims;
    };
    let Some(m) = h.tiered_scout_coarse_mult else {
        return coarse_sims;
    };
    if !m.is_finite() || m <= 0.0 {
        return coarse_sims;
    }
    let s = scout_cap.max(1);
    let mut q = ((coarse_sims as f64) * m).round() as usize;
    q = q.max(1);
    if s > 1 {
        q = q.min(s.saturating_sub(1));
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coarse_hint_scales_and_clamps() {
        let h = OptimizerBudgetHints {
            tiered_scout_coarse_mult: Some(0.5),
        };
        assert_eq!(apply_tiered_coarse_hint(400, 500, Some(&h)), 200);
        assert_eq!(apply_tiered_coarse_hint(400, 500, None), 400);
        let bad = OptimizerBudgetHints {
            tiered_scout_coarse_mult: Some(-1.0),
        };
        assert_eq!(apply_tiered_coarse_hint(400, 500, Some(&bad)), 400);
    }
}
