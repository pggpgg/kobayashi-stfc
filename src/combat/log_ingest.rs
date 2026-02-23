//! Raw combat log ingestion for replay/parity with simulator output.
//!
//! See [docs/combat_log_format.md](../../../docs/combat_log_format.md) for the documented format.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::combat::{CombatEvent, EventSource, SimulationResult};

/// Ingested combat log (parsed from raw JSON or export).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestedCombatLog {
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

/// Single event from an ingested log (aligns with CombatEvent for parity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestedEvent {
    pub event_type: String,
    pub round_index: u32,
    pub phase: String,
    #[serde(default)]
    pub values: serde_json::Map<String, Value>,
}

/// Parse a combat log from JSON string (format per docs/combat_log_format.md).
pub fn parse_combat_log_json(input: &str) -> Result<IngestedCombatLog, String> {
    serde_json::from_str(input).map_err(|e| e.to_string())
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
        })
        .collect()
}
