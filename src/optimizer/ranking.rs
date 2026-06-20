use crate::optimizer::chain::ChainSimulationSummary;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::SimulationResult;
use serde::Serialize;
use std::collections::HashSet;

/// Default head size (when the client omits `novelty_diverse_top`) for MMR reordering of optimize results.
pub(crate) const DEFAULT_NOVELTY_DIVERSE_TOP: usize = 20;

#[derive(Debug, Clone, Copy, Serialize, Default)]
pub struct RankingScore {
    pub value: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankedCrewResult {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    /// Scout or exhaustive Monte Carlo trials backing this row (0 if unknown / synthetic).
    #[serde(default)]
    pub trials_run: usize,
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
    /// Closed-form expected hull damage when ranked by linear eval (no Monte Carlo).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_hull_damage: Option<f64>,
}

/// Build ranked rows from linear-eval scores (pure expected hull damage, no Monte Carlo).
pub fn rank_results_by_expected_damage(scored: Vec<(CrewCandidate, f64)>) -> Vec<RankedCrewResult> {
    let mut scored = scored;
    scored.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| a.0.captain.cmp(&b.0.captain))
    });

    scored
        .into_iter()
        .map(|(candidate, damage)| RankedCrewResult {
            captain: candidate.captain,
            bridge: candidate.bridge,
            below_decks: candidate.below_decks,
            trials_run: 0,
            win_rate: 0.0,
            win_rate_ci_low: 0.0,
            win_rate_ci_high: 0.0,
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
            score: RankingScore {
                value: damage as f32,
            },
            chain: None,
            expected_hull_damage: Some(damage),
        })
        .collect()
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
                trials_run: result.trials_run,
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
                expected_hull_damage: result.expected_hull_damage,
            }
        })
        .collect();

    ranked.sort_by(
        |left, right| match (left.expected_hull_damage, right.expected_hull_damage) {
            (Some(l), Some(r)) => r
                .total_cmp(&l)
                .then_with(|| left.captain.cmp(&right.captain)),
            _ => match (&left.chain, &right.chain) {
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
            },
        },
    );

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

/// Relevance term for MMR: matches [`rank_results`] strength ordering (`RankingScore::value`).
fn mmr_relevance_value(r: &RankedCrewResult) -> f64 {
    r.score.value as f64
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
/// using officer-set overlap as redundancy, keeping the tail in original strength order.
///
/// `lambda` ∈ (0, 1]: higher values keep scores closer to pure strength ordering within the diverse head.
///
/// `history_anchors` — optional crews from past runs (e.g. `optimize_history`); each contributes to the
/// redundancy term like an already-selected row but is never emitted in the output.
pub fn apply_novelty_mmr_reordering(
    ranked: Vec<RankedCrewResult>,
    lambda: f32,
    diverse_top: usize,
    pool: usize,
    history_anchors: &[RankedCrewResult],
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
            let rel = mmr_relevance_value(&ranked[j]);
            let max_sim_selected = selected
                .iter()
                .map(|&i| material_jaccard_similarity(&ranked[j], &ranked[i]))
                .fold(0.0_f64, f64::max);
            let max_sim_anchors = history_anchors
                .iter()
                .map(|a| material_jaccard_similarity(&ranked[j], a))
                .fold(0.0_f64, f64::max);
            let max_sim = max_sim_selected.max(max_sim_anchors);
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
    history_anchors: &[RankedCrewResult],
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
    apply_novelty_mmr_reordering(ranked, lambda, diverse, pool, history_anchors)
}

#[cfg(test)]
mod novelty_tests {
    use super::*;

    fn dummy(captain: &str, bridge: [&str; 2], below: &[&str], win_rate: f64) -> RankedCrewResult {
        RankedCrewResult {
            captain: captain.to_string(),
            bridge: bridge.iter().map(|s| (*s).to_string()).collect(),
            below_decks: below.iter().map(|s| (*s).to_string()).collect(),
            trials_run: 0,
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
            expected_hull_damage: None,
        }
    }

    #[test]
    fn mmr_prefers_materially_different_second_slot_at_equal_win_rate() {
        let a = dummy("CapA", ["B1", "B2"], &["D1", "D2", "D3"], 0.6);
        let b = dummy("CapA", ["B1", "B2"], &["D1", "D2", "D4"], 0.6);
        let c = dummy("CapZ", ["X1", "X2"], &["Y1", "Y2", "Y3"], 0.6);
        let ranked = vec![a, b, c];
        let out = apply_novelty_mmr_reordering(ranked, 0.65, 2, 3, &[]);
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
        let out = apply_novelty_mmr_reordering(crews, 1.0, 3, 4, &[]);
        assert_eq!(out.len(), 4);
        assert!((out[0].win_rate - 0.9).abs() < 1e-9);
        assert!((out[1].win_rate - 0.89).abs() < 1e-9);
        assert!((out[2].win_rate - 0.88).abs() < 1e-9);
    }

    #[test]
    fn mmr_if_configured_none_lambda_returns_input_unchanged_even_with_extras() {
        let ranked = vec![
            dummy("A", ["b1", "b2"], &["d1", "d2", "d3"], 0.9),
            dummy("B", ["b1", "b2"], &["d1", "d2", "d4"], 0.8),
        ];
        let expected: Vec<String> = ranked.iter().map(|r| r.captain.clone()).collect();
        let out = apply_novelty_mmr_if_configured(ranked, None, Some(2), Some(64), &[]);
        assert_eq!(
            out.iter().map(|r| r.captain.as_str()).collect::<Vec<_>>(),
            expected.iter().map(|s| s.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn mmr_reordering_invalid_lambda_returns_unchanged() {
        let ranked = vec![
            dummy("A", ["b1", "b2"], &["d1", "d2", "d3"], 0.9),
            dummy("B", ["x1", "x2"], &["y1", "y2", "y3"], 0.8),
        ];
        let expected: Vec<String> = ranked.iter().map(|r| r.captain.clone()).collect();
        for bad_lambda in [0.0_f32, -0.5_f32, 1.000_000_1_f32] {
            let dup = ranked.clone();
            let out = apply_novelty_mmr_reordering(dup, bad_lambda, 2, 2, &[]);
            assert_eq!(
                out.iter().map(|r| r.captain.as_str()).collect::<Vec<_>>(),
                expected.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                "bad_lambda={bad_lambda}"
            );
        }
    }

    #[test]
    fn mmr_diverse_top_zero_is_noop() {
        let ranked = vec![
            dummy("A", ["b1", "b2"], &["d1", "d2", "d3"], 0.9),
            dummy("B", ["x1", "x2"], &["y1", "y2", "y3"], 0.8),
        ];
        let expected: Vec<String> = ranked.iter().map(|r| r.captain.clone()).collect();
        let out = apply_novelty_mmr_reordering(ranked, 0.65, 0, 4, &[]);
        assert_eq!(
            out.iter().map(|r| r.captain.as_str()).collect::<Vec<_>>(),
            expected.iter().map(|s| s.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn mmr_pool_composition_selected_then_remaining_pool_indices_then_tail() {
        // Strength order: P0 > P1 > P2 > P3 > P4 > P5 (same officer template for 0–1 = near dup).
        let p0 = dummy("P0", ["b1", "b2"], &["d1", "d2", "d3"], 0.95);
        let p1 = dummy("P1", ["b1", "b2"], &["d1", "d2", "d4"], 0.94);
        let p2 = dummy("P2", ["x1", "x2"], &["y1", "y2", "y3"], 0.93);
        let p3 = dummy("P3", ["x1", "x2"], &["y1", "y2", "y4"], 0.92);
        let p4 = dummy("P4", ["m1", "m2"], &["n1", "n2", "n3"], 0.50);
        let p5 = dummy("P5", ["m1", "m2"], &["n1", "n2", "n4"], 0.40);
        let ranked = vec![p0, p1, p2, p3, p4, p5];
        let out = apply_novelty_mmr_reordering(ranked, 0.65, 2, 4, &[]);
        assert_eq!(out.len(), 6);
        // Anchor P0; second pick favors material diversity → P2 over near-duplicate P1.
        assert_eq!(out[0].captain, "P0");
        assert_eq!(out[1].captain, "P2");
        // Remaining pool indices 1..4 not selected: P1 (idx 1), P3 (idx 3) in original order.
        assert_eq!(out[2].captain, "P1");
        assert_eq!(out[3].captain, "P3");
        // Beyond pool: unchanged tail in original list order.
        assert_eq!(out[4].captain, "P4");
        assert_eq!(out[5].captain, "P5");
    }

    #[test]
    fn mmr_very_low_lambda_prioritizes_diversity_over_small_win_rate_gap() {
        // Same captain/bridge; below_decks differ by one id — high Jaccard between adjacent rows.
        let near_a = dummy("Cap", ["B1", "B2"], &["D1", "D2", "D3"], 0.62);
        let near_b = dummy("Cap", ["B1", "B2"], &["D1", "D2", "D4"], 0.61);
        let near_c = dummy("Cap", ["B1", "B2"], &["D1", "D2", "D5"], 0.60);
        let far = dummy("CapZ", ["X1", "X2"], &["Y1", "Y2", "Y3"], 0.595);
        let ranked = vec![near_a, near_b, near_c, far];
        let out = apply_novelty_mmr_reordering(ranked, 0.05, 2, 4, &[]);
        assert_eq!(out[0].captain, "Cap");
        // With tiny λ, redundancy penalty dominates: pick materially different second row.
        assert_eq!(out[1].captain, "CapZ");
    }

    /// Strength order by blended score: B (0.87) > C (~0.744) > A (0.74), but win_rate alone would rank A > C.
    #[test]
    fn mmr_relevance_uses_ranking_score_not_raw_win_rate_for_non_chain() {
        let mut b = dummy("B", ["b1", "b2"], &["d1", "d2", "d3"], 0.85);
        b.avg_hull_remaining = 0.95;
        b.score.value = (0.85_f64 * 0.8 + 0.95 * 0.2) as f32;
        let mut c = dummy("C", ["b1", "b2"], &["d1", "d2", "d4"], 0.88);
        c.avg_hull_remaining = 0.2;
        c.score.value = (0.88 * 0.8 + 0.2 * 0.2) as f32;
        let mut a = dummy("A", ["x1", "x2"], &["y1", "y2", "y3"], 0.9);
        a.avg_hull_remaining = 0.1;
        a.score.value = (0.9 * 0.8 + 0.1 * 0.2) as f32;
        let ranked = vec![b, c, a];
        let out = apply_novelty_mmr_reordering(ranked, 1.0, 3, 3, &[]);
        assert_eq!(out[0].captain, "B");
        assert_eq!(out[1].captain, "C");
        assert_eq!(out[2].captain, "A");
    }

    #[test]
    fn mmr_history_anchor_penalizes_overlap_even_when_first_anchor_not_in_pool() {
        // Historical winner overlaps crew P0 (same officers); current run has P0 strongest then near-dup P1 then diverse P2.
        let anchor = dummy("P0", ["b1", "b2"], &["d1", "d2", "d3"], 0.99);
        let p0 = dummy("P0", ["b1", "b2"], &["d1", "d2", "d3"], 0.95);
        let p1 = dummy("P1", ["b1", "b2"], &["d1", "d2", "d4"], 0.94);
        let p2 = dummy("P2", ["x1", "x2"], &["y1", "y2", "y3"], 0.93);
        let ranked = vec![p0, p1, p2];
        let out = apply_novelty_mmr_reordering(ranked, 0.65, 2, 3, &[anchor]);
        assert_eq!(out[0].captain, "P0");
        // Second slot prefers P2 over near-duplicate P1 because anchor overlaps P0/P1 lineage.
        assert_eq!(out[1].captain, "P2");
    }
}
