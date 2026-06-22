mod execution;
mod pvp;
mod requests;

pub use pvp::{validate_scenario_target, ScenarioTarget, ScenarioTargetFields};

pub use execution::{
    cancel_job, get_job_status, patch_optimize_job_for_tests, run_optimize,
    seed_optimize_job_for_tests, start_optimize_job, CrewRecommendation, OptimizeJobState,
    OptimizeJobStatus, OptimizeResponse, OptimizeStartResponse, OptimizeStatusError,
    OptimizeStatusResponse, ScenarioSummary,
};
pub use requests::{
    chain_grind_params_from_request, parse_optimize_request_body, validate_request,
    ChainGrindRequest, DefenderOfficerCrewDto, OfficerGroupConstraintDto, OptimizeConstraintsDto,
    OptimizePayloadError, OptimizeRequest, ReplaySeedCrew, ReplaySeedRequest,
    ValidationErrorResponse, ValidationIssue, WarmStartCrewDto, DEFAULT_SIMS, MAX_CANDIDATES,
    MAX_SIMS,
};

use crate::data::building_summary::building_combat_summary_for_profile;
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::{list_heuristics_seeds, DEFAULT_HEURISTICS_DIR};
use crate::data::hostile_loca::resolve_hostile_display_name;
use crate::data::import::load_imported_ships;
use crate::data::import::{
    import_roster_csv_to, import_spocks_export_to, load_imported_forbidden_tech,
    load_imported_roster_ids_unlocked_only, roster_import_fallback_warning_message,
};

use crate::data::profile::{validate_player_profile_payload, PlayerProfile};
use crate::data::profile_index::{
    create_profile, delete_profile, effective_profile_id, load_profile_index, profile_path,
    FORBIDDEN_TECH_IMPORTED, PRESETS_SUBDIR, PROFILE_JSON, ROSTER_IMPORTED, SHIPS_IMPORTED,
};
use crate::data::research_summary::research_combat_summary_for_profile_with_scenario;
use crate::data::support_buffs;
use crate::optimizer::crew_generator::{
    resolve_below_decks_slots_for_ship, CandidateStrategy, CrewCandidate, CrewGenerator,
    BRIDGE_SLOTS,
};
use crate::optimizer::enforce_candidate_legality_with_registry;
use crate::optimizer::monte_carlo::scenario::{
    DefenderOpponent, PlayerDefenderOfficerCrewOverride,
};
use crate::optimizer::monte_carlo::{
    compare_crews_monte_carlo_with_registry, replay_optimize_iteration_with_registry,
    run_monte_carlo_with_registry, SimulationResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::Write;
use std::sync::Arc;

/// GET `/api/health` — liveness plus build identity, effective CPU concurrency, and loaded data signals.
pub fn health_payload(
    registry: &DataRegistry,
    started_at_utc: chrono::DateTime<chrono::Utc>,
    cpu_jobs: &tokio::sync::Semaphore,
    cpu_job_queue_wait: Option<std::time::Duration>,
    cpu_job_queue_wait_env_present: bool,
) -> Result<String, serde_json::Error> {
    let git_short = env!("KOBAYASHI_GIT_SHA_SHORT");
    let git_sha_short = if git_short.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(git_short.to_string())
    };

    let hostile_index = registry.hostile_index();
    let ship_index = registry.ship_index();
    let max_cpu = super::max_concurrent_cpu_jobs_from_env();
    let permits_available = cpu_jobs.available_permits() as u64;
    let queue_wait_ms_json = cpu_job_queue_wait
        .map(|d| serde_json::Value::from(d.as_millis() as u64))
        .unwrap_or(serde_json::Value::Null);
    let officer_count = registry.officers().len();

    serde_json::to_string(&serde_json::json!({
        "status": "ok",
        "service": "kobayashi-api",
        "build": {
            "cargo_pkg_version": env!("CARGO_PKG_VERSION"),
            "git_sha_short": git_sha_short,
        },
        "server": {
            "started_at_utc": started_at_utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "max_concurrent_cpu_jobs": max_cpu,
            "max_concurrent_cpu_jobs_from_env": std::env::var("KOBAYASHI_MAX_CONCURRENT_CPU_JOBS").is_ok(),
            "cpu_job_permits_available": permits_available,
            "cpu_job_permits_total": max_cpu,
            "cpu_job_queue_wait_ms": queue_wait_ms_json,
            "cpu_job_queue_wait_ms_from_env": cpu_job_queue_wait_env_present,
        },
        "data": {
            "officer_count": officer_count,
            "hostile_data_version": hostile_index.and_then(|i| i.data_version.clone()),
            "ship_data_version": ship_index.and_then(|i| i.data_version.clone()),
            "hostile_index_loaded": hostile_index.is_some(),
            "ship_index_loaded": ship_index.is_some(),
        },
    }))
}

/// Parse query string for owned_only=1
fn parse_owned_only(path: &str) -> bool {
    let query = path.split('?').nth(1).unwrap_or("");
    query.split('&').any(|p| {
        p.trim().eq_ignore_ascii_case("owned_only=1")
            || p.trim().eq_ignore_ascii_case("owned_only=true")
    })
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct OfficerListItem {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

pub fn officers_payload(
    registry: &DataRegistry,
    path: &str,
    profile_id: Option<&str>,
) -> Result<String, serde_json::Error> {
    let officers = registry.officers();
    let roster_path = if parse_owned_only(path) {
        let id = resolve_profile_id(profile_id);
        profile_path(&id, ROSTER_IMPORTED)
            .to_string_lossy()
            .to_string()
    } else {
        String::new()
    };
    let owned_ids = if roster_path.is_empty() {
        None
    } else {
        load_imported_roster_ids_unlocked_only(&roster_path)
    };
    let list: Vec<OfficerListItem> = officers
        .iter()
        .filter(|o| owned_ids.as_ref().is_none_or(|ids| ids.contains(&o.id)))
        .map(|o| OfficerListItem {
            id: o.id.clone(),
            name: o.name.clone(),
            slot: o.slot.clone(),
        })
        .collect();
    serde_json::to_string(&serde_json::json!({ "officers": list }))
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ShipListItem {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
    /// From roster when owned_only: tier of first roster entry for this ship.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<u32>,
    /// From roster when owned_only: level of first roster entry for this ship.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
}

const HULL_ID_REGISTRY_PATH: &str = "data/hull_id_registry.json";

/// When several `ships.imported.json` rows share the same hull → same Kobayashi ship id (e.g. two
/// Amalgams), keep the row with the highest tier, breaking ties by highest level.
fn merge_roster_tier_level(existing: (u32, u32), incoming: (u32, u32)) -> (u32, u32) {
    if existing.0 > incoming.0 || (existing.0 == incoming.0 && existing.1 > incoming.1) {
        existing
    } else {
        incoming
    }
}

/// Load hull_id -> ship_id mapping. Returns empty map if file missing or invalid.
pub(crate) fn load_hull_id_registry() -> HashMap<i64, String> {
    let raw = match fs::read_to_string(HULL_ID_REGISTRY_PATH) {
        Ok(s) => s,
        _ => return HashMap::new(),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        _ => return HashMap::new(),
    };
    let obj = match parsed.get("hull_id_to_ship_id").and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return HashMap::new(),
    };
    let mut out = HashMap::new();
    for (k, v) in obj {
        if let (Ok(hid), Some(sid)) = (k.parse::<i64>(), v.as_str()) {
            out.insert(hid, sid.to_string());
        }
    }
    out
}

#[allow(clippy::type_complexity)] // owned roster + tier/level map for ship list response
pub fn ships_payload(
    registry: &DataRegistry,
    owned_only: bool,
    profile_id: Option<&str>,
    hull_id_registry: &HashMap<i64, String>,
) -> Result<String, serde_json::Error> {
    let idx = match registry.ship_index() {
        Some(i) => i,
        None => {
            return serde_json::to_string(&serde_json::json!({ "ships": [] }));
        }
    };

    let (owned_ship_ids, roster_tier_level): (
        Option<std::collections::HashSet<String>>,
        std::collections::HashMap<String, (u32, u32)>,
    ) = if owned_only {
        let pid = resolve_profile_id(profile_id);
        let ships_path = profile_path(&pid, SHIPS_IMPORTED)
            .to_string_lossy()
            .to_string();
        let imported = load_imported_ships(&ships_path);

        let mut roster_tier_level = std::collections::HashMap::new();
        if let Some(ships) = &imported {
            for entry in ships {
                if let Some(sid) = hull_id_registry.get(&entry.hull_id) {
                    let t = entry.tier.max(0) as u32;
                    let l = entry.level.max(0) as u32;
                    roster_tier_level
                        .entry(sid.clone())
                        .and_modify(|cur| *cur = merge_roster_tier_level(*cur, (t, l)))
                        .or_insert((t, l));
                }
            }
        }

        if hull_id_registry.is_empty() {
            (None, roster_tier_level)
        } else if let Some(ships) = imported {
            let mut ids = std::collections::HashSet::new();
            for entry in &ships {
                if let Some(sid) = hull_id_registry.get(&entry.hull_id) {
                    ids.insert(sid.clone());
                }
            }
            if ids.is_empty() {
                (None, roster_tier_level)
            } else {
                (Some(ids), roster_tier_level)
            }
        } else {
            (None, roster_tier_level)
        }
    } else {
        (None, std::collections::HashMap::new())
    };

    let list: Vec<ShipListItem> = idx
        .ships
        .iter()
        .filter(|e| {
            owned_ship_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&e.id))
        })
        .map(|e| {
            let (tier, level) = roster_tier_level
                .get(&e.id)
                .copied()
                .map(|(t, l)| (Some(t), Some(l)))
                .unwrap_or((None, None));
            ShipListItem {
                id: e.id.clone(),
                ship_name: e.ship_name.clone(),
                ship_class: e.ship_class.clone(),
                tier,
                level,
            }
        })
        .collect();

    serde_json::to_string(&serde_json::json!({ "ships": list }))
}

/// Default tier/level options when extended ship data is missing (e.g. no data/ships_extended).
const DEFAULT_TIERS: &[u32] = &[1];
const DEFAULT_LEVELS: &[u32] = &[1, 10, 20, 30, 40, 50, 60];

pub fn ship_tiers_levels_payload(
    ship_id: &str,
    registry: &DataRegistry,
) -> Result<String, serde_json::Error> {
    let (mut tiers, mut levels, crew_slots) = registry
        .ship_tiers_levels_and_crew_slots(ship_id)
        .unwrap_or_else(|| (DEFAULT_TIERS.to_vec(), DEFAULT_LEVELS.to_vec(), vec![]));
    if tiers.is_empty() {
        tiers = DEFAULT_TIERS.to_vec();
    }
    if levels.is_empty() {
        levels = DEFAULT_LEVELS.to_vec();
    }
    serde_json::to_string(
        &serde_json::json!({ "tiers": tiers, "levels": levels, "crew_slots": crew_slots }),
    )
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct HostileListItem {
    pub id: String,
    /// Raw name from `data/hostiles` (may be a placeholder when using numeric upstream ids).
    pub hostile_name: String,
    /// Human-readable name for UI (from `loca_id` → translation map when available).
    pub display_name: String,
    pub level: u32,
    pub ship_class: String,
}

pub fn hostiles_payload(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    let loca_map = registry.hostile_loca_display();
    let list: Vec<HostileListItem> = registry
        .hostile_index()
        .map(|idx| {
            idx.hostiles
                .iter()
                .map(|e| {
                    let display_name =
                        resolve_hostile_display_name(loca_map, e.loca_id, &e.hostile_name);
                    HostileListItem {
                        id: e.id.clone(),
                        hostile_name: e.hostile_name.clone(),
                        display_name,
                        level: e.level,
                        ship_class: e.ship_class.clone(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::to_string(&serde_json::json!({ "hostiles": list }))
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct MechanicStatus {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct DataVersionResponse {
    pub officer_version: Option<String>,
    pub hostile_version: Option<String>,
    pub ship_version: Option<String>,
    pub mechanics: Vec<MechanicStatus>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SimulateRequest {
    pub ship: String,
    pub hostile: String,
    /// Ship tier (1-based). When set, uses data/ships_extended if present.
    pub ship_tier: Option<u32>,
    /// Ship level (1-based). When set with tier, applies level bonuses from extended data.
    pub ship_level: Option<u32>,
    pub crew: SimulateCrew,
    pub num_sims: Option<u32>,
    pub seed: Option<u64>,
    /// Below-decks slot count for padding crew (0–7). Omitted = resolved from ship level + `crew_slots` when present, else tier heuristic.
    pub below_decks_slots: Option<u32>,
    /// Optional alliance/ship support buff ids (see `data/support_buffs.json`).
    #[serde(default)]
    pub support_buffs: Option<Vec<String>>,
    /// PvP: defender alliance support buff ids (see `data/support_buffs.json`).
    #[serde(default)]
    pub defender_support_buffs: Option<Vec<String>>,
    /// PvP: alliance debuffs applied to the attacker (see `data/support_buffs.json`).
    #[serde(default)]
    pub defender_alliance_debuffs: Option<Vec<String>>,
    #[serde(default)]
    pub chain: Option<requests::ChainGrindRequest>,
    #[serde(default)]
    pub defender_opponent: DefenderOpponent,
    /// Optional LCARS crew for the **defending** side (merged with hostile ship abilities in PvE).
    /// Same shape as `crew`; requires non-empty `captain` to take effect.
    #[serde(default)]
    pub defender_crew: Option<SimulateCrew>,
    /// Player ship id for PvP (mutually exclusive with `hostile`). Requires `defender_profile_id`.
    #[serde(default)]
    pub defender_ship: Option<String>,
    pub defender_ship_tier: Option<u32>,
    pub defender_ship_level: Option<u32>,
    /// Opponent profile id when `defender_ship` is set (not the attacker profile header).
    #[serde(default)]
    pub defender_profile_id: Option<String>,
    /// Combat scenario for officer eligibility interpretability (e.g. `"mission_bosses"`). Snake_case
    /// [`crate::combat::EnemyType`]. When unset/invalid, inferred from the target.
    #[serde(default)]
    pub enemy_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SimulateCrew {
    pub captain: Option<String>,
    /// Bridge officer IDs; null entries mean "no officer" in that slot.
    pub bridge: Option<Vec<Option<String>>>,
    /// Below-deck officer IDs; null entries mean "no officer" in that slot.
    pub below_deck: Option<Vec<Option<String>>>,
}

/// One per-officer eligibility verdict for the resolved scenario. Interpretability only — this
/// never changes the simulation. Emitted for `does_not_work` and `conditional` verdicts (a
/// `works` verdict produces no note).
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct EligibilityNote {
    /// Crew officer display name.
    pub officer: String,
    /// Seat the officer occupies: `captain` | `officer` (bridge) | `below_decks`.
    pub slot: String,
    pub verdict: crate::data::officer_eligibility::EligibilityVerdict,
    /// Cheat-sheet reason text (gating condition for `conditional`, non-combat modifier for
    /// `does_not_work`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ability_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SimulateResponse {
    pub status: &'static str,
    pub stats: SimulateStats,
    pub seed: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Crew officers that did not resolve to an LCARS combat definition (so they contributed no
    /// effects). Empty for any roster-legal crew; a non-empty list signals a canonical↔LCARS gap.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved_officers: Vec<String>,
    /// Per-officer eligibility verdicts for the resolved scenario (does-not-work / conditional).
    /// Interpretability only — never affects the simulation. Empty when every crew ability works
    /// (or the eligibility matrix is unavailable).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub eligibility_notes: Vec<EligibilityNote>,
}

/// Human-friendly label for a combat scenario (used in eligibility warning strings).
fn enemy_type_label(e: crate::combat::EnemyType) -> &'static str {
    use crate::combat::EnemyType::*;
    match e {
        PvpSpace => "PvP (space)",
        PvpStation => "PvP (station)",
        RedMovingSpace => "hostiles",
        Waves => "wave defense",
        MissionBosses => "mission bosses",
        QTrial => "Q's Trial",
        SoloArmadas => "solo armadas",
        GroupArmadas => "group armadas",
        Assaults => "assaults",
        InvadingEntities => "invading entities",
        OutpostArmadas => "outpost armadas",
        OutpostRetaliationAttackers => "outpost retaliators",
    }
}

/// Resolve one crew member's eligibility for `slot` against `enemy`; when the verdict is
/// `conditional` or `does_not_work`, append a structured note and a human-readable warning.
/// A `works` verdict (or an officer/ability absent from the matrix) produces nothing.
fn collect_eligibility_note(
    matrix: &crate::data::officer_eligibility::EligibilityMatrix,
    officer_index: &std::collections::HashMap<String, crate::data::officer::Officer>,
    display_name: &str,
    slot: &str,
    enemy: crate::combat::EnemyType,
    notes: &mut Vec<EligibilityNote>,
    warnings: &mut Vec<String>,
) {
    use crate::data::officer_eligibility::EligibilityVerdict;
    if display_name.trim().is_empty() {
        return;
    }
    let key = crate::data::officer::normalize_officer_lookup_key(display_name);
    let Some(officer) = officer_index.get(&key) else {
        return;
    };
    let Some((verdict, reason)) =
        crate::data::officer_eligibility::seat_best_verdict(matrix, officer, slot, enemy)
    else {
        return;
    };
    if verdict == EligibilityVerdict::Works {
        return;
    }
    let ability_id = officer
        .abilities
        .iter()
        .find(|a| a.slot.eq_ignore_ascii_case(slot))
        .and_then(|a| a.ability_id.clone());
    let seat_label = match slot {
        "captain" => "captain",
        "officer" => "bridge",
        _ => "below decks",
    };
    let scenario_label = enemy_type_label(enemy);
    let message = match (verdict, reason.as_deref()) {
        (EligibilityVerdict::DoesNotWork, Some(r)) => {
            format!("{display_name} ({seat_label}) may not work vs {scenario_label}: {r}")
        }
        (EligibilityVerdict::DoesNotWork, None) => {
            format!("{display_name} ({seat_label}) may not work vs {scenario_label}")
        }
        (EligibilityVerdict::Conditional, Some(r)) => format!(
            "{display_name} ({seat_label}) only works vs {scenario_label} if conditions are met ({r})"
        ),
        (EligibilityVerdict::Conditional, None) => format!(
            "{display_name} ({seat_label}) only works vs {scenario_label} if conditions are met"
        ),
        (EligibilityVerdict::Works, _) => return,
    };
    warnings.push(message);
    notes.push(EligibilityNote {
        officer: display_name.to_string(),
        slot: slot.to_string(),
        verdict,
        reason,
        ability_id,
    });
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SimulateStats {
    pub win_rate: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub avg_hull_remaining: f64,
    /// Mean hostile hull remaining as a fraction of max hull (0–1), all trials.
    pub avg_defender_hull_remaining: f64,
    pub n: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate_95_ci: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct CompareCrewsRequest {
    pub ship: String,
    pub hostile: String,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub crews: Vec<SimulateCrew>,
    #[serde(default)]
    pub num_sims: Option<u32>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub below_decks_slots: Option<u32>,
    /// Traced trials per crew (capped) to estimate proc-like event rates; 0 = skip.
    #[serde(default)]
    pub proc_sample_trials: Option<u32>,
    #[serde(default)]
    pub support_buffs: Option<Vec<String>>,
    #[serde(default)]
    pub defender_support_buffs: Option<Vec<String>>,
    #[serde(default)]
    pub defender_alliance_debuffs: Option<Vec<String>>,
    #[serde(default)]
    pub defender_opponent: DefenderOpponent,
    #[serde(default)]
    pub defender_crew: Option<SimulateCrew>,
    #[serde(default)]
    pub defender_ship: Option<String>,
    pub defender_ship_tier: Option<u32>,
    pub defender_ship_level: Option<u32>,
    #[serde(default)]
    pub defender_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct CompareCrewsResponse {
    pub status: &'static str,
    pub seed: u64,
    pub crews: Vec<crate::optimizer::monte_carlo::CompareCrewDistribution>,
    pub using_placeholder_combatants: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum CompareCrewsError {
    Parse(serde_json::Error),
    Validation(String),
}

impl fmt::Display for CompareCrewsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Validation(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for CompareCrewsError {}

fn officer_id_to_name(id: &str, officers: &[(String, String)]) -> String {
    officers
        .iter()
        .find(|(oid, _)| oid.eq_ignore_ascii_case(id))
        .map(|(_, name)| name.as_str())
        .unwrap_or(id)
        .to_string()
}

fn crew_candidate_from_officer_fields(
    captain: Option<&str>,
    bridge: Option<&[Option<String>]>,
    below_deck: Option<&[Option<String>]>,
    officers: &[(String, String)],
    below_decks_slots: usize,
) -> Result<CrewCandidate, String> {
    let captain = captain
        .map(|s| officer_id_to_name(s, officers))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "crew.captain is required".to_string())?;
    let bridge_names: Vec<String> = bridge
        .map(|v| {
            v.iter()
                .take(BRIDGE_SLOTS)
                .map(|s| {
                    s.as_ref()
                        .map(|id| officer_id_to_name(id, officers))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let below_names: Vec<String> = below_deck
        .map(|v| {
            v.iter()
                .take(below_decks_slots)
                .map(|s| {
                    s.as_ref()
                        .map(|id| officer_id_to_name(id, officers))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let bridge = pad_to_len(bridge_names, BRIDGE_SLOTS);
    let below_decks = pad_to_len(below_names, below_decks_slots);
    Ok(CrewCandidate {
        captain,
        bridge,
        below_decks,
    })
}

fn pad_to_len(mut v: Vec<String>, len: usize) -> Vec<String> {
    let first = v.first().cloned().unwrap_or_default();
    while v.len() < len {
        v.push(first.clone());
    }
    v.truncate(len);
    v
}

/// Resolve optional API defender crew into LCARS merge input for Monte Carlo / shared scenario.
/// Returns `Ok(None)` when unset or effectively empty; errors use human-readable messages.
pub(crate) fn resolve_player_defender_officer_crew_for_simulate(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    defender: Option<&SimulateCrew>,
) -> Result<Option<PlayerDefenderOfficerCrewOverride>, String> {
    let Some(d) = defender else {
        return Ok(None);
    };
    resolve_player_defender_officer_crew_from_officer_fields(
        registry,
        profile_id,
        below_decks_slots,
        d.captain.as_ref(),
        d.bridge.as_ref(),
        d.below_deck.as_ref(),
    )
}

pub(crate) fn resolve_player_defender_officer_crew_for_optimize(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    defender: Option<&requests::DefenderOfficerCrewDto>,
) -> Result<Option<PlayerDefenderOfficerCrewOverride>, String> {
    let Some(d) = defender else {
        return Ok(None);
    };
    resolve_player_defender_officer_crew_from_officer_fields(
        registry,
        profile_id,
        below_decks_slots,
        d.captain.as_ref(),
        d.bridge.as_ref(),
        d.below_deck.as_ref(),
    )
}

fn resolve_player_defender_officer_crew_from_officer_fields(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    captain: Option<&String>,
    bridge: Option<&Vec<Option<String>>>,
    below_deck: Option<&Vec<Option<String>>>,
) -> Result<Option<PlayerDefenderOfficerCrewOverride>, String> {
    let has_other = bridge
        .map(|b| {
            b.iter()
                .any(|s| s.as_ref().map(|x| !x.trim().is_empty()).unwrap_or(false))
        })
        .unwrap_or(false)
        || below_deck
            .map(|v| {
                v.iter()
                    .any(|s| s.as_ref().map(|x| !x.trim().is_empty()).unwrap_or(false))
            })
            .unwrap_or(false);

    let cap_trimmed = match captain.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(s) => s,
        None => {
            if has_other {
                return Err(
                    "defender_crew.captain is required when bridge or below_deck entries are set"
                        .into(),
                );
            }
            return Ok(None);
        }
    };

    let officers: Vec<(String, String)> = registry
        .officers()
        .iter()
        .map(|o| (o.id.clone(), o.name.clone()))
        .collect();
    let d_candidate = crew_candidate_from_officer_fields(
        Some(cap_trimmed),
        bridge.map(|v| v.as_slice()),
        below_deck.map(|v| v.as_slice()),
        &officers,
        below_decks_slots,
    )?;
    let (candidates_d, _) = enforce_candidate_legality_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        vec![d_candidate],
    );
    if candidates_d.is_empty() {
        return Err("defender_crew is not roster/seat legal for this scenario".into());
    }
    Ok(Some(PlayerDefenderOfficerCrewOverride {
        captain: captain.cloned(),
        bridge: bridge.cloned(),
        below_deck: below_deck.cloned(),
        below_decks_slots,
    }))
}

fn binomial_95_ci(wins: u32, n: u32) -> [f64; 2] {
    if n == 0 {
        return [0.0, 0.0];
    }
    let p = wins as f64 / n as f64;
    let z = 1.96;
    let se = (p * (1.0 - p) / n as f64).sqrt();
    let lo = (p - z * se).max(0.0);
    let hi = (p + z * se).min(1.0);
    [lo, hi]
}

pub fn simulate_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, SimulateError> {
    let req: SimulateRequest = serde_json::from_str(body).map_err(SimulateError::Parse)?;
    if let Err(errors) = validate_scenario_target(&ScenarioTargetFields {
        hostile: Some(req.hostile.clone()),
        defender_ship: req.defender_ship.clone(),
        defender_ship_tier: req.defender_ship_tier,
        defender_ship_level: req.defender_ship_level,
        defender_profile_id: req.defender_profile_id.clone(),
    }) {
        return Err(SimulateError::Validation(
            errors
                .into_iter()
                .flat_map(|e| e.messages)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let pvp = crate::optimizer::monte_carlo::pvp_scenario_params_from_api_fields(
        req.defender_ship.as_deref(),
        req.defender_ship_tier,
        req.defender_ship_level,
        req.defender_profile_id.as_deref(),
    );
    let scenario_hostile: String = pvp
        .as_ref()
        .map(|p| p.defender_ship.clone())
        .unwrap_or_else(|| req.hostile.trim().to_string());
    // Captured before `pvp` is moved into the simulation; reused by eligibility interpretability.
    let is_pvp = pvp.is_some();
    let defender_opponent = if pvp.is_some() {
        DefenderOpponent::Player
    } else {
        req.defender_opponent
    };
    let num_sims = req.num_sims.unwrap_or(5000).clamp(1, 100_000);
    let seed = req.seed.unwrap_or(0);
    if let Some(n) = req.below_decks_slots {
        let lo = crate::optimizer::crew_generator::MIN_BELOW_DECKS_SLOTS as u32;
        let hi = crate::optimizer::crew_generator::MAX_BELOW_DECKS_SLOTS as u32;
        if !(lo..=hi).contains(&n) {
            return Err(SimulateError::Validation(format!(
                "below_decks_slots must be between {lo} and {hi}"
            )));
        }
    }
    let below_decks_slots = resolve_below_decks_slots_for_ship(
        &req.ship,
        req.ship_tier,
        req.ship_level,
        req.below_decks_slots,
    );

    if let Some(ref ch) = &req.chain {
        if let Err(msg) = requests::chain_grind_params_from_request(ch) {
            return Err(SimulateError::Validation(msg));
        }
    }
    let chain_grind = req
        .chain
        .as_ref()
        .and_then(|c| requests::chain_grind_params_from_request(c).ok().flatten());

    let officers: Vec<(String, String)> = registry
        .officers()
        .iter()
        .map(|o| (o.id.clone(), o.name.clone()))
        .collect();

    let candidate = crew_candidate_from_officer_fields(
        req.crew.captain.as_deref(),
        req.crew.bridge.as_deref(),
        req.crew.below_deck.as_deref(),
        &officers,
        below_decks_slots,
    )
    .map_err(SimulateError::Validation)?;

    let CrewCandidate {
        captain,
        bridge,
        below_decks,
    } = candidate.clone();
    let (candidates, _) = enforce_candidate_legality_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        vec![candidate],
    );
    if candidates.is_empty() {
        return Err(SimulateError::Validation(
            "crew is not roster/seat legal for this scenario".to_string(),
        ));
    }
    let player_defender_officer_crew = resolve_player_defender_officer_crew_for_simulate(
        registry,
        profile_id,
        below_decks_slots,
        req.defender_crew.as_ref(),
    )
    .map_err(SimulateError::Validation)?;
    let support_buff_request = support_buffs::SupportBuffScenarioRequest::from_api_options(
        req.support_buffs.as_deref(),
        req.defender_support_buffs.as_deref(),
        req.defender_alliance_debuffs.as_deref(),
    );
    let (results, using_placeholder_combatants) = run_monte_carlo_with_registry(
        registry,
        &req.ship,
        scenario_hostile.as_str(),
        req.ship_tier,
        req.ship_level,
        &candidates,
        num_sims as usize,
        seed,
        profile_id,
        support_buff_request,
        chain_grind,
        defender_opponent,
        player_defender_officer_crew,
        pvp,
    );
    let result = results.into_iter().next().unwrap_or(SimulationResult {
        candidate: CrewCandidate {
            captain,
            bridge,
            below_decks,
        },
        trials_run: 0,
        win_rate: 0.0,
        win_rate_ci_low: 0.0,
        win_rate_ci_high: 0.0,
        stall_rate: 0.0,
        stall_rate_ci_low: 0.0,
        stall_rate_ci_high: 0.0,
        loss_rate: 0.0,
        loss_rate_ci_low: 0.0,
        loss_rate_ci_high: 0.0,
        r1_kill_rate: 0.0,
        r1_kill_rate_ci_low: 0.0,
        r1_kill_rate_ci_high: 0.0,
        avg_hull_remaining: 0.0,
        avg_hull_remaining_ci_low: 0.0,
        avg_hull_remaining_ci_high: 0.0,
        avg_defender_hull_remaining: 0.0,
        avg_defender_hull_remaining_ci_low: 0.0,
        avg_defender_hull_remaining_ci_high: 0.0,
        chain: None,
        expected_hull_damage: None,
    });

    let wins = (result.win_rate * num_sims as f64).round() as u32;
    let ci = binomial_95_ci(wins, num_sims);

    let mut warnings = Vec::new();
    if let Some(v) = &req.support_buffs {
        if v.len() > support_buffs::MAX_SUPPORT_BUFFS_PER_REQUEST {
            warnings.push(format!(
                "support_buffs: only the first {} entries are applied",
                support_buffs::MAX_SUPPORT_BUFFS_PER_REQUEST
            ));
        }
    }
    if let Some(sb) = req.support_buffs.as_deref() {
        if let Some(cat) = registry.support_buffs_catalog() {
            let (_, unk) = support_buffs::resolve_selected_support_buff_ids(cat, sb);
            for u in unk {
                warnings.push(format!("Unknown support_buff id: {u}"));
            }
            let inactive = support_buffs::inactive_defender_static_support_buff_labels(
                cat,
                sb,
                req.defender_opponent.defender_is_player_ship(),
            );
            if !inactive.is_empty() {
                warnings.push(format!(
                    "Direct static bonuses for support buff(s) {} apply only vs a player-shaped defender (defender_opponent: player); they are ignored vs NPC hostiles.",
                    inactive.join(", ")
                ));
            }
        }
    }
    if let Some(cat) = registry.support_buffs_catalog() {
        if let Some(sb) = req.defender_support_buffs.as_deref() {
            let (_, unk) = support_buffs::resolve_selected_support_buff_ids(cat, sb);
            for u in unk {
                warnings.push(format!("Unknown defender_support_buff id: {u}"));
            }
        }
        if let Some(sb) = req.defender_alliance_debuffs.as_deref() {
            let (_, unk) = support_buffs::resolve_selected_support_buff_ids(cat, sb);
            for u in unk {
                warnings.push(format!("Unknown defender_alliance_debuff id: {u}"));
            }
        }
    }
    if using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats. Results do not reflect real ship/hostile values."
                .to_string(),
        );
    }
    let unresolved_officers = crate::optimizer::monte_carlo::unresolved_officers_for_candidate(
        registry,
        &result.candidate,
    );
    if !unresolved_officers.is_empty() {
        warnings.push(format!(
            "Officer(s) with no LCARS combat definition contributed no effects: {}",
            unresolved_officers.join(", ")
        ));
    }
    if let Some(message) = roster_import_fallback_warning_message(profile_id) {
        warnings.push(message);
    }

    // Eligibility interpretability (never affects the simulation): flag any crew member whose seat
    // ability does-not-work / is-conditional vs the resolved scenario, per the eligibility matrix.
    let mut eligibility_notes: Vec<EligibilityNote> = Vec::new();
    if let Some(matrix) = registry.eligibility_matrix() {
        let hostile_for_scenario = if is_pvp {
            None
        } else {
            registry.resolve_hostile(scenario_hostile.trim())
        };
        let enemy = crate::data::officer_eligibility::resolve_enemy_type(
            req.enemy_type.as_deref(),
            is_pvp,
            hostile_for_scenario.as_ref(),
        );
        let officer_index = registry.officer_index();
        collect_eligibility_note(
            matrix,
            officer_index,
            &result.candidate.captain,
            "captain",
            enemy,
            &mut eligibility_notes,
            &mut warnings,
        );
        for name in &result.candidate.bridge {
            collect_eligibility_note(
                matrix,
                officer_index,
                name,
                "officer",
                enemy,
                &mut eligibility_notes,
                &mut warnings,
            );
        }
        for name in &result.candidate.below_decks {
            collect_eligibility_note(
                matrix,
                officer_index,
                name,
                "below_decks",
                enemy,
                &mut eligibility_notes,
                &mut warnings,
            );
        }
    }

    let response = SimulateResponse {
        status: "ok",
        stats: SimulateStats {
            win_rate: result.win_rate,
            stall_rate: result.stall_rate,
            loss_rate: result.loss_rate,
            avg_hull_remaining: result.avg_hull_remaining,
            avg_defender_hull_remaining: result.avg_defender_hull_remaining,
            n: num_sims,
            win_rate_95_ci: Some(ci),
        },
        seed,
        warnings,
        unresolved_officers,
        eligibility_notes,
    };
    serde_json::to_string(&response).map_err(SimulateError::Parse)
}

pub fn compare_crews_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, CompareCrewsError> {
    let req: CompareCrewsRequest = serde_json::from_str(body).map_err(CompareCrewsError::Parse)?;
    if let Err(errors) = validate_scenario_target(&ScenarioTargetFields {
        hostile: Some(req.hostile.clone()),
        defender_ship: req.defender_ship.clone(),
        defender_ship_tier: req.defender_ship_tier,
        defender_ship_level: req.defender_ship_level,
        defender_profile_id: req.defender_profile_id.clone(),
    }) {
        return Err(CompareCrewsError::Validation(
            errors
                .into_iter()
                .flat_map(|e| e.messages)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let pvp = crate::optimizer::monte_carlo::pvp_scenario_params_from_api_fields(
        req.defender_ship.as_deref(),
        req.defender_ship_tier,
        req.defender_ship_level,
        req.defender_profile_id.as_deref(),
    );
    let scenario_hostile: String = pvp
        .as_ref()
        .map(|p| p.defender_ship.clone())
        .unwrap_or_else(|| req.hostile.trim().to_string());
    let defender_opponent = if pvp.is_some() {
        DefenderOpponent::Player
    } else {
        req.defender_opponent
    };
    let crew_count = req.crews.len();
    if !(2..=5).contains(&crew_count) {
        return Err(CompareCrewsError::Validation(
            "crews must contain between 2 and 5 entries".to_string(),
        ));
    }
    let num_sims = req.num_sims.unwrap_or(3000).clamp(200, 20_000);
    let seed = req.seed.unwrap_or(0);
    if let Some(n) = req.below_decks_slots {
        let lo = crate::optimizer::crew_generator::MIN_BELOW_DECKS_SLOTS as u32;
        let hi = crate::optimizer::crew_generator::MAX_BELOW_DECKS_SLOTS as u32;
        if !(lo..=hi).contains(&n) {
            return Err(CompareCrewsError::Validation(format!(
                "below_decks_slots must be between {lo} and {hi}"
            )));
        }
    }
    let proc_sample = req.proc_sample_trials.unwrap_or(0).min(150);
    let below_decks_slots = resolve_below_decks_slots_for_ship(
        &req.ship,
        req.ship_tier,
        req.ship_level,
        req.below_decks_slots,
    );

    let officers: Vec<(String, String)> = registry
        .officers()
        .iter()
        .map(|o| (o.id.clone(), o.name.clone()))
        .collect();

    let mut candidates = Vec::with_capacity(crew_count);
    for c in &req.crews {
        let cand = crew_candidate_from_officer_fields(
            c.captain.as_deref(),
            c.bridge.as_deref(),
            c.below_deck.as_deref(),
            &officers,
            below_decks_slots,
        )
        .map_err(CompareCrewsError::Validation)?;
        candidates.push(cand);
    }
    let (candidates, _) = enforce_candidate_legality_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        candidates,
    );
    if candidates.len() != crew_count {
        return Err(CompareCrewsError::Validation(
            "one or more crews are not roster/seat legal for this scenario".to_string(),
        ));
    }

    let support_buff_request = support_buffs::SupportBuffScenarioRequest::from_api_options(
        req.support_buffs.as_deref(),
        req.defender_support_buffs.as_deref(),
        req.defender_alliance_debuffs.as_deref(),
    );
    let outcome = compare_crews_monte_carlo_with_registry(
        registry,
        &req.ship,
        scenario_hostile.as_str(),
        req.ship_tier,
        req.ship_level,
        &candidates,
        num_sims as usize,
        seed,
        profile_id,
        proc_sample,
        support_buff_request,
        defender_opponent,
        pvp,
    );

    let mut warnings = Vec::new();
    if let Some(v) = &req.support_buffs {
        if v.len() > support_buffs::MAX_SUPPORT_BUFFS_PER_REQUEST {
            warnings.push(format!(
                "support_buffs: only the first {} entries are applied",
                support_buffs::MAX_SUPPORT_BUFFS_PER_REQUEST
            ));
        }
    }
    if let Some(sb) = req.support_buffs.as_deref() {
        if let Some(cat) = registry.support_buffs_catalog() {
            let (_, unk) = support_buffs::resolve_selected_support_buff_ids(cat, sb);
            for u in unk {
                warnings.push(format!("Unknown support_buff id: {u}"));
            }
            let inactive = support_buffs::inactive_defender_static_support_buff_labels(
                cat,
                sb,
                req.defender_opponent.defender_is_player_ship(),
            );
            if !inactive.is_empty() {
                warnings.push(format!(
                    "Direct static bonuses for support buff(s) {} apply only vs a player-shaped defender (defender_opponent: player); they are ignored vs NPC hostiles.",
                    inactive.join(", ")
                ));
            }
        }
    }
    if outcome.using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats."
                .to_string(),
        );
    }
    if let Some(message) = roster_import_fallback_warning_message(profile_id) {
        warnings.push(message);
    }

    let response = CompareCrewsResponse {
        status: "ok",
        seed,
        crews: outcome.crews,
        using_placeholder_combatants: outcome.using_placeholder_combatants,
        warnings,
    };
    serde_json::to_string(&response).map_err(CompareCrewsError::Parse)
}

#[derive(Debug)]
pub enum SimulateError {
    Parse(serde_json::Error),
    Validation(String),
}

impl fmt::Display for SimulateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Validation(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for SimulateError {}

#[derive(Debug)]
pub enum SensitivityError {
    Parse(serde_json::Error),
    Validation(String),
    Run(String),
}

impl fmt::Display for SensitivityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Validation(m) => write!(f, "{m}"),
            Self::Run(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for SensitivityError {}

/// POST `/api/sensitivity` — stat-level Δ-on-outcome analysis (paired CRN Monte Carlo).
/// Synchronous v1; long runs are gated by the process-wide CPU admission semaphore.
pub fn sensitivity_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, SensitivityError> {
    use crate::optimizer::sensitivity::{run_sensitivity, SensitivityRequest};

    let mut req: SensitivityRequest =
        serde_json::from_str(body).map_err(SensitivityError::Parse)?;
    if req.ship.trim().is_empty() {
        return Err(SensitivityError::Validation(
            "ship must not be empty".into(),
        ));
    }
    if req.hostile.trim().is_empty() {
        return Err(SensitivityError::Validation(
            "hostile must not be empty".into(),
        ));
    }
    // Header / query profile id wins when the body did not supply one (matches /api/simulate).
    if req.profile_id.is_none() {
        if let Some(pid) = profile_id {
            if !pid.is_empty() {
                req.profile_id = Some(pid.to_string());
            }
        }
    }
    // Bound the sim count so a single request can't monopolise the CPU pool indefinitely.
    if let Some(n) = req.num_sims {
        req.num_sims = Some(n.clamp(2, 50_000));
    }

    let response = run_sensitivity(registry, &req).map_err(SensitivityError::Run)?;
    serde_json::to_string(&response)
        .map_err(|e| SensitivityError::Validation(format!("serialize response: {e}")))
}

/// GET `/api/sensitivity/defaults` — per-stat default delta catalog. Frontend uses this to
/// pre-fill the override table. Body is `{"deltas":[{"stat":"weapon_damage","delta":0.05}, ...]}`.
pub fn sensitivity_defaults_payload() -> String {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Row {
        stat: &'static str,
        delta: f64,
        multiplicative: bool,
    }

    #[derive(Serialize)]
    struct Resp {
        deltas: Vec<Row>,
    }

    let deltas = crate::optimizer::sensitivity::default_deltas()
        .into_iter()
        .map(|(s, d)| Row {
            stat: s.as_str(),
            delta: d,
            multiplicative: s.is_multiplicative(),
        })
        .collect();
    serde_json::to_string(&Resp { deltas }).expect("serialize sensitivity defaults")
}

/// POST `/api/sensitivity/morris` — Morris-method screening (random trajectories, μ\*/σ).
/// Synchronous; gated by the CPU admission semaphore.
pub fn sensitivity_morris_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, SensitivityError> {
    use crate::optimizer::sensitivity_morris::{run_morris, MorrisRequest};

    let mut req: MorrisRequest = serde_json::from_str(body).map_err(SensitivityError::Parse)?;
    if req.ship.trim().is_empty() {
        return Err(SensitivityError::Validation(
            "ship must not be empty".into(),
        ));
    }
    if req.hostile.trim().is_empty() {
        return Err(SensitivityError::Validation(
            "hostile must not be empty".into(),
        ));
    }
    if req.profile_id.is_none() {
        if let Some(pid) = profile_id {
            if !pid.is_empty() {
                req.profile_id = Some(pid.to_string());
            }
        }
    }

    let response = run_morris(registry, &req).map_err(SensitivityError::Run)?;
    serde_json::to_string(&response)
        .map_err(|e| SensitivityError::Validation(format!("serialize response: {e}")))
}

/// GET `/api/sensitivity/morris/defaults` — defaults for the Morris UI: per-stat δ catalog
/// plus default trajectory count and sims-per-point.
pub fn sensitivity_morris_defaults_payload() -> String {
    use crate::optimizer::sensitivity_morris::{
        DEFAULT_NUM_SIMS_PER_POINT, DEFAULT_R_TRAJECTORIES, MAX_NUM_SIMS_PER_POINT,
        MAX_R_TRAJECTORIES,
    };
    use serde::Serialize;

    #[derive(Serialize)]
    struct Row {
        stat: &'static str,
        delta: f64,
        multiplicative: bool,
    }

    #[derive(Serialize)]
    struct Resp {
        deltas: Vec<Row>,
        r_trajectories_default: u32,
        r_trajectories_max: u32,
        num_sims_default: u32,
        num_sims_max: u32,
    }

    let deltas = crate::optimizer::sensitivity::default_deltas()
        .into_iter()
        .map(|(s, d)| Row {
            stat: s.as_str(),
            delta: d,
            multiplicative: s.is_multiplicative(),
        })
        .collect();
    serde_json::to_string(&Resp {
        deltas,
        r_trajectories_default: DEFAULT_R_TRAJECTORIES,
        r_trajectories_max: MAX_R_TRAJECTORIES,
        num_sims_default: DEFAULT_NUM_SIMS_PER_POINT,
        num_sims_max: MAX_NUM_SIMS_PER_POINT,
    })
    .expect("serialize sensitivity morris defaults")
}

/// POST `/api/sensitivity/sobol` — Sobol variance-based sensitivity (Saltelli design, Jansen
/// estimators). Synchronous; gated by the CPU admission semaphore.
pub fn sensitivity_sobol_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, SensitivityError> {
    use crate::optimizer::sensitivity_sobol::{run_sobol, SobolRequest};

    let mut req: SobolRequest = serde_json::from_str(body).map_err(SensitivityError::Parse)?;
    if req.ship.trim().is_empty() {
        return Err(SensitivityError::Validation(
            "ship must not be empty".into(),
        ));
    }
    if req.hostile.trim().is_empty() {
        return Err(SensitivityError::Validation(
            "hostile must not be empty".into(),
        ));
    }
    if req.profile_id.is_none() {
        if let Some(pid) = profile_id {
            if !pid.is_empty() {
                req.profile_id = Some(pid.to_string());
            }
        }
    }

    let response = run_sobol(registry, &req).map_err(SensitivityError::Run)?;
    serde_json::to_string(&response)
        .map_err(|e| SensitivityError::Validation(format!("serialize response: {e}")))
}

/// GET `/api/sensitivity/sobol/defaults` — δ catalog + default / max sample count.
pub fn sensitivity_sobol_defaults_payload() -> String {
    use crate::optimizer::sensitivity_sobol::{DEFAULT_N_SAMPLES, MAX_N_SAMPLES};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Row {
        stat: &'static str,
        delta: f64,
        multiplicative: bool,
    }

    #[derive(Serialize)]
    struct Resp {
        deltas: Vec<Row>,
        n_samples_default: u32,
        n_samples_max: u32,
    }

    let deltas = crate::optimizer::sensitivity::default_deltas()
        .into_iter()
        .map(|(s, d)| Row {
            stat: s.as_str(),
            delta: d,
            multiplicative: s.is_multiplicative(),
        })
        .collect();
    serde_json::to_string(&Resp {
        deltas,
        n_samples_default: DEFAULT_N_SAMPLES,
        n_samples_max: MAX_N_SAMPLES,
    })
    .expect("serialize sensitivity sobol defaults")
}

const DEFAULT_REPLAY_MAX_TRACE_EVENTS: u32 = 500;
const MAX_REPLAY_MAX_TRACE_EVENTS_CAP: u32 = 2000;

#[derive(Debug)]
pub enum ReplaySeedError {
    Parse(serde_json::Error),
    Validation(String),
}

impl fmt::Display for ReplaySeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Validation(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ReplaySeedError {}

/// POST `/api/optimize/replay-seed` — replay one Monte Carlo iteration with a combat event trace.
pub fn replay_optimize_seed_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, ReplaySeedError> {
    let req: ReplaySeedRequest = serde_json::from_str(body).map_err(ReplaySeedError::Parse)?;

    if req.ship.trim().is_empty() {
        return Err(ReplaySeedError::Validation(
            "ship must not be empty".to_string(),
        ));
    }
    if req.hostile.trim().is_empty() {
        return Err(ReplaySeedError::Validation(
            "hostile must not be empty".to_string(),
        ));
    }

    let officers: Vec<(String, String)> = registry
        .officers()
        .iter()
        .map(|o| (o.id.clone(), o.name.clone()))
        .collect();

    let below_decks_slots =
        resolve_below_decks_slots_for_ship(&req.ship, req.ship_tier, req.ship_level, None);
    let candidate = crew_candidate_from_officer_fields(
        req.crew.captain.as_deref(),
        req.crew.bridge.as_deref(),
        req.crew.below_deck.as_deref(),
        &officers,
        below_decks_slots,
    )
    .map_err(ReplaySeedError::Validation)?;
    let (candidates, _) = enforce_candidate_legality_with_registry(
        registry,
        profile_id,
        below_decks_slots,
        vec![candidate],
    );
    let Some(candidate) = candidates.into_iter().next() else {
        return Err(ReplaySeedError::Validation(
            "crew is not roster/seat legal for this scenario".to_string(),
        ));
    };

    let scenario_seed = req.seed.unwrap_or(0);
    let max_trace = req
        .max_trace_events
        .unwrap_or(DEFAULT_REPLAY_MAX_TRACE_EVENTS)
        .clamp(1, MAX_REPLAY_MAX_TRACE_EVENTS_CAP) as usize;

    let replay = replay_optimize_iteration_with_registry(
        registry,
        req.ship.trim(),
        req.hostile.trim(),
        req.ship_tier,
        req.ship_level,
        &candidate,
        scenario_seed,
        req.sim_index,
        profile_id,
        max_trace,
        req.support_buffs.as_deref(),
        req.defender_opponent,
    );

    let mut warnings = Vec::new();
    if replay.using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats."
                .to_string(),
        );
    }
    if let Some(message) = roster_import_fallback_warning_message(profile_id) {
        warnings.push(message);
    }
    for id in replay
        .external_buffs
        .pointer("/support_buffs/unknown_ids")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
    {
        warnings.push(format!("Unknown support_buff id: {id}"));
    }
    if let Some(sb) = req.support_buffs.as_deref() {
        if let Some(cat) = registry.support_buffs_catalog() {
            let inactive = support_buffs::inactive_defender_static_support_buff_labels(
                cat,
                sb,
                req.defender_opponent.defender_is_player_ship(),
            );
            if !inactive.is_empty() {
                warnings.push(format!(
                    "Direct static bonuses for support buff(s) {} apply only vs a player-shaped defender (defender_opponent: player); they are ignored vs NPC hostiles.",
                    inactive.join(", ")
                ));
            }
        }
    }

    let response_json = serde_json::json!({
        "status": "ok",
        "scenario": {
            "ship": req.ship.trim(),
            "hostile": req.hostile.trim(),
            "ship_tier": req.ship_tier,
            "ship_level": req.ship_level,
            "scenario_seed": scenario_seed,
            "sim_index": req.sim_index,
            "base_seed": replay.base_seed,
            "iteration_seed": replay.iteration_seed,
            "effective_defender_hull": replay.effective_defender_hull,
        },
        "summary": {
            "attacker_won": replay.attacker_won,
            "winner_by_round_limit": replay.winner_by_round_limit,
            "rounds_simulated": replay.rounds_simulated,
            "total_damage": replay.total_damage,
            "total_isolytic_damage": replay.total_isolytic_damage,
            "attacker_hull_remaining": replay.attacker_hull_remaining,
            "defender_hull_remaining": replay.defender_hull_remaining,
            "defender_shield_remaining": replay.defender_shield_remaining,
        },
        "trace": {
            "event_count": replay.trace_event_count,
            "events_returned": replay.trace_events_returned,
            "truncated": replay.trace_truncated,
            "external_buffs": replay.external_buffs,
            "events": replay.trace_events,
        },
        "warnings": warnings,
    });

    serde_json::to_string(&response_json).map_err(ReplaySeedError::Parse)
}

/// Resolve profile id from optional param; falls back to index default.
fn resolve_profile_id(profile_id: Option<&str>) -> String {
    let index = load_profile_index();
    profile_id
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| effective_profile_id(&index))
}

pub fn profile_get_payload(profile_id: Option<&str>) -> Result<String, serde_json::Error> {
    let id = resolve_profile_id(profile_id);
    let path = profile_path(&id, PROFILE_JSON);
    let profile: PlayerProfile = if path.exists() {
        let raw = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        PlayerProfile::default()
    };
    serde_json::to_string(&profile)
}

pub fn profile_put_payload(
    body: &str,
    profile_id: Option<&str>,
    registry: &DataRegistry,
) -> Result<String, serde_json::Error> {
    let profile: PlayerProfile = serde_json::from_str(body)?;
    let profile = validate_player_profile_payload(profile, registry.forbidden_chaos_catalog())
        .map_err(|issues| {
            serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                issues.join("; "),
            ))
        })?;
    let id = resolve_profile_id(profile_id);
    let path = profile_path(&id, PROFILE_JSON);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let body = serde_json::to_string(&profile)?;
    fs::write(&path, body).map_err(serde_json::Error::io)?;
    serde_json::to_string(&serde_json::json!({ "status": "ok" }))
}

/// GET /api/profile/forbidden-tech-imported — stfc-mod `forbidden_tech.imported.json` rows for equip UI.
pub fn profile_forbidden_tech_imported_payload(
    profile_id: Option<&str>,
) -> Result<String, serde_json::Error> {
    let id = resolve_profile_id(profile_id);
    let path = profile_path(&id, FORBIDDEN_TECH_IMPORTED);
    let entries = load_imported_forbidden_tech(&path.to_string_lossy()).unwrap_or_default();
    serde_json::to_string(&serde_json::json!({
        "profile_id": id,
        "forbidden_tech": entries,
    }))
}

/// GET /api/profile/buildings-summary — synced module levels and building-derived combat bonuses.
pub fn profile_buildings_summary_payload(
    profile_id: Option<&str>,
) -> Result<String, serde_json::Error> {
    let id = resolve_profile_id(profile_id);
    let summary = building_combat_summary_for_profile(&id);
    serde_json::to_string(&summary)
}

/// GET /api/profile/research-summary — synced research levels and research-derived combat bonuses.
/// Optional query: `ship_id`, `hostile_id` for scenario-effective flat totals.
pub fn profile_research_summary_payload(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    ship_id: Option<&str>,
    hostile_id: Option<&str>,
) -> Result<String, serde_json::Error> {
    let id = resolve_profile_id(profile_id);
    let (ship_faction, defender_faction, defender_ship_class) = match (ship_id, hostile_id) {
        (Some(ship), Some(hostile)) => {
            let ship_rec = registry.resolve_ship(ship);
            let hostile_rec = registry.resolve_hostile(hostile);
            match (ship_rec, hostile_rec) {
                (Some(s), Some(h)) => (
                    s.faction.clone(),
                    Some(h.opponent_faction_tag()),
                    Some(h.ship_class.clone()),
                ),
                _ => (None, None, None),
            }
        }
        _ => (None, None, None),
    };
    let summary = research_combat_summary_for_profile_with_scenario(
        &id,
        registry.research_catalog(),
        ship_id,
        hostile_id,
        ship_faction,
        defender_faction,
        defender_ship_class.as_deref(),
    );
    serde_json::to_string(&summary)
}

pub fn profiles_list_payload() -> Result<String, serde_json::Error> {
    let index = load_profile_index();
    serde_json::to_string(&serde_json::json!({
        "profiles": index.profiles,
        "default_id": index.default_id
    }))
}

#[derive(Debug)]
pub enum ProfileApiError {
    Parse(serde_json::Error),
    Create(String),
}

impl fmt::Display for ProfileApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Create(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProfileApiError {}

pub fn profiles_create_payload(body: &str) -> Result<String, ProfileApiError> {
    #[derive(Deserialize)]
    struct In {
        id: Option<String>,
        name: String,
    }
    let in_: In = serde_json::from_str(body).map_err(ProfileApiError::Parse)?;
    let mut index = load_profile_index();
    let entry = create_profile(&mut index, in_.id.as_deref(), &in_.name)
        .map_err(ProfileApiError::Create)?;
    serde_json::to_string(&entry).map_err(ProfileApiError::Parse)
}

pub fn profiles_delete_payload(id: &str) -> Result<(), String> {
    let mut index = load_profile_index();
    delete_profile(&mut index, id)
}

fn write_temp_import_file(body: &[u8], ext: &str) -> Result<std::path::PathBuf, std::io::Error> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("kobayashi_import_{}_{}", std::process::id(), ext));
    let mut f = fs::File::create(&path)?;
    f.write_all(body)?;
    f.sync_all()?;
    Ok(path)
}

pub fn officers_import_payload(
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, ImportError> {
    let body = body.trim();
    let id = resolve_profile_id(profile_id);
    let output_path = profile_path(&id, ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();
    let report = if body.starts_with('{') || body.starts_with('[') {
        let p = write_temp_import_file(body.as_bytes(), "json").map_err(ImportError::Io)?;
        let out = import_spocks_export_to(p.to_str().unwrap(), &output_path)?;
        let _ = fs::remove_file(&p);
        out
    } else {
        let p = write_temp_import_file(body.as_bytes(), "txt").map_err(ImportError::Io)?;
        let out = import_roster_csv_to(p.to_str().unwrap(), &output_path)?;
        let _ = fs::remove_file(&p);
        out
    };
    serde_json::to_string(&report).map_err(ImportError::Serialize)
}

#[derive(Debug)]
pub enum ImportError {
    Io(std::io::Error),
    Import(crate::data::import::ImportError),
    Serialize(serde_json::Error),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Import(e) => write!(f, "{e}"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<crate::data::import::ImportError> for ImportError {
    fn from(e: crate::data::import::ImportError) -> Self {
        Self::Import(e)
    }
}

#[derive(Debug)]
pub enum OfficerResolveError {
    NotFound,
    Io(std::io::Error),
    Serialize(serde_json::Error),
}

impl fmt::Display for OfficerResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "Officer not found"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OfficerResolveError {}

pub fn officer_resolved_payload(
    registry: &DataRegistry,
    officer_id: &str,
) -> Result<String, OfficerResolveError> {
    // Get LCARS officers from registry
    let lcars_officers = registry
        .lcars_officers()
        .ok_or(OfficerResolveError::NotFound)?;

    // Try to find the officer by id or name (case-insensitive)
    let officer = lcars_officers
        .iter()
        .find(|o| o.id == officer_id)
        .or_else(|| {
            let lower = officer_id.to_lowercase();
            lcars_officers
                .iter()
                .find(|o| o.name.to_lowercase() == lower)
        })
        .ok_or(OfficerResolveError::NotFound)?;

    // Build the LCARS officer map
    let by_id = crate::lcars::index_lcars_officers_by_id(lcars_officers.to_vec());

    // Resolve the officer
    let opts = crate::lcars::ResolveOptions::default();
    let buff_set = crate::lcars::resolve_crew_to_buff_set(
        &officer.id,
        std::slice::from_ref(&officer.id),
        std::slice::from_ref(&officer.id),
        &by_id,
        &opts,
    );

    // Create a response struct
    #[derive(Serialize)]
    struct ResolvedOfficer {
        id: String,
        name: String,
        static_buffs: std::collections::HashMap<String, f64>,
        crew_config: String, // Debug format since CrewConfiguration doesn't impl Serialize
        proc_chance: f64,
        proc_multiplier: f64,
    }

    let response = ResolvedOfficer {
        id: officer.id.clone(),
        name: officer.name.clone(),
        static_buffs: buff_set.static_buffs,
        crew_config: format!("{:#?}", buff_set.crew),
        proc_chance: buff_set.proc_chance,
        proc_multiplier: buff_set.proc_multiplier,
    };

    serde_json::to_string(&response).map_err(OfficerResolveError::Serialize)
}

#[derive(Debug)]
pub enum CombatEffectSpecDebugError {
    Disabled,
    /// [`DataRegistry::lcars_officers`] is empty because `KOBAYASHI_OFFICER_SOURCE` was not `lcars` at startup.
    LcarsOfficersNotLoaded,
    NotFound,
    Serialize(serde_json::Error),
}

impl fmt::Display for CombatEffectSpecDebugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(
                f,
                "CombatEffectSpec HTTP debug disabled (set KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG=1)"
            ),
            Self::LcarsOfficersNotLoaded => {
                write!(f, "LCARS officers not loaded (officer data failed to load)")
            }
            Self::NotFound => write!(f, "Officer not found"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CombatEffectSpecDebugError {}

/// JSON payload for `GET /api/debug/combat-effect-spec/officers/:id` when
/// [`crate::data::combat_effect_spec::combat_effect_spec_debug_http_enabled`] is true.
/// Each LCARS effect row includes an optional [`crate::data::combat_effect_spec::CombatEffectSpec`] when
/// [`crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec`] maps it.
pub fn combat_effect_spec_debug_officer_payload(
    registry: &DataRegistry,
    officer_id: &str,
) -> Result<String, CombatEffectSpecDebugError> {
    use crate::data::combat_effect_spec::CombatEffectSpec;
    use crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec;

    if !crate::data::combat_effect_spec::combat_effect_spec_debug_http_enabled() {
        return Err(CombatEffectSpecDebugError::Disabled);
    }

    let lcars_officers = registry
        .lcars_officers()
        .ok_or(CombatEffectSpecDebugError::LcarsOfficersNotLoaded)?;

    let officer = lcars_officers
        .iter()
        .find(|o| o.id == officer_id)
        .or_else(|| {
            let lower = officer_id.to_lowercase();
            lcars_officers
                .iter()
                .find(|o| o.name.to_lowercase() == lower)
        })
        .ok_or(CombatEffectSpecDebugError::NotFound)?;

    #[derive(Serialize)]
    struct EffectRow {
        index: usize,
        #[serde(rename = "type")]
        effect_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        spec: Option<CombatEffectSpec>,
    }

    #[derive(Serialize)]
    struct AbilityBlock {
        slot: &'static str,
        name: String,
        effects: Vec<EffectRow>,
    }

    #[derive(Serialize)]
    struct Payload {
        officer_id: String,
        officer_name: String,
        combat_effect_spec_enabled: bool,
        abilities: Vec<AbilityBlock>,
    }

    fn map_ability(
        officer_id: &str,
        slot: &'static str,
        ability: &crate::lcars::LcarsAbility,
    ) -> AbilityBlock {
        let mut effects = Vec::with_capacity(ability.effects.len());
        for (i, e) in ability.effects.iter().enumerate() {
            let stable_id = format!("{}:{}:{}", officer_id, ability.name, i);
            let spec = lcars_effect_to_combat_effect_spec(
                e,
                &stable_id,
                officer_id,
                &ability.name,
                None,
                None,
            );
            effects.push(EffectRow {
                index: i,
                effect_type: e.effect_type.clone(),
                spec,
            });
        }
        AbilityBlock {
            slot,
            name: ability.name.clone(),
            effects,
        }
    }

    let mut abilities = Vec::new();
    if let Some(ref a) = officer.captain_ability {
        abilities.push(map_ability(&officer.id, "captain", a));
    }
    if let Some(ref a) = officer.bridge_ability {
        abilities.push(map_ability(&officer.id, "bridge", a));
    }
    if let Some(ref a) = officer.below_decks_ability {
        abilities.push(map_ability(&officer.id, "below_decks", a));
    }

    let payload = Payload {
        officer_id: officer.id.clone(),
        officer_name: officer.name.clone(),
        combat_effect_spec_enabled: crate::data::combat_effect_spec::combat_effect_spec_enabled(),
        abilities,
    };

    serde_json::to_string(&payload).map_err(CombatEffectSpecDebugError::Serialize)
}

fn presets_dir_for_profile(profile_id: &str) -> std::path::PathBuf {
    profile_path(profile_id, PRESETS_SUBDIR)
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PresetCrew {
    pub captain: Option<String>,
    pub bridge: Option<Vec<String>>,
    pub below_deck: Option<Vec<String>>,
}

/// Snapshot written when saving a preset.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PresetProvenance {
    /// RFC3339 timestamp when the preset was written.
    pub saved_at: String,
    pub kobayashi_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostile_data_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_data_version: Option<String>,
    #[serde(default = "default_preset_source")]
    pub source: String,
}

fn default_preset_source() -> String {
    "kobayashi_api".to_string()
}

fn snapshot_registry_data_versions(registry: &DataRegistry) -> (Option<String>, Option<String>) {
    let hostile = registry
        .hostile_index
        .as_ref()
        .and_then(|i| i.data_version.clone());
    let ship = registry
        .ship_index
        .as_ref()
        .and_then(|i| i.data_version.clone());
    (hostile, ship)
}

fn build_preset_provenance(registry: &DataRegistry) -> PresetProvenance {
    let (hostile_data_version, ship_data_version) = snapshot_registry_data_versions(registry);
    PresetProvenance {
        saved_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        kobayashi_version: env!("CARGO_PKG_VERSION").to_string(),
        hostile_data_version,
        ship_data_version,
        source: default_preset_source(),
    }
}

pub const PRESET_SCHEMA_VERSION: u32 = 2;

fn default_preset_schema_version() -> u32 {
    PRESET_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Preset {
    #[serde(default = "default_preset_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub ship: String,
    pub scenario: String,
    pub crew: PresetCrew,
    pub provenance: PresetProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PresetSummary {
    pub id: String,
    pub name: String,
    pub ship: String,
    pub scenario: String,
    pub schema_version: u32,
}

fn preset_id_from_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let s = s.trim_matches('_');
    if s.is_empty() {
        format!("preset_{}", std::process::id())
    } else {
        s.to_string()
    }
}

fn ensure_presets_dir(profile_id: &str) -> std::io::Result<()> {
    fs::create_dir_all(presets_dir_for_profile(profile_id))
}

pub fn presets_list_payload(profile_id: Option<&str>) -> Result<String, serde_json::Error> {
    let id = resolve_profile_id(profile_id);
    ensure_presets_dir(&id).map_err(serde_json::Error::io)?;
    let dir_path = presets_dir_for_profile(&id);
    let mut list = Vec::new();
    let dir = fs::read_dir(&dir_path).map_err(serde_json::Error::io)?;
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(p) = serde_json::from_str::<Preset>(&raw) {
                    list.push(PresetSummary {
                        id: p.id,
                        name: p.name,
                        ship: p.ship,
                        scenario: p.scenario,
                        schema_version: p.schema_version,
                    });
                }
            }
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    serde_json::to_string(&serde_json::json!({ "presets": list }))
}

pub fn preset_get_payload(id: &str, profile_id: Option<&str>) -> Result<String, PresetError> {
    let pid = resolve_profile_id(profile_id);
    let path = presets_dir_for_profile(&pid).join(sanitize_preset_id(id));
    if !path.exists() {
        return Err(PresetError::NotFound);
    }
    let raw = fs::read_to_string(&path).map_err(PresetError::Io)?;
    let preset: Preset = serde_json::from_str(&raw).map_err(PresetError::Serialize)?;
    serde_json::to_string(&preset).map_err(PresetError::Serialize)
}

fn sanitize_preset_id(id: &str) -> String {
    let s: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if s.is_empty() {
        "unknown".to_string()
    } else {
        format!("{}.json", s)
    }
}

#[derive(Debug)]
pub enum PresetError {
    NotFound,
    Io(std::io::Error),
    Serialize(serde_json::Error),
}

impl fmt::Display for PresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "Preset not found"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PresetError {}

pub fn preset_post_payload(
    body: &str,
    profile_id: Option<&str>,
    registry: &DataRegistry,
) -> Result<String, PresetError> {
    #[derive(Debug, Deserialize)]
    struct In {
        name: Option<String>,
        ship: String,
        scenario: String,
        crew: PresetCrew,
    }
    let in_: In = serde_json::from_str(body).map_err(PresetError::Serialize)?;
    let name = in_.name.unwrap_or_else(|| "Unnamed".to_string());
    let id = preset_id_from_name(&name);
    let pid = resolve_profile_id(profile_id);
    let path = presets_dir_for_profile(&pid).join(sanitize_preset_id(&id));
    ensure_presets_dir(&pid).map_err(PresetError::Io)?;
    let preset = Preset {
        schema_version: PRESET_SCHEMA_VERSION,
        id: id.clone(),
        name: name.clone(),
        ship: in_.ship,
        scenario: in_.scenario,
        crew: in_.crew,
        provenance: build_preset_provenance(registry),
    };
    let raw = serde_json::to_string(&preset).map_err(PresetError::Serialize)?;
    fs::write(&path, raw).map_err(PresetError::Io)?;
    serde_json::to_string(&preset).map_err(PresetError::Serialize)
}

pub fn data_version_payload(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    let hostile_index = registry.hostile_index();
    let ship_index = registry.ship_index();
    let mechanics = vec![
        MechanicStatus {
            name: "Mitigation".to_string(),
            status: "implemented".to_string(),
        },
        MechanicStatus {
            name: "Piercing".to_string(),
            status: "implemented".to_string(),
        },
        MechanicStatus {
            name: "Armor".to_string(),
            status: "implemented".to_string(),
        },
        MechanicStatus {
            name: "Critical".to_string(),
            status: "implemented".to_string(),
        },
        MechanicStatus {
            name: "Burn".to_string(),
            status: "implemented".to_string(),
        },
        MechanicStatus {
            name: "Regeneration".to_string(),
            status: "partial".to_string(),
        },
        MechanicStatus {
            name: "Isolytic".to_string(),
            status: "implemented".to_string(),
        },
        MechanicStatus {
            name: "Apex".to_string(),
            status: "partial".to_string(),
        },
    ];
    let response = DataVersionResponse {
        officer_version: Some("canonical".to_string()),
        hostile_version: hostile_index.and_then(|i| i.data_version.clone()),
        ship_version: ship_index.and_then(|i| i.data_version.clone()),
        mechanics,
    };
    serde_json::to_string(&response)
}

pub fn heuristics_list_payload() -> Result<String, serde_json::Error> {
    let seeds = list_heuristics_seeds(DEFAULT_HEURISTICS_DIR);
    serde_json::to_string(&serde_json::json!({ "seeds": seeds }))
}

/// Per-officer ability ids by seat, so the client can map a crew slot to the relevant ability.
/// The `officer` (bridge) seat may hold up to two abilities; the client takes the best verdict.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OfficerSeatAbilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captain: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub officer: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub below_decks: Option<String>,
}

/// GET /api/eligibility: the officer eligibility matrix for the frontend (live crew badges).
/// Returns ability-keyed per-scenario verdicts plus an officer→seat→ability_id index. The `matrix`
/// is empty when the eligibility file is not loaded (badges then simply don't render).
pub fn eligibility_payload(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    use crate::data::officer_eligibility::EligibilityScenario;
    let scenarios: Vec<&'static str> = EligibilityScenario::ALL
        .iter()
        .filter(|s| !matches!(s, EligibilityScenario::Loot | EligibilityScenario::Utility))
        .map(|s| s.as_key())
        .collect();

    let mut officer_abilities: HashMap<String, OfficerSeatAbilities> = HashMap::new();
    for officer in registry.officers() {
        let mut seats = OfficerSeatAbilities::default();
        for ability in &officer.abilities {
            let Some(id) = ability.ability_id.clone() else {
                continue;
            };
            match ability.slot.as_str() {
                "captain" => seats.captain = Some(id),
                "officer" => seats.officer.push(id),
                "below_decks" => seats.below_decks = Some(id),
                _ => {}
            }
        }
        if seats.captain.is_some() || !seats.officer.is_empty() || seats.below_decks.is_some() {
            officer_abilities.insert(officer.id.clone(), seats);
        }
    }

    let matrix_abilities = registry
        .eligibility_matrix()
        .map(|m| &m.abilities)
        .cloned()
        .unwrap_or_default();

    serde_json::to_string(&serde_json::json!({
        "scenarios": scenarios,
        "matrix": matrix_abilities,
        "officer_abilities": officer_abilities,
    }))
}

/// GET /api/forbidden-tech: returns the forbidden/chaos tech catalog for UI dropdown.
pub fn forbidden_tech_catalog_payload(
    registry: &DataRegistry,
) -> Result<String, serde_json::Error> {
    let body = match registry.forbidden_chaos_catalog() {
        Some(c) => serde_json::to_string(&serde_json::json!({ "items": c.items }))?,
        None => serde_json::to_string(&serde_json::json!({ "items": [] }))?,
    };
    Ok(body)
}

/// Rough seconds per (candidate × sim) on a typical multi-core machine; used for time estimates.
const ESTIMATE_SEC_PER_CANDIDATE_SIM: f64 = 4e-9;

pub fn optimize_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, OptimizePayloadError> {
    let request = parse_optimize_request_body(body)?;
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    validate_request(&request, sims)?;
    let response = execution::run_optimize(registry, &request, profile_id)?;
    serde_json::to_string(&response).map_err(OptimizePayloadError::Parse)
}

pub fn optimize_start_payload(
    cpu_permit: tokio::sync::OwnedSemaphorePermit,
    registry: Arc<DataRegistry>,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, OptimizePayloadError> {
    let request = parse_optimize_request_body(body)?;
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    validate_request(&request, sims)?;
    let start_response = execution::start_optimize_job(registry, request, profile_id, cpu_permit)?;
    serde_json::to_string(&start_response).map_err(OptimizePayloadError::Parse)
}

/// Request cancellation of a running optimize job. Idempotent if already done/cancelled.
pub fn optimize_cancel_payload(job_id: &str) -> Result<String, OptimizeStatusError> {
    if let Ok(status) = execution::get_job_status(job_id) {
        if status.status == "done" || status.status == "error" {
            let body = serde_json::json!({ "status": "ok", "message": "Job already finished" });
            return serde_json::to_string(&body).map_err(OptimizeStatusError::Serialize);
        }
    }
    execution::cancel_job(job_id)?;
    let body = serde_json::json!({ "status": "ok", "message": "Cancelled" });
    serde_json::to_string(&body).map_err(OptimizeStatusError::Serialize)
}

/// Return current status (and result when done) for an optimize job.
pub fn optimize_status_payload(job_id: &str) -> Result<String, OptimizeStatusError> {
    let response = execution::get_job_status(job_id)?;
    serde_json::to_string(&response).map_err(OptimizeStatusError::Serialize)
}

pub fn optimize_estimate_payload(
    registry: &DataRegistry,
    path: &str,
    profile_id: Option<&str>,
) -> Result<String, OptimizePayloadError> {
    let query = path.split('?').nth(1).unwrap_or("");
    let (
        ship,
        hostile,
        sims,
        max_candidates,
        below_decks_pool_mode,
        ship_tier,
        ship_level,
        bd_explicit,
    ) = requests::parse_optimize_estimate_query(query)?;
    let sims = sims.clamp(1, MAX_SIMS);
    if ship.trim().is_empty() || hostile.trim().is_empty() {
        return Err(OptimizePayloadError::Validation(ValidationErrorResponse {
            status: "error",
            message: "Validation failed",
            errors: vec![ValidationIssue {
                field: "ship",
                messages: vec!["ship and hostile are required for estimate".to_string()],
            }],
        }));
    }
    let below_decks_slots =
        resolve_below_decks_slots_for_ship(ship.trim(), ship_tier, ship_level, bd_explicit);
    let estimated_candidates = match max_candidates {
        Some(cap) if cap <= MAX_CANDIDATES => {
            let generator = CrewGenerator::with_strategy(CandidateStrategy {
                max_candidates: Some(cap as usize),
                below_decks_pool_mode,
                below_decks_slots,
                ..CandidateStrategy::default()
            });
            generator
                .generate_candidates_from_registry(registry, &ship, &hostile, 0, profile_id)
                .len()
        }
        Some(_) => {
            return Err(OptimizePayloadError::Validation(ValidationErrorResponse {
                status: "error",
                message: "Validation failed",
                errors: vec![ValidationIssue {
                    field: "max_candidates",
                    messages: vec![format!("must be at most {MAX_CANDIDATES}")],
                }],
            }));
        }
        None => {
            let generator = CrewGenerator::with_strategy(CandidateStrategy {
                below_decks_pool_mode,
                below_decks_slots,
                ..CandidateStrategy::default()
            });
            generator.count_candidates_from_registry(registry, &ship, &hostile, 0, profile_id)
        }
    };
    let estimated_seconds =
        (estimated_candidates as f64) * (sims as f64) * ESTIMATE_SEC_PER_CANDIDATE_SIM;
    let estimated_seconds = estimated_seconds.clamp(0.1, 3600.0); // clamp to 0.1s–1h for display
    let payload = serde_json::json!({
        "estimated_candidates": estimated_candidates,
        "sims_per_crew": sims,
        "estimated_seconds": (estimated_seconds * 10.0).round() / 10.0,
    });
    serde_json::to_string(&payload).map_err(OptimizePayloadError::Parse)
}

#[cfg(test)]
mod roster_tier_merge_tests {
    use super::merge_roster_tier_level;

    #[test]
    fn merge_prefers_higher_tier() {
        assert_eq!(merge_roster_tier_level((3, 45), (6, 1)), (6, 1));
        assert_eq!(merge_roster_tier_level((6, 1), (3, 45)), (6, 1));
    }

    #[test]
    fn merge_same_tier_prefers_higher_level() {
        assert_eq!(merge_roster_tier_level((5, 10), (5, 30)), (5, 30));
        assert_eq!(merge_roster_tier_level((5, 30), (5, 10)), (5, 30));
    }
}
