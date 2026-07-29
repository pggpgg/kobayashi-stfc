//! Append-only JSON lines for post-hoc analysis of optimize budgets (`KOBAYASHI_BUDGET_TELEMETRY`).

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::data::profile_index::{profile_path, BUDGET_TELEMETRY_JSONL};

/// One optimize completion row (stable field names for log parsers).
#[derive(Debug, Serialize)]
pub struct BudgetTelemetryRow<'a> {
    pub ts_ms: u128,
    pub ship: &'a str,
    pub hostile: &'a str,
    pub strategy: &'a str,
    pub result_crews: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_role_captains: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_role_bridge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_role_below_decks: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banned_role_captains: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banned_role_bridge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banned_role_below_decks: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligible_role_captains: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligible_role_bridge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligible_role_below_decks: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roster_role_captains: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roster_role_bridge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roster_role_below_decks: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_role_captains: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_role_bridge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_role_below_decks: Option<u32>,
    pub heuristic_candidates: u32,
    pub warm_start_candidates: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_candidates: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_warm_start_dedupe: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_constraints: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytical_prefilter_from: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytical_prefilter_kept: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scout_candidates: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_candidates: Option<u32>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub phase_durations_ms: BTreeMap<String, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_scout_trials_final: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_scout_trials_executed_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_confirm_trials_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exhaustive_scout_trials_final: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exhaustive_scout_trials_executed_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exhaustive_confirm_trials_total: Option<u64>,
    pub optimize_history_confirm_hits: u32,
    pub optimize_history_wrote: bool,
}

fn telemetry_enabled() -> bool {
    matches!(
        std::env::var("KOBAYASHI_BUDGET_TELEMETRY")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true")),
        Ok(true)
    )
}

fn resolve_log_path(profile_id: Option<&str>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KOBAYASHI_BUDGET_TELEMETRY_PATH") {
        let t = p.trim();
        if !t.is_empty() {
            return Some(PathBuf::from(t));
        }
    }
    let pid = profile_id?.trim();
    if pid.is_empty() {
        return None;
    }
    Some(profile_path(pid, BUDGET_TELEMETRY_JSONL))
}

/// When `KOBAYASHI_BUDGET_TELEMETRY=1` and a log path resolves (env path or profile `budget_telemetry.jsonl`), append one JSON line.
pub fn maybe_append_row<'a>(profile_id: Option<&str>, row: &BudgetTelemetryRow<'a>) {
    if !telemetry_enabled() {
        return;
    }
    let Some(path) = resolve_log_path(profile_id) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let line = match serde_json::to_string(row) {
        Ok(s) => s,
        Err(_) => return,
    };
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(f, "{line}");
    drop(f);
    // Same unbounded-append hazard as the observation log; same bounded rotation.
    crate::data::optimize_observations::rotate_if_oversized(
        &path,
        crate::data::optimize_observations::MAX_OPTIMIZE_OBSERVATIONS_BYTES,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_serializes() {
        let row = BudgetTelemetryRow {
            ts_ms: 1,
            ship: "s",
            hostile: "h",
            strategy: "tiered",
            result_crews: 3,
            raw_role_captains: Some(10),
            raw_role_bridge: Some(11),
            raw_role_below_decks: Some(12),
            banned_role_captains: Some(9),
            banned_role_bridge: Some(10),
            banned_role_below_decks: Some(11),
            eligible_role_captains: Some(8),
            eligible_role_bridge: Some(9),
            eligible_role_below_decks: Some(10),
            roster_role_captains: Some(7),
            roster_role_bridge: Some(8),
            roster_role_below_decks: Some(9),
            final_role_captains: Some(4),
            final_role_bridge: Some(5),
            final_role_below_decks: Some(6),
            heuristic_candidates: 2,
            warm_start_candidates: 1,
            generated_candidates: Some(20),
            after_warm_start_dedupe: Some(21),
            after_constraints: Some(18),
            analytical_prefilter_from: Some(18),
            analytical_prefilter_kept: Some(6),
            scout_candidates: Some(6),
            confirmed_candidates: Some(3),
            phase_durations_ms: BTreeMap::from([("tiered".to_string(), 42)]),
            tiered_scout_trials_final: Some(100),
            tiered_scout_trials_executed_total: Some(110),
            tiered_confirm_trials_total: Some(50),
            exhaustive_scout_trials_final: None,
            exhaustive_scout_trials_executed_total: None,
            exhaustive_confirm_trials_total: None,
            optimize_history_confirm_hits: 0,
            optimize_history_wrote: false,
        };
        let s = serde_json::to_string(&row).expect("json");
        assert!(s.contains("\"ship\":\"s\""));
    }
}
