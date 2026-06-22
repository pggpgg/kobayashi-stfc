//! Request DTOs and validation for the API.

use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;

use crate::data::heuristics::BelowDecksStrategy;
use crate::data::optimize_history::{validate_optimize_cache_key, MAX_OPTIMIZE_CACHE_KEY_BYTES};
use crate::optimizer::chain::{ChainGrindParams, ChainSecondaryObjective};
use crate::optimizer::constraints::{
    normalize_officer_name, CrewSearchConstraints, OfficerGroupConstraint,
};
use crate::optimizer::crew_generator::{MAX_BELOW_DECKS_SLOTS, MIN_BELOW_DECKS_SLOTS};
use crate::optimizer::monte_carlo::scenario::DefenderOpponent;
use crate::optimizer::OptimizerStrategy;

pub const DEFAULT_SIMS: u32 = 5000;
pub const MAX_SIMS: u32 = 100_000;
pub const MAX_CANDIDATES: u32 = 2_000_000;
/// Upper bound for `analytical_prefilter_keep` when set (must be ≥ 1 to truncate).
pub const MAX_ANALYTICAL_PREFILTER_KEEP: u32 = 500_000;
pub const MAX_TIERED_SCOUT_SIMS: u32 = 100_000;
pub const MAX_TIERED_TOP_K: u32 = 500;
pub const MAX_NOVELTY_DIVERSE_TOP: u32 = 500;
pub const MAX_NOVELTY_POOL: u32 = 10_000;
pub const MAX_WARM_START_CREWS: usize = 24;

pub const MAX_OPTIMIZE_CONSTRAINT_LIST_LEN: usize = 32;
pub const MAX_OPTIMIZE_CONSTRAINT_GROUPS: usize = 8;
pub const MAX_OPTIMIZE_GROUP_OFFICERS: usize = 32;
pub const MAX_CHAIN_KILLS_TARGET: u32 = 50;

/// Split `captain_must_be` request field on commas/semicolons (captain may be any listed officer).
fn parse_captain_must_be_tokens(s: &str) -> Vec<String> {
    s.split([',', ';'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Chain grinding: N sequential fights, HHP carry-over, full SHP each link (optimizer / simulate).
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ChainGrindRequest {
    #[serde(default)]
    pub enabled: bool,
    pub kills_target: Option<u32>,
    /// `min_hull_damage` (default) or `max_loot_per_hull_proxy` (placeholder economy).
    pub secondary: Option<String>,
}

/// Build params when `enabled` and fields are valid.
pub fn chain_grind_params_from_request(
    c: &ChainGrindRequest,
) -> Result<Option<ChainGrindParams>, String> {
    if !c.enabled {
        return Ok(None);
    }
    let kt = c
        .kills_target
        .ok_or_else(|| "chain.enabled requires chain.kills_target".to_string())?;
    if !(1..=MAX_CHAIN_KILLS_TARGET).contains(&kt) {
        return Err(format!(
            "chain.kills_target must be 1..={MAX_CHAIN_KILLS_TARGET}"
        ));
    }
    let sec = match c.secondary.as_deref().unwrap_or("min_hull_damage") {
        "min_hull_damage" => ChainSecondaryObjective::MinHullDamage,
        "max_loot_per_hull_proxy" => ChainSecondaryObjective::MaxLootPerHullProxy,
        other => {
            return Err(format!(
                "chain.secondary: unknown {other:?} (expected min_hull_damage or max_loot_per_hull_proxy)"
            ));
        }
    };
    Ok(Some(ChainGrindParams {
        kills_target: kt,
        secondary: sec,
    }))
}

/// Same JSON shape as simulate `crew` — duplicated so `requests` stays independent of `api`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ReplaySeedCrew {
    pub captain: Option<String>,
    pub bridge: Option<Vec<Option<String>>>,
    pub below_deck: Option<Vec<Option<String>>>,
}

/// Replay one Monte Carlo draw from an optimize/simulate run (`seed` + `sim_index` → `iteration_seed`).
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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
    #[serde(default)]
    pub support_buffs: Option<Vec<String>>,
    #[serde(default)]
    pub defender_opponent: DefenderOpponent,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct WarmStartCrewDto {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
}

/// Optional LCARS crew for the defending side (same shape as simulate `defender_crew`).
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct DefenderOfficerCrewDto {
    pub captain: Option<String>,
    pub bridge: Option<Vec<Option<String>>>,
    pub below_deck: Option<Vec<Option<String>>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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
    /// Below-decks pool tier: `"strict"` (combat modifier only — default), `"scored"` (all
    /// below-decks-ability officers ranked by combat relevance with officer power as tiebreaker),
    /// or `"relaxed"` (all officers, ranked by power). Unknown values fall back to strict.
    #[serde(default)]
    pub below_decks_pool_mode: Option<String>,
    pub heuristics_seeds: Option<Vec<String>>,
    pub heuristics_only: Option<bool>,
    pub below_decks_strategy: Option<String>,
    /// Keep only this many crews after approximate analytical ranking (closed-form expected hull damage) before Monte Carlo. Omitted = evaluate all generated candidates unless auto-cap applies. Ignored for genetic strategy.
    pub analytical_prefilter_keep: Option<u32>,
    /// Analytical prefilter only: disable learned pair co-occurrence prior from warm-start/history refs.
    /// Omitted defaults to enabled (backward compatible).
    #[serde(default)]
    pub enable_learned_pair_prior: Option<bool>,
    /// Tiered strategy: scouting-phase sims per crew (1..=MAX_TIERED_SCOUT_SIMS). Omitted = scaled from candidate count.
    pub tiered_scout_sims: Option<u32>,
    /// Tiered strategy: how many top scout crews receive full confirmation sims (1..=MAX_TIERED_TOP_K). Omitted = scaled from candidate count.
    pub tiered_top_k: Option<u32>,
    /// When true, tiered scouting uses one uniform pass at the scout cap (legacy). Omitted or false = adaptive coarse→refine scout.
    #[serde(default)]
    pub tiered_scout_uniform: Option<bool>,
    /// When set (tiered): cap total confirmation iterations across the top-K crews to `floor(mult * tiered_top_k * sims)` after per-crew adaptive allocation.
    #[serde(default)]
    pub tiered_confirm_budget_cap_mult: Option<f64>,
    /// Exhaustive strategy only: scout-phase sims per crew before ranking (pair with `exhaustive_scout_top_keep`).
    #[serde(default)]
    pub exhaustive_scout_sims: Option<u32>,
    /// Exhaustive strategy only: how many top scout crews receive full `sims` confirmation (pair with `exhaustive_scout_sims`).
    #[serde(default)]
    pub exhaustive_scout_top_keep: Option<u32>,
    /// Optional crews prepended before generated candidates (deduped); e.g. UI warm-start persistence.
    #[serde(default)]
    pub warm_start_crews: Option<Vec<WarmStartCrewDto>>,
    /// Below-decks slot count (0–7). Omitted = resolved from ship level + `crew_slots` when present, else tier heuristic.
    pub below_decks_slots: Option<u32>,
    /// Optional crew search constraints (must-include, exclude, groups, seating).
    #[serde(default)]
    pub constraints: Option<OptimizeConstraintsDto>,
    #[serde(default)]
    pub support_buffs: Option<Vec<String>>,
    /// PvP: defender alliance support buff ids (see `data/support_buffs.json`).
    #[serde(default)]
    pub defender_support_buffs: Option<Vec<String>>,
    /// PvP: alliance debuffs applied to the attacker (see `data/support_buffs.json`).
    #[serde(default)]
    pub defender_alliance_debuffs: Option<Vec<String>>,
    #[serde(default)]
    pub chain: Option<ChainGrindRequest>,
    #[serde(default)]
    pub defender_opponent: DefenderOpponent,
    /// Optional LCARS crew for the **defending** side (merged with hostile ship abilities).
    /// Requires non-empty `captain`. Not supported when `strategy` is `genetic`.
    #[serde(default)]
    pub defender_crew: Option<DefenderOfficerCrewDto>,
    /// When true with non-empty `heuristics_seeds`, expanded heuristic crews are merged into the
    /// optimizer warm-start list so they flow through analytical prefilter and tiered (or exhaustive)
    /// Monte Carlo instead of a separate full-sim pass on every seed crew first.
    #[serde(default)]
    pub fast_discovery: Option<bool>,
    /// Maximal marginal relevance blend (0, 1]; when set, reorders the recommendation head for materially diverse officer sets.
    pub novelty_lambda: Option<f32>,
    /// How many leading recommendations use MMR (optional; server defaults when `novelty_lambda` is set).
    pub novelty_diverse_top: Option<u32>,
    /// Strength-sorted pool size considered for MMR (optional; must be ≥ `novelty_diverse_top` when both are set).
    pub novelty_pool: Option<u32>,
    /// When true with `novelty_lambda`, treat persisted `optimize_history` top crews as redundancy anchors for MMR (requires profile + `optimize_cache_key`).
    #[serde(default)]
    pub novelty_history_anchors: Option<bool>,
    /// Opaque fingerprint (same string as SPA `buildOptimizeWarmStartKey`) for `profiles/{id}/optimize_history.json`.
    #[serde(default)]
    pub optimize_cache_key: Option<String>,
    /// PvP: defender player ship (mutually exclusive with `hostile`). Requires `defender_profile_id`.
    #[serde(default)]
    pub defender_ship: Option<String>,
    pub defender_ship_tier: Option<u32>,
    pub defender_ship_level: Option<u32>,
    #[serde(default)]
    pub defender_profile_id: Option<String>,
    /// Combat scenario for officer eligibility filtering (e.g. `"mission_bosses"`, `"solo_armadas"`,
    /// `"pvp_space"`). Snake_case [`crate::combat::EnemyType`]. When unset/invalid, the server infers
    /// it from the target (PvP, group armada, outpost) defaulting to `"red_moving_space"`.
    #[serde(default)]
    pub enemy_type: Option<String>,
}

/// Resolve the effective [`BelowDecksPoolMode`] for a request. Unrecognized or unset
/// `below_decks_pool_mode` falls back to [`BelowDecksPoolMode::Strict`].
pub fn below_decks_pool_mode_resolved(
    request: &OptimizeRequest,
) -> crate::data::heuristics::BelowDecksPoolMode {
    request
        .below_decks_pool_mode
        .as_deref()
        .and_then(crate::data::heuristics::BelowDecksPoolMode::parse_api_str)
        .unwrap_or(crate::data::heuristics::BelowDecksPoolMode::Strict)
}

/// When `true`: relaxed search — no below-decks combat heuristic stripping on seeds, wide
/// below-decks pools.
pub fn relax_below_decks_combat_strictness(request: &OptimizeRequest) -> bool {
    matches!(
        below_decks_pool_mode_resolved(request),
        crate::data::heuristics::BelowDecksPoolMode::Relaxed
    )
}

/// JSON body for `OptimizeRequest.constraints`.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct OfficerGroupConstraintDto {
    pub officers: Vec<String>,
    pub min_count: u32,
}

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct ValidationIssue {
    pub field: &'static str,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
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

const REMOVED_PRIORITIZE_BELOW_DECKS_MSG: &str =
    "removed: use below_decks_pool_mode (set \"relaxed\" for relaxed below-decks search)";

/// Deserialize an optimize JSON body and reject the removed `prioritize_below_decks_ability` field.
pub fn parse_optimize_request_body(body: &str) -> Result<OptimizeRequest, OptimizePayloadError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(OptimizePayloadError::Parse)?;
    if let Some(obj) = v.as_object() {
        if obj.contains_key("prioritize_below_decks_ability") {
            return Err(OptimizePayloadError::Validation(ValidationErrorResponse {
                status: "error",
                message: "Validation failed",
                errors: vec![ValidationIssue {
                    field: "prioritize_below_decks_ability",
                    messages: vec![REMOVED_PRIORITIZE_BELOW_DECKS_MSG.to_string()],
                }],
            }));
        }
    }
    serde_json::from_value(v).map_err(OptimizePayloadError::Parse)
}

pub fn validate_request(request: &OptimizeRequest, sims: u32) -> Result<(), OptimizePayloadError> {
    let mut errors: Vec<ValidationIssue> = Vec::new();

    if request.ship.trim().is_empty() {
        errors.push(ValidationIssue {
            field: "ship",
            messages: vec!["must not be empty".to_string()],
        });
    }

    if let Err(pvp_errors) =
        super::pvp::validate_scenario_target(&super::pvp::ScenarioTargetFields {
            hostile: Some(request.hostile.clone()),
            defender_ship: request.defender_ship.clone(),
            defender_ship_tier: request.defender_ship_tier,
            defender_ship_level: request.defender_ship_level,
            defender_profile_id: request.defender_profile_id.clone(),
        })
    {
        errors.extend(pvp_errors);
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

    if let Some(s) = request.tiered_scout_sims {
        if !(1..=MAX_TIERED_SCOUT_SIMS).contains(&s) {
            errors.push(ValidationIssue {
                field: "tiered_scout_sims",
                messages: vec![format!(
                    "if set, must be between 1 and {MAX_TIERED_SCOUT_SIMS}"
                )],
            });
        }
    }
    if let Some(k) = request.tiered_top_k {
        if !(1..=MAX_TIERED_TOP_K).contains(&k) {
            errors.push(ValidationIssue {
                field: "tiered_top_k",
                messages: vec![format!("if set, must be between 1 and {MAX_TIERED_TOP_K}")],
            });
        }
    }

    if let Some(m) = request.tiered_confirm_budget_cap_mult {
        if !m.is_finite() || m <= 0.0 || m > 20.0 {
            errors.push(ValidationIssue {
                field: "tiered_confirm_budget_cap_mult",
                messages: vec![
                    "if set, must be a finite number in (0, 20] (typical: 1.5–3)".to_string(),
                ],
            });
        }
    }

    let ex_scout = request.exhaustive_scout_sims;
    let ex_keep = request.exhaustive_scout_top_keep;
    if ex_scout.is_some() != ex_keep.is_some() {
        errors.push(ValidationIssue {
            field: "exhaustive_scout_sims",
            messages: vec![
                "exhaustive_scout_sims and exhaustive_scout_top_keep must both be set or both omitted"
                    .to_string(),
            ],
        });
    }
    if let Some(s) = ex_scout {
        if !(1..=MAX_TIERED_SCOUT_SIMS).contains(&s) {
            errors.push(ValidationIssue {
                field: "exhaustive_scout_sims",
                messages: vec![format!(
                    "if set, must be between 1 and {MAX_TIERED_SCOUT_SIMS}"
                )],
            });
        }
    }
    if let Some(k) = ex_keep {
        if !(1..=MAX_TIERED_TOP_K).contains(&k) {
            errors.push(ValidationIssue {
                field: "exhaustive_scout_top_keep",
                messages: vec![format!("if set, must be between 1 and {MAX_TIERED_TOP_K}")],
            });
        }
    }

    if let Some(ref crews) = request.warm_start_crews {
        if crews.len() > MAX_WARM_START_CREWS {
            errors.push(ValidationIssue {
                field: "warm_start_crews",
                messages: vec![format!("at most {MAX_WARM_START_CREWS} crews")],
            });
        } else {
            for (idx, c) in crews.iter().enumerate() {
                if c.captain.trim().is_empty() {
                    errors.push(ValidationIssue {
                        field: "warm_start_crews",
                        messages: vec![format!("entry {idx}: captain must not be empty")],
                    });
                    break;
                }
                if c.bridge.len() != 2 {
                    errors.push(ValidationIssue {
                        field: "warm_start_crews",
                        messages: vec![format!(
                            "entry {idx}: bridge must contain exactly 2 officer names"
                        )],
                    });
                    break;
                }
                if c.below_decks.len() > MAX_BELOW_DECKS_SLOTS {
                    errors.push(ValidationIssue {
                        field: "warm_start_crews",
                        messages: vec![format!(
                            "entry {idx}: at most {MAX_BELOW_DECKS_SLOTS} below_decks officers"
                        )],
                    });
                    break;
                }
            }
        }
    }

    if let Some(ref k) = request.optimize_cache_key {
        let t = k.trim();
        if !t.is_empty() && !validate_optimize_cache_key(t) {
            errors.push(ValidationIssue {
                field: "optimize_cache_key",
                messages: vec![format!(
                    "if set, must be non-empty after trim, at most {MAX_OPTIMIZE_CACHE_KEY_BYTES} bytes, and contain no control characters"
                )],
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

    let has_novelty_extras = request.novelty_diverse_top.is_some()
        || request.novelty_pool.is_some()
        || request.novelty_history_anchors == Some(true);
    if has_novelty_extras && request.novelty_lambda.is_none() {
        errors.push(ValidationIssue {
            field: "novelty_lambda",
            messages: vec!["required when novelty_diverse_top or novelty_pool is set".to_string()],
        });
    }
    if let Some(l) = request.novelty_lambda {
        if l <= 0.0 || l > 1.0 {
            errors.push(ValidationIssue {
                field: "novelty_lambda",
                messages: vec!["if set, must be greater than 0 and at most 1".to_string()],
            });
        }
    }
    if let Some(d) = request.novelty_diverse_top {
        if d == 0 || d > MAX_NOVELTY_DIVERSE_TOP {
            errors.push(ValidationIssue {
                field: "novelty_diverse_top",
                messages: vec![format!(
                    "if set, must be between 1 and {MAX_NOVELTY_DIVERSE_TOP}"
                )],
            });
        }
    }
    if let Some(p) = request.novelty_pool {
        if !(2..=MAX_NOVELTY_POOL).contains(&p) {
            errors.push(ValidationIssue {
                field: "novelty_pool",
                messages: vec![format!("if set, must be between 2 and {MAX_NOVELTY_POOL}")],
            });
        }
    }
    if let (Some(d), Some(p)) = (request.novelty_diverse_top, request.novelty_pool) {
        if p < d {
            errors.push(ValidationIssue {
                field: "novelty_pool",
                messages: vec![
                    "if both novelty_pool and novelty_diverse_top are set, novelty_pool must be >= novelty_diverse_top".to_string(),
                ],
            });
        }
    }

    validate_optimize_constraints(request, &mut errors);

    if let Some(ref ch) = request.chain {
        if let Err(msg) = chain_grind_params_from_request(ch) {
            errors.push(ValidationIssue {
                field: "chain",
                messages: vec![msg],
            });
        }
    }

    if request.fast_discovery == Some(true) {
        if request.heuristics_only == Some(true) {
            errors.push(ValidationIssue {
                field: "fast_discovery",
                messages: vec![
                    "cannot be used together with heuristics_only (no main optimize pass)"
                        .to_string(),
                ],
            });
        }
        if request
            .heuristics_seeds
            .as_ref()
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            errors.push(ValidationIssue {
                field: "fast_discovery",
                messages: vec!["requires non-empty heuristics_seeds".to_string()],
            });
        }
        if request.strategy.as_deref() == Some("genetic") {
            errors.push(ValidationIssue {
                field: "fast_discovery",
                messages: vec![
                    "cannot be used with strategy genetic (use heuristics_seeds without fast_discovery for seeded GA)"
                        .to_string(),
                ],
            });
        }
    }

    let strategy = parse_strategy(request.strategy.as_ref());
    if strategy == OptimizerStrategy::LinearEval {
        if request.chain.as_ref().is_some_and(|c| c.enabled) {
            errors.push(ValidationIssue {
                field: "chain",
                messages: vec![
                    "chain grind is not supported with strategy linear_eval (expected hull damage only)"
                        .to_string(),
                ],
            });
        }
        if request.heuristics_only == Some(true) {
            errors.push(ValidationIssue {
                field: "heuristics_only",
                messages: vec![
                    "heuristics_only is not supported with strategy linear_eval (no Monte Carlo)"
                        .to_string(),
                ],
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

    if let Some(ref cap_raw) = dto.captain_must_be {
        for alt in parse_captain_must_be_tokens(cap_raw) {
            let n = normalize_officer_name(&alt);
            if !n.is_empty() && exclude_n.contains(&n) {
                errors.push(ValidationIssue {
                    field: "constraints.captain_must_be",
                    messages: vec![
                        "captain_must_be cannot list an officer who is also in exclude".to_string(),
                    ],
                });
                break;
            }
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
        captain_must_be: dto
            .captain_must_be
            .as_ref()
            .map(|s| parse_captain_must_be_tokens(s))
            .unwrap_or_default(),
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
    match s {
        Some(v) if v.trim().eq_ignore_ascii_case("exploration") => BelowDecksStrategy::Exploration,
        _ => BelowDecksStrategy::Ordered,
    }
}

pub fn parse_strategy(s: Option<&String>) -> OptimizerStrategy {
    match s {
        Some(v) if v.trim().eq_ignore_ascii_case("genetic") => OptimizerStrategy::Genetic,
        Some(v) if v.trim().eq_ignore_ascii_case("tiered") => OptimizerStrategy::Tiered,
        Some(v) if v.trim().eq_ignore_ascii_case("linear_eval") => OptimizerStrategy::LinearEval,
        _ => OptimizerStrategy::Exhaustive,
    }
}

/// Parses query string for optimize estimate: ship, hostile, sims, optional max_candidates,
/// optional `below_decks_pool_mode`, optional ship_tier, ship_level, below_decks_slots.
#[allow(clippy::type_complexity)]
pub fn parse_optimize_estimate_query(
    query: &str,
) -> Result<
    (
        String,
        String,
        u32,
        Option<u32>,
        crate::data::heuristics::BelowDecksPoolMode,
        Option<u32>,
        Option<u32>,
        Option<u32>,
    ),
    OptimizePayloadError,
> {
    let mut ship = String::new();
    let mut hostile = String::new();
    let mut sims = DEFAULT_SIMS;
    let mut max_candidates: Option<u32> = None;
    let mut below_decks_pool_mode: Option<String> = None;
    let mut ship_tier: Option<u32> = None;
    let mut ship_level: Option<u32> = None;
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
                "below_decks_pool_mode" => {
                    below_decks_pool_mode = Some(value.to_string());
                }
                "prioritize_below_decks_ability" => {
                    return Err(OptimizePayloadError::Validation(ValidationErrorResponse {
                        status: "error",
                        message: "Validation failed",
                        errors: vec![ValidationIssue {
                            field: "prioritize_below_decks_ability",
                            messages: vec![format!(
                                "{REMOVED_PRIORITIZE_BELOW_DECKS_MSG} (query: below_decks_pool_mode=relaxed)"
                            )],
                        }],
                    }));
                }
                "ship_tier" => ship_tier = value.parse().ok(),
                "ship_level" => ship_level = value.parse().ok(),
                "below_decks_slots" => below_decks_slots = value.parse().ok(),
                _ => {}
            }
        }
    }
    let mode = below_decks_pool_mode
        .as_deref()
        .and_then(crate::data::heuristics::BelowDecksPoolMode::parse_api_str)
        .unwrap_or(crate::data::heuristics::BelowDecksPoolMode::Strict);
    Ok((
        ship,
        hostile,
        sims,
        max_candidates,
        mode,
        ship_tier,
        ship_level,
        below_decks_slots,
    ))
}

#[cfg(test)]
mod below_decks_relax_tests {
    use super::{
        below_decks_pool_mode_resolved, relax_below_decks_combat_strictness, OptimizeRequest,
    };
    use crate::data::heuristics::BelowDecksPoolMode;

    fn req_from_json(s: &str) -> OptimizeRequest {
        serde_json::from_str(s).expect("json")
    }

    #[test]
    fn strict_when_pool_mode_absent() {
        let r = req_from_json(r#"{"ship":"s","hostile":"h"}"#);
        assert!(!relax_below_decks_combat_strictness(&r));
        assert_eq!(
            below_decks_pool_mode_resolved(&r),
            BelowDecksPoolMode::Strict
        );
    }

    #[test]
    fn relax_when_pool_mode_relaxed() {
        let r = req_from_json(r#"{"ship":"s","hostile":"h","below_decks_pool_mode":"relaxed"}"#);
        assert!(relax_below_decks_combat_strictness(&r));
        assert_eq!(
            below_decks_pool_mode_resolved(&r),
            BelowDecksPoolMode::Relaxed
        );
    }

    #[test]
    fn pool_mode_scored_resolves() {
        let r = req_from_json(r#"{"ship":"s","hostile":"h","below_decks_pool_mode":"scored"}"#);
        assert_eq!(
            below_decks_pool_mode_resolved(&r),
            BelowDecksPoolMode::Scored
        );
        assert!(!relax_below_decks_combat_strictness(&r));
    }

    #[test]
    fn pool_mode_unknown_value_falls_back_to_strict() {
        let r = req_from_json(r#"{"ship":"s","hostile":"h","below_decks_pool_mode":"unknown"}"#);
        assert_eq!(
            below_decks_pool_mode_resolved(&r),
            BelowDecksPoolMode::Strict
        );
    }
}

#[cfg(test)]
mod parse_optimize_body_tests {
    use super::{parse_optimize_estimate_query, parse_optimize_request_body, OptimizePayloadError};

    #[test]
    fn rejects_removed_prioritize_in_json_body() {
        let err = parse_optimize_request_body(
            r#"{"ship":"s","hostile":"h","prioritize_below_decks_ability":false}"#,
        )
        .unwrap_err();
        match err {
            OptimizePayloadError::Validation(v) => {
                assert!(v
                    .errors
                    .iter()
                    .any(|e| e.field == "prioritize_below_decks_ability"));
            }
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn rejects_removed_prioritize_in_estimate_query() {
        let err =
            parse_optimize_estimate_query("ship=s&hostile=h&prioritize_below_decks_ability=false")
                .unwrap_err();
        match err {
            OptimizePayloadError::Validation(v) => {
                assert!(v
                    .errors
                    .iter()
                    .any(|e| e.field == "prioritize_below_decks_ability"));
            }
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn estimate_query_pool_mode_relaxed() {
        use crate::data::heuristics::BelowDecksPoolMode;
        let (_, _, _, _, mode, _, _, _) =
            parse_optimize_estimate_query("ship=s&hostile=h&below_decks_pool_mode=relaxed")
                .expect("parse");
        assert_eq!(mode, BelowDecksPoolMode::Relaxed);
    }

    #[test]
    fn estimate_query_pool_mode_scored() {
        use crate::data::heuristics::BelowDecksPoolMode;
        let (_, _, _, _, mode, _, _, _) =
            parse_optimize_estimate_query("ship=s&hostile=h&below_decks_pool_mode=scored")
                .expect("parse");
        assert_eq!(mode, BelowDecksPoolMode::Scored);
    }

    #[test]
    fn estimate_query_pool_mode_strict_default_when_absent() {
        use crate::data::heuristics::BelowDecksPoolMode;
        let (_, _, _, _, mode, _, _, _) =
            parse_optimize_estimate_query("ship=s&hostile=h").expect("parse");
        assert_eq!(mode, BelowDecksPoolMode::Strict);
    }

    #[test]
    fn estimate_query_pool_mode_unknown_falls_back_to_strict() {
        use crate::data::heuristics::BelowDecksPoolMode;
        let (_, _, _, _, mode, _, _, _) =
            parse_optimize_estimate_query("ship=s&hostile=h&below_decks_pool_mode=garbage")
                .expect("parse");
        assert_eq!(mode, BelowDecksPoolMode::Strict);
    }
}
