//! Append-only per-crew optimizer observations (`KOBAYASHI_OPTIMIZE_OBSERVATIONS`).
//!
//! This is intentionally separate from `optimize_history`: history is an online cache for current
//! optimizer behavior, while observations are a durable training/evaluation substrate for future
//! surrogate models and optimizer benchmark analysis.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::profile_index::{profile_path, OPTIMIZE_OBSERVATIONS_JSONL};

/// Current row shape. `1` predates the reuse fingerprint, where `profile_hash` was a hash of the
/// profile **id** and `simulator_version` was a constant crate version.
pub const OPTIMIZE_OBSERVATION_SCHEMA_VERSION: u8 = 2;

/// Rotate once the live log passes this size, so an always-on observation run cannot grow without
/// bound. Override with `KOBAYASHI_OPTIMIZE_OBSERVATIONS_MAX_BYTES`.
pub const MAX_OPTIMIZE_OBSERVATIONS_BYTES: u64 = 32 * 1024 * 1024;
/// Generations kept beside the live file (`.1`, `.2`), so disk use is bounded at ~3× the cap.
pub const OPTIMIZE_OBSERVATIONS_ROTATIONS: usize = 2;

/// One final ranked crew observation from an optimize run.
#[derive(Debug, Serialize)]
pub struct OptimizeObservationRow<'a> {
    pub schema_version: u8,
    pub ts_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<&'a str>,
    /// Digest of the profile's **contents** (the `profile` segment of
    /// [`crate::data::optimize_fingerprint::ReuseFingerprint`]). Schema 1 rows carry a hash of the
    /// profile *id* here instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_hash: Option<u64>,
    /// Full reuse fingerprint of the run (`schema:engine:data:profile:scenario`), so an observation
    /// can be matched against — or excluded from — a later build's data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reuse_fingerprint: Option<&'a str>,
    pub simulator_version: &'a str,
    pub ship: &'a str,
    pub hostile: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_tier: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_level: Option<u32>,
    pub below_decks_slots: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enemy_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_buffs: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defender_support_buffs: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defender_alliance_debuffs: Option<&'a [String]>,
    pub chain_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_kills_target: Option<u32>,
    pub seed: u64,
    pub sims_requested: u32,
    pub trials_run: usize,
    pub strategy: &'a str,
    pub method_provenance: &'a str,
    pub crew_hash: u64,
    pub captain: &'a str,
    pub bridge: &'a [String],
    pub below_decks: &'a [String],
    pub win_rate: f64,
    pub win_rate_ci_low: f64,
    pub win_rate_ci_high: f64,
    pub stall_rate: f64,
    pub stall_rate_ci_low: f64,
    pub stall_rate_ci_high: f64,
    pub loss_rate: f64,
    pub loss_rate_ci_low: f64,
    pub loss_rate_ci_high: f64,
    pub r1_kill_rate: f64,
    pub r1_kill_rate_ci_low: f64,
    pub r1_kill_rate_ci_high: f64,
    pub avg_hull_remaining: f64,
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
    pub avg_defender_hull_remaining: f64,
    pub avg_defender_hull_remaining_ci_low: f64,
    pub avg_defender_hull_remaining_ci_high: f64,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_hull_damage: Option<f64>,
}

/// Deterministic FNV-1a fingerprint for grouping observations without depending on Rust hasher
/// internals. This is not meant to be cryptographic.
pub fn stable_text_hash(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in value.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn observations_enabled() -> bool {
    matches!(
        std::env::var("KOBAYASHI_OPTIMIZE_OBSERVATIONS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true")),
        Ok(true)
    )
}

/// Path of the live observation log for a profile, or the `KOBAYASHI_OPTIMIZE_OBSERVATIONS_PATH`
/// override. Public so the inspector CLI reads exactly what the writer writes.
pub fn observations_log_path(profile_id: Option<&str>) -> Option<PathBuf> {
    resolve_log_path(profile_id)
}

fn resolve_log_path(profile_id: Option<&str>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KOBAYASHI_OPTIMIZE_OBSERVATIONS_PATH") {
        let t = p.trim();
        if !t.is_empty() {
            return Some(PathBuf::from(t));
        }
    }
    let pid = profile_id?.trim();
    if pid.is_empty() {
        return None;
    }
    Some(profile_path(pid, OPTIMIZE_OBSERVATIONS_JSONL))
}

/// When `KOBAYASHI_OPTIMIZE_OBSERVATIONS=1`, append ranked crew observations as JSON lines.
pub fn maybe_append_rows<'a>(profile_id: Option<&str>, rows: &[OptimizeObservationRow<'a>]) {
    if rows.is_empty() || !observations_enabled() {
        return;
    }
    let Some(path) = resolve_log_path(profile_id) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    for row in rows {
        let Ok(line) = serde_json::to_string(row) else {
            continue;
        };
        let _ = writeln!(f, "{line}");
    }
    drop(f);
    rotate_if_oversized(&path, max_observations_bytes());
}

fn max_observations_bytes() -> u64 {
    std::env::var("KOBAYASHI_OPTIMIZE_OBSERVATIONS_MAX_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(MAX_OPTIMIZE_OBSERVATIONS_BYTES)
}

/// Shift `X.jsonl` → `X.jsonl.1` → `X.jsonl.2` (dropping the oldest) once the live file exceeds
/// `cap`. Two renames and no rewriting, so appending stays cheap; a row-count cap would mean
/// re-reading the whole file on every append.
pub fn rotate_if_oversized(path: &Path, cap: u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() <= cap {
        return;
    }
    let rotated_name = |generation: usize| {
        let mut name = path.as_os_str().to_os_string();
        name.push(format!(".{generation}"));
        PathBuf::from(name)
    };
    for generation in (1..OPTIMIZE_OBSERVATIONS_ROTATIONS).rev() {
        let _ = std::fs::rename(rotated_name(generation), rotated_name(generation + 1));
    }
    let _ = std::fs::rename(path, rotated_name(1));
}

/// Owned mirror of [`OptimizeObservationRow`] for reading a log back. Every field defaults so both
/// schema 1 and 2 rows deserialize.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OptimizeObservationRecord {
    pub schema_version: u8,
    pub ts_ms: u128,
    pub profile_id: Option<String>,
    pub profile_hash: Option<u64>,
    pub reuse_fingerprint: Option<String>,
    pub simulator_version: Option<String>,
    pub ship: String,
    pub hostile: String,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub enemy_type: Option<String>,
    pub seed: u64,
    pub sims_requested: u32,
    pub trials_run: usize,
    pub strategy: String,
    pub method_provenance: String,
    pub crew_hash: u64,
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub win_rate: f64,
    pub avg_hull_remaining: f64,
    pub score: f32,
}

/// Read observation rows from `path`, newest last. Unparseable lines are skipped rather than failing
/// the whole read, so a partially written tail cannot make the log unreadable.
pub fn read_observation_records(
    path: &Path,
    limit: Option<usize>,
) -> std::io::Result<Vec<OptimizeObservationRecord>> {
    let file = std::fs::File::open(path)?;
    let mut rows: Vec<OptimizeObservationRecord> = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    if let Some(limit) = limit {
        if rows.len() > limit {
            rows = rows.split_off(rows.len() - limit);
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_serializes_core_identity_and_metrics() {
        let bridge = vec!["B".to_string(), "C".to_string()];
        let below_decks = vec!["D".to_string()];
        let row = OptimizeObservationRow {
            schema_version: OPTIMIZE_OBSERVATION_SCHEMA_VERSION,
            ts_ms: 1,
            profile_id: Some("demo"),
            profile_hash: Some(stable_text_hash("demo")),
            reuse_fingerprint: Some("1:aaaa:bbbb:cccc:dddd"),
            simulator_version: "engine-0000000000000001+0.1.0",
            ship: "saladin",
            hostile: "2918121098",
            ship_tier: Some(3),
            ship_level: Some(15),
            below_decks_slots: 1,
            enemy_type: Some("red_moving_space"),
            support_buffs: None,
            defender_support_buffs: None,
            defender_alliance_debuffs: None,
            chain_enabled: false,
            chain_kills_target: None,
            seed: 7,
            sims_requested: 500,
            trials_run: 500,
            strategy: "tiered",
            method_provenance: "tiered_confirmed",
            crew_hash: 42,
            captain: "A",
            bridge: &bridge,
            below_decks: &below_decks,
            win_rate: 0.75,
            win_rate_ci_low: 0.7,
            win_rate_ci_high: 0.8,
            stall_rate: 0.1,
            stall_rate_ci_low: 0.08,
            stall_rate_ci_high: 0.12,
            loss_rate: 0.15,
            loss_rate_ci_low: 0.12,
            loss_rate_ci_high: 0.18,
            r1_kill_rate: 0.2,
            r1_kill_rate_ci_low: 0.16,
            r1_kill_rate_ci_high: 0.24,
            avg_hull_remaining: 0.4,
            avg_hull_remaining_ci_low: 0.35,
            avg_hull_remaining_ci_high: 0.45,
            avg_defender_hull_remaining: 0.05,
            avg_defender_hull_remaining_ci_low: 0.03,
            avg_defender_hull_remaining_ci_high: 0.07,
            score: 0.75,
            expected_hull_damage: None,
        };
        let s = serde_json::to_string(&row).expect("json");
        assert!(s.contains("\"method_provenance\":\"tiered_confirmed\""));
        assert!(s.contains("\"crew_hash\":42"));
        assert!(s.contains("\"reuse_fingerprint\":\"1:aaaa:bbbb:cccc:dddd\""));
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kobayashi_obs_{name}_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Schema-1 rows (no `reuse_fingerprint`, fewer fields) must still deserialize.
    #[test]
    fn reader_tolerates_both_schemas_and_skips_junk() {
        let dir = scratch_dir("reader");
        let path = dir.join("obs.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"schema_version":1,"ts_ms":1,"ship":"a","hostile":"h","strategy":"tiered","method_provenance":"tiered_confirmed","crew_hash":1,"captain":"C","win_rate":0.5}"#,
                "\n",
                "not json at all\n",
                "\n",
                r#"{"schema_version":2,"ts_ms":2,"ship":"b","hostile":"h","strategy":"genetic","method_provenance":"genetic","crew_hash":2,"captain":"D","win_rate":0.9,"reuse_fingerprint":"1:a:b:c:d"}"#,
                "\n",
            ),
        )
        .expect("write");
        let rows = read_observation_records(&path, None).expect("read");
        assert_eq!(rows.len(), 2, "junk line should be skipped, not fatal");
        assert_eq!(rows[0].schema_version, 1);
        assert!(rows[0].reuse_fingerprint.is_none());
        assert_eq!(rows[1].reuse_fingerprint.as_deref(), Some("1:a:b:c:d"));

        let tail = read_observation_records(&path, Some(1)).expect("read");
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].ship, "b", "limit keeps the newest rows");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_keeps_bounded_generations() {
        let dir = scratch_dir("rotate");
        let path = dir.join("obs.jsonl");
        std::fs::write(&path, "0123456789").expect("write");

        rotate_if_oversized(&path, 100);
        assert!(path.exists(), "under the cap: nothing moves");
        assert!(!dir.join("obs.jsonl.1").exists());

        rotate_if_oversized(&path, 4);
        assert!(!path.exists(), "over the cap: live file rotates away");
        assert!(dir.join("obs.jsonl.1").exists());

        std::fs::write(&path, "abcdefghij").expect("write");
        rotate_if_oversized(&path, 4);
        assert!(dir.join("obs.jsonl.1").exists());
        assert!(dir.join("obs.jsonl.2").exists());

        std::fs::write(&path, "klmnopqrst").expect("write");
        rotate_if_oversized(&path, 4);
        assert!(
            !dir.join("obs.jsonl.3").exists(),
            "generations stay bounded at {OPTIMIZE_OBSERVATIONS_ROTATIONS}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
