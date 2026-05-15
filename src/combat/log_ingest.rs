//! Raw combat log ingestion for replay/parity with simulator output.
//!
//! See [docs/combat_log_format.md](../../../docs/combat_log_format.md) for the documented format.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::combat::snapshot::CombatStateSnapshot;
use crate::combat::{CombatEvent, EventSource, SimulationResult};

fn default_schema_version() -> u32 {
    1
}

/// Ingested combat log (parsed from raw JSON or export).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestedCombatLog {
    /// Format revision; defaults to `1` for backward compatibility. [`crate::combat::log_validate::validate_canonical_timeline`] applies strict checks when ≥ 2.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub rounds_simulated: u32,
    pub total_damage: f64,
    pub attacker_won: bool,
    #[serde(default)]
    pub winner_by_round_limit: bool,
    pub defender_hull_remaining: f64,
    #[serde(default)]
    pub defender_shield_remaining: f64,
    pub events: Vec<IngestedEvent>,
}

/// Single event from an ingested log (aligns with [`CombatEvent`] for parity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestedEvent {
    pub event_type: String,
    pub round_index: u32,
    pub phase: String,
    #[serde(default)]
    pub values: serde_json::Map<String, Value>,
    /// Sub-round (weapon) index when the simulator uses multi-weapon resolution.
    /// Omitted for round-level events in the exported log format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_index: Option<u32>,
    /// Monotonic timeline position within the log when validating canonical ordering (optional on legacy logs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
    /// Opaque toolbox/client discriminator for correlation (does not imply engine phase equality).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_payload: Option<Value>,
    /// Structured simulator snapshot ([`CombatStateSnapshot`]) for schema_version ≥ 3 tooling.
    /// May be omitted when the same object appears under `values.snapshot` (Kobayashi trace export);
    /// use [`hydrate_ingested_state_snapshots_from_values`] or [`try_event_state_snapshot`] to read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_snapshot: Option<CombatStateSnapshot>,
    /// Optional flat snapshot of observable stats at this step (labels are convention; document keys you emit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats_snapshot: Option<serde_json::Map<String, Value>>,
}

/// Options for [`compare_ingested_trace_to_simulator`].
#[derive(Debug, Clone, Default)]
pub struct TraceCompareOptions {
    /// When set, numeric equality for overlapping keys uses this tolerance.
    pub value_tolerance: Option<f64>,
    /// Keys in `values` to compare when present on both matched events.
    pub compare_value_keys: Vec<String>,
}

/// Parse a combat log from JSON string (format per docs/combat_log_format.md).
pub fn parse_combat_log_json(input: &str) -> Result<IngestedCombatLog, String> {
    serde_json::from_str(input).map_err(|e| e.to_string())
}

/// Copy `values.snapshot` into [`IngestedEvent::state_snapshot`] when the typed field is absent (trace export shape).
pub fn hydrate_ingested_state_snapshots_from_values(log: &mut IngestedCombatLog) {
    for ev in &mut log.events {
        if ev.state_snapshot.is_none() {
            if let Some(v) = ev.values.get("snapshot") {
                if let Ok(s) = serde_json::from_value::<CombatStateSnapshot>(v.clone()) {
                    ev.state_snapshot = Some(s);
                }
            }
        }
    }
}

/// Resolve structured snapshot from [`IngestedEvent::state_snapshot`] or embedded `values.snapshot`.
pub fn try_event_state_snapshot(ev: &IngestedEvent) -> Option<CombatStateSnapshot> {
    ev.state_snapshot.clone().or_else(|| {
        ev.values
            .get("snapshot")
            .and_then(|v| serde_json::from_value::<CombatStateSnapshot>(v.clone()).ok())
    })
}

/// Convert ingested log to a result comparable to SimulationResult (for parity checks).
pub fn ingested_to_comparable(log: &IngestedCombatLog) -> (f64, bool, u32, f64, f64) {
    (
        log.total_damage,
        log.attacker_won,
        log.rounds_simulated,
        log.defender_hull_remaining,
        log.defender_shield_remaining,
    )
}

/// Compare simulator result to ingested log within tolerance (for tests).
pub fn parity_within_tolerance(
    sim: &SimulationResult,
    log: &IngestedCombatLog,
    damage_tol: f64,
    hull_tol: f64,
) -> bool {
    (sim.total_damage - log.total_damage).abs() <= damage_tol
        && sim.attacker_won == log.attacker_won
        && sim.rounds_simulated == log.rounds_simulated
        && (sim.defender_hull_remaining - log.defender_hull_remaining).abs() <= hull_tol
        && (sim.defender_shield_remaining - log.defender_shield_remaining).abs() <= hull_tol
}

/// Structural match between simulator trace row and ingested row (ignores [`EventSource`] and extra ingest-only fields).
pub fn trace_event_matches_skeleton(sim: &CombatEvent, ing: &IngestedEvent) -> bool {
    sim.event_type == ing.event_type
        && sim.round_index == ing.round_index
        && sim.phase == ing.phase
        && sim.weapon_index == ing.weapon_index
}

fn compare_values_if_requested(
    sim: &CombatEvent,
    ing: &IngestedEvent,
    opts: &TraceCompareOptions,
    ing_idx: usize,
    errs: &mut Vec<String>,
) {
    if opts.compare_value_keys.is_empty() {
        return;
    }
    let tol = opts.value_tolerance.unwrap_or(0.0);
    for key in &opts.compare_value_keys {
        let sv = sim.values.get(key).and_then(|v| v.as_f64());
        let iv = ing.values.get(key).and_then(|v| v.as_f64());
        if let (Some(a), Some(b)) = (sv, iv) {
            if (a - b).abs() > tol {
                errs.push(format!(
                    "ingested[{ing_idx}] values[{key}] sim={a} ingested={b} tol={tol}"
                ));
            }
        }
    }
}

/// Ensure `ingested` appears as an ordered subsequence of `sim_events` (same event_type / round_index / phase / weapon_index).
///
/// Extra simulator-only rows (sources, additional trace kinds) are skipped until each ingested row matches.
pub fn compare_ingested_trace_to_simulator(
    sim_events: &[CombatEvent],
    ingested: &[IngestedEvent],
    opts: &TraceCompareOptions,
) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    let mut sim_pos = 0usize;

    for (ing_idx, ing) in ingested.iter().enumerate() {
        let mut matched = false;
        while sim_pos < sim_events.len() {
            let sim = &sim_events[sim_pos];
            sim_pos += 1;
            if trace_event_matches_skeleton(sim, ing) {
                compare_values_if_requested(sim, ing, opts, ing_idx, &mut errs);
                matched = true;
                break;
            }
        }
        if !matched {
            errs.push(format!(
                "ingested[{ing_idx}] {} phase={} round={} weapon={:?}: no matching simulator event",
                ing.event_type, ing.phase, ing.round_index, ing.weapon_index
            ));
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Convert ingested events to engine CombatEvents (same shape for trace comparison).
pub fn ingested_events_to_combat_events(events: &[IngestedEvent]) -> Vec<CombatEvent> {
    events
        .iter()
        .map(|e| CombatEvent {
            event_type: e.event_type.clone(),
            round_index: e.round_index,
            phase: e.phase.clone(),
            source: EventSource::default(),
            values: e.values.clone(),
            weapon_index: e.weapon_index,
        })
        .collect()
}
