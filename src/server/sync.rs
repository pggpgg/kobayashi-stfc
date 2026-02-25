//! Sync ingress for STFC Community Mod: accepts type-specific JSON arrays and
//! updates roster (and optionally other state) for quasi real-time optimizer use.

use crate::data::import;
use crate::server::routes::HttpResponse;
use chrono::TimeZone;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// Default path for game officer id -> canonical_officer_id mapping (same as id_registry).
pub const DEFAULT_GAME_ID_MAP_PATH: &str = "data/officers/id_registry.json";

static SYNC_ROSTER_MTX: Mutex<()> = Mutex::new(());

/// Handles POST /api/sync/ingress: validates token, parses body, dispatches by type.
pub fn ingress_payload(body: &str, sync_token: Option<&str>) -> HttpResponse {
    let expected_token = std::env::var("KOBAYASHI_SYNC_TOKEN").ok();
    if let Some(ref expected) = expected_token {
        let provided = sync_token.unwrap_or("").trim();
        if provided != expected.as_str() {
            return json_error_response(401, "Unauthorized", "Invalid or missing stfc-sync-token");
        }
    }

    let payload: Vec<serde_json::Value> = match serde_json::from_str(body) {
        Ok(arr) => arr,
        Err(e) => {
            return json_error_response(
                400,
                "Bad Request",
                &format!("Request body must be a JSON array: {e}"),
            );
        }
    };

    if payload.is_empty() {
        return ok_accepted_response(&[]);
    }

    let first = &payload[0];
    let type_str = first
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let type_lower = type_str.to_ascii_lowercase();

    let accepted = match type_lower.as_str() {
        "officer" => {
            match apply_officer_sync(&payload, DEFAULT_GAME_ID_MAP_PATH, import::DEFAULT_IMPORT_OUTPUT_PATH) {
                Ok(accepted_count) => {
                    vec![format!("officer({accepted_count})")]
                }
                Err(e) => {
                    return json_error_response(500, "Internal Server Error", &e.to_string());
                }
            }
        }
        "research" | "buildings" | "ships" | "resources" | "missions" | "battlelogs"
        | "traits" | "tech" | "slots" | "buffs" | "inventory" | "jobs" => {
            vec![type_str.to_string()]
        }
        _ => vec![type_str.to_string()],
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

fn ok_accepted_response(accepted: &[String]) -> HttpResponse {
    let body = serde_json::json!({
        "status": "ok",
        "accepted": accepted,
    });
    let body_str = serde_json::to_string_pretty(&body).unwrap_or_else(|_| r#"{"status":"ok","accepted":[]}"#.to_string());
    HttpResponse {
        status_code: 200,
        status_text: "OK",
        content_type: "application/json",
        body: body_str,
    }
}

fn json_error_response(status_code: u16, status_text: &'static str, message: &str) -> HttpResponse {
    let body = serde_json::json!({
        "status": "error",
        "message": message,
    });
    let body_str = serde_json::to_string_pretty(&body).unwrap_or_else(|_| format!(r#"{{"status":"error","message":{}}}"#, serde_json::to_string(message).unwrap_or_default()));
    HttpResponse {
        status_code,
        status_text,
        content_type: "application/json",
        body: body_str,
    }
}

/// Handles GET /api/sync/status: returns roster path and last modified time (ISO8601) or null if missing.
pub fn sync_status_payload() -> HttpResponse {
    let roster_path = import::DEFAULT_IMPORT_OUTPUT_PATH;
    let last_modified_iso: Option<String> = std::fs::metadata(roster_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| {
            t.duration_since(UNIX_EPOCH).ok().and_then(|d| {
                chrono::Utc
                    .timestamp_opt(d.as_secs() as i64, d.subsec_nanos())
                    .single()
                    .map(|dt| dt.to_rfc3339())
            })
        });
    let body = serde_json::json!({
        "roster_path": roster_path,
        "last_modified_iso": last_modified_iso,
    });
    let body_str = serde_json::to_string_pretty(&body).unwrap_or_else(|_| r#"{"roster_path":"rosters/roster.imported.json","last_modified_iso":null}"#.to_string());
    HttpResponse {
        status_code: 200,
        status_text: "OK",
        content_type: "application/json",
        body: body_str,
    }
}

#[cfg(test)]
mod tests {
    use super::ingress_payload;

    #[test]
    fn ingress_empty_array_returns_200_and_accepted() {
        let r = ingress_payload("[]", None);
        assert_eq!(r.status_code, 200);
        assert!(r.body.contains("\"status\": \"ok\""));
        assert!(r.body.contains("\"accepted\""));
    }

    #[test]
    fn ingress_non_array_body_returns_400() {
        let r = ingress_payload("{}", None);
        assert_eq!(r.status_code, 400);
        assert!(r.body.contains("array"));
    }

    #[test]
    fn ingress_unknown_type_returns_200_and_accepts_type() {
        let r = ingress_payload(r#"[{"type":"unknown","x":1}]"#, None);
        assert_eq!(r.status_code, 200);
        assert!(r.body.contains("unknown"));
    }

    #[test]
    fn ingress_research_type_returns_200() {
        let r = ingress_payload(r#"[{"type":"research","rid":"r1","level":1}]"#, None);
        assert_eq!(r.status_code, 200);
        assert!(r.body.contains("research"));
    }
}
