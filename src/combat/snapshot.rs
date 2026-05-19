//! Versioned combat state snapshots for trace export and ingest ([`CombatStateSnapshot`]).
//!
//! Emitted as trace rows with `event_type` `state_snapshot` when
//! [`crate::combat::types::SimulationConfig::emit_state_snapshots`] is set with trace enabled.
//! See [docs/combat_log_format.md](../../../docs/combat_log_format.md) schema_version 3.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::combat::abilities::CombatContext;
use crate::combat::effect_accumulator::EffectAccumulator;
use crate::combat::types::{CombatEvent, Combatant, EventSource};

/// Canonical points where the simulator records a full state row (ordering must match [`crate::combat::engine`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotAnchor {
    /// After round-start regen, morale, and bench setup; before weapon sub-rounds.
    AfterRoundStart,
    /// After attack/defense phase effects merged for this weapon; before the outbound hit loop.
    BeforeOutboundShot,
    /// After each outbound `damage_application` (per hit when multi-shot).
    AfterOutboundDamage,
    /// After after-subround crew effects and carry for this weapon index.
    AfterSubround,
    /// After round-end burning, bonus damage, regen, and duration tick-down; aligned with `end_of_round_effects` trace.
    EndOfRoundPostEffects,
}

/// Per-ship resource state at snapshot time (**simulator-sourced** remaining HP, not client).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatantSnapshotResources {
    pub id: String,
    pub hull_remaining: f64,
    pub shield_remaining: f64,
    pub max_hull: f64,
    pub max_shield: f64,
}

/// Gating flags aligned with [`CombatContext`] / round-local counters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatSnapshotFlags {
    pub attacker_morale_active: bool,
    #[serde(default)]
    pub defender_morale_active: bool,
    pub defender_burning_active: bool,
    pub attacker_burning_active: bool,
    pub defender_hull_breach_active: bool,
    pub attacker_hull_breach_active: bool,
    pub assimilated_rounds_remaining: u32,
    pub defender_assimilated_rounds_remaining: u32,
}

/// Structured state snapshot for reverse-engineering and strict ingest (schema_version 3+).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatStateSnapshot {
    pub anchor: SnapshotAnchor,
    pub round_index: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub weapon_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hit_index: Option<u32>,
    pub attacker: CombatantSnapshotResources,
    pub defender: CombatantSnapshotResources,
    /// Cumulative hull damage applied to defender (running; same convention as trace `running_hull_damage`).
    #[serde(default)]
    pub total_defender_hull_damage: f64,
    #[serde(default)]
    pub total_attacker_hull_damage: f64,
    pub flags: CombatSnapshotFlags,
    /// Stacking channels for the attacker shot accumulator when applicable; omitted when empty.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attacker_stacking: Option<Map<String, Value>>,
}

/// Build a snapshot from engine locals.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_combat_state_snapshot(
    anchor: SnapshotAnchor,
    round_index: u32,
    weapon_index: Option<u32>,
    hit_index: Option<u32>,
    attacker: &Combatant,
    defender: &Combatant,
    total_defender_hull_damage: f64,
    total_attacker_hull_damage: f64,
    defender_shield_remaining: f64,
    attacker_shield_remaining: f64,
    ctx: &CombatContext,
    assimilated_rounds_remaining: u32,
    defender_assimilated_rounds_remaining: u32,
    attacker_stacking: Option<&EffectAccumulator>,
) -> CombatStateSnapshot {
    let max_a_hull = attacker.hull_health.max(0.0);
    let max_d_hull = defender.hull_health.max(0.0);
    let max_a_sh = attacker.shield_health.max(0.0);
    let max_d_sh = defender.shield_health.max(0.0);

    let stacking = attacker_stacking.map(|a| a.stacking_summary_for_snapshot());
    let stacking = if stacking.as_ref().is_some_and(|m| m.is_empty()) {
        None
    } else {
        stacking
    };

    CombatStateSnapshot {
        anchor,
        round_index,
        weapon_index,
        hit_index,
        attacker: CombatantSnapshotResources {
            id: attacker.id.clone(),
            hull_remaining: (max_a_hull - total_attacker_hull_damage).max(0.0),
            shield_remaining: attacker_shield_remaining.max(0.0),
            max_hull: max_a_hull,
            max_shield: max_a_sh,
        },
        defender: CombatantSnapshotResources {
            id: defender.id.clone(),
            hull_remaining: (max_d_hull - total_defender_hull_damage).max(0.0),
            shield_remaining: defender_shield_remaining.max(0.0),
            max_hull: max_d_hull,
            max_shield: max_d_sh,
        },
        total_defender_hull_damage,
        total_attacker_hull_damage,
        flags: CombatSnapshotFlags {
            attacker_morale_active: ctx.attacker_morale_active,
            defender_morale_active: ctx.defender_morale_active,
            defender_burning_active: ctx.defender_burning_active,
            attacker_burning_active: ctx.attacker_burning_active,
            defender_hull_breach_active: ctx.defender_hull_breach_active,
            attacker_hull_breach_active: ctx.attacker_hull_breach_active,
            assimilated_rounds_remaining,
            defender_assimilated_rounds_remaining,
        },
        attacker_stacking: stacking,
    }
}

/// Serialize as a [`CombatEvent`] row (`event_type` `state_snapshot`).
pub fn state_snapshot_as_combat_event(snap: &CombatStateSnapshot) -> CombatEvent {
    let payload = serde_json::to_value(snap).expect("CombatStateSnapshot serializes to JSON");
    let mut values = Map::new();
    values.insert("snapshot".to_string(), payload);
    CombatEvent {
        event_type: "state_snapshot".to_string(),
        round_index: snap.round_index,
        phase: "snapshot".to_string(),
        source: EventSource::default(),
        weapon_index: snap.weapon_index,
        values,
    }
}
