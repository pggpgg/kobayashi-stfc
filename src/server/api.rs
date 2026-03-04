use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::{
    expand_crews, list_heuristics_seeds, load_seed_file, BelowDecksStrategy,
    DEFAULT_HEURISTICS_DIR,
};
use crate::data::import::{
    import_roster_csv_to, import_spocks_export_to, load_imported_roster_ids_unlocked_only,
};
use crate::data::profile_index::{
    create_profile, delete_profile, effective_profile_id, load_profile_index,
    profile_path, PRESETS_SUBDIR, PROFILE_JSON, ROSTER_IMPORTED,
};
use crate::optimizer::crew_generator::{
    CandidateStrategy, CrewCandidate, CrewGenerator, BELOW_DECKS_SLOTS, BRIDGE_SLOTS,
};
use crate::optimizer::monte_carlo::{
    run_monte_carlo_parallel_with_registry, run_monte_carlo_with_registry, SimulationResult,
};
use crate::optimizer::ranking::rank_results;
use crate::optimizer::{
    optimize_scenario_with_progress_with_registry, optimize_scenario_with_registry,
    OptimizationScenario, OptimizerStrategy,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_SIMS: u32 = 5000;
const MAX_SIMS: u32 = 100_000;
const MAX_CANDIDATES: u32 = 2_000_000;

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizeRequest {
    pub ship: String,
    pub hostile: String,
    pub sims: Option<u32>,
    pub seed: Option<u64>,
    /// When None, all crew combinations are explored. When Some(n), generation stops after n candidates.
    pub max_candidates: Option<u32>,
    /// Optimizer strategy: "exhaustive" (default) or "genetic".
    pub strategy: Option<String>,
    /// When true, below-decks pool only includes officers that have a below-decks ability.
    pub prioritize_below_decks_ability: Option<bool>,
    /// Names of heuristics seed files to run first (stems only, e.g. ["heuristics-seed"]).
    pub heuristics_seeds: Option<Vec<String>>,
    /// When true, only heuristics crews are simulated; normal optimization is skipped.
    pub heuristics_only: Option<bool>,
    /// How to assign below-decks when a seed lists more officers than the ship has slots.
    /// "ordered" (default) or "exploration".
    pub below_decks_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrewRecommendation {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub win_rate: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub avg_hull_remaining: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeResponse {
    pub status: &'static str,
    pub engine: &'static str,
    pub scenario: ScenarioSummary,
    pub recommendations: Vec<CrewRecommendation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub notes: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
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

/// Parse query string for owned_only=1
fn parse_owned_only(path: &str) -> bool {
    let query = path.split('?').nth(1).unwrap_or("");
    query
        .split('&')
        .any(|p| p.trim().eq_ignore_ascii_case("owned_only=1") || p.trim().eq_ignore_ascii_case("owned_only=true"))
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
        profile_path(&id, ROSTER_IMPORTED).to_string_lossy().to_string()
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
        .filter(|o| owned_ids.as_ref().map_or(true, |ids| ids.contains(&o.id)))
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
}

pub fn ships_payload(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    let list: Vec<ShipListItem> = registry
        .ship_index()
        .map(|idx| {
            idx.ships
                .iter()
                .map(|e| ShipListItem {
                    id: e.id.clone(),
                    ship_name: e.ship_name.clone(),
                    ship_class: e.ship_class.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::to_string_pretty(&serde_json::json!({ "ships": list }))
}

#[derive(Debug, Clone, Serialize)]
pub struct HostileListItem {
    pub id: String,
    pub hostile_name: String,
    pub level: u32,
    pub ship_class: String,
}

pub fn hostiles_payload(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    let list: Vec<HostileListItem> = registry
        .hostile_index()
        .map(|idx| {
            idx.hostiles
                .iter()
                .map(|e| HostileListItem {
                    id: e.id.clone(),
                    hostile_name: e.hostile_name.clone(),
                    level: e.level,
                    ship_class: e.ship_class.clone(),
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
    pub crew: SimulateCrew,
    pub num_sims: Option<u32>,
    pub seed: Option<u64>,
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

fn officer_id_to_name(id: &str, officers: &[(String, String)]) -> String {
    officers
        .iter()
        .find(|(oid, _)| oid.eq_ignore_ascii_case(id))
        .map(|(_, name)| name.as_str())
        .unwrap_or(id)
        .to_string()
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
    let num_sims = req.num_sims.unwrap_or(5000).min(100_000).max(1);
    let seed = req.seed.unwrap_or(0);

    let officers: Vec<(String, String)> = registry
        .officers()
        .iter()
        .map(|o| (o.id.clone(), o.name.clone()))
        .collect();

    let captain = req
        .crew
        .captain
        .as_ref()
        .map(|s| officer_id_to_name(s, &officers))
        .unwrap_or_else(|| "".to_string());
    let bridge_names: Vec<String> = req
        .crew
        .bridge
        .as_ref()
        .map(|v| {
            v.iter()
                .take(BRIDGE_SLOTS)
                .map(|s| s.as_ref().map(|id| officer_id_to_name(id, &officers)).unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let below_names: Vec<String> = req
        .crew
        .below_deck
        .as_ref()
        .map(|v| {
            v.iter()
                .take(BELOW_DECKS_SLOTS)
                .map(|s| s.as_ref().map(|id| officer_id_to_name(id, &officers)).unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if captain.is_empty() {
        return Err(SimulateError::Validation("crew.captain is required".to_string()));
    }

    // Pad to fixed slot counts: 2 bridge, 3 below decks (repeat first if fewer provided).
    let bridge = pad_to_len(bridge_names, BRIDGE_SLOTS);
    let below_decks = pad_to_len(below_names, BELOW_DECKS_SLOTS);

    let candidate = CrewCandidate {
        captain: captain.clone(),
        bridge: bridge.clone(),
        below_decks: below_decks.clone(),
    };
    let candidates = vec![candidate];
    let results = run_monte_carlo_with_registry(
        registry,
        &req.ship,
        &req.hostile,
        &candidates,
        num_sims as usize,
        seed,
        profile_id,
    );
    let result = results.into_iter().next().unwrap_or(SimulationResult {
        candidate: CrewCandidate {
            captain,
            bridge,
            below_decks,
        },
        win_rate: 0.0,
        stall_rate: 0.0,
        loss_rate: 0.0,
        avg_hull_remaining: 0.0,
    });

    let wins = (result.win_rate * num_sims as f64).round() as u32;
    let ci = binomial_95_ci(wins, num_sims);

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
        warnings: Vec::new(),
    };
    serde_json::to_string_pretty(&response).map_err(SimulateError::Parse)
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

pub fn profile_put_payload(body: &str, profile_id: Option<&str>) -> Result<String, serde_json::Error> {
    let _: PlayerProfile = serde_json::from_str(body).map_err(|e| e)?;
    let id = resolve_profile_id(profile_id);
    let path = profile_path(&id, PROFILE_JSON);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, body).map_err(serde_json::Error::io)?;
    serde_json::to_string_pretty(&serde_json::json!({ "status": "ok" }))
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

pub fn officers_import_payload(body: &str, profile_id: Option<&str>) -> Result<String, ImportError> {
    let body = body.trim();
    let id = resolve_profile_id(profile_id);
    let output_path = profile_path(&id, ROSTER_IMPORTED).to_string_lossy().to_string();
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

pub fn officer_resolved_payload(registry: &DataRegistry, officer_id: &str) -> Result<String, OfficerResolveError> {
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
        &[officer.id.clone()],
        &[officer.id.clone()],
        &by_id,
        &opts,
    );

    // Create a response struct
    #[derive(Serialize)]
    struct ResolvedOfficer {
        id: String,
        name: String,
        static_buffs: std::collections::HashMap<String, f64>,
        crew_config: String,  // Debug format since CrewConfiguration doesn't impl Serialize
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub ship: String,
    pub scenario: String,
    pub crew: PresetCrew,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetSummary {
    pub id: String,
    pub name: String,
    pub ship: String,
    pub scenario: String,
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
        if path.extension().map_or(false, |e| e == "json") {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(p) = serde_json::from_str::<Preset>(&raw) {
                    list.push(PresetSummary {
                        id: p.id,
                        name: p.name,
                        ship: p.ship,
                        scenario: p.scenario,
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
    Ok(raw)
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

pub fn preset_post_payload(body: &str, profile_id: Option<&str>) -> Result<String, PresetError> {
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
        id: id.clone(),
        name: name.clone(),
        ship: in_.ship,
        scenario: in_.scenario,
        crew: in_.crew,
    };
    let raw = serde_json::to_string_pretty(&preset).map_err(PresetError::Serialize)?;
    fs::write(&path, raw).map_err(PresetError::Io)?;
    serde_json::to_string_pretty(&preset).map_err(PresetError::Serialize)
}

pub fn data_version_payload(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    let hostile_index = registry.hostile_index();
    let ship_index = registry.ship_index();
    let mechanics = vec![
        MechanicStatus { name: "Mitigation".to_string(), status: "implemented".to_string() },
        MechanicStatus { name: "Piercing".to_string(), status: "implemented".to_string() },
        MechanicStatus { name: "Armor".to_string(), status: "implemented".to_string() },
        MechanicStatus { name: "Critical".to_string(), status: "implemented".to_string() },
        MechanicStatus { name: "Burn".to_string(), status: "partial".to_string() },
        MechanicStatus { name: "Regeneration".to_string(), status: "partial".to_string() },
        MechanicStatus { name: "Isolytic".to_string(), status: "planned".to_string() },
        MechanicStatus { name: "Apex".to_string(), status: "planned".to_string() },
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

fn parse_below_decks_strategy(s: Option<&String>) -> BelowDecksStrategy {
    match s.as_deref() {
        Some(v) if v.trim().eq_ignore_ascii_case("exploration") => BelowDecksStrategy::Exploration,
        _ => BelowDecksStrategy::Ordered,
    }
}

/// Load heuristics seeds and expand them into CrewCandidates.
/// Uses registry for officer name resolution when provided (no disk reload).
fn load_heuristics_candidates(
    registry: &DataRegistry,
    seed_names: &[String],
    bd_strategy: BelowDecksStrategy,
) -> Vec<CrewCandidate> {
    let canonical_names: Vec<String> = registry.officers().iter().map(|o| o.name.clone()).collect();
    seed_names
        .iter()
        .flat_map(|name| {
            let parsed = load_seed_file(name, DEFAULT_HEURISTICS_DIR, Some(&canonical_names));
            let candidates = expand_crews(parsed, BELOW_DECKS_SLOTS, bd_strategy);
            candidates.into_iter().map(|c| CrewCandidate {
                captain: c.captain,
                bridge: c.bridge,
                below_decks: c.below_decks,
            })
        })
        .collect()
}

/// Rough seconds per (candidate × sim) on a typical multi-core machine; used for time estimates.
const ESTIMATE_SEC_PER_CANDIDATE_SIM: f64 = 4e-9;

fn parse_strategy(s: Option<&String>) -> OptimizerStrategy {
    match s.as_deref() {
        Some(v) if v.trim().eq_ignore_ascii_case("genetic") => OptimizerStrategy::Genetic,
        _ => OptimizerStrategy::Exhaustive,
    }
}

pub fn optimize_payload(
    registry: &DataRegistry,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, OptimizePayloadError> {
    let request: OptimizeRequest =
        serde_json::from_str(body).map_err(OptimizePayloadError::Parse)?;
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    validate_request(&request, sims)?;
    let seed = request.seed.unwrap_or(0);
    let strategy = parse_strategy(request.strategy.as_ref());
    let heuristics_only = request.heuristics_only.unwrap_or(false);
    let bd_strategy = parse_below_decks_strategy(request.below_decks_strategy.as_ref());
    let heuristics_seeds = request.heuristics_seeds.as_deref().unwrap_or(&[]);

    let start = Instant::now();

    // Load heuristics candidates (shared between all strategies).
    let h_candidates = if !heuristics_seeds.is_empty() {
        load_heuristics_candidates(registry, heuristics_seeds, bd_strategy)
    } else {
        Vec::new()
    };
    let is_seeded_genetic = strategy == OptimizerStrategy::Genetic && !h_candidates.is_empty();

    // Phase 1: evaluate heuristics via MC — but NOT for genetic strategy (seeds go into GA instead).
    let mut all_results: Vec<SimulationResult> = if !h_candidates.is_empty() && !is_seeded_genetic {
        run_monte_carlo_parallel_with_registry(
            registry,
            &request.ship,
            &request.hostile,
            &h_candidates,
            sims as usize,
            seed,
            profile_id,
        )
    } else {
        Vec::new()
    };

    // Phase 2: normal optimization (skipped when heuristics_only is set).
    if !heuristics_only {
        let scenario = OptimizationScenario {
            ship: &request.ship,
            hostile: &request.hostile,
            simulation_count: sims as usize,
            seed,
            max_candidates: request.max_candidates.map(|n| n as usize),
            strategy,
            only_below_decks_with_ability: request.prioritize_below_decks_ability.unwrap_or(false),
            seed_population: if is_seeded_genetic { h_candidates.clone() } else { Vec::new() },
            profile_id,
        };
        all_results.extend(optimize_scenario_with_registry(registry, &scenario).into_iter().map(|r| SimulationResult {
            candidate: CrewCandidate {
                captain: r.captain,
                bridge: r.bridge,
                below_decks: r.below_decks,
            },
            win_rate: r.win_rate,
            stall_rate: r.stall_rate,
            loss_rate: r.loss_rate,
            avg_hull_remaining: r.avg_hull_remaining,
        }));
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let ranked_results = rank_results(all_results);

    let engine = if heuristics_only {
        "heuristics"
    } else if is_seeded_genetic {
        "seeded_genetic"
    } else {
        match strategy {
            OptimizerStrategy::Exhaustive => "optimizer_v1",
            OptimizerStrategy::Genetic => "genetic",
        }
    };
    let mut notes = vec!["Results are deterministic for the same ship, hostile, simulation count, and seed."];
    if is_seeded_genetic {
        let note = "GA population seeded with heuristics crews.";
        notes.insert(0, note);
    } else if !heuristics_seeds.is_empty() {
        notes.insert(0, "Heuristics crews were evaluated first.");
    }

    let response = OptimizeResponse {
        status: "ok",
        engine,
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
                bridge: result.bridge.clone(),
                below_decks: result.below_decks.clone(),
                win_rate: result.win_rate,
                stall_rate: result.stall_rate,
                loss_rate: result.loss_rate,
                avg_hull_remaining: result.avg_hull_remaining,
            })
            .collect(),
        duration_ms: Some(duration_ms),
        notes,
        warnings: Vec::new(),
    };

    serde_json::to_string_pretty(&response).map_err(OptimizePayloadError::Parse)
}

// --- Optimize job store (for progress polling) ---

#[derive(Debug, Clone)]
pub enum OptimizeJobStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct OptimizeJobState {
    pub status: OptimizeJobStatus,
    pub progress: u8,
    pub crews_done: u32,
    pub total_crews: u32,
    pub result: Option<OptimizeResponse>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeStartResponse {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeStatusResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crews_done: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_crews: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<OptimizeResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

static OPTIMIZE_JOB_COUNTER: OnceLock<AtomicU64> = OnceLock::new();
static OPTIMIZE_JOBS: OnceLock<Mutex<HashMap<String, OptimizeJobState>>> = OnceLock::new();

fn optimize_jobs() -> &'static Mutex<HashMap<String, OptimizeJobState>> {
    OPTIMIZE_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_job_id() -> String {
    let counter = OPTIMIZE_JOB_COUNTER.get_or_init(|| AtomicU64::new(0));
    let n = counter.fetch_add(1, Ordering::Relaxed);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("opt_{}_{}", ms, n)
}

/// Start an optimize job in the background; returns job_id immediately.
pub fn optimize_start_payload(
    registry: std::sync::Arc<DataRegistry>,
    body: &str,
    profile_id: Option<&str>,
) -> Result<String, OptimizePayloadError> {
    let request: OptimizeRequest =
        serde_json::from_str(body).map_err(OptimizePayloadError::Parse)?;
    let sims = request.sims.unwrap_or(DEFAULT_SIMS);
    validate_request(&request, sims)?;
    let seed = request.seed.unwrap_or(0);

    let job_id = next_job_id();
    let jobs = optimize_jobs();
    {
        let mut map = jobs.lock().unwrap();
        map.insert(
            job_id.clone(),
            OptimizeJobState {
                status: OptimizeJobStatus::Running,
                progress: 0,
                crews_done: 0,
                total_crews: 0,
                result: None,
                error: None,
            },
        );
    }

    let registry = registry;
    let ship = request.ship.clone();
    let hostile = request.hostile.clone();
    let job_id_clone = job_id.clone();
    let max_candidates = request.max_candidates.map(|n| n as usize);
    let strategy = parse_strategy(request.strategy.as_ref());
    let prioritize_below_decks_ability = request.prioritize_below_decks_ability.unwrap_or(false);
    let heuristics_only = request.heuristics_only.unwrap_or(false);
    let bd_strategy = parse_below_decks_strategy(request.below_decks_strategy.as_ref());
    let heuristics_seeds = request.heuristics_seeds.clone().unwrap_or_default();
    let profile_id_owned = profile_id.map(String::from);

    std::thread::spawn(move || {
        let start = Instant::now();
        let registry_ref = registry.as_ref();

        // Load heuristics candidates (shared between all strategies).
        let h_candidates = if !heuristics_seeds.is_empty() {
            load_heuristics_candidates(registry_ref, &heuristics_seeds, bd_strategy)
        } else {
            Vec::new()
        };
        let is_seeded_genetic = strategy == OptimizerStrategy::Genetic && !h_candidates.is_empty();

        // Phase 1: evaluate heuristics via MC — but NOT for genetic strategy (seeds go into GA).
        let mut all_results: Vec<SimulationResult> = if !h_candidates.is_empty() && !is_seeded_genetic {
            let h_total = h_candidates.len() as u32;
            if let Ok(mut map) = optimize_jobs().lock() {
                if let Some(state) = map.get_mut(&job_id_clone) {
                    state.total_crews = h_total;
                }
            }
            let results = run_monte_carlo_parallel_with_registry(
                registry_ref,
                &ship,
                &hostile,
                &h_candidates,
                sims as usize,
                seed,
                profile_id_owned.as_deref(),
            );
            if let Ok(mut map) = optimize_jobs().lock() {
                if let Some(state) = map.get_mut(&job_id_clone) {
                    state.crews_done = h_total;
                    state.progress = if heuristics_only { 100 } else { 10 };
                }
            }
            results
        } else {
            Vec::new()
        };

        // Phase 2: normal optimization
        if !heuristics_only {
            let scenario = OptimizationScenario {
                ship: &ship,
                hostile: &hostile,
                simulation_count: sims as usize,
                seed,
                max_candidates,
                strategy,
                only_below_decks_with_ability: prioritize_below_decks_ability,
                seed_population: if is_seeded_genetic { h_candidates.clone() } else { Vec::new() },
                profile_id: profile_id_owned.as_deref(),
            };
            let normal_results = optimize_scenario_with_progress_with_registry(
                registry_ref,
                &scenario,
                |crews_done, total_crews| {
                    let base_progress = if !heuristics_seeds.is_empty() && !is_seeded_genetic { 10u8 } else { 0u8 };
                    let progress = if total_crews == 0 {
                        base_progress
                    } else {
                        let pct = (crews_done as f64 / total_crews as f64) * (100.0 - base_progress as f64);
                        (base_progress as f64 + pct).round().min(100.0) as u8
                    };
                    if let Ok(mut map) = optimize_jobs().lock() {
                        if let Some(state) = map.get_mut(&job_id_clone) {
                            state.progress = progress;
                            state.crews_done = crews_done;
                            state.total_crews = total_crews;
                        }
                    }
                }
            );
            all_results.extend(normal_results.into_iter().map(|r| SimulationResult {
                candidate: CrewCandidate {
                    captain: r.captain,
                    bridge: r.bridge,
                    below_decks: r.below_decks,
                },
                win_rate: r.win_rate,
                stall_rate: r.stall_rate,
                loss_rate: r.loss_rate,
                avg_hull_remaining: r.avg_hull_remaining,
            }));
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let ranked_results = rank_results(all_results);

        let engine = if heuristics_only {
            "heuristics"
        } else if is_seeded_genetic {
            "seeded_genetic"
        } else {
            match strategy {
                OptimizerStrategy::Exhaustive => "optimizer_v1",
                OptimizerStrategy::Genetic => "genetic",
            }
        };
        let mut notes = vec!["Results are deterministic for the same ship, hostile, simulation count, and seed."];
        if is_seeded_genetic {
            let note = "GA population seeded with heuristics crews.";
            notes.insert(0, note);
        } else if !heuristics_seeds.is_empty() {
            notes.insert(0, "Heuristics crews were evaluated first.");
        }

        let response = OptimizeResponse {
            status: "ok",
            engine,
            scenario: ScenarioSummary {
                ship: ship.clone(),
                hostile: hostile.clone(),
                sims,
                seed,
            },
            recommendations: ranked_results
                .into_iter()
                .map(|result| CrewRecommendation {
                    captain: result.captain,
                    bridge: result.bridge.clone(),
                    below_decks: result.below_decks.clone(),
                    win_rate: result.win_rate,
                    stall_rate: result.stall_rate,
                    loss_rate: result.loss_rate,
                    avg_hull_remaining: result.avg_hull_remaining,
                })
                .collect(),
            duration_ms: Some(duration_ms),
            notes,
            warnings: Vec::new(),
        };

        if let Ok(mut map) = optimize_jobs().lock() {
            if let Some(state) = map.get_mut(&job_id_clone) {
                state.status = OptimizeJobStatus::Done;
                state.progress = 100;
                state.result = Some(response);
            }
        }
    });

    let start_response = OptimizeStartResponse { job_id };
    serde_json::to_string_pretty(&start_response).map_err(OptimizePayloadError::Parse)
}

#[derive(Debug)]
pub enum OptimizeStatusError {
    NotFound,
    Serialize(serde_json::Error),
}

impl fmt::Display for OptimizeStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "Job not found"),
            Self::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OptimizeStatusError {}

/// Return current status (and result when done) for an optimize job.
pub fn optimize_status_payload(job_id: &str) -> Result<String, OptimizeStatusError> {
    let jobs = optimize_jobs();
    let map = jobs.lock().unwrap();
    let state = map.get(job_id).ok_or(OptimizeStatusError::NotFound)?;
    let status_str = match &state.status {
        OptimizeJobStatus::Running => "running",
        OptimizeJobStatus::Done => "done",
        OptimizeJobStatus::Error => "error",
    };
    let response = OptimizeStatusResponse {
        status: status_str.to_string(),
        progress: Some(state.progress),
        crews_done: Some(state.crews_done),
        total_crews: Some(state.total_crews),
        result: state.result.clone(),
        error: state.error.clone(),
    };
    serde_json::to_string_pretty(&response).map_err(OptimizeStatusError::Serialize)
}

/// Parses query string for optimize estimate: ship, hostile, sims, optional max_candidates, optional prioritize_below_decks_ability.
fn parse_optimize_estimate_query(query: &str) -> (String, String, u32, Option<u32>, bool) {
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
                "prioritize_below_decks_ability" => prioritize_below_decks_ability = value.eq_ignore_ascii_case("true") || value == "1",
                _ => {}
            }
        }
    }
    (ship, hostile, sims, max_candidates, prioritize_below_decks_ability)
}

pub fn optimize_estimate_payload(
    registry: &DataRegistry,
    path: &str,
    profile_id: Option<&str>,
) -> Result<String, OptimizePayloadError> {
    let query = path.split('?').nth(1).unwrap_or("");
    let (ship, hostile, sims, max_candidates, prioritize_below_decks_ability) =
        parse_optimize_estimate_query(query);
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
    let estimated_candidates = match max_candidates {
        Some(cap) if cap <= MAX_CANDIDATES => {
            let generator = CrewGenerator::with_strategy(CandidateStrategy {
                max_candidates: Some(cap as usize),
                only_below_decks_with_ability: prioritize_below_decks_ability,
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
                ..CandidateStrategy::default()
            });
            generator.count_candidates_from_registry(registry, &ship, &hostile, 0, profile_id)
        }
    };
    let estimated_seconds = (estimated_candidates as f64) * (sims as f64) * ESTIMATE_SEC_PER_CANDIDATE_SIM;
    let estimated_seconds = estimated_seconds.max(0.1).min(3600.0); // clamp to 0.1s–1h for display
    let payload = serde_json::json!({
        "estimated_candidates": estimated_candidates,
        "sims_per_crew": sims,
        "estimated_seconds": (estimated_seconds * 10.0).round() / 10.0,
    });
    serde_json::to_string_pretty(&payload).map_err(OptimizePayloadError::Parse)
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
