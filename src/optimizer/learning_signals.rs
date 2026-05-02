//! Learning signals computed from optimize-history entries.
//!
//! Provides [`LearningSignals`] — convergence and diversity metrics derived from a
//! [`crate::data::optimize_history::OptimizeHistoryEntry`]. These signals feed into
//! the auto-tuning loop (Phase B of the local learning loop) to adjust exploration
//! parameters without user intervention.

use crate::data::optimize_history::OptimizeHistoryEntry;
use std::collections::HashSet;

/// Convergence and diversity signals from a single history entry.
/// Used to auto-tune exploration parameters on subsequent runs.
#[derive(Debug, Clone)]
pub struct LearningSignals {
    /// Wilson interval lower bound of rank-1 minus Wilson lower bound of rank-K.
    /// Large positive = clear winner (sims were sufficient).
    /// Near zero or negative = tight race (needs more sims).
    pub top_margin: f64,

    /// Fraction of unique officers across the top-K crews vs the total officer
    /// slots in those crews. Ranges [0.0, 1.0].
    /// - 1.0 = every slot is a different officer (maximum diversity).
    /// - Near 0.0 = same few officers repeat across all crews (stagnation).
    pub officer_diversity: f64,

    /// Jaccard similarity of captain+bridge sets between rank-1 and the rest of
    /// the top-K. Ranges [0.0, 1.0].
    /// - 1.0 = all top crews have identical captain and bridge (high stagnation).
    /// - 0.0 = completely different captain+bridge combos (healthy exploration).
    pub captain_bridge_stagnation: f64,
}

impl Default for LearningSignals {
    fn default() -> Self {
        Self {
            top_margin: 0.0,
            officer_diversity: 0.0,
            captain_bridge_stagnation: 0.0,
        }
    }
}

impl LearningSignals {
    /// Whether enough top crews exist to produce meaningful signals.
    pub fn has_data(&self) -> bool {
        // top_margin being exactly 0.0 is a marker for empty/inadequate data
        self.top_margin > 0.0 || self.officer_diversity > 0.0
    }
}

/// Compute learning signals from a single history entry.
///
/// Returns a [`LearningSignals`] with:
/// - `top_margin`: how confident we are that rank-1 beats rank-K (Wilson CIs).
/// - `officer_diversity`: how much officer variety exists in the top crews.
/// - `captain_bridge_stagnation`: how similar the top crews' captain+bridge combos are.
///
/// Returns default (all zeros) when the entry has fewer than 2 crews.
pub fn compute_learning_signals(entry: &OptimizeHistoryEntry) -> LearningSignals {
    if entry.crews.len() < 2 {
        return LearningSignals::default();
    }

    let k = entry.crews.len().min(8); // analyze top-8 at most
    let top_margin = compute_top_margin(&entry.crews, k);
    let officer_diversity = compute_officer_diversity(&entry.crews, k);
    let captain_bridge_stagnation = compute_captain_bridge_stagnation(&entry.crews, k);

    LearningSignals {
        top_margin,
        officer_diversity,
        captain_bridge_stagnation,
    }
}

/// Wilson lower bound of rank-1 minus Wilson lower bound of rank-K.
/// Large positive = clear separation. Near zero = tight race.
fn compute_top_margin(
    crews: &[crate::data::optimize_history::OptimizeHistoryCrewRecord],
    k: usize,
) -> f64 {
    let limit = k.min(crews.len());
    let first_low = crews[0].win_rate_ci_low;
    let kth_low = crews[limit - 1].win_rate_ci_low;
    (first_low - kth_low).max(0.0)
}

/// Fraction of unique officers in the top-K crews vs total officer slots.
fn compute_officer_diversity(
    crews: &[crate::data::optimize_history::OptimizeHistoryCrewRecord],
    k: usize,
) -> f64 {
    let limit = k.min(crews.len());
    let mut seen: HashSet<&str> = HashSet::new();
    let mut total_slots = 0usize;

    for crew in crews.iter().take(limit) {
        seen.insert(crew.captain.as_str());
        total_slots += 1;
        for b in &crew.bridge {
            seen.insert(b.as_str());
            total_slots += 1;
        }
        for bd in &crew.below_decks {
            seen.insert(bd.as_str());
            total_slots += 1;
        }
    }

    if total_slots == 0 {
        return 0.0;
    }
    seen.len() as f64 / total_slots as f64
}

/// Jaccard similarity of the captain+bridge set between the top crew and the
/// remaining top-K crews (averaged).
fn compute_captain_bridge_stagnation(
    crews: &[crate::data::optimize_history::OptimizeHistoryCrewRecord],
    k: usize,
) -> f64 {
    let limit = k.min(crews.len()).max(1);
    let first = &crews[0];
    let ref_set: HashSet<&str> = std::iter::once(first.captain.as_str())
        .chain(first.bridge.iter().map(String::as_str))
        .collect();

    if ref_set.is_empty() || limit < 2 {
        return 0.0;
    }

    let mut total_similarity = 0.0;
    let mut count = 0;

    for crew in crews.iter().take(limit).skip(1) {
        let crew_set: HashSet<&str> = std::iter::once(crew.captain.as_str())
            .chain(crew.bridge.iter().map(String::as_str))
            .collect();
        let intersection = ref_set.intersection(&crew_set).count();
        let union = ref_set.union(&crew_set).count();
        if union > 0 {
            total_similarity += intersection as f64 / union as f64;
        }
        count += 1;
    }

    if count == 0 {
        0.0
    } else {
        total_similarity / count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::optimize_history::OptimizeHistoryCrewRecord;

    fn make_crew(
        captain: &str,
        bridge: &[&str],
        below: &[&str],
        wr: f64,
        wr_low: f64,
    ) -> OptimizeHistoryCrewRecord {
        OptimizeHistoryCrewRecord {
            captain: captain.to_string(),
            bridge: bridge.iter().map(|s| s.to_string()).collect(),
            below_decks: below.iter().map(|s| s.to_string()).collect(),
            win_rate: wr,
            win_rate_ci_low: wr_low,
            win_rate_ci_high: wr_low + 0.10,
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.0,
            loss_rate: 0.0,
            loss_rate_ci_low: 0.0,
            loss_rate_ci_high: 0.0,
            r1_kill_rate: 0.0,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 0.0,
            avg_hull_remaining: 0.0,
            avg_hull_remaining_ci_low: 0.0,
            avg_hull_remaining_ci_high: 0.0,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            chain: None,
            trials_run: 100,
        }
    }

    fn make_entry(crews: Vec<OptimizeHistoryCrewRecord>) -> OptimizeHistoryEntry {
        OptimizeHistoryEntry {
            updated_at_ms: 0,
            sims: 1000,
            seed: 42,
            tiered_scout_sims: 100,
            tiered_top_k: 8,
            n_candidates: 50,
            optimize_history_kind: 0,
            tiered_scout_allocator: 1,
            tiered_budget_policy: 2,
            tiered_confirm_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            exhaustive_confirm_policy: 0,
            chain_fingerprint: "0".to_string(),
            crews,
        }
    }

    #[test]
    fn empty_entry_returns_defaults() {
        let entry = make_entry(vec![]);
        let signals = compute_learning_signals(&entry);
        assert!(!signals.has_data());
        assert!((signals.top_margin - 0.0).abs() < 1e-9);
    }

    #[test]
    fn single_crew_returns_defaults() {
        let crews = vec![make_crew("A", &["B", "C"], &["D", "E", "F"], 0.9, 0.85)];
        let entry = make_entry(crews);
        let signals = compute_learning_signals(&entry);
        assert!(!signals.has_data());
    }

    #[test]
    fn clear_winner_gives_large_margin() {
        let crews = vec![
            make_crew("A", &["B", "C"], &["D", "E", "F"], 0.95, 0.90),
            make_crew("X", &["Y", "Z"], &["U", "V", "W"], 0.60, 0.50),
        ];
        let entry = make_entry(crews);
        let signals = compute_learning_signals(&entry);
        assert!((signals.top_margin - 0.40).abs() < 1e-9);
    }

    #[test]
    fn identical_captain_bridge_gives_high_stagnation() {
        let crews = vec![
            make_crew("A", &["B", "C"], &["D", "E", "F"], 0.9, 0.85),
            make_crew("A", &["B", "C"], &["G", "H", "I"], 0.8, 0.75),
            make_crew("A", &["B", "C"], &["J", "K", "L"], 0.7, 0.65),
        ];
        let entry = make_entry(crews);
        let signals = compute_learning_signals(&entry);
        // Same captain+bridge in all 3 crews → Jaccard = 1.0
        assert!((signals.captain_bridge_stagnation - 1.0).abs() < 1e-9);
    }

    #[test]
    fn diverse_crews_give_high_diversity() {
        let crews = vec![
            make_crew("A1", &["B1", "C1"], &["D1", "E1", "F1"], 0.9, 0.85),
            make_crew("A2", &["B2", "C2"], &["D2", "E2", "F2"], 0.8, 0.75),
        ];
        let entry = make_entry(crews);
        let signals = compute_learning_signals(&entry);
        // 2 crews × (1+2+3) = 12 slots, 12 unique → diversity = 1.0
        assert!((signals.officer_diversity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn repeated_officers_reduce_diversity() {
        let crews = vec![
            make_crew("A", &["B", "C"], &["D", "E", "F"], 0.9, 0.85),
            make_crew("A", &["B", "C"], &["D", "E", "F"], 0.8, 0.75),
        ];
        let entry = make_entry(crews);
        let signals = compute_learning_signals(&entry);
        // 2 crews × 6 = 12 slots, 6 unique → diversity = 0.5
        assert!((signals.officer_diversity - 0.5).abs() < 1e-9);
    }
}
