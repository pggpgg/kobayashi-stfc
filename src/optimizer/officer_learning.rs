//! Per-officer performance scores for learning-based warm start in crew generation.
//!
//! Maintains scores per `(officer_id, hostile_faction, ship_type)` derived from
//! optimization history. Used by [`crate::optimizer::crew_generator::sampled_candidates`]
//! to bias below-decks officer selection toward historically strong officers via
//! epsilon-greedy weighted sampling.

use crate::optimizer::ranking::RankedCrewResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Composite key for per-officer scores: officer name, hostile faction, and ship type.
/// Matches the grouping granularity recommended for learning which officers work well
/// in specific matchups.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OfficerScoreKey {
    pub officer_name: String,
    pub hostile_faction: String,
    pub ship_type: String,
}

/// Per-officer performance score derived from optimization history.
/// Higher score = officer appeared more frequently in top-ranked crews for this matchup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficerPerformanceScores {
    /// Scores keyed by `(officer_name, hostile_faction, ship_type)`.
    scores: HashMap<OfficerScoreKey, f64>,
    /// Minimum score assigned to officers with no history (ensures all have non-zero prob).
    floor: f64,
    /// Exploration rate for epsilon-greedy sampling (fraction of time to sample uniformly).
    /// Default: 0.20 (20% exploration).
    epsilon: f64,
    /// Exponential decay factor applied per update so old history fades.
    /// Default: 0.95 (each update retains 95% of prior score).
    decay: f64,
}

impl Default for OfficerPerformanceScores {
    fn default() -> Self {
        Self {
            scores: HashMap::new(),
            floor: 0.01,
            epsilon: 0.20,
            decay: 0.95,
        }
    }
}

impl OfficerPerformanceScores {
    /// Create with default parameters (0.01 floor, 0.20 epsilon, 0.95 decay).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom parameters.
    pub fn with_params(floor: f64, epsilon: f64, decay: f64) -> Self {
        Self {
            scores: HashMap::new(),
            floor: floor.max(0.0),
            epsilon: epsilon.clamp(0.0, 1.0),
            decay: decay.clamp(0.0, 1.0),
        }
    }

    /// Update scores from a set of ranked optimization results.
    ///
    /// For each result in `ranked`, officers appearing in higher-ranked crews receive
    /// a larger score increment. The increment for the K-th ranked crew is `1.0 / (K + 1)`,
    /// so the top-ranked crew's officers get +1.0, second gets +0.5, etc.
    ///
    /// Scores are decayed by `self.decay` before adding new increments, so old history fades.
    pub fn update_from_results(
        &mut self,
        ranked: &[RankedCrewResult],
        hostile_faction: &str,
        ship_type: &str,
    ) {
        // Decay existing scores
        for score in self.scores.values_mut() {
            *score *= self.decay;
        }

        // Add increments from ranked results (higher rank = larger increment)
        for (rank, result) in ranked.iter().enumerate() {
            let increment = 1.0 / (rank as f64 + 1.0);
            let officer_names: Vec<&str> = std::iter::once(result.captain.as_str())
                .chain(result.bridge.iter().map(String::as_str))
                .chain(result.below_decks.iter().map(String::as_str))
                .collect();

            for name in officer_names {
                let key = OfficerScoreKey {
                    officer_name: name.to_string(),
                    hostile_faction: hostile_faction.to_string(),
                    ship_type: ship_type.to_string(),
                };
                let entry = self.scores.entry(key).or_insert(0.0);
                *entry += increment;
            }
        }
    }

    /// Get the score for an officer in a specific matchup context.
    /// Returns at least `self.floor` so no officer has zero probability.
    pub fn get_score(&self, officer_name: &str, hostile_faction: &str, ship_type: &str) -> f64 {
        let key = OfficerScoreKey {
            officer_name: officer_name.to_string(),
            hostile_faction: hostile_faction.to_string(),
            ship_type: ship_type.to_string(),
        };
        self.scores
            .get(&key)
            .copied()
            .unwrap_or(self.floor)
            .max(self.floor)
    }

    /// Compute relative weights for a list of officer names (uniform across factions/ship types
    /// when no specific context is available). Each weight is `score / sum(scores)`.
    fn officer_weights(&self, names: &[String], hostile_faction: &str, ship_type: &str) -> Vec<f64> {
        let raw: Vec<f64> = names
            .iter()
            .map(|n| self.get_score(n, hostile_faction, ship_type))
            .collect();
        let total: f64 = raw.iter().sum();
        if total <= 0.0 {
            // Uniform fallback
            let w = 1.0 / names.len().max(1) as f64;
            return vec![w; names.len()];
        }
        raw.into_iter().map(|s| s / total).collect()
    }

    /// Select K distinct indices from `available_names` using epsilon-greedy weighted sampling.
    ///
    /// - With probability `epsilon`, each selection is uniform random.
    /// - With probability `1 - epsilon`, selections are weighted by historical scores.
    ///
    /// Desired number of selections is `k`. Returns indices into `available_names`, or empty
    /// if `available_names.len() < k`.
    pub fn epsilon_greedy_sample(
        &self,
        available_names: &[String],
        k: usize,
        hostile_faction: &str,
        ship_type: &str,
        rng: &mut impl RngExt,
    ) -> Vec<usize> {
        let n = available_names.len();
        if k == 0 || n < k {
            return Vec::new();
        }
        if k == n {
            return (0..n).collect();
        }

        let weights = self.officer_weights(available_names, hostile_faction, ship_type);

        let use_weighted = rng.next_f64() >= self.epsilon;

        if use_weighted {
            // Weighted reservoir sampling: select k indices without replacement
            weighted_sample_without_replacement(&weights, k, rng)
        } else {
            // Uniform random: shuffle indices and take first k
            let mut indices: Vec<usize> = (0..n).collect();
            fisher_yates_shuffle(&mut indices, rng);
            indices.truncate(k);
            indices
        }
    }

    /// Number of entries in the score map.
    pub fn len(&self) -> usize {
        self.scores.len()
    }

    /// Whether the score map is empty.
    pub fn is_empty(&self) -> bool {
        self.scores.is_empty()
    }
}

/// Weighted random sampling without replacement using A-ES algorithm (Efraimidis & Spirakis).
/// Selects `k` indices with probability proportional to `weights[i]`.
fn weighted_sample_without_replacement(
    weights: &[f64],
    k: usize,
    rng: &mut impl RngExt,
) -> Vec<usize> {
    let n = weights.len();
    if k >= n {
        return (0..n).collect();
    }

    // Compute keys: u^(1/w) where u ∈ (0,1)
    let mut keys: Vec<(usize, f64)> = weights
        .iter()
        .enumerate()
        .map(|(i, &w)| {
            let u = rng.next_f64().max(1e-12);
            let key = if w <= 0.0 {
                f64::NEG_INFINITY
            } else {
                // u.powf(1.0 / w) but numerically stable
                (u.ln() / w).exp()
            };
            (i, key)
        })
        .collect();

    // Sort descending by key, take top k
    keys.sort_by(|a, b| b.1.total_cmp(&a.1));
    keys.truncate(k);
    keys.into_iter().map(|(i, _)| i).collect()
}

/// Fisher-Yates in-place shuffle.
fn fisher_yates_shuffle<T>(items: &mut [T], rng: &mut impl RngExt) {
    for i in (1..items.len()).rev() {
        let j = rng.index(i + 1);
        items.swap(i, j);
    }
}

/// Minimal RNG trait for weighted sampling (compatible with `crate::combat::rng::Rng`).
pub trait RngExt {
    fn index(&mut self, n: usize) -> usize;
    fn next_f64(&mut self) -> f64;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic RNG for testing.
    struct SeededRng {
        state: u64,
    }

    impl SeededRng {
        fn new(seed: u64) -> Self {
            Self { state: seed }
        }
    }

    impl RngExt for SeededRng {
        fn index(&mut self, n: usize) -> usize {
            if n == 0 {
                return 0;
            }
            self.state = self
                .state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.state as usize) % n
        }

        fn next_f64(&mut self) -> f64 {
            self.state = self
                .state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.state >> 11) as f64 / (u64::MAX as f64 + 1.0)
        }
    }

    #[test]
    fn empty_scores_return_floor() {
        let scores = OfficerPerformanceScores::new();
        assert!((scores.get_score("unknown", "swarm", "enterprise") - 0.01).abs() < 1e-9);
    }

    #[test]
    fn update_from_results_increases_scores() {
        let mut scores = OfficerPerformanceScores::new();
        // Create a minimal ranked result (just needs officer names)
        let result = crate::optimizer::ranking::RankedCrewResult {
            captain: "Kirk".into(),
            bridge: vec!["Spock".into(), "McCoy".into()],
            below_decks: vec!["Scotty".into(), "Sulu".into(), "Uhura".into()],
            trials_run: 100,
            win_rate: 0.9,
            win_rate_ci_low: 0.85,
            win_rate_ci_high: 0.95,
            stall_rate: 0.05,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.1,
            loss_rate: 0.05,
            loss_rate_ci_low: 0.0,
            loss_rate_ci_high: 0.1,
            r1_kill_rate: 0.3,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 1.0,
            avg_hull_remaining: 0.5,
            avg_hull_remaining_ci_low: 0.0,
            avg_hull_remaining_ci_high: 1.0,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            score: crate::optimizer::ranking::RankingScore { value: 0.82 },
            chain: None,
        };

        scores.update_from_results(&[result], "romulan", "defiant");
        // Rank 0 → increment 1.0 for all 6 officers
        assert!((scores.get_score("Kirk", "romulan", "defiant") - 1.0).abs() < 1e-9);
        assert!((scores.get_score("Spock", "romulan", "defiant") - 1.0).abs() < 1e-9);
        // Unknown officer still gets floor
        assert!((scores.get_score("Unknown", "romulan", "defiant") - 0.01).abs() < 1e-9);
    }

    #[test]
    fn decay_reduces_old_scores_on_update() {
        let mut scores = OfficerPerformanceScores::with_params(0.01, 0.2, 0.5);
        let result = crate::optimizer::ranking::RankedCrewResult {
            captain: "Kirk".into(),
            bridge: vec!["Spock".into(), "McCoy".into()],
            below_decks: vec!["Scotty".into(), "Sulu".into(), "Uhura".into()],
            trials_run: 100,
            win_rate: 0.9,
            win_rate_ci_low: 0.85,
            win_rate_ci_high: 0.95,
            stall_rate: 0.05,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.1,
            loss_rate: 0.05,
            loss_rate_ci_low: 0.0,
            loss_rate_ci_high: 0.1,
            r1_kill_rate: 0.3,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 1.0,
            avg_hull_remaining: 0.5,
            avg_hull_remaining_ci_low: 0.0,
            avg_hull_remaining_ci_high: 1.0,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            score: crate::optimizer::ranking::RankingScore { value: 0.82 },
            chain: None,
        };
        scores.update_from_results(&[result.clone()], "romulan", "defiant");
        let after_first = scores.get_score("Kirk", "romulan", "defiant");
        assert!((after_first - 1.0).abs() < 1e-9);

        scores.update_from_results(&[result], "romulan", "defiant");
        let after_second = scores.get_score("Kirk", "romulan", "defiant");
        // 1.0 * 0.5 + 1.0 = 1.5
        assert!((after_second - 1.5).abs() < 1e-9);
    }

    #[test]
    fn epsilon_greedy_produces_k_indices() {
        let scores = OfficerPerformanceScores::new();
        let names: Vec<String> = (0..10).map(|i| format!("Officer{i}")).collect();
        let mut rng = SeededRng::new(42);

        for k in [2, 5, 8] {
            let selected = scores.epsilon_greedy_sample(&names, k, "swarm", "enterprise", &mut rng);
            assert_eq!(selected.len(), k);
            // All indices should be distinct
            let mut sorted = selected.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), k);
        }
    }

    #[test]
    fn epsilon_greedy_favors_high_scoring_officers() {
        let mut scores = OfficerPerformanceScores::with_params(0.01, 0.0, 1.0); // epsilon=0, always weighted
        // Give "Officer9" a very high score
        let key = OfficerScoreKey {
            officer_name: "Officer9".into(),
            hostile_faction: "swarm".into(),
            ship_type: "enterprise".into(),
        };
        scores.scores.insert(key, 100.0);

        let names: Vec<String> = (0..10).map(|i| format!("Officer{i}")).collect();
        let mut rng = SeededRng::new(123);
        let selected = scores.epsilon_greedy_sample(&names, 3, "swarm", "enterprise", &mut rng);

        // With epsilon=0 and a dominant score, "Officer9" should be in the selection
        assert!(
            selected.iter().any(|&i| names[i] == "Officer9"),
            "high-scoring officer should be selected: {:?}",
            selected.iter().map(|&i| &names[i]).collect::<Vec<_>>()
        );
    }

    #[test]
    fn weighted_sample_without_replacement_respects_weights() {
        let weights = vec![0.1, 0.1, 0.1, 100.0, 0.1];
        let mut rng = SeededRng::new(42);

        // Run many trials and count how often index 3 (highest weight) is selected
        let mut count_3 = 0;
        let trials = 500;
        for _ in 0..trials {
            let selected =
                weighted_sample_without_replacement(&weights, 2, &mut rng);
            if selected.contains(&3) {
                count_3 += 1;
            }
        }
        // Index 3 should be selected in >90% of trials with weight 100 vs 0.1
        assert!(
            count_3 > trials * 9 / 10,
            "heaviest item selected {count_3}/{trials} times"
        );
    }
}
