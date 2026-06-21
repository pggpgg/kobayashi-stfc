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
pub fn compute_apex_damage_factor(effective_apex_shred: f64, effective_apex_barrier: f64) -> f64 {
    let effective_barrier = effective_apex_barrier / (1.0 + effective_apex_shred).max(EPSILON);
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
    let isolytic_component = isolytic_damage(
        damage,
        effective_isolytic_damage,
        effective_isolytic_cascade,
    );
    isolytic_component / (1.0 + effective_isolytic_defense)
}

/// Combine standard weapon damage and isolytic for the pre-apex pool applied to defender HP.
/// When [`defender_isolytic_vulnerability`] is active, only the isolytic leg depletes shields/hull.
#[inline]
pub fn combine_outbound_damage_before_apex(
    standard_damage: f64,
    isolytic_taken: f64,
    defender_isolytic_vulnerability: bool,
) -> f64 {
    if defender_isolytic_vulnerability {
        isolytic_taken.max(0.0)
    } else {
        (standard_damage + isolytic_taken).max(0.0)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── compute_damage_through_factor ──

    #[test]
    fn damage_through_zero_mitigation_full_pierce() {
        // (1 - 0) + 0 + 0 = 1.0 → 100% damage-through
        let f = compute_damage_through_factor(1.0, 0.0, 0.0);
        assert!((f - 1.0).abs() < 1e-12);
    }

    #[test]
    fn damage_through_full_mitigation_no_pierce() {
        // 80% mitigation with no pierce → 0.2 damage-through
        let f = compute_damage_through_factor(0.2, 0.0, 0.0);
        assert!((f - 0.2).abs() < 1e-12);
    }

    #[test]
    fn damage_through_mitigation_plus_pierce_plus_bonus() {
        // (1 - 0.7) + 0.15 + 0.05 = 0.5
        let f = compute_damage_through_factor(0.3, 0.15, 0.05);
        assert!((f - 0.5).abs() < 1e-12);
    }

    #[test]
    fn damage_through_negative_clamped_to_zero() {
        // mitigation=1.2 would give -0.2 + 0.1 → -0.1, clamped to 0
        let f = compute_damage_through_factor(-0.2, 0.1, 0.0);
        assert!((f - 0.0).abs() < 1e-12);
    }

    #[test]
    fn damage_through_pierce_exceeds_mitigation() {
        // 60% pierce vs 40% mitigation → (1-0.6) + 0.4 = 0.8
        let f = compute_damage_through_factor(0.4, 0.4, 0.0);
        assert!((f - 0.8).abs() < 1e-12);
    }

    // ── compute_crit_multiplier ──

    #[test]
    fn crit_multiplier_non_crit_always_one() {
        let m = compute_crit_multiplier(false, 2.5, true);
        assert!((m - 1.0).abs() < 1e-12);

        let m = compute_crit_multiplier(false, 2.5, false);
        assert!((m - 1.0).abs() < 1e-12);
    }

    #[test]
    fn crit_multiplier_crit_no_hull_breach() {
        let m = compute_crit_multiplier(true, 2.0, false);
        assert!((m - 2.0).abs() < 1e-12);
    }

    #[test]
    fn crit_multiplier_crit_with_hull_breach() {
        let m = compute_crit_multiplier(true, 2.0, true);
        // 2.0 * 1.5 = 3.0
        assert!((m - 3.0).abs() < 1e-12);
    }

    #[test]
    fn crit_multiplier_crit_base_one_with_hull_breach() {
        // base multiplier of 1.0 (no bonus) with hull breach → 1.0 * 1.5 = 1.5
        let m = compute_crit_multiplier(true, 1.0, true);
        assert!((m - HULL_BREACH_CRIT_BONUS).abs() < 1e-12);
    }

    // ── compute_apex_damage_factor ──

    #[test]
    fn apex_zero_barrier_is_full_damage() {
        let f = compute_apex_damage_factor(0.0, 0.0);
        assert!((f - 1.0).abs() < 1e-12);
    }

    #[test]
    fn apex_barrier_reduces_damage() {
        // 10000 / (10000 + 5000) = 10000/15000 ≈ 0.6667
        let f = compute_apex_damage_factor(0.0, 5000.0);
        assert!((f - 10000.0 / 15000.0).abs() < 1e-12);
    }

    #[test]
    fn apex_shred_weakens_barrier() {
        // effective_barrier = 5000 / (1 + 1.0) = 2500
        // 10000 / (10000 + 2500) = 0.8
        let f = compute_apex_damage_factor(1.0, 5000.0);
        assert!((f - 0.8).abs() < 1e-12);
    }

    #[test]
    fn apex_extreme_barrier_near_zero_damage() {
        let f = compute_apex_damage_factor(0.0, 1_000_000.0);
        assert!(f < 0.01); // nearly all damage absorbed
        assert!(f > 0.0);
    }

    #[test]
    fn apex_extreme_shred_near_full_damage() {
        let f = compute_apex_damage_factor(1_000_000.0, 5000.0);
        // effective_barrier ≈ 5000 / 1,000,001 ≈ 0.005
        // 10000 / 10000.005 ≈ 0.9999995
        assert!(f > 0.999);
    }

    // ── compute_isolytic_taken ──

    #[test]
    fn isolytic_taken_zero_damage_yields_zero() {
        let iso = compute_isolytic_taken(0.0, 0.5, 0.0, 0.0);
        assert!((iso - 0.0).abs() < 1e-12);
    }

    #[test]
    fn isolytic_taken_basic_no_defense_no_cascade() {
        // isolytic_component = 1000 * 0.1 = 100
        // taken = 100 / (1 + 0) = 100
        let iso = compute_isolytic_taken(1000.0, 0.1, 0.0, 0.0);
        assert!((iso - 100.0).abs() < 1e-12);
    }

    #[test]
    fn isolytic_taken_with_defense_reduces_damage() {
        // isolytic_component = 1000 * 0.1 = 100
        // taken = 100 / (1 + 0.5) ≈ 66.6667
        let iso = compute_isolytic_taken(1000.0, 0.1, 0.5, 0.0);
        assert!((iso - 100.0 / 1.5).abs() < 1e-9);
    }

    #[test]
    fn isolytic_taken_with_cascade_amplifies() {
        // isolytic_component = 1000 * (0.1 + (1 + 0.1) * 0.2) = 1000 * (0.1 + 0.22) = 1000 * 0.32 = 320
        // taken = 320 / 1 = 320
        let iso = compute_isolytic_taken(1000.0, 0.1, 0.0, 0.2);
        assert!((iso - 320.0).abs() < 1e-9);
    }

    // ── apply_shield_hull_split ──

    #[test]
    fn shield_split_full_shields_80_percent_mitigation() {
        let (shield_dmg, hull_dmg) = apply_shield_hull_split(1000.0, 0.8, 1000.0);
        // shield_portion = 1000 * 0.8 = 800
        // actual_shield = min(800, 1000) = 800
        // hull = 1000 * 0.2 + 0 = 200
        assert!((shield_dmg - 800.0).abs() < 1e-12);
        assert!((hull_dmg - 200.0).abs() < 1e-12);
    }

    #[test]
    fn shield_split_no_shields_all_to_hull() {
        let (shield_dmg, hull_dmg) = apply_shield_hull_split(1000.0, 0.8, 0.0);
        // shield_portion = 1000 * 0.8 = 800
        // actual_shield = min(800, 0) = 0
        // overflow = 800 - 0 = 800
        // hull = 1000 * 0.2 + 800 = 200 + 800 = 1000
        assert!((shield_dmg - 0.0).abs() < 1e-12);
        assert!((hull_dmg - 1000.0).abs() < 1e-12);
    }

    #[test]
    fn shield_split_partial_depletion_overflow_to_hull() {
        // shields have 300 remaining, shield_portion would be 500
        let (shield_dmg, hull_dmg) = apply_shield_hull_split(1000.0, 0.5, 300.0);
        // shield_portion = 1000 * 0.5 = 500
        // actual_shield = min(500, 300) = 300
        // overflow = 500 - 300 = 200
        // hull = 1000 * 0.5 + 200 = 500 + 200 = 700
        assert!((shield_dmg - 300.0).abs() < 1e-12);
        assert!((hull_dmg - 700.0).abs() < 1e-12);
    }

    #[test]
    fn shield_split_zero_damage_yields_zeros() {
        let (shield_dmg, hull_dmg) = apply_shield_hull_split(0.0, 0.8, 500.0);
        assert!((shield_dmg - 0.0).abs() < 1e-12);
        assert!((hull_dmg - 0.0).abs() < 1e-12);
    }
}
