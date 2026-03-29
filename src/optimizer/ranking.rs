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
    pub score: RankingScore,
}

pub fn rank_results(simulation_results: Vec<SimulationResult>) -> Vec<RankedCrewResult> {
    let mut ranked: Vec<RankedCrewResult> = simulation_results
        .into_iter()
        .map(|result| {
            let score = (result.win_rate * 0.8 + result.avg_hull_remaining * 0.2) as f32;
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
                score: RankingScore { value: score },
            }
        })
        .collect();

    ranked.sort_by(|left, right| {
        right
            .score
            .value
            .total_cmp(&left.score.value)
            .then_with(|| right.win_rate.total_cmp(&left.win_rate))
            .then_with(|| right.avg_hull_remaining.total_cmp(&left.avg_hull_remaining))
    });

    ranked
}
