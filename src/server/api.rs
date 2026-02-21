use crate::optimizer::{optimize_scenario, OptimizationScenario};
use serde::{Deserialize, Serialize};
use std::fmt;

const DEFAULT_SIMS: u32 = 5000;
const MAX_SIMS: u32 = 100_000;

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

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub field: &'static str,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationErrorResponse {
    pub status: &'static str,
    pub message: &'static str,
    pub errors: Vec<ValidationIssue>,
}

#[derive(Debug)]
pub enum OptimizePayloadError {
    Parse(serde_json::Error),
    Validation(ValidationErrorResponse),
}

impl fmt::Display for OptimizePayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "{err}"),
            Self::Validation(_) => write!(f, "invalid optimize request"),
        }
    }
}

impl std::error::Error for OptimizePayloadError {}

pub fn health_payload() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&serde_json::json!({
        "status": "ok",
        "service": "kobayashi-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

pub fn optimize_payload(body: &str) -> Result<String, OptimizePayloadError> {
    let request: OptimizeRequest =
        serde_json::from_str(body).map_err(OptimizePayloadError::Parse)?;
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    validate_request(&request, sims)?;
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

    serde_json::to_string_pretty(&response).map_err(OptimizePayloadError::Parse)
}

fn validate_request(request: &OptimizeRequest, sims: u32) -> Result<(), OptimizePayloadError> {
    let mut errors: Vec<ValidationIssue> = Vec::new();

    if request.ship.trim().is_empty() {
        errors.push(ValidationIssue {
            field: "ship",
            messages: vec!["must not be empty".to_string()],
        });
    }

    if request.hostile.trim().is_empty() {
        errors.push(ValidationIssue {
            field: "hostile",
            messages: vec!["must not be empty".to_string()],
        });
    }

    if !(1..=MAX_SIMS).contains(&sims) {
        errors.push(ValidationIssue {
            field: "sims",
            messages: vec![format!("must be between 1 and {MAX_SIMS}")],
        });
    }

    if errors.is_empty() {
        return Ok(());
    }

    Err(OptimizePayloadError::Validation(ValidationErrorResponse {
        status: "error",
        message: "Validation failed",
        errors,
    }))
}
