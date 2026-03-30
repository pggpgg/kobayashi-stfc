mod execution;
mod requests;

pub use execution::{
    cancel_job, get_job_status, run_optimize, start_optimize_job, CrewRecommendation,
    OptimizeJobState, OptimizeResponse, OptimizeStartResponse, OptimizeStatusError,
    OptimizeStatusResponse, ScenarioSummary,
};
pub use requests::{
    validate_request, OptimizePayloadError, OptimizeRequest, ReplaySeedRequest,
    ValidationErrorResponse, ValidationIssue, DEFAULT_SIMS, MAX_CANDIDATES, MAX_SIMS,
};

use crate::data::building_summary::building_combat_summary_for_profile;
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::{list_heuristics_seeds, DEFAULT_HEURISTICS_DIR};
use crate::data::hostile_loca::resolve_hostile_display_name;
use crate::data::import::load_imported_ships;
use crate::data::import::{
    import_roster_csv_to, import_spocks_export_to, load_imported_roster_ids_unlocked_only,
};
use crate::data::loader::ship_tiers_levels;
use crate::data::profile_index::{
    create_profile, delete_profile, effective_profile_id, load_profile_index, profile_path,
    PRESETS_SUBDIR, PROFILE_JSON, ROSTER_IMPORTED, SHIPS_IMPORTED,
};
use crate::data::research_summary::research_combat_summary_for_profile;
use crate::data::support_buffs;
use crate::optimizer::crew_generator::{
    resolve_below_decks_slots, CandidateStrategy, CrewCandidate, CrewGenerator, BRIDGE_SLOTS,
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
    let officer_count = registry.officers().len();

    serde_json::to_string_pretty(&serde_json::json!({
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

#[derive(Debug, Clone, Serialize)]
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
    serde_json::to_string_pretty(&serde_json::json!({ "officers": list }))
}

#[derive(Debug, Clone, Serialize)]
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

/// Load hull_id -> ship_id mapping. Returns empty map if file missing or invalid.
fn load_hull_id_registry() -> HashMap<i64, String> {
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
) -> Result<String, serde_json::Error> {
    let idx = match registry.ship_index() {
        Some(i) => i,
        None => {
            return serde_json::to_string_pretty(&serde_json::json!({ "ships": [] }));
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
        let hull_registry = load_hull_id_registry();

        let mut roster_tier_level = std::collections::HashMap::new();
        if let Some(ships) = &imported {
            for entry in ships {
                if let Some(sid) = hull_registry.get(&entry.hull_id) {
                    roster_tier_level
                        .entry(sid.clone())
                        .or_insert_with(|| (entry.tier as u32, entry.level as u32));
                }
            }
        }

        if hull_registry.is_empty() {
            (None, roster_tier_level)
        } else if let Some(ships) = imported {
            let mut ids = std::collections::HashSet::new();
            for entry in &ships {
                if let Some(sid) = hull_registry.get(&entry.hull_id) {
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

    serde_json::to_string_pretty(&serde_json::json!({ "ships": list }))
}

/// Default tier/level options when extended ship data is missing (e.g. no data/ships_extended).
const DEFAULT_TIERS: &[u32] = &[1];
const DEFAULT_LEVELS: &[u32] = &[1, 10, 20, 30, 40, 50, 60];

pub fn ship_tiers_levels_payload(ship_id: &str) -> Result<String, serde_json::Error> {
    let (mut tiers, mut levels) = ship_tiers_levels(ship_id)
        .unwrap_or_else(|| (DEFAULT_TIERS.to_vec(), DEFAULT_LEVELS.to_vec()));
    if tiers.is_empty() {
        tiers = DEFAULT_TIERS.to_vec();
    }
    if levels.is_empty() {
        levels = DEFAULT_LEVELS.to_vec();
    }
    serde_json::to_string_pretty(&serde_json::json!({ "tiers": tiers, "levels": levels }))
}

#[derive(Debug, Clone, Serialize)]
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
    serde_json::to_string_pretty(&serde_json::json!({ "hostiles": list }))
}

#[derive(Debug, Clone, Serialize)]
pub struct MechanicStatus {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataVersionResponse {
    pub officer_version: Option<String>,
    pub hostile_version: Option<String>,
    pub ship_version: Option<String>,
    pub mechanics: Vec<MechanicStatus>,
}

#[derive(Debug, Clone, Deserialize)]
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
    /// Below-decks slot count for padding crew (2–5). Omitted = tier default.
    pub below_decks_slots: Option<u32>,
    /// Optional alliance/ship support buff ids (see `data/support_buffs.json`).
    #[serde(default)]
    pub support_buffs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SimulateCrew {
    pub captain: Option<String>,
    /// Bridge officer IDs; null entries mean "no officer" in that slot.
    pub bridge: Option<Vec<Option<String>>>,
    /// Below-deck officer IDs; null entries mean "no officer" in that slot.
    pub below_deck: Option<Vec<Option<String>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulateResponse {
    pub status: &'static str,
    pub stats: SimulateStats,
    pub seed: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulateStats {
    pub win_rate: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub avg_hull_remaining: f64,
    pub n: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate_95_ci: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize)]
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
    let below_decks_slots = resolve_below_decks_slots(req.ship_tier, req.below_decks_slots);

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
    let candidates = vec![candidate];
    let (results, using_placeholder_combatants) = run_monte_carlo_with_registry(
        registry,
        &req.ship,
        &req.hostile,
        req.ship_tier,
        req.ship_level,
        &candidates,
        num_sims as usize,
        seed,
        profile_id,
        req.support_buffs.as_deref(),
    );
    let result = results.into_iter().next().unwrap_or(SimulationResult {
        candidate: CrewCandidate {
            captain,
            bridge,
            below_decks,
        },
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
        }
    }
    if using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats. Results do not reflect real ship/hostile values."
                .to_string(),
        );
    }

    let response = SimulateResponse {
        status: "ok",
        stats: SimulateStats {
            win_rate: result.win_rate,
            stall_rate: result.stall_rate,
            loss_rate: result.loss_rate,
            avg_hull_remaining: result.avg_hull_remaining,
            n: num_sims,
            win_rate_95_ci: Some(ci),
        },
        seed,
        warnings,
    };
    serde_json::to_string_pretty(&response).map_err(SimulateError::Parse)
}

pub fn compare_crews_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, CompareCrewsError> {
    let req: CompareCrewsRequest = serde_json::from_str(body).map_err(CompareCrewsError::Parse)?;
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
    let below_decks_slots = resolve_below_decks_slots(req.ship_tier, req.below_decks_slots);

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

    let outcome = compare_crews_monte_carlo_with_registry(
        registry,
        &req.ship,
        &req.hostile,
        req.ship_tier,
        req.ship_level,
        &candidates,
        num_sims as usize,
        seed,
        profile_id,
        proc_sample,
        req.support_buffs.as_deref(),
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
        }
    }
    if outcome.using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats."
                .to_string(),
        );
    }

    let response = CompareCrewsResponse {
        status: "ok",
        seed,
        crews: outcome.crews,
        using_placeholder_combatants: outcome.using_placeholder_combatants,
        warnings,
    };
    serde_json::to_string_pretty(&response).map_err(CompareCrewsError::Parse)
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

    let below_decks_slots = resolve_below_decks_slots(req.ship_tier, None);
    let candidate = crew_candidate_from_officer_fields(
        req.crew.captain.as_deref(),
        req.crew.bridge.as_deref(),
        req.crew.below_deck.as_deref(),
        &officers,
        below_decks_slots,
    )
    .map_err(ReplaySeedError::Validation)?;

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
    );

    let mut warnings = Vec::new();
    if replay.using_placeholder_combatants {
        warnings.push(
            "Ship or hostile did not resolve from loaded data; combat used deterministic placeholder stats."
                .to_string(),
        );
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
            "attacker_hull_remaining": replay.attacker_hull_remaining,
            "defender_hull_remaining": replay.defender_hull_remaining,
            "defender_shield_remaining": replay.defender_shield_remaining,
        },
        "trace": {
            "event_count": replay.trace_event_count,
            "events_returned": replay.trace_events_returned,
            "truncated": replay.trace_truncated,
            "events": replay.trace_events,
        },
        "warnings": warnings,
    });

    serde_json::to_string_pretty(&response_json).map_err(ReplaySeedError::Parse)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    #[serde(default)]
    pub bonuses: std::collections::HashMap<String, f64>,
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
        serde_json::from_str(&raw).unwrap_or(PlayerProfile {
            bonuses: std::collections::HashMap::new(),
        })
    } else {
        PlayerProfile {
            bonuses: std::collections::HashMap::new(),
        }
    };
    serde_json::to_string_pretty(&profile)
}

pub fn profile_put_payload(
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, serde_json::Error> {
    let _: PlayerProfile = serde_json::from_str(body)?;
    let id = resolve_profile_id(profile_id);
    let path = profile_path(&id, PROFILE_JSON);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, body).map_err(serde_json::Error::io)?;
    serde_json::to_string_pretty(&serde_json::json!({ "status": "ok" }))
}

/// GET /api/profile/buildings-summary — synced module levels and building-derived combat bonuses.
pub fn profile_buildings_summary_payload(
    profile_id: Option<&str>,
) -> Result<String, serde_json::Error> {
    let id = resolve_profile_id(profile_id);
    let summary = building_combat_summary_for_profile(&id);
    serde_json::to_string_pretty(&summary)
}

/// GET /api/profile/research-summary — synced research levels and research-derived combat bonuses.
pub fn profile_research_summary_payload(
    registry: &DataRegistry,
    profile_id: Option<&str>,
) -> Result<String, serde_json::Error> {
    let id = resolve_profile_id(profile_id);
    let summary = research_combat_summary_for_profile(&id, registry.research_catalog());
    serde_json::to_string_pretty(&summary)
}

pub fn profiles_list_payload() -> Result<String, serde_json::Error> {
    let index = load_profile_index();
    serde_json::to_string_pretty(&serde_json::json!({
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
    serde_json::to_string_pretty(&entry).map_err(ProfileApiError::Parse)
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
    serde_json::to_string_pretty(&report).map_err(ImportError::Serialize)
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

    serde_json::to_string_pretty(&response).map_err(OfficerResolveError::Serialize)
}

fn presets_dir_for_profile(profile_id: &str) -> std::path::PathBuf {
    profile_path(profile_id, PRESETS_SUBDIR)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetCrew {
    pub captain: Option<String>,
    pub bridge: Option<Vec<String>>,
    pub below_deck: Option<Vec<String>>,
}

/// Snapshot written when saving a preset (task 14 provenance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetProvenance {
    /// RFC3339 timestamp when the preset was written (or file mtime for legacy reads).
    pub saved_at: String,
    pub kobayashi_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostile_data_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_data_version: Option<String>,
    #[serde(default = "default_preset_source")]
    pub source: String,
    /// True when provenance was synthesized for a legacy on-disk file (no embedded snapshot).
    #[serde(default)]
    pub inferred: bool,
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

fn build_preset_provenance(registry: &DataRegistry, inferred: bool) -> PresetProvenance {
    let (hostile_data_version, ship_data_version) = snapshot_registry_data_versions(registry);
    PresetProvenance {
        saved_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        kobayashi_version: env!("CARGO_PKG_VERSION").to_string(),
        hostile_data_version,
        ship_data_version,
        source: default_preset_source(),
        inferred,
    }
}

/// Preset JSON schema: v1 = original fields only; v2 adds `schema_version` + `provenance`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    #[serde(default)]
    pub schema_version: Option<u32>,
    pub id: String,
    pub name: String,
    pub ship: String,
    pub scenario: String,
    pub crew: PresetCrew,
    #[serde(default)]
    pub provenance: Option<PresetProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetSummary {
    pub id: String,
    pub name: String,
    pub ship: String,
    pub scenario: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
}

fn enrich_preset_on_read(path: &std::path::Path, mut preset: Preset) -> Preset {
    let saved_from_mtime = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        })
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "unknown".to_string());

    if preset.provenance.is_none() {
        preset.provenance = Some(if preset.schema_version == Some(2) {
            PresetProvenance {
                saved_at: saved_from_mtime.clone(),
                kobayashi_version: env!("CARGO_PKG_VERSION").to_string(),
                hostile_data_version: None,
                ship_data_version: None,
                source: "repaired_v2_missing_provenance".to_string(),
                inferred: true,
            }
        } else {
            PresetProvenance {
                saved_at: saved_from_mtime,
                kobayashi_version: "unknown".to_string(),
                hostile_data_version: None,
                ship_data_version: None,
                source: "legacy_file_no_provenance".to_string(),
                inferred: true,
            }
        });
        if preset.schema_version.is_none() {
            preset.schema_version = Some(1);
        }
        return preset;
    }

    if preset.schema_version.is_none() {
        preset.schema_version = Some(1);
        if let Some(ref mut pr) = preset.provenance {
            pr.inferred = true;
        }
    }

    preset
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
    serde_json::to_string_pretty(&serde_json::json!({ "presets": list }))
}

pub fn preset_get_payload(id: &str, profile_id: Option<&str>) -> Result<String, PresetError> {
    let pid = resolve_profile_id(profile_id);
    let path = presets_dir_for_profile(&pid).join(sanitize_preset_id(id));
    if !path.exists() {
        return Err(PresetError::NotFound);
    }
    let raw = fs::read_to_string(&path).map_err(PresetError::Io)?;
    let preset: Preset = serde_json::from_str(&raw).map_err(PresetError::Serialize)?;
    let enriched = enrich_preset_on_read(&path, preset);
    serde_json::to_string_pretty(&enriched).map_err(PresetError::Serialize)
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
        schema_version: Some(2),
        id: id.clone(),
        name: name.clone(),
        ship: in_.ship,
        scenario: in_.scenario,
        crew: in_.crew,
        provenance: Some(build_preset_provenance(registry, false)),
    };
    let raw = serde_json::to_string_pretty(&preset).map_err(PresetError::Serialize)?;
    fs::write(&path, raw).map_err(PresetError::Io)?;
    serde_json::to_string_pretty(&preset).map_err(PresetError::Serialize)
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
            status: "planned".to_string(),
        },
        MechanicStatus {
            name: "Apex".to_string(),
            status: "planned".to_string(),
        },
    ];
    let response = DataVersionResponse {
        officer_version: Some("canonical".to_string()),
        hostile_version: hostile_index.and_then(|i| i.data_version.clone()),
        ship_version: ship_index.and_then(|i| i.data_version.clone()),
        mechanics,
    };
    serde_json::to_string_pretty(&response)
}

pub fn heuristics_list_payload() -> Result<String, serde_json::Error> {
    let seeds = list_heuristics_seeds(DEFAULT_HEURISTICS_DIR);
    serde_json::to_string_pretty(&serde_json::json!({ "seeds": seeds }))
}

/// GET /api/forbidden-tech: returns the forbidden/chaos tech catalog for UI dropdown.
pub fn forbidden_tech_catalog_payload(
    registry: &DataRegistry,
) -> Result<String, serde_json::Error> {
    let body = match registry.forbidden_chaos_catalog() {
        Some(c) => serde_json::to_string_pretty(&serde_json::json!({ "items": c.items }))?,
        None => serde_json::to_string_pretty(&serde_json::json!({ "items": [] }))?,
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
    let request: OptimizeRequest =
        serde_json::from_str(body).map_err(OptimizePayloadError::Parse)?;
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    validate_request(&request, sims)?;
    let response = execution::run_optimize(registry, &request, profile_id)?;
    serde_json::to_string_pretty(&response).map_err(OptimizePayloadError::Parse)
}

pub fn optimize_start_payload(
    cpu_permit: tokio::sync::OwnedSemaphorePermit,
    registry: Arc<DataRegistry>,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, OptimizePayloadError> {
    let request: OptimizeRequest =
        serde_json::from_str(body).map_err(OptimizePayloadError::Parse)?;
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    validate_request(&request, sims)?;
    let start_response = execution::start_optimize_job(registry, request, profile_id, cpu_permit)?;
    serde_json::to_string_pretty(&start_response).map_err(OptimizePayloadError::Parse)
}

/// Request cancellation of a running optimize job. Idempotent if already done/cancelled.
pub fn optimize_cancel_payload(job_id: &str) -> Result<String, OptimizeStatusError> {
    if let Ok(status) = execution::get_job_status(job_id) {
        if status.status == "done" || status.status == "error" {
            let body = serde_json::json!({ "status": "ok", "message": "Job already finished" });
            return serde_json::to_string_pretty(&body).map_err(OptimizeStatusError::Serialize);
        }
    }
    execution::cancel_job(job_id)?;
    let body = serde_json::json!({ "status": "ok", "message": "Cancelled" });
    serde_json::to_string_pretty(&body).map_err(OptimizeStatusError::Serialize)
}

/// Return current status (and result when done) for an optimize job.
pub fn optimize_status_payload(job_id: &str) -> Result<String, OptimizeStatusError> {
    let response = execution::get_job_status(job_id)?;
    serde_json::to_string_pretty(&response).map_err(OptimizeStatusError::Serialize)
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
        prioritize_below_decks_ability,
        ship_tier,
        bd_explicit,
    ) = requests::parse_optimize_estimate_query(query);
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
    let below_decks_slots = resolve_below_decks_slots(ship_tier, bd_explicit);
    let estimated_candidates = match max_candidates {
        Some(cap) if cap <= MAX_CANDIDATES => {
            let generator = CrewGenerator::with_strategy(CandidateStrategy {
                max_candidates: Some(cap as usize),
                only_below_decks_with_ability: prioritize_below_decks_ability,
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
                only_below_decks_with_ability: prioritize_below_decks_ability,
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
    serde_json::to_string_pretty(&payload).map_err(OptimizePayloadError::Parse)
}

#[cfg(test)]
mod preset_schema_tests {
    use super::*;

    #[test]
    fn preset_deserializes_legacy_json_without_schema_or_provenance() {
        let j = r#"{"id":"x","name":"n","ship":"s","scenario":"h","crew":{}}"#;
        let p: Preset = serde_json::from_str(j).unwrap();
        assert_eq!(p.schema_version, None);
        assert!(p.provenance.is_none());
    }

    #[test]
    fn enrich_legacy_adds_inferred_provenance_and_schema_v1() {
        let p: Preset =
            serde_json::from_str(r#"{"id":"x","name":"n","ship":"s","scenario":"h","crew":{}}"#)
                .unwrap();
        let tmp = std::env::temp_dir().join(format!(
            "kobayashi_preset_legacy_{}.json",
            std::process::id()
        ));
        std::fs::write(&tmp, "{}").unwrap();
        let out = enrich_preset_on_read(&tmp, p);
        assert_eq!(out.schema_version, Some(1));
        let pr = out.provenance.expect("provenance");
        assert!(pr.inferred);
        assert_eq!(pr.source, "legacy_file_no_provenance");
        assert_eq!(pr.kobayashi_version, "unknown");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn enrich_v2_missing_provenance_repairs() {
        let p: Preset = serde_json::from_str(
            r#"{"schema_version":2,"id":"x","name":"n","ship":"s","scenario":"h","crew":{}}"#,
        )
        .unwrap();
        assert!(p.provenance.is_none());
        let tmp =
            std::env::temp_dir().join(format!("kobayashi_preset_v2_{}.json", std::process::id()));
        std::fs::write(&tmp, "{}").unwrap();
        let out = enrich_preset_on_read(&tmp, p);
        assert_eq!(out.schema_version, Some(2));
        let pr = out.provenance.expect("provenance");
        assert!(pr.inferred);
        assert_eq!(pr.source, "repaired_v2_missing_provenance");
        let _ = std::fs::remove_file(&tmp);
    }
}
