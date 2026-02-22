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
    pub avg_hull_remaining: f64,
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
                avg_hull_remaining: result.avg_hull_remaining,
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
