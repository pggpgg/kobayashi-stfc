//! Per-shot damage math helpers for the combat loop.

use crate::combat::mitigation::isolytic_damage;
use crate::combat::types::{EPSILON, HULL_BREACH_CRIT_BONUS};

/// Damage-through factor: (1 - mitigation) + pierce + defense_mitigation_bonus, clamped to >= 0.
/// Public for explainability tooling ([`crate::combat::mitigation_sensitivity`]).
#[inline]
pub fn compute_damage_through_factor(
    mitigation_multiplier: f64,
    effective_pierce: f64,
    defense_mitigation_bonus: f64,
) -> f64 {
    (mitigation_multiplier + effective_pierce + defense_mitigation_bonus).max(0.0)
}

/// Critical multiplier: base_crit_multiplier, or base * HULL_BREACH_CRIT_BONUS when hull_breach_active.
#[inline]
pub(crate) fn compute_crit_multiplier(
    is_crit: bool,
    base_crit_multiplier: f64,
    hull_breach_active: bool,
) -> f64 {
    if is_crit {
        if hull_breach_active {
            base_crit_multiplier * HULL_BREACH_CRIT_BONUS
        } else {
            base_crit_multiplier
        }
    } else {
        1.0
    }
}

/// Apex damage factor: 10000 / (10000 + effective_barrier), where barrier is adjusted by shred.
#[inline]
pub fn compute_apex_damage_factor(
    effective_apex_shred: f64,
    effective_apex_barrier: f64,
) -> f64 {
    let effective_barrier =
        effective_apex_barrier / (1.0 + effective_apex_shred).max(EPSILON);
    10000.0 / (10000.0 + effective_barrier)
}

/// Isolytic taken from standard damage: isolytic_damage(...) / (1 + isolytic_defense).
#[inline]
pub fn compute_isolytic_taken(
    damage: f64,
    effective_isolytic_damage: f64,
    effective_isolytic_defense: f64,
    effective_isolytic_cascade: f64,
) -> f64 {
    let isolytic_component =
        isolytic_damage(damage, effective_isolytic_damage, effective_isolytic_cascade);
    isolytic_component / (1.0 + effective_isolytic_defense)
}

/// Shield/hull split: returns (actual_shield_damage, hull_damage_this_round).
/// When shield_remaining is 0, shield_mitigation is treated as 0 (all damage to hull).
#[inline]
pub fn apply_shield_hull_split(
    damage_after_apex: f64,
    shield_mitigation: f64,
    defender_shield_remaining: f64,
) -> (f64, f64) {
    let shield_portion = damage_after_apex * shield_mitigation;
    let hull_portion = damage_after_apex * (1.0 - shield_mitigation);
    let actual_shield_damage = shield_portion.min(defender_shield_remaining);
    let shield_overflow = shield_portion - actual_shield_damage;
    let hull_damage_this_round = hull_portion + shield_overflow;
    (actual_shield_damage, hull_damage_this_round)
}
