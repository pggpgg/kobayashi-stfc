//! Sync ingress for STFC Community Mod: accepts type-specific JSON arrays and
//! updates roster (and optionally other state) for quasi real-time optimizer use.

use axum::http::StatusCode;
use crate::data::import;
use crate::data::profile_index::{effective_profile_id, load_profile_index, profile_id_by_sync_token, profile_path,
    BUFFS_IMPORTED, FORBIDDEN_TECH_IMPORTED, ROSTER_IMPORTED, RESEARCH_IMPORTED, BUILDINGS_IMPORTED,
    SHIPS_IMPORTED};
use crate::data::research::{load_research_catalog, DEFAULT_RESEARCH_CATALOG_PATH};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// Default path for game officer id -> canonical_officer_id mapping (same as id_registry).
pub const DEFAULT_GAME_ID_MAP_PATH: &str = "data/officers/id_registry.json";

/// Log file for sync ingress (append-only). Written when POST /api/sync/ingress is received.
pub const SYNC_LOG_PATH: &str = "sync.log";

fn append_sync_log(line: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(SYNC_LOG_PATH)
        .and_then(|mut f| writeln!(f, "{}", line));
}

static SYNC_ROSTER_MTX: Mutex<()> = Mutex::new(());
static SYNC_RESEARCH_MTX: Mutex<()> = Mutex::new(());
static SYNC_BUILDINGS_MTX: Mutex<()> = Mutex::new(());
static SYNC_SHIPS_MTX: Mutex<()> = Mutex::new(());
static SYNC_FT_MTX: Mutex<()> = Mutex::new(());
static SYNC_BUFFS_MTX: Mutex<()> = Mutex::new(());

/// Handles POST /api/sync/ingress: token-based routing. The stfc-sync-token header
/// identifies the profile; sync data is written to that profile's paths.
/// Returns `(StatusCode, json_body_string)`.
pub fn ingress_payload(body: &str, sync_token: Option<&str>) -> (StatusCode, String) {
    let body_len = body.len();
    let ts = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
    append_sync_log(&format!("{} POST /api/sync/ingress body_len={}", ts, body_len));
    eprintln!("[sync] POST /api/sync/ingress received, body_len={}", body_len);

    let index = load_profile_index();
    let profile_id = profile_id_by_sync_token(&index, sync_token.unwrap_or(""));
    let Some(ref pid) = profile_id else {
        eprintln!("[sync] 401 Unauthorized (no profile for stfc-sync-token)");
        return json_error_response(StatusCode::UNAUTHORIZED, "Invalid or missing stfc-sync-token");
    };

    let roster_path = profile_path(pid, ROSTER_IMPORTED).to_string_lossy().to_string();
    let research_path = profile_path(pid, RESEARCH_IMPORTED).to_string_lossy().to_string();
    let buildings_path = profile_path(pid, BUILDINGS_IMPORTED).to_string_lossy().to_string();
    let ships_path = profile_path(pid, SHIPS_IMPORTED).to_string_lossy().to_string();
    let ft_path = profile_path(pid, FORBIDDEN_TECH_IMPORTED).to_string_lossy().to_string();
    let buffs_path = profile_path(pid, BUFFS_IMPORTED).to_string_lossy().to_string();

    let payload: Vec<serde_json::Value> = match serde_json::from_str(body) {
        Ok(arr) => arr,
        Err(e) => {
            eprintln!("[sync] 400 Bad Request: body is not a JSON array: {e}");
            return json_error_response(
                StatusCode::BAD_REQUEST,
                &format!("Request body must be a JSON array: {e}"),
            );
        }
    };

    if payload.is_empty() {
        eprintln!("[sync] 200 OK accepted=[] (empty array)");
        return ok_accepted_response(&[]);
    }

    let first = &payload[0];
    let type_str = first
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let type_lower = type_str.to_ascii_lowercase();
    eprintln!("[sync] type={type_str} count={}", payload.len());

    let accepted = match type_lower.as_str() {
        "officer" => {
            match apply_officer_sync(&payload, DEFAULT_GAME_ID_MAP_PATH, &roster_path) {
                Ok(accepted_count) => {
                    eprintln!("[sync] 200 OK accepted officer({accepted_count})");
                    vec![format!("officer({accepted_count})")]
                }
                Err(e) => {
                    eprintln!("[sync] 500 Internal Server Error (officer): {e}");
                    return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                }
            }
        }
        "research" => {
            match apply_research_sync(&payload, &research_path) {
                Ok(accepted_count) => {
                    eprintln!("[sync] 200 OK accepted research({accepted_count})");
                    vec![format!("research({accepted_count})")]
                }
                Err(e) => {
                    eprintln!("[sync] 500 Internal Server Error (research): {e}");
                    return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                }
            }
        }
        "buildings" | "module" => {
            match apply_buildings_sync(&payload, &buildings_path) {
                Ok(accepted_count) => {
                    eprintln!("[sync] 200 OK accepted buildings({accepted_count})");
                    vec![format!("buildings({accepted_count})")]
                }
                Err(e) => {
                    eprintln!("[sync] 500 Internal Server Error (buildings): {e}");
                    return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                }
            }
        }
        "ships" | "ship" => {
            match apply_ships_sync(&payload, &ships_path) {
                Ok(accepted_count) => {
                    eprintln!("[sync] 200 OK accepted ships({accepted_count})");
                    vec![format!("ships({accepted_count})")]
                }
                Err(e) => {
                    eprintln!("[sync] 500 Internal Server Error (ships): {e}");
                    return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                }
            }
        }
        "ft" => {
            match apply_ft_sync(&payload, &ft_path) {
                Ok(accepted_count) => {
                    eprintln!("[sync] 200 OK accepted ft({accepted_count})");
                    vec![format!("ft({accepted_count})")]
                }
                Err(e) => {
                    eprintln!("[sync] 500 Internal Server Error (ft): {e}");
                    return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                }
            }
        }
        // stfc-mod uses JSON type "tech" for forbidden/chaos tech (same payload as "ft": fid, tier, level, shard_count).
        "tech" => {
            match apply_ft_sync(&payload, &ft_path) {
                Ok(accepted_count) => {
                    eprintln!("[sync] 200 OK accepted tech({accepted_count}) -> forbidden_tech.imported.json");
                    vec![format!("tech({accepted_count})")]
                }
                Err(e) => {
                    eprintln!("[sync] 500 Internal Server Error (tech/ft): {e}");
                    return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                }
            }
        }
        // Mod may send only removals in a batch (`type: "expired_buffs"` on the first row).
        "buffs" | "expired_buffs" => {
            match apply_buffs_sync(&payload, &buffs_path) {
                Ok(accepted_count) => {
                    eprintln!("[sync] 200 OK accepted buffs({accepted_count})");
                    vec![format!("buffs({accepted_count})")]
                }
                Err(e) => {
                    eprintln!("[sync] 500 Internal Server Error (buffs): {e}");
                    return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                }
            }
        }
        "resources" | "missions" | "battlelogs" | "traits" | "slots" | "inventory" | "jobs" => {
            eprintln!("[sync] 200 OK accepted {} (not persisted)", type_str);
            vec![type_str.to_string()]
        }
        _ => {
            eprintln!("[sync] 200 OK accepted {} (unknown type)", type_str);
            vec![type_str.to_string()]
        }
    };

    ok_accepted_response(&accepted)
}

#[derive(Debug, Deserialize)]
struct SyncOfficerItem {
    #[serde(rename = "type", default)]
    _type: Option<String>,
    #[serde(default)]
    oid: Option<serde_json::Value>,
    #[serde(default)]
    rank: Option<i32>,
    #[serde(default)]
    level: Option<i32>,
    #[serde(default, rename = "shard_count")]
    _shard_count: Option<i32>,
}

/// Merges sync officer payload into the roster file using game_id map; returns count accepted.
fn apply_officer_sync(
    payload: &[serde_json::Value],
    game_id_map_path: &str,
    roster_output_path: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let _guard = SYNC_ROSTER_MTX.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let game_id_to_canonical = load_game_id_map(game_id_map_path)?;
    let canonical_names = load_canonical_names("data/officers/officers.canonical.json")?;

    let mut roster_map: HashMap<String, import::RosterEntry> = load_existing_roster(roster_output_path)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.canonical_officer_id.clone(), e))
        .collect();

    let mut accepted = 0usize;
    for item in payload {
        let item: SyncOfficerItem = match serde_json::from_value(item.clone()) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let oid_key = oid_to_map_key(item.oid.as_ref())?;
        let Some(canonical_id) = game_id_to_canonical.get(&oid_key).cloned() else {
            continue;
        };
        let canonical_name = canonical_names
            .get(&canonical_id)
            .cloned()
            .unwrap_or_else(|| canonical_id.clone());
        let rank = item.rank.and_then(|r| u8::try_from(r).ok());
        let level = item
            .level
            .and_then(|l| u16::try_from(l).ok());
        let tier = rank;

        let entry = import::RosterEntry {
            canonical_officer_id: canonical_id,
            canonical_name,
            rank,
            tier,
            level,
        };
        roster_map.insert(entry.canonical_officer_id.clone(), entry);
        accepted += 1;
    }

    let mut roster: Vec<import::RosterEntry> = roster_map.into_values().collect();
    roster.sort_by(|a, b| a.canonical_officer_id.cmp(&b.canonical_officer_id));

    let output_payload = serde_json::json!({
        "source_path": "stfc-mod sync",
        "officers": roster,
    });
    if let Some(parent) = std::path::Path::new(roster_output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(&output_payload)?;
    std::fs::write(roster_output_path, serialized)?;

    Ok(accepted)
}

fn oid_to_map_key(oid: Option<&serde_json::Value>) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let v = oid.ok_or("missing oid")?;
    if let Some(s) = v.as_str() {
        return Ok(s.to_string());
    }
    if let Some(n) = v.as_f64() {
        return Ok(format!("{:E}", n));
    }
    if let Some(n) = v.as_i64() {
        return Ok(format!("{:E}", n as f64));
    }
    if let Some(n) = v.as_u64() {
        return Ok(format!("{:E}", n as f64));
    }
    Err("oid must be string or number".into())
}

fn load_game_id_map(path: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let raw = std::fs::read_to_string(path)?;
    let parsed: HashMap<String, String> = serde_json::from_str(&raw)?;
    Ok(parsed)
}

fn load_canonical_names(path: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    #[derive(serde::Deserialize)]
    struct File {
        officers: Vec<CanonicalOfficer>,
    }
    #[derive(serde::Deserialize)]
    struct CanonicalOfficer {
        id: String,
        name: String,
    }
    let raw = std::fs::read_to_string(path)?;
    let file: File = serde_json::from_str(&raw)?;
    let map = file
        .officers
        .into_iter()
        .map(|o| (o.id, o.name))
        .collect();
    Ok(map)
}


fn load_existing_roster(path: &str) -> Option<Vec<import::RosterEntry>> {
    #[derive(serde::Deserialize)]
    struct Payload {
        officers: Vec<import::RosterEntry>,
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let payload: Payload = serde_json::from_str(&raw).ok()?;
    Some(payload.officers)
}

// ----- Research sync -----

#[derive(Debug, Deserialize)]
struct SyncResearchItem {
    #[serde(rename = "type", default)]
    _type: Option<String>,
    #[serde(default)]
    rid: Option<i64>,
    #[serde(default)]
    level: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SyncBuffItem {
    #[serde(rename = "type", default)]
    _type: Option<String>,
    #[serde(default)]
    bid: Option<i64>,
    #[serde(default)]
    level: Option<i64>,
    #[serde(default)]
    expiry_time: Option<Value>,
}

fn apply_buffs_sync(
    payload: &[serde_json::Value],
    output_path: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let _guard = SYNC_BUFFS_MTX.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let mut by_bid: HashMap<i64, import::GlobalBuffEntry> = import::load_imported_buffs(output_path)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.bid, e))
        .collect();

    let mut accepted = 0usize;
    for item in payload {
        let parsed: SyncBuffItem = match serde_json::from_value(item.clone()) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let t = parsed
            ._type
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        let expired = t == "expired_buffs" || (t.starts_with("expired_") && t.contains("buff"));
        let Some(bid) = parsed.bid else {
            continue;
        };

        if expired {
            by_bid.remove(&bid);
        } else if t == "buffs" {
            let level = parsed.level.unwrap_or(0);
            let expiry_time = match &parsed.expiry_time {
                None | Some(Value::Null) => None,
                Some(Value::Number(n)) => n.as_i64(),
                _ => None,
            };
            by_bid.insert(
                bid,
                import::GlobalBuffEntry {
                    bid,
                    level,
                    expiry_time,
                },
            );
        } else {
            continue;
        }
        accepted += 1;
    }

    let mut buffs: Vec<import::GlobalBuffEntry> = by_bid.into_values().collect();
    buffs.sort_by(|a, b| a.bid.cmp(&b.bid));

    let output_payload = serde_json::json!({
        "source_path": "stfc-mod sync",
        "buffs": buffs,
    });
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_string_pretty(&output_payload)?)?;
    Ok(accepted)
}

fn apply_research_sync(
    payload: &[serde_json::Value],
    output_path: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let _guard = SYNC_RESEARCH_MTX.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let mut by_rid: HashMap<i64, import::ResearchEntry> = import::load_imported_research(output_path)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.rid, e))
        .collect();

    let mut accepted = 0usize;
    for item in payload {
        let item: SyncResearchItem = match serde_json::from_value(item.clone()) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let Some(rid) = item.rid else { continue };
        let level = item.level.unwrap_or(0);
        by_rid.insert(rid, import::ResearchEntry { rid, level });
        accepted += 1;
    }

    let mut research: Vec<import::ResearchEntry> = by_rid.into_values().collect();
    research.sort_by(|a, b| a.rid.cmp(&b.rid));

    let output_payload = serde_json::json!({
        "source_path": "stfc-mod sync",
        "research": research,
    });
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_string_pretty(&output_payload)?)?;
    Ok(accepted)
}

// ----- Buildings sync -----

#[derive(Debug, Deserialize)]
struct SyncBuildingItem {
    #[serde(rename = "type", default)]
    _type: Option<String>,
    #[serde(default)]
    bid: Option<i64>,
    #[serde(default)]
    level: Option<i64>,
}

fn apply_buildings_sync(
    payload: &[serde_json::Value],
    output_path: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let _guard = SYNC_BUILDINGS_MTX.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let mut by_bid: HashMap<i64, import::BuildingEntry> =
        import::load_imported_buildings(output_path)
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.bid, e))
            .collect();

    let mut accepted = 0usize;
    for item in payload {
        let item: SyncBuildingItem = match serde_json::from_value(item.clone()) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let Some(bid) = item.bid else { continue };
        let level = item.level.unwrap_or(0);
        by_bid.insert(bid, import::BuildingEntry { bid, level });
        accepted += 1;
    }

    let mut buildings: Vec<import::BuildingEntry> = by_bid.into_values().collect();
    buildings.sort_by(|a, b| a.bid.cmp(&b.bid));

    let output_payload = serde_json::json!({
        "source_path": "stfc-mod sync",
        "buildings": buildings,
    });
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_string_pretty(&output_payload)?)?;
    Ok(accepted)
}

// ----- Ships sync -----

#[derive(Debug, Deserialize)]
struct SyncShipItem {
    #[serde(rename = "type", default)]
    _type: Option<String>,
    #[serde(default)]
    psid: Option<i64>,
    #[serde(default)]
    tier: Option<i64>,
    #[serde(default)]
    level: Option<i64>,
    #[serde(default)]
    level_percentage: Option<f64>,
    #[serde(default)]
    hull_id: Option<i64>,
    #[serde(default)]
    components: Option<Vec<i64>>,
}

fn apply_ships_sync(
    payload: &[serde_json::Value],
    output_path: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let _guard = SYNC_SHIPS_MTX.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let mut by_psid: HashMap<i64, import::ShipEntry> = import::load_imported_ships(output_path)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.psid, e))
        .collect();

    let mut accepted = 0usize;
    for item in payload {
        let item: SyncShipItem = match serde_json::from_value(item.clone()) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let Some(psid) = item.psid else { continue };
        let hull_id = item.hull_id.unwrap_or(0);
        let entry = import::ShipEntry {
            psid,
            tier: item.tier.unwrap_or(0),
            level: item.level.unwrap_or(0),
            level_percentage: item.level_percentage.unwrap_or(-1.0),
            hull_id,
            components: item.components.unwrap_or_default(),
        };
        by_psid.insert(psid, entry);
        accepted += 1;
    }

    let mut ships: Vec<import::ShipEntry> = by_psid.into_values().collect();
    ships.sort_by(|a, b| a.psid.cmp(&b.psid));

    let output_payload = serde_json::json!({
        "source_path": "stfc-mod sync",
        "ships": ships,
    });
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_string_pretty(&output_payload)?)?;
    Ok(accepted)
}

// ----- Forbidden tech (ft) sync -----

#[derive(Debug, Deserialize)]
struct SyncFtItem {
    #[serde(rename = "type", default)]
    _type: Option<String>,
    #[serde(default)]
    fid: Option<i64>,
    #[serde(default)]
    tier: Option<i64>,
    #[serde(default)]
    level: Option<i64>,
    #[serde(default, rename = "shard_count")]
    shard_count: Option<i64>,
}

fn apply_ft_sync(
    payload: &[serde_json::Value],
    output_path: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let _guard = SYNC_FT_MTX.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let mut by_fid: HashMap<i64, import::ForbiddenTechEntry> =
        import::load_imported_forbidden_tech(output_path)
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.fid, e))
            .collect();

    let mut accepted = 0usize;
    for item in payload {
        let item: SyncFtItem = match serde_json::from_value(item.clone()) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let Some(fid) = item.fid else { continue };
        let entry = import::ForbiddenTechEntry {
            fid,
            tier: item.tier.unwrap_or(0),
            level: item.level.unwrap_or(0),
            shard_count: item.shard_count.unwrap_or(0),
        };
        by_fid.insert(fid, entry);
        accepted += 1;
    }

    let mut forbidden_tech: Vec<import::ForbiddenTechEntry> = by_fid.into_values().collect();
    forbidden_tech.sort_by(|a, b| a.fid.cmp(&b.fid));

    let output_payload = serde_json::json!({
        "source_path": "stfc-mod sync",
        "forbidden_tech": forbidden_tech,
    });
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_string_pretty(&output_payload)?)?;
    Ok(accepted)
}

fn ok_accepted_response(accepted: &[String]) -> (StatusCode, String) {
    let body = serde_json::json!({
        "status": "ok",
        "accepted": accepted,
    });
    let body_str = serde_json::to_string_pretty(&body)
        .unwrap_or_else(|_| r#"{"status":"ok","accepted":[]}"#.to_string());
    (StatusCode::OK, body_str)
}

fn json_error_response(status_code: StatusCode, message: &str) -> (StatusCode, String) {
    let body = serde_json::json!({
        "status": "error",
        "message": message,
    });
    let body_str = serde_json::to_string_pretty(&body).unwrap_or_else(|_| {
        format!(
            r#"{{"status":"error","message":{}}}"#,
            serde_json::to_string(message).unwrap_or_default()
        )
    });
    (status_code, body_str)
}

fn last_modified_iso(path: &str) -> Option<String> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| {
            t.duration_since(UNIX_EPOCH).ok().and_then(|d| {
                chrono::Utc
                    .timestamp_opt(d.as_secs() as i64, d.subsec_nanos())
                    .single()
                    .map(|dt| dt.to_rfc3339())
            })
        })
}

/// Handles GET /api/sync/status: returns roster path and last modified time (ISO8601) or null if missing.
/// Uses the default profile's paths so the response matches where the optimizer reads (profile_path(profile_id, ...)).
/// Also includes research_path, buildings_path, ships_path, forbidden_tech_path, buffs_path and their last_modified_iso when present.
/// Returns `(StatusCode, json_body_string)`.
pub fn sync_status_payload() -> (StatusCode, String) {
    let index = load_profile_index();
    let pid = effective_profile_id(&index);
    let roster_path = profile_path(&pid, ROSTER_IMPORTED).to_string_lossy().to_string();
    let research_path = profile_path(&pid, RESEARCH_IMPORTED).to_string_lossy().to_string();
    let buildings_path = profile_path(&pid, BUILDINGS_IMPORTED).to_string_lossy().to_string();
    let ships_path = profile_path(&pid, SHIPS_IMPORTED).to_string_lossy().to_string();
    let forbidden_tech_path = profile_path(&pid, FORBIDDEN_TECH_IMPORTED).to_string_lossy().to_string();
    let buffs_path = profile_path(&pid, BUFFS_IMPORTED).to_string_lossy().to_string();

    let research_catalog = load_research_catalog(DEFAULT_RESEARCH_CATALOG_PATH);
    let research_catalog_loaded = research_catalog.is_some();
    let research_catalog_item_count = research_catalog
        .as_ref()
        .map(|c| c.items.len() as u32)
        .unwrap_or(0);

    let body = serde_json::json!({
        "roster_path": roster_path,
        "last_modified_iso": last_modified_iso(&roster_path),
        "research_path": research_path,
        "research_last_modified_iso": last_modified_iso(&research_path),
        "buildings_path": buildings_path,
        "buildings_last_modified_iso": last_modified_iso(&buildings_path),
        "ships_path": ships_path,
        "ships_last_modified_iso": last_modified_iso(&ships_path),
        "forbidden_tech_path": forbidden_tech_path,
        "forbidden_tech_last_modified_iso": last_modified_iso(&forbidden_tech_path),
        "buffs_path": buffs_path,
        "buffs_last_modified_iso": last_modified_iso(&buffs_path),
        "research_catalog_loaded": research_catalog_loaded,
        "research_catalog_item_count": research_catalog_item_count,
    });
    let body_str = serde_json::to_string_pretty(&body).unwrap_or_else(|_| {
        r#"{"roster_path":"rosters/roster.imported.json","last_modified_iso":null}"#.to_string()
    });
    (StatusCode::OK, body_str)
}

#[cfg(test)]
mod tests {
    use super::ingress_payload;
    use axum::http::StatusCode;
    use crate::data::import;
    use crate::data::profile_index::{create_profile, delete_profile, load_profile_index, profile_path,
        BUFFS_IMPORTED, RESEARCH_IMPORTED, BUILDINGS_IMPORTED, SHIPS_IMPORTED, FORBIDDEN_TECH_IMPORTED};
    use std::sync::Mutex;
    use std::sync::Once;
    use uuid::Uuid;

    /// Serialize sync tests to avoid races on shared profiles/index.json.
    static SYNC_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Remove any leftover sync_test_* profiles from previous test runs (e.g. before drop guards existed).
    /// Cleans both index entries and orphaned directories on disk.
    static CLEANUP_ORPHANS: Once = Once::new();

    fn cleanup_orphan_sync_test_profiles() {
        CLEANUP_ORPHANS.call_once(|| {
            let mut index = load_profile_index();
            let ids: Vec<String> = index
                .profiles
                .iter()
                .filter(|p| p.id.starts_with("sync_test_"))
                .map(|p| p.id.clone())
                .collect();
            for id in &ids {
                let _ = delete_profile(&mut index, id);
            }
            let profiles_dir = std::path::Path::new(crate::data::profile_index::PROFILES_DIR);
            if let Ok(entries) = std::fs::read_dir(profiles_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.starts_with("sync_test_") {
                                let _ = std::fs::remove_dir_all(&path);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Guard that deletes the test profile on drop so we don't leave sync_test_* dirs in profiles/.
    struct TestProfileGuard(String);

    impl Drop for TestProfileGuard {
        fn drop(&mut self) {
            let mut index = load_profile_index();
            let _ = delete_profile(&mut index, &self.0);
        }
    }

    /// Create a unique test profile and return (sync_token, profile_id, cleanup_guard).
    fn ensure_test_profile() -> (String, String, TestProfileGuard) {
        cleanup_orphan_sync_test_profiles();
        let mut index = load_profile_index();
        let id = format!("sync_test_{}", Uuid::new_v4().as_simple());
        let entry = create_profile(&mut index, Some(&id), "Sync Test").expect("create test profile");
        (entry.sync_token, entry.id.clone(), TestProfileGuard(entry.id))
    }

    #[test]
    fn ingress_empty_array_returns_200_and_accepted() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, _, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload("[]", Some(&token));
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("\"status\": \"ok\""));
        assert!(body.contains("\"accepted\""));
    }

    #[test]
    fn ingress_non_array_body_returns_400() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, _, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload("{}", Some(&token));
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("array"));
    }

    #[test]
    fn ingress_unknown_type_returns_200_and_accepts_type() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, _, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(r#"[{"type":"unknown","x":1}]"#, Some(&token));
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("unknown"));
    }

    #[test]
    fn ingress_research_type_returns_200() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, _, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(r#"[{"type":"research","rid":1,"level":1}]"#, Some(&token));
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("research"));
    }

    #[test]
    fn ingress_research_persists_to_file() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(r#"[{"type":"research","rid":919291,"level":3}]"#, Some(&token));
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("research(1)"));
        let research_path = profile_path(&profile_id, RESEARCH_IMPORTED).to_string_lossy().to_string();
        let entries = import::load_imported_research(&research_path)
            .expect("research.imported.json should exist after sync");
        assert!(
            entries.iter().any(|e| e.rid == 919291 && e.level == 3),
            "expected rid=919291 level=3 in {:?}",
            entries
        );
    }

    #[test]
    fn ingress_buffs_persist_and_expired_removes() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(
            r#"[{"type":"buffs","bid":777001,"level":3,"expiry_time":null}]"#,
            Some(&token),
        );
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("buffs(1)"));
        let path = profile_path(&profile_id, BUFFS_IMPORTED)
            .to_string_lossy()
            .to_string();
        let entries = import::load_imported_buffs(&path).expect("buffs.imported.json");
        assert!(
            entries
                .iter()
                .any(|e| e.bid == 777001 && e.level == 3 && e.expiry_time.is_none()),
            "expected buff 777001: {:?}",
            entries
        );

        let (status2, _) = ingress_payload(
            r#"[{"type":"expired_buffs","bid":777001}]"#,
            Some(&token),
        );
        assert_eq!(status2, StatusCode::OK);
        let entries2 = import::load_imported_buffs(&path).unwrap_or_default();
        assert!(
            !entries2.iter().any(|e| e.bid == 777001),
            "expired_buffs should remove bid: {:?}",
            entries2
        );
    }

    #[test]
    fn ingress_buildings_persist_to_file() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(r#"[{"type":"buildings","bid":919292,"level":5}]"#, Some(&token));
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("buildings(1)"));
        let buildings_path = profile_path(&profile_id, BUILDINGS_IMPORTED).to_string_lossy().to_string();
        let entries = import::load_imported_buildings(&buildings_path)
            .expect("buildings.imported.json should exist after sync");
        assert!(
            entries.iter().any(|e| e.bid == 919292 && e.level == 5),
            "expected bid=919292 level=5 in {:?}",
            entries
        );
    }

    #[test]
    fn ingress_ships_persist_to_file() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(
            r#"[{"type":"ships","psid":919293,"tier":2,"level":10,"level_percentage":0.5,"hull_id":100,"components":[1,2,3]}]"#,
            Some(&token),
        );
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("ships(1)"));
        let ships_path = profile_path(&profile_id, SHIPS_IMPORTED).to_string_lossy().to_string();
        let entries = import::load_imported_ships(&ships_path)
            .expect("ships.imported.json should exist after sync");
        assert!(
            entries.iter().any(|e| {
                e.psid == 919293 && e.tier == 2 && e.level == 10 && e.hull_id == 100
            }),
            "expected psid=919293 in {:?}",
            entries
        );
    }

    #[test]
    fn ingress_module_type_persists_to_file() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(r#"[{"type":"module","bid":919294,"level":7}]"#, Some(&token));
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("buildings(1)"));
        let buildings_path = profile_path(&profile_id, BUILDINGS_IMPORTED).to_string_lossy().to_string();
        let entries = import::load_imported_buildings(&buildings_path)
            .expect("buildings.imported.json should exist after sync");
        assert!(
            entries.iter().any(|e| e.bid == 919294 && e.level == 7),
            "expected bid=919294 level=7 in {:?}",
            entries
        );
    }

    #[test]
    fn ingress_ship_type_persists_to_file() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(
            r#"[{"type":"ship","psid":919295,"tier":3,"level":15,"level_percentage":0.0,"hull_id":200,"components":[]}]"#,
            Some(&token),
        );
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("ships(1)"));
        let ships_path = profile_path(&profile_id, SHIPS_IMPORTED).to_string_lossy().to_string();
        let entries = import::load_imported_ships(&ships_path)
            .expect("ships.imported.json should exist after sync");
        assert!(
            entries.iter().any(|e| {
                e.psid == 919295 && e.tier == 3 && e.level == 15 && e.hull_id == 200
            }),
            "expected psid=919295 in {:?}",
            entries
        );
    }

    #[test]
    fn ingress_ft_persists_to_file() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(
            r#"[{"type":"ft","fid":919296,"tier":1,"level":5,"shard_count":10}]"#,
            Some(&token),
        );
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("ft(1)"));
        let ft_path = profile_path(&profile_id, FORBIDDEN_TECH_IMPORTED).to_string_lossy().to_string();
        let entries = import::load_imported_forbidden_tech(&ft_path)
            .expect("forbidden_tech.imported.json should exist after sync");
        assert!(
            entries.iter().any(|e| {
                e.fid == 919296 && e.tier == 1 && e.level == 5 && e.shard_count == 10
            }),
            "expected fid=919296 in {:?}",
            entries
        );
    }

    /// stfc-mod queues forbidden/chaos tech with JSON type "tech" (not "ft").
    #[test]
    fn ingress_tech_type_persists_to_forbidden_tech_file() {
        let _guard = SYNC_TEST_LOCK.lock().unwrap();
        let (token, profile_id, _cleanup) = ensure_test_profile();
        let (status, body) = ingress_payload(
            r#"[{"type":"tech","fid":424242,"tier":2,"level":3,"shard_count":0}]"#,
            Some(&token),
        );
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("tech(1)"));
        let ft_path = profile_path(&profile_id, FORBIDDEN_TECH_IMPORTED).to_string_lossy().to_string();
        let entries = import::load_imported_forbidden_tech(&ft_path)
            .expect("forbidden_tech.imported.json should exist after tech sync");
        assert!(
            entries.iter().any(|e| e.fid == 424242 && e.tier == 2 && e.level == 3),
            "expected fid=424242 in {:?}",
            entries
        );
    }
}
