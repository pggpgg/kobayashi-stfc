//! Append-only per-crew optimizer observations (`KOBAYASHI_OPTIMIZE_OBSERVATIONS`).
//!
//! This is intentionally separate from `optimize_history`: history is an online cache for current
//! optimizer behavior, while observations are a durable training/evaluation substrate for future
//! surrogate models and optimizer benchmark analysis.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::data::profile_index::{profile_path, OPTIMIZE_OBSERVATIONS_JSONL};

/// One final ranked crew observation from an optimize run.
#[derive(Debug, Serialize)]
pub struct OptimizeObservationRow<'a> {
    pub schema_version: u8,
    pub ts_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_hash: Option<u64>,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_serializes_core_identity_and_metrics() {
        let bridge = vec!["B".to_string(), "C".to_string()];
        let below_decks = vec!["D".to_string()];
        let row = OptimizeObservationRow {
            schema_version: 1,
            ts_ms: 1,
            profile_id: Some("demo"),
            profile_hash: Some(stable_text_hash("demo")),
            simulator_version: "0.1.0",
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
    }
}
