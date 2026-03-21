//! Request DTOs and validation for the API.

use serde::Deserialize;
use std::fmt;

use crate::data::heuristics::BelowDecksStrategy;
use crate::optimizer::OptimizerStrategy;

pub const DEFAULT_SIMS: u32 = 5000;
pub const MAX_SIMS: u32 = 100_000;
pub const MAX_CANDIDATES: u32 = 2_000_000;

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizeRequest {
    pub ship: String,
    pub hostile: String,
    /// Ship tier (1-based). When set, uses data/ships_extended for accurate stats.
    pub ship_tier: Option<u32>,
    /// Ship level (1-based). When set with tier, applies level bonuses from extended data.
    pub ship_level: Option<u32>,
    pub sims: Option<u32>,
    pub seed: Option<u64>,
    pub max_candidates: Option<u32>,
    pub strategy: Option<String>,
    pub prioritize_below_decks_ability: Option<bool>,
    pub heuristics_seeds: Option<Vec<String>>,
    pub heuristics_only: Option<bool>,
    pub below_decks_strategy: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationIssue {
    pub field: &'static str,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
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

pub fn validate_request(
    request: &OptimizeRequest,
    sims: u32,
) -> Result<(), OptimizePayloadError> {
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

    if let Some(cap) = request.max_candidates {
        if cap > MAX_CANDIDATES {
            errors.push(ValidationIssue {
                field: "max_candidates",
                messages: vec![format!("must be at most {MAX_CANDIDATES}")],
            });
        }
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

pub fn parse_below_decks_strategy(s: Option<&String>) -> BelowDecksStrategy {
    match s.as_deref() {
        Some(v) if v.trim().eq_ignore_ascii_case("exploration") => BelowDecksStrategy::Exploration,
        _ => BelowDecksStrategy::Ordered,
    }
}

pub fn parse_strategy(s: Option<&String>) -> OptimizerStrategy {
    match s.as_deref() {
        Some(v) if v.trim().eq_ignore_ascii_case("genetic") => OptimizerStrategy::Genetic,
        Some(v) if v.trim().eq_ignore_ascii_case("tiered") => OptimizerStrategy::Tiered,
        _ => OptimizerStrategy::Exhaustive,
    }
}

/// Parses query string for optimize estimate: ship, hostile, sims, optional max_candidates,
/// optional prioritize_below_decks_ability.
pub fn parse_optimize_estimate_query(
    query: &str,
) -> (String, String, u32, Option<u32>, bool) {
    let mut ship = String::new();
    let mut hostile = String::new();
    let mut sims = DEFAULT_SIMS;
    let mut max_candidates: Option<u32> = None;
    let mut prioritize_below_decks_ability = false;
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "ship" => ship = value.to_string(),
                "hostile" => hostile = value.to_string(),
                "sims" => sims = value.parse().unwrap_or(DEFAULT_SIMS),
                "max_candidates" => max_candidates = value.parse().ok(),
                "prioritize_below_decks_ability" => {
                    prioritize_below_decks_ability =
                        value.eq_ignore_ascii_case("true") || value == "1"
                }
                _ => {}
            }
        }
    }
    (
        ship,
        hostile,
        sims,
        max_candidates,
        prioritize_below_decks_ability,
    )
}
