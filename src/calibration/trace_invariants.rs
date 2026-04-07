//! Structural invariants for combat traces ([`crate::combat::TraceMode::Events`]).
//!
//! [`check_trace_invariants`] validates stream ordering, numeric sanity on known [`CombatEvent`]
//! kinds, mitigation multiplier consistency with documented engine behavior, and contiguity of
//! per-shot `hit_index` groups. It does **not** assert parity with client combat logs or golden
//! damage totals — use drift bands and recorded-fight calibration for numeric regression.
//!
//! Use [`TraceInvariantContext`] to pass the simulation round cap and optional stricter checks
//! (e.g. monotonic defender `running_hull_damage` when hull regen is impossible).

use std::collections::HashMap;

use serde_json::Value;

use crate::combat::CombatEvent;

/// Single failed invariant with the index of the offending event in the trace slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceInvariantViolation {
    pub event_index: usize,
    pub code: &'static str,
    pub detail: String,
}

/// Inputs for [`check_trace_invariants`].
#[derive(Debug, Clone, Copy)]
pub struct TraceInvariantContext {
    /// Upper bound from [`crate::combat::SimulationConfig::rounds`]; no event may exceed this round.
    pub max_config_rounds: u32,
    /// When true, `running_hull_damage` on each `damage_application` must not decrease. Do not
    /// enable for fights where defender hull regen can reduce cumulative hull damage.
    pub expect_monotonic_defender_running_hull_damage: bool,
}

impl Default for TraceInvariantContext {
    fn default() -> Self {
        Self {
            max_config_rounds: u32::MAX,
            expect_monotonic_defender_running_hull_damage: false,
        }
    }
}

const MITIGATION_MULT_TOL: f64 = 1e-5;

fn record(
    errs: &mut Vec<TraceInvariantViolation>,
    event_index: usize,
    code: &'static str,
    detail: impl Into<String>,
) {
    errs.push(TraceInvariantViolation {
        event_index,
        code,
        detail: detail.into(),
    });
}

fn optional_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

fn optional_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().and_then(|x| (x >= 0).then_some(x as u64)))
}

fn require_finite_nonneg(
    errs: &mut Vec<TraceInvariantViolation>,
    event_index: usize,
    code: &'static str,
    name: &str,
    x: f64,
) {
    if !x.is_finite() || x.is_sign_negative() {
        record(
            errs,
            event_index,
            code,
            format!("{name} must be finite and >= 0, got {x}"),
        );
    }
}

fn check_damage_application(
    errs: &mut Vec<TraceInvariantViolation>,
    event_index: usize,
    values: &serde_json::Map<String, Value>,
) {
    for key in [
        "damage_after_apex",
        "shield_damage",
        "hull_damage",
        "running_hull_damage",
        "defender_shield_remaining",
    ] {
        if let Some(v) = values.get(key) {
            if let Some(x) = optional_f64(v) {
                require_finite_nonneg(errs, event_index, "damage_numeric", key, x);
            } else {
                record(
                    errs,
                    event_index,
                    "damage_type",
                    format!("{key} must be a JSON number"),
                );
            }
        }
    }
    if let Some(v) = values.get("shield_mitigation") {
        if let Some(s) = optional_f64(v) {
            if !s.is_finite() || !(0.0..=1.0).contains(&s) {
                record(
                    errs,
                    event_index,
                    "shield_mitigation_range",
                    format!("shield_mitigation must be finite in [0,1], got {s}"),
                );
            }
        } else {
            record(
                errs,
                event_index,
                "shield_mitigation_type",
                "shield_mitigation must be a JSON number",
            );
        }
    }
}

fn check_crit_resolution(
    errs: &mut Vec<TraceInvariantViolation>,
    event_index: usize,
    values: &serde_json::Map<String, Value>,
) {
    for key in ["roll", "effective_crit_chance"] {
        if let Some(v) = values.get(key) {
            if let Some(x) = optional_f64(v) {
                if !x.is_finite() || !(0.0..=1.0).contains(&x) {
                    record(
                        errs,
                        event_index,
                        "crit_range",
                        format!("{key} must be finite in [0,1], got {x}"),
                    );
                }
            } else {
                record(
                    errs,
                    event_index,
                    "crit_type",
                    format!("{key} must be a JSON number"),
                );
            }
        }
    }
}

fn check_mitigation_calc(
    errs: &mut Vec<TraceInvariantViolation>,
    event_index: usize,
    values: &serde_json::Map<String, Value>,
) {
    let mit = values.get("mitigation").and_then(optional_f64);
    let mult = values.get("multiplier").and_then(optional_f64);
    if let Some(m) = mit {
        if !m.is_finite() {
            record(
                errs,
                event_index,
                "mitigation_finite",
                format!("mitigation must be finite, got {m}"),
            );
        }
    }
    if let Some(m) = mult {
        if !m.is_finite() {
            record(
                errs,
                event_index,
                "multiplier_finite",
                format!("multiplier must be finite, got {m}"),
            );
        }
    }
    if let (Some(mitigation), Some(multiplier)) = (mit, mult) {
        let expected = (1.0 - mitigation).max(0.0);
        if (multiplier - expected).abs() > MITIGATION_MULT_TOL {
            record(
                errs,
                event_index,
                "mitigation_multiplier_consistency",
                format!(
                    "multiplier {multiplier} inconsistent with mitigation {mitigation} (expected {expected} ≈ (1-mitigation).max(0))"
                ),
            );
        }
    }
}

fn finalize_hit_groups(
    errs: &mut Vec<TraceInvariantViolation>,
    hit_groups: HashMap<(String, u32, u32), Vec<u64>>,
) {
    for ((event_type, round_index, weapon_index), hits) in hit_groups {
        for (j, &h) in hits.iter().enumerate() {
            if h != j as u64 {
                record(
                    errs,
                    0,
                    "hit_index_sequence",
                    format!(
                        "event_type={event_type} round={round_index} weapon={weapon_index}: expected hit_index {j}, got {h}"
                    ),
                );
            }
        }
    }
}

/// Validate `events`. On failure returns all violations (event index refers to the trace slice).
pub fn check_trace_invariants(
    events: &[CombatEvent],
    ctx: &TraceInvariantContext,
) -> Result<(), Vec<TraceInvariantViolation>> {
    let mut errs = Vec::new();
    let mut last_round: Option<u32> = None;
    let mut last_running_hull: Option<f64> = None;
    let mut hit_groups: HashMap<(String, u32, u32), Vec<u64>> = HashMap::new();

    for (i, ev) in events.iter().enumerate() {
        if ev.round_index < 1 {
            record(
                &mut errs,
                i,
                "round_low",
                format!("round_index {} < 1", ev.round_index),
            );
        }
        if ev.round_index > ctx.max_config_rounds {
            record(
                &mut errs,
                i,
                "round_high",
                format!(
                    "round_index {} > max_config_rounds {}",
                    ev.round_index, ctx.max_config_rounds
                ),
            );
        }
        if let Some(lr) = last_round {
            if ev.round_index < lr {
                record(
                    &mut errs,
                    i,
                    "round_order",
                    format!("round_index {} decreased after {}", ev.round_index, lr),
                );
            }
        }
        last_round = Some(ev.round_index);

        match ev.event_type.as_str() {
            "damage_application" => {
                check_damage_application(&mut errs, i, &ev.values);
                if ctx.expect_monotonic_defender_running_hull_damage {
                    match ev.values.get("running_hull_damage").and_then(optional_f64) {
                        Some(rh) => {
                            if !rh.is_finite() || rh < 0.0 {
                                record(
                                    &mut errs,
                                    i,
                                    "running_hull_monotone",
                                    format!("running_hull_damage invalid: {rh}"),
                                );
                            } else if let Some(prev) = last_running_hull {
                                if rh + 1e-9 < prev {
                                    record(
                                        &mut errs,
                                        i,
                                        "running_hull_monotone",
                                        format!(
                                            "running_hull_damage decreased from {prev} to {rh}"
                                        ),
                                    );
                                }
                            }
                            last_running_hull = Some(rh);
                        }
                        None => record(
                            &mut errs,
                            i,
                            "running_hull_monotone",
                            "expect_monotonic_defender_running_hull_damage: missing running_hull_damage",
                        ),
                    }
                }
            }
            "crit_resolution" => check_crit_resolution(&mut errs, i, &ev.values),
            "mitigation_calc" => check_mitigation_calc(&mut errs, i, &ev.values),
            _ => {}
        }

        if let Some(hv) = ev.values.get("hit_index") {
            match optional_u64(hv) {
                Some(h) => match ev.weapon_index {
                    Some(wi) => {
                        hit_groups
                            .entry((ev.event_type.clone(), ev.round_index, wi))
                            .or_default()
                            .push(h);
                    }
                    None => record(
                        &mut errs,
                        i,
                        "hit_index_weapon",
                        "hit_index present but weapon_index is None",
                    ),
                },
                None => record(
                    &mut errs,
                    i,
                    "hit_index_type",
                    "hit_index must be a non-negative integer",
                ),
            }
        }
    }

    finalize_hit_groups(&mut errs, hit_groups);

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}
