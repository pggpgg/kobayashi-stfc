use crate::optimizer::chain::ChainSimulationSummary;
use crate::optimizer::monte_carlo::SimulationResult;
use serde::Serialize;
use std::collections::HashSet;

/// Default head size (when the client omits `novelty_diverse_top`) for MMR reordering of optimize results.
pub(crate) const DEFAULT_NOVELTY_DIVERSE_TOP: usize = 20;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RankingScore {
    pub value: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankedCrewResult {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub win_rate: f64,
    pub win_rate_ci_low: f64,
    pub win_rate_ci_high: f64,
    pub stall_rate: f64,
    pub stall_rate_ci_low: f64,
    pub stall_rate_ci_high: f64,
    pub loss_rate: f64,
    pub loss_rate_ci_low: f64,
    pub loss_rate_ci_high: f64,
    pub r1_kill_rate: f64,
    pub r1_kill_rate_ci_low: f64,
    pub r1_kill_rate_ci_high: f64,
    pub avg_hull_remaining: f64,
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
    pub avg_defender_hull_remaining: f64,
    pub avg_defender_hull_remaining_ci_low: f64,
    pub avg_defender_hull_remaining_ci_high: f64,
    pub score: RankingScore,
    /// Chain grind summary when optimize used sequential fights; primary = `win_rate`, secondary = `avg_hull_remaining`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainSimulationSummary>,
}

pub fn rank_results(simulation_results: Vec<SimulationResult>) -> Vec<RankedCrewResult> {
    let mut ranked: Vec<RankedCrewResult> = simulation_results
        .into_iter()
        .map(|result| {
            let score = if result.chain.is_some() {
                // Lexicographic proxy: primary dominates; secondary uses avg_hull_remaining (conditional mean).
                result.win_rate as f32 * 1e4 + result.avg_hull_remaining.min(1.0) as f32
            } else {
                (result.win_rate * 0.8 + result.avg_hull_remaining * 0.2) as f32
            };
            RankedCrewResult {
                captain: result.candidate.captain,
                bridge: result.candidate.bridge.clone(),
                below_decks: result.candidate.below_decks.clone(),
                win_rate: result.win_rate,
                win_rate_ci_low: result.win_rate_ci_low,
                win_rate_ci_high: result.win_rate_ci_high,
                stall_rate: result.stall_rate,
                stall_rate_ci_low: result.stall_rate_ci_low,
                stall_rate_ci_high: result.stall_rate_ci_high,
                loss_rate: result.loss_rate,
                loss_rate_ci_low: result.loss_rate_ci_low,
                loss_rate_ci_high: result.loss_rate_ci_high,
                r1_kill_rate: result.r1_kill_rate,
                r1_kill_rate_ci_low: result.r1_kill_rate_ci_low,
                r1_kill_rate_ci_high: result.r1_kill_rate_ci_high,
                avg_hull_remaining: result.avg_hull_remaining,
                avg_hull_remaining_ci_low: result.avg_hull_remaining_ci_low,
                avg_hull_remaining_ci_high: result.avg_hull_remaining_ci_high,
                avg_defender_hull_remaining: result.avg_defender_hull_remaining,
                avg_defender_hull_remaining_ci_low: result.avg_defender_hull_remaining_ci_low,
                avg_defender_hull_remaining_ci_high: result.avg_defender_hull_remaining_ci_high,
                score: RankingScore { value: score },
                chain: result.chain.clone(),
            }
        })
        .collect();

    ranked.sort_by(|left, right| match (&left.chain, &right.chain) {
        (Some(_), Some(_)) => right
            .win_rate
            .total_cmp(&left.win_rate)
            .then_with(|| right.avg_hull_remaining.total_cmp(&left.avg_hull_remaining)),
        _ => right
            .score
            .value
            .total_cmp(&left.score.value)
            .then_with(|| right.win_rate.total_cmp(&left.win_rate))
            .then_with(|| right.avg_hull_remaining.total_cmp(&left.avg_hull_remaining)),
    });

    ranked
}

fn material_officer_set(r: &RankedCrewResult) -> HashSet<&str> {
    let mut s = HashSet::with_capacity(1 + r.bridge.len() + r.below_decks.len());
    s.insert(r.captain.as_str());
    for o in &r.bridge {
        s.insert(o.as_str());
    }
    for o in &r.below_decks {
        s.insert(o.as_str());
    }
    s
}

/// Jaccard similarity on the set of officer names (captain + bridge + below decks).
pub(crate) fn material_jaccard_similarity(a: &RankedCrewResult, b: &RankedCrewResult) -> f64 {
    let sa = material_officer_set(a);
    let sb = material_officer_set(b);
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Maximal Marginal Relevance on the strength-sorted list: reorders the first `diverse_top` positions
/// using officer-set overlap as redundancy, keeping the tail in original win-rate order.
///
/// `lambda` ∈ (0, 1]: higher values keep scores closer to pure win-rate ordering within the diverse head.
pub fn apply_novelty_mmr_reordering(
    ranked: Vec<RankedCrewResult>,
    lambda: f32,
    diverse_top: usize,
    pool: usize,
) -> Vec<RankedCrewResult> {
    let n = ranked.len();
    if n <= 1 || diverse_top == 0 {
        return ranked;
    }
    let lam = lambda as f64;
    if lam <= 0.0 || lam > 1.0 {
        return ranked;
    }

    let diverse = diverse_top.min(n).max(1);
    let pool = pool.max(diverse).min(n);
    if pool < 2 {
        return ranked;
    }

    // First pick is always the strongest crew in the pool (matches standard MMR anchor).
    let mut selected: Vec<usize> = vec![0];
    let mut remaining: Vec<usize> = (1..pool).collect();

    while selected.len() < diverse && !remaining.is_empty() {
        let mut best_j: usize = remaining[0];
        let mut best_score = f64::NEG_INFINITY;
        for &j in &remaining {
            let rel = ranked[j].win_rate;
            let max_sim = selected
                .iter()
                .map(|&i| material_jaccard_similarity(&ranked[j], &ranked[i]))
                .fold(0.0_f64, f64::max);
            let mmr = lam * rel - (1.0 - lam) * max_sim;
            if mmr > best_score || (mmr == best_score && j < best_j) {
                best_score = mmr;
                best_j = j;
            }
        }
        remaining.retain(|&x| x != best_j);
        selected.push(best_j);
    }

    let selected_set: HashSet<usize> = selected.iter().copied().collect();
    let mut out: Vec<RankedCrewResult> = Vec::with_capacity(n);
    for &i in &selected {
        out.push(ranked[i].clone());
    }
    for (i, crew) in ranked.iter().enumerate().take(pool) {
        if !selected_set.contains(&i) {
            out.push(crew.clone());
        }
    }
    for crew in ranked.iter().skip(pool) {
        out.push(crew.clone());
    }
    out
}

/// When `novelty_lambda` is set, reorders the head of the strength-sorted list for more diverse officer material.
pub fn apply_novelty_mmr_if_configured(
    ranked: Vec<RankedCrewResult>,
    novelty_lambda: Option<f32>,
    novelty_diverse_top: Option<usize>,
    novelty_pool_limit: Option<usize>,
) -> Vec<RankedCrewResult> {
    let Some(lambda) = novelty_lambda else {
        return ranked;
    };
    let n = ranked.len();
    if n <= 1 {
        return ranked;
    }
    let diverse = novelty_diverse_top
        .unwrap_or(DEFAULT_NOVELTY_DIVERSE_TOP)
        .max(1)
        .min(n);
    let pool_default = (diverse.saturating_mul(5).max(64)).min(n);
    let pool = novelty_pool_limit
        .map(|p| p.max(diverse).min(n))
        .unwrap_or(pool_default)
        .min(n);
    apply_novelty_mmr_reordering(ranked, lambda, diverse, pool)
}

#[cfg(test)]
mod novelty_tests {
    use super::*;

    fn dummy(captain: &str, bridge: [&str; 2], below: &[&str], win_rate: f64) -> RankedCrewResult {
        RankedCrewResult {
            captain: captain.to_string(),
            bridge: bridge.iter().map(|s| (*s).to_string()).collect(),
            below_decks: below.iter().map(|s| (*s).to_string()).collect(),
            win_rate,
            win_rate_ci_low: 0.0,
            win_rate_ci_high: 1.0,
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.0,
            loss_rate: 0.0,
            loss_rate_ci_low: 0.0,
            loss_rate_ci_high: 0.0,
            r1_kill_rate: 0.0,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 0.0,
            avg_hull_remaining: 0.5,
            avg_hull_remaining_ci_low: 0.0,
            avg_hull_remaining_ci_high: 1.0,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            score: RankingScore {
                value: win_rate as f32,
            },
            chain: None,
        }
    }

    #[test]
    fn mmr_prefers_materially_different_second_slot_at_equal_win_rate() {
        let a = dummy("CapA", ["B1", "B2"], &["D1", "D2", "D3"], 0.6);
        let b = dummy("CapA", ["B1", "B2"], &["D1", "D2", "D4"], 0.6);
        let c = dummy("CapZ", ["X1", "X2"], &["Y1", "Y2", "Y3"], 0.6);
        let ranked = vec![a, b, c];
        let out = apply_novelty_mmr_reordering(ranked, 0.65, 2, 3);
        assert_eq!(out[0].captain, "CapA");
        // Second slot should be c (more different from CapA/B1/B2 than b is), not near-duplicate b.
        assert_eq!(out[1].captain, "CapZ");
    }

    #[test]
    fn mmr_lambda_one_preserves_pure_strength_head() {
        let crews: Vec<RankedCrewResult> = (0..4)
            .map(|i| {
                let wr = 0.9 - (i as f64) * 0.01;
                dummy("C", ["b1", "b2"], &["d1", "d2", "d3"], wr)
            })
            .collect();
        let out = apply_novelty_mmr_reordering(crews, 1.0, 3, 4);
        assert_eq!(out.len(), 4);
        assert!((out[0].win_rate - 0.9).abs() < 1e-9);
        assert!((out[1].win_rate - 0.89).abs() < 1e-9);
        assert!((out[2].win_rate - 0.88).abs() < 1e-9);
    }
}
