//! Canonical timeline validation for ingested combat logs ([`crate::combat::log_ingest::IngestedCombatLog`]).
//!
//! See [docs/combat_log_format.md](../../../docs/combat_log_format.md) for schema versioning and ordering rules.

use crate::combat::log_ingest::{try_event_state_snapshot, IngestedCombatLog, IngestedEvent};
use crate::combat::snapshot::SnapshotAnchor;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineValidationOutcome {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

fn push_issue(strict: bool, errors: &mut Vec<String>, warnings: &mut Vec<String>, msg: String) {
    if strict {
        errors.push(msg);
    } else {
        warnings.push(msg);
    }
}

fn ordered_event_refs(log: &IngestedCombatLog) -> Vec<(usize, &IngestedEvent)> {
    let mut pairs: Vec<(usize, &IngestedEvent)> = log.events.iter().enumerate().collect();
    let all_seq = log.events.iter().all(|e| e.sequence.is_some());
    if all_seq {
        pairs.sort_by_key(|(_, e)| e.sequence.unwrap_or(0));
    }
    pairs
}

/// Validate round structure when [`IngestedCombatLog::schema_version`] ≥ 2 or when any event carries `sequence`.
///
/// - **Strict (`schema_version` ≥ 2):** violations go to [`TimelineValidationOutcome::errors`].
/// - **Lenient (`schema_version` == 1 with sequences):** same checks produce [`TimelineValidationOutcome::warnings`] only.
/// - **`schema_version` ≥ 3:** additionally validates structured [`crate::combat::snapshot::CombatStateSnapshot`] pairing (strict [`TimelineValidationOutcome::errors`] only).
pub fn validate_canonical_timeline(log: &IngestedCombatLog) -> TimelineValidationOutcome {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let should_validate =
        log.schema_version >= 2 || log.events.iter().any(|e| e.sequence.is_some());
    if !should_validate {
        return TimelineValidationOutcome { errors, warnings };
    }

    let strict = log.schema_version >= 2;
    let seq_count = log.events.iter().filter(|e| e.sequence.is_some()).count();

    if seq_count > 0 && seq_count != log.events.len() {
        push_issue(
            strict,
            &mut errors,
            &mut warnings,
            format!(
                "either all events must include sequence or none; got {} of {}",
                seq_count,
                log.events.len()
            ),
        );
    }

    if strict && seq_count == log.events.len() {
        let mut prev: Option<u32> = None;
        for e in &log.events {
            let s = e.sequence.expect("checked seq_count");
            if let Some(p) = prev {
                if s <= p {
                    errors.push(format!(
                        "sequence must be strictly increasing: {s} follows {p}"
                    ));
                }
            }
            prev = Some(s);
        }
    }

    let ordered = ordered_event_refs(log);
    let mut by_round: std::collections::BTreeMap<u32, Vec<&IngestedEvent>> =
        std::collections::BTreeMap::new();
    for (_, ev) in ordered {
        by_round.entry(ev.round_index).or_default().push(ev);
    }

    for (round, evts) in &by_round {
        let mut saw_round_start = false;
        let mut damage_before_rs = false;
        for e in evts.iter().copied() {
            if e.event_type == "round_start" {
                saw_round_start = true;
            } else if e.event_type == "damage_application" && !saw_round_start {
                damage_before_rs = true;
                break;
            }
        }
        if damage_before_rs {
            push_issue(
                strict,
                &mut errors,
                &mut warnings,
                format!("round {round}: damage_application occurs before round_start"),
            );
        }

        let end_positions: Vec<usize> = evts
            .iter()
            .enumerate()
            .filter(|(_, e)| e.event_type == "end_of_round_effects")
            .map(|(i, _)| i)
            .collect();
        if end_positions.is_empty() {
            continue;
        }
        if end_positions.len() > 1 {
            push_issue(
                strict,
                &mut errors,
                &mut warnings,
                format!("round {round}: multiple end_of_round_effects entries"),
            );
        }

        if log.schema_version >= 3 {
            let n = evts.len();
            if n < 2 {
                errors.push(format!(
                    "round {round}: schema_version 3 expects end_of_round_effects then state_snapshot (need at least 2 trailing events)"
                ));
            } else {
                let last = evts[n - 1];
                let prev = evts[n - 2];
                if prev.event_type != "end_of_round_effects" {
                    errors.push(format!(
                        "round {round}: schema_version 3 requires end_of_round_effects immediately before final state_snapshot"
                    ));
                }
                if last.event_type != "state_snapshot" {
                    errors.push(format!(
                        "round {round}: schema_version 3 requires final event to be state_snapshot"
                    ));
                } else if let Some(s) = try_event_state_snapshot(last) {
                    if s.anchor != SnapshotAnchor::EndOfRoundPostEffects {
                        errors.push(format!(
                            "round {round}: closing state_snapshot must use anchor end_of_round_post_effects, got {:?}",
                            s.anchor
                        ));
                    }
                } else {
                    errors.push(format!(
                        "round {round}: closing state_snapshot is not parseable"
                    ));
                }
            }
        } else {
            let last_ix = evts.len().saturating_sub(1);
            if let Some(&first_end) = end_positions.first() {
                if first_end != last_ix {
                    push_issue(
                        strict,
                        &mut errors,
                        &mut warnings,
                        format!(
                        "round {round}: end_of_round_effects must be the last event for that round in timeline order"
                    ),
                    );
                }
            }
        }
    }

    if log.schema_version >= 3 {
        validate_schema_v3_state_snapshots(log, &mut errors);
    }

    TimelineValidationOutcome { errors, warnings }
}

fn validate_schema_v3_state_snapshots(log: &IngestedCombatLog, errors: &mut Vec<String>) {
    let ordered = ordered_event_refs(log);
    for (i, (_, ev)) in ordered.iter().enumerate() {
        if ev.event_type == "state_snapshot" && try_event_state_snapshot(ev).is_none() {
            errors.push(format!(
                "schema_version 3: state_snapshot at index {i} has no parseable snapshot (use values.snapshot or state_snapshot field)"
            ));
        }

        if ev.event_type == "damage_application" && ev.phase == "damage" {
            match ordered.get(i + 1) {
                Some((_, next)) => {
                    if next.event_type != "state_snapshot" {
                        errors.push(format!(
                            "schema_version 3: damage_application at index {i} must be immediately followed by state_snapshot, got {}",
                            next.event_type
                        ));
                    } else if let Some(s) = try_event_state_snapshot(next) {
                        if s.anchor != SnapshotAnchor::AfterOutboundDamage {
                            errors.push(format!(
                                "schema_version 3: snapshot after damage_application must use anchor after_outbound_damage, got {:?}",
                                s.anchor
                            ));
                        }
                    } else {
                        errors.push(format!(
                            "schema_version 3: snapshot after damage_application at index {} is unparsable",
                            i + 1
                        ));
                    }
                }
                None => errors.push(format!(
                    "schema_version 3: damage_application at index {i} has no following state_snapshot"
                )),
            }
        }

        if ev.event_type == "end_of_round_effects" {
            match ordered.get(i + 1) {
                Some((_, next)) => {
                    if next.event_type != "state_snapshot" {
                        errors.push(format!(
                            "schema_version 3: end_of_round_effects at index {i} must be immediately followed by state_snapshot, got {}",
                            next.event_type
                        ));
                    } else if let Some(s) = try_event_state_snapshot(next) {
                        if s.anchor != SnapshotAnchor::EndOfRoundPostEffects {
                            errors.push(format!(
                                "schema_version 3: snapshot after end_of_round_effects must use anchor end_of_round_post_effects, got {:?}",
                                s.anchor
                            ));
                        }
                    } else {
                        errors.push(format!(
                            "schema_version 3: snapshot after end_of_round_effects at index {} is unparsable",
                            i + 1
                        ));
                    }
                }
                None => errors.push(format!(
                    "schema_version 3: end_of_round_effects at index {i} has no following state_snapshot"
                )),
            }
        }
    }
}
