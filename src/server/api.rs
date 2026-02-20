use crate::optimizer::{optimize_scenario, OptimizationScenario};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizeRequest {
    pub ship: String,
    pub hostile: String,
    pub sims: Option<u32>,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrewRecommendation {
    pub captain: String,
    pub bridge: String,
    pub below_decks: String,
    pub win_rate: f64,
    pub avg_hull_remaining: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeResponse {
    pub status: &'static str,
    pub engine: &'static str,
    pub scenario: ScenarioSummary,
    pub recommendations: Vec<CrewRecommendation>,
    pub notes: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioSummary {
    pub ship: String,
    pub hostile: String,
    pub sims: u32,
    pub seed: u64,
}

pub fn health_payload() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&serde_json::json!({
        "status": "ok",
        "service": "kobayashi-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

pub fn optimize_payload(body: &str) -> Result<String, serde_json::Error> {
    let request: OptimizeRequest = serde_json::from_str(body)?;
    let sims = request.sims.unwrap_or(5000);
    let seed = request.seed.unwrap_or(0);

    let scenario = OptimizationScenario {
        ship: &request.ship,
        hostile: &request.hostile,
        simulation_count: sims as usize,
        seed,
    };
    let ranked_results = optimize_scenario(&scenario);

    let response = OptimizeResponse {
        status: "ok",
        engine: "optimizer_v1",
        scenario: ScenarioSummary {
            ship: request.ship,
            hostile: request.hostile,
            sims,
            seed,
        },
        recommendations: ranked_results
            .into_iter()
            .map(|result| CrewRecommendation {
                captain: result.captain,
                bridge: result.bridge,
                below_decks: result.below_decks,
                win_rate: result.win_rate,
                avg_hull_remaining: result.avg_hull_remaining,
            })
            .collect(),
        notes: vec![
            "Recommendations are generated from candidate generation, simulation, and ranking passes.",
            "Results are deterministic for the same ship, hostile, simulation count, and seed.",
        ],
    };

    serde_json::to_string_pretty(&response)
}
