//! Request DTOs and validation for the API.

use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;

use crate::data::heuristics::BelowDecksStrategy;
use crate::optimizer::constraints::{
    normalize_officer_name, CrewSearchConstraints, OfficerGroupConstraint,
};
use crate::optimizer::crew_generator::{MAX_BELOW_DECKS_SLOTS, MIN_BELOW_DECKS_SLOTS};
use crate::optimizer::OptimizerStrategy;

pub const DEFAULT_SIMS: u32 = 5000;
pub const MAX_SIMS: u32 = 100_000;
pub const MAX_CANDIDATES: u32 = 2_000_000;
/// Upper bound for `analytical_prefilter_keep` when set (must be ≥ 1 to truncate).
pub const MAX_ANALYTICAL_PREFILTER_KEEP: u32 = 500_000;

pub const MAX_OPTIMIZE_CONSTRAINT_LIST_LEN: usize = 32;
pub const MAX_OPTIMIZE_CONSTRAINT_GROUPS: usize = 8;
pub const MAX_OPTIMIZE_GROUP_OFFICERS: usize = 32;

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
    /// Optional crew search constraints (must-include, exclude, groups, seating).
    #[serde(default)]
    pub constraints: Option<OptimizeConstraintsDto>,
}

/// JSON body for `OptimizeRequest.constraints`.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct OptimizeConstraintsDto {
    #[serde(default)]
    pub must_include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub groups: Vec<OfficerGroupConstraintDto>,
    pub captain_must_be: Option<String>,
    #[serde(default)]
    pub bridge_must_include: Vec<String>,
    #[serde(default)]
    pub below_decks_must_include: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct OfficerGroupConstraintDto {
    pub officers: Vec<String>,
    pub min_count: u32,
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

pub fn validate_request(request: &OptimizeRequest, sims: u32) -> Result<(), OptimizePayloadError> {
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

    validate_optimize_constraints(request, &mut errors);

    if errors.is_empty() {
        return Ok(());
    }

    Err(OptimizePayloadError::Validation(ValidationErrorResponse {
        status: "error",
        message: "Validation failed",
        errors,
    }))
}

fn validate_optimize_constraints(request: &OptimizeRequest, errors: &mut Vec<ValidationIssue>) {
    let Some(dto) = request.constraints.as_ref() else {
        return;
    };

    let mut check_list = |field: &'static str, v: &[String]| {
        if v.len() > MAX_OPTIMIZE_CONSTRAINT_LIST_LEN {
            errors.push(ValidationIssue {
                field,
                messages: vec![format!(
                    "at most {MAX_OPTIMIZE_CONSTRAINT_LIST_LEN} entries"
                )],
            });
        }
    };

    check_list("constraints.must_include", &dto.must_include);
    check_list("constraints.exclude", &dto.exclude);
    check_list("constraints.bridge_must_include", &dto.bridge_must_include);
    check_list(
        "constraints.below_decks_must_include",
        &dto.below_decks_must_include,
    );

    if dto.groups.len() > MAX_OPTIMIZE_CONSTRAINT_GROUPS {
        errors.push(ValidationIssue {
            field: "constraints.groups",
            messages: vec![format!("at most {MAX_OPTIMIZE_CONSTRAINT_GROUPS} groups")],
        });
    }

    for (gi, g) in dto.groups.iter().enumerate() {
        let field = "constraints.groups";
        if g.officers.len() > MAX_OPTIMIZE_GROUP_OFFICERS {
            errors.push(ValidationIssue {
                field,
                messages: vec![format!(
                    "group {gi}: at most {MAX_OPTIMIZE_GROUP_OFFICERS} officers"
                )],
            });
        }
        if g.min_count == 0 {
            errors.push(ValidationIssue {
                field,
                messages: vec![format!("group {gi}: min_count must be at least 1")],
            });
        }
        let usable = g.officers.iter().filter(|s| !s.trim().is_empty()).count();
        if (g.min_count as usize) > usable {
            errors.push(ValidationIssue {
                field,
                messages: vec![format!(
                    "group {gi}: min_count cannot exceed non-empty officer names in group"
                )],
            });
        }
    }

    let mut exclude_n: HashSet<String> = HashSet::new();
    for s in &dto.exclude {
        let n = normalize_officer_name(s);
        if !n.is_empty() {
            exclude_n.insert(n);
        }
    }

    let mut must_n: HashSet<String> = HashSet::new();
    for s in &dto.must_include {
        let n = normalize_officer_name(s);
        if !n.is_empty() {
            must_n.insert(n);
        }
    }

    for n in &must_n {
        if exclude_n.contains(n) {
            errors.push(ValidationIssue {
                field: "constraints",
                messages: vec![
                    "must_include and exclude both reference the same officer".to_string()
                ],
            });
            break;
        }
    }

    if let Some(ref cap) = dto.captain_must_be {
        let n = normalize_officer_name(cap);
        if !n.is_empty() && exclude_n.contains(&n) {
            errors.push(ValidationIssue {
                field: "constraints.captain_must_be",
                messages: vec!["captain_must_be cannot be listed in exclude".to_string()],
            });
        }
    }

    for s in &dto.bridge_must_include {
        let n = normalize_officer_name(s);
        if !n.is_empty() && exclude_n.contains(&n) {
            errors.push(ValidationIssue {
                field: "constraints.bridge_must_include",
                messages: vec!["bridge_must_include cannot include an excluded officer".to_string()],
            });
            break;
        }
    }

    for s in &dto.below_decks_must_include {
        let n = normalize_officer_name(s);
        if !n.is_empty() && exclude_n.contains(&n) {
            errors.push(ValidationIssue {
                field: "constraints.below_decks_must_include",
                messages: vec![
                    "below_decks_must_include cannot include an excluded officer".to_string(),
                ],
            });
            break;
        }
    }

    for (gi, g) in dto.groups.iter().enumerate() {
        let available = g
            .officers
            .iter()
            .filter(|s| !s.trim().is_empty())
            .filter(|s| !exclude_n.contains(&normalize_officer_name(s)))
            .count();
        if (g.min_count as usize) > available {
            errors.push(ValidationIssue {
                field: "constraints.groups",
                messages: vec![format!(
                    "group {gi}: min_count exceeds officers not in exclude"
                )],
            });
        }
    }
}

/// Builds optimizer constraints after validation. Returns `None` when unset or all-empty.
pub fn build_crew_search_constraints(request: &OptimizeRequest) -> Option<CrewSearchConstraints> {
    let dto = request.constraints.as_ref()?;

    let trim_vec = |v: &[String]| -> Vec<String> {
        v.iter()
            .filter_map(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            })
            .collect()
    };

    let groups: Vec<OfficerGroupConstraint> = dto
        .groups
        .iter()
        .map(|g| OfficerGroupConstraint {
            officers: trim_vec(&g.officers),
            min_count: g.min_count,
        })
        .collect();

    let c = CrewSearchConstraints {
        must_include: trim_vec(&dto.must_include),
        exclude: trim_vec(&dto.exclude),
        groups,
        captain_must_be: dto.captain_must_be.as_ref().and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        bridge_must_include: trim_vec(&dto.bridge_must_include),
        below_decks_must_include: trim_vec(&dto.below_decks_must_include),
    };

    if c.is_empty() {
        None
    } else {
        Some(c)
    }
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
