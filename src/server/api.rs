use crate::data::hostile::{load_hostile_index, DEFAULT_HOSTILES_INDEX_PATH};
use crate::data::import::{
    import_roster_csv, import_spocks_export, load_imported_roster_ids,
    DEFAULT_IMPORT_OUTPUT_PATH,
};
use crate::data::officer::{load_canonical_officers, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::ship::{load_ship_index, DEFAULT_SHIPS_INDEX_PATH};
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::{run_monte_carlo, SimulationResult};
use crate::optimizer::{optimize_scenario, OptimizationScenario};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;
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

pub fn officers_payload(path: &str) -> Result<String, serde_json::Error> {
    let officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH).unwrap_or_default();
    let owned_ids = if parse_owned_only(path) {
        load_imported_roster_ids(DEFAULT_IMPORT_OUTPUT_PATH)
    } else {
        None
    };
    let list: Vec<OfficerListItem> = officers
        .into_iter()
        .filter(|o| owned_ids.as_ref().map_or(true, |ids| ids.contains(&o.id)))
        .map(|o| OfficerListItem {
            id: o.id,
            name: o.name,
            slot: o.slot,
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

pub fn ships_payload() -> Result<String, serde_json::Error> {
    let index = load_ship_index(DEFAULT_SHIPS_INDEX_PATH);
    let list: Vec<ShipListItem> = index
        .map(|idx| {
            idx.ships
                .into_iter()
                .map(|e| ShipListItem {
                    id: e.id,
                    ship_name: e.ship_name,
                    ship_class: e.ship_class,
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

pub fn hostiles_payload() -> Result<String, serde_json::Error> {
    let index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH);
    let list: Vec<HostileListItem> = index
        .map(|idx| {
            idx.hostiles
                .into_iter()
                .map(|e| HostileListItem {
                    id: e.id,
                    hostile_name: e.hostile_name,
                    level: e.level,
                    ship_class: e.ship_class,
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
    pub bridge: Option<Vec<String>>,
    pub below_deck: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulateResponse {
    pub status: &'static str,
    pub stats: SimulateStats,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulateStats {
    pub win_rate: f64,
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

pub fn simulate_payload(body: &str) -> Result<String, SimulateError> {
    let req: SimulateRequest = serde_json::from_str(body).map_err(SimulateError::Parse)?;
    let num_sims = req.num_sims.unwrap_or(5000).min(100_000).max(1);
    let seed = req.seed.unwrap_or(0);

    let officers: Vec<(String, String)> = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .map(|list| list.into_iter().map(|o| (o.id.clone(), o.name.clone())).collect())
        .unwrap_or_default();

    let captain = req
        .crew
        .captain
        .as_ref()
        .map(|s| officer_id_to_name(s, &officers))
        .unwrap_or_else(|| "".to_string());
    let bridge = req
        .crew
        .bridge
        .as_ref()
        .and_then(|v| v.first())
        .map(|s| officer_id_to_name(s, &officers))
        .unwrap_or_else(|| "".to_string());
    let below_decks = req
        .crew
        .below_deck
        .as_ref()
        .and_then(|v| v.first())
        .map(|s| officer_id_to_name(s, &officers))
        .unwrap_or_else(|| "".to_string());

    if captain.is_empty() {
        return Err(SimulateError::Validation("crew.captain is required".to_string()));
    }

    let candidate = CrewCandidate {
        captain: captain.clone(),
        bridge: bridge.clone(),
        below_decks: below_decks.clone(),
    };
    let candidates = vec![candidate];
    let results = run_monte_carlo(
        &req.ship,
        &req.hostile,
        &candidates,
        num_sims as usize,
        seed,
    );
    let result = results.into_iter().next().unwrap_or(SimulationResult {
        candidate: CrewCandidate {
            captain,
            bridge,
            below_decks,
        },
        win_rate: 0.0,
        avg_hull_remaining: 0.0,
    });

    let wins = (result.win_rate * num_sims as f64).round() as u32;
    let ci = binomial_95_ci(wins, num_sims);

    let response = SimulateResponse {
        status: "ok",
        stats: SimulateStats {
            win_rate: result.win_rate,
            avg_hull_remaining: result.avg_hull_remaining,
            n: num_sims,
            win_rate_95_ci: Some(ci),
        },
        seed,
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

const PROFILE_PATH: &str = "data/profile.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    #[serde(default)]
    pub bonuses: std::collections::HashMap<String, f64>,
}

pub fn profile_get_payload() -> Result<String, serde_json::Error> {
    let path = Path::new(PROFILE_PATH);
    let profile: PlayerProfile = if path.exists() {
        let raw = fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string());
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

pub fn profile_put_payload(body: &str) -> Result<String, serde_json::Error> {
    let _: PlayerProfile = serde_json::from_str(body).map_err(|e| e)?;
    if let Some(parent) = Path::new(PROFILE_PATH).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(PROFILE_PATH, body).map_err(serde_json::Error::io)?;
    serde_json::to_string_pretty(&serde_json::json!({ "status": "ok" }))
}

fn write_temp_import_file(body: &[u8], ext: &str) -> Result<std::path::PathBuf, std::io::Error> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("kobayashi_import_{}_{}", std::process::id(), ext));
    let mut f = fs::File::create(&path)?;
    f.write_all(body)?;
    f.sync_all()?;
    Ok(path)
}

pub fn officers_import_payload(body: &str) -> Result<String, ImportError> {
    let body = body.trim();
    let report = if body.starts_with('{') || body.starts_with('[') {
        let p = write_temp_import_file(body.as_bytes(), "json").map_err(ImportError::Io)?;
        let out = import_spocks_export(p.to_str().unwrap())?;
        let _ = fs::remove_file(&p);
        out
    } else {
        let p = write_temp_import_file(body.as_bytes(), "txt").map_err(ImportError::Io)?;
        let out = import_roster_csv(p.to_str().unwrap())?;
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

const PRESETS_DIR: &str = "data/presets";

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

fn ensure_presets_dir() -> std::io::Result<()> {
    fs::create_dir_all(PRESETS_DIR)
}

pub fn presets_list_payload() -> Result<String, serde_json::Error> {
    ensure_presets_dir().map_err(serde_json::Error::io)?;
    let mut list = Vec::new();
    let dir = fs::read_dir(PRESETS_DIR).map_err(serde_json::Error::io)?;
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

pub fn preset_get_payload(id: &str) -> Result<String, PresetError> {
    let path = Path::new(PRESETS_DIR).join(sanitize_preset_id(id));
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

pub fn preset_post_payload(body: &str) -> Result<String, PresetError> {
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
    let path = Path::new(PRESETS_DIR).join(sanitize_preset_id(&id));
    ensure_presets_dir().map_err(PresetError::Io)?;
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

pub fn data_version_payload() -> Result<String, serde_json::Error> {
    let hostile_index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH);
    let ship_index = load_ship_index(DEFAULT_SHIPS_INDEX_PATH);
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
        hostile_version: hostile_index.and_then(|i| i.data_version),
        ship_version: ship_index.and_then(|i| i.data_version),
        mechanics,
    };
    serde_json::to_string_pretty(&response)
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
