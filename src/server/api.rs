use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizeRequest {
    pub ship: String,
    pub hostile: String,
    pub sims: Option<u32>,
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

    let response = OptimizeResponse {
        status: "ok",
        engine: "optimizer_stub",
        scenario: ScenarioSummary {
            ship: request.ship,
            hostile: request.hostile,
            sims,
        },
        recommendations: vec![
            CrewRecommendation {
                captain: "Khan".to_string(),
                bridge: "Nero".to_string(),
                below_decks: "T'Laan".to_string(),
                win_rate: 0.873,
                avg_hull_remaining: 0.412,
            },
            CrewRecommendation {
                captain: "Pike".to_string(),
                bridge: "Moreau".to_string(),
                below_decks: "Chen".to_string(),
                win_rate: 0.831,
                avg_hull_remaining: 0.368,
            },
        ],
        notes: vec![
            "This endpoint is infrastructure-first and currently returns stubbed recommendations.",
            "Next step: wire into the Rust optimizer pipeline.",
        ],
    };

    serde_json::to_string_pretty(&response)
}
