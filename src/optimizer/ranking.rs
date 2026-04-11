use crate::optimizer::chain::ChainSimulationSummary;
use crate::optimizer::monte_carlo::SimulationResult;
use serde::Serialize;

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
