//! Request DTOs and validation for the API.

use serde::Deserialize;
use std::fmt;

use crate::data::heuristics::BelowDecksStrategy;
use crate::optimizer::crew_generator::{MAX_BELOW_DECKS_SLOTS, MIN_BELOW_DECKS_SLOTS};
use crate::optimizer::OptimizerStrategy;

pub const DEFAULT_SIMS: u32 = 5000;
pub const MAX_SIMS: u32 = 100_000;
pub const MAX_CANDIDATES: u32 = 2_000_000;
/// Upper bound for `analytical_prefilter_keep` when set (must be ≥ 1 to truncate).
pub const MAX_ANALYTICAL_PREFILTER_KEEP: u32 = 500_000;

/// Same JSON shape as simulate `crew` — duplicated so `requests` stays independent of `api`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplaySeedCrew {
    pub captain: Option<String>,
    pub bridge: Option<Vec<Option<String>>>,
    pub below_deck: Option<Vec<Option<String>>>,
}

/// Replay one Monte Carlo draw from an optimize/simulate run (`seed` + `sim_index` → `iteration_seed`).
#[derive(Debug, Clone, Deserialize)]
pub struct ReplaySeedRequest {
    pub ship: String,
    pub hostile: String,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    /// Scenario seed from the optimize/simulate request (defaults to 0).
    pub seed: Option<u64>,
    /// Zero-based iteration index: `iteration_seed = stable_base.wrapping_add(sim_index)`.
    pub sim_index: u64,
    /// Cap on returned trace events (tail of the fight). Default 500, max 2000.
    pub max_trace_events: Option<u32>,
    pub crew: ReplaySeedCrew,
}

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
    /// Keep only this many crews after approximate analytical ranking (closed-form expected hull damage) before Monte Carlo. Omitted = evaluate all generated candidates. Ignored for genetic strategy.
    pub analytical_prefilter_keep: Option<u32>,
    /// Below-decks slot count (2–5). Omitted = tier default (tier 1 → 2, else 3).
    pub below_decks_slots: Option<u32>,
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

    if let Some(k) = request.analytical_prefilter_keep {
        if k == 0 {
            errors.push(ValidationIssue {
                field: "analytical_prefilter_keep",
                messages: vec!["if set, must be at least 1".to_string()],
            });
        } else if k > MAX_ANALYTICAL_PREFILTER_KEEP {
            errors.push(ValidationIssue {
                field: "analytical_prefilter_keep",
                messages: vec![format!("must be at most {MAX_ANALYTICAL_PREFILTER_KEEP}")],
            });
        }
    }

    if let Some(n) = request.below_decks_slots {
        let lo = MIN_BELOW_DECKS_SLOTS as u32;
        let hi = MAX_BELOW_DECKS_SLOTS as u32;
        if !(lo..=hi).contains(&n) {
            errors.push(ValidationIssue {
                field: "below_decks_slots",
                messages: vec![format!("if set, must be between {lo} and {hi}")],
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
/// optional prioritize_below_decks_ability, optional ship_tier and below_decks_slots.
pub fn parse_optimize_estimate_query(
    query: &str,
) -> (
    String,
    String,
    u32,
    Option<u32>,
    bool,
    Option<u32>,
    Option<u32>,
) {
    let mut ship = String::new();
    let mut hostile = String::new();
    let mut sims = DEFAULT_SIMS;
    let mut max_candidates: Option<u32> = None;
    let mut prioritize_below_decks_ability = false;
    let mut ship_tier: Option<u32> = None;
    let mut below_decks_slots: Option<u32> = None;
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
                "ship_tier" => ship_tier = value.parse().ok(),
                "below_decks_slots" => below_decks_slots = value.parse().ok(),
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
        ship_tier,
        below_decks_slots,
    )
}
