//! Vehicle weapon **crit** resolution for one hit (outbound player/hostile shot or counter-fire).
//!
//! Single entry point so crit chance (weapon + additive officer bonus, clamped) and crit damage
//! multiplier (weapon × officer crit-damage chain) stay aligned with [`compute_crit_multiplier`]
//! (hull breach bonus on the **defender** when `hull_breach_active`).

use crate::combat::damage::compute_crit_multiplier;
use crate::combat::proc::uniform_open_01;
use crate::combat::rng::Rng;

#[derive(Debug, Clone, Copy)]
pub(crate) struct VehicleCritResolution {
    pub roll: f64,
    pub effective_crit_chance: f64,
    pub is_crit: bool,
    /// Final multiplier applied to pre-proc damage for this hit (includes hull-breach crit bonus when applicable).
    pub multiplier: f64,
}

/// One crit roll for a weapon hit: same math for outbound and defender counter (`hull_breach_active`
/// is false on counter today — defender is not hull-breached for their own outgoing counter).
///
/// **Reduction → floor → hull-breach order.** On crit hits:
/// 1. Compose `raw_base = weapon_crit × officer_crit + additive_crit_damage_bonus`.
/// 2. Subtract `attacker_crit_reduction_additive` (Xindi percentage-point debuff).
/// 3. Multiply by `(1 - attacker_crit_reduction_mult)` when mult > 0 (Crozier / profile fraction).
/// 4. Clamp with [`Combatant::crit_damage_floor`] (player Critical Damage Floor research).
/// 5. Hull-breach amplification last.
///
/// The old single `attacker_crit_reduction` multiplicative param with a hard `0.05` engine floor
/// is split: additive debuffs use the player's real `crit_damage_floor`; counter-fire Crozier
/// still applies multiplicative fraction in the engine after this call.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_vehicle_weapon_crit(
    weapon_crit_chance: f64,
    crit_chance_bonus: f64,
    weapon_crit_multiplier: f64,
    crit_damage_multiplier: f64,
    additive_crit_damage_bonus: f64,
    attacker_crit_reduction_additive: f64,
    attacker_crit_reduction_mult: f64,
    crit_damage_floor: f64,
    hull_breach_active: bool,
    rng: &mut Rng,
) -> VehicleCritResolution {
    let effective_crit_chance = (weapon_crit_chance + crit_chance_bonus).clamp(0.0, 1.0);
    let roll = uniform_open_01(rng);
    let is_crit = roll < effective_crit_chance;
    let raw_base =
        weapon_crit_multiplier * crit_damage_multiplier + additive_crit_damage_bonus.max(0.0);
    let after_additive = if is_crit && attacker_crit_reduction_additive > 0.0 {
        // Xindi debuffs subtract percentage points from the crit *bonus* (mult above 1.0), same
        // family as research/officer crit-damage bonuses that add points to the composed mult.
        let crit_bonus = (raw_base - 1.0).max(0.0);
        1.0 + (crit_bonus - attacker_crit_reduction_additive).max(0.0)
    } else {
        raw_base
    };
    let after_mult = if is_crit && attacker_crit_reduction_mult > 0.0 {
        after_additive * (1.0 - attacker_crit_reduction_mult)
    } else {
        after_additive
    };
    let floored = if is_crit {
        after_mult.max(crit_damage_floor)
    } else {
        after_mult
    };
    let multiplier = compute_crit_multiplier(is_crit, floored, hull_breach_active);
    VehicleCritResolution {
        roll,
        effective_crit_chance,
        is_crit,
        multiplier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::types::HULL_BREACH_CRIT_BONUS;

    #[test]
    fn non_crit_yields_unit_multiplier() {
        let mut rng = Rng::new(1);
        let c = resolve_vehicle_weapon_crit(0.0, 0.0, 2.0, 1.5, 0.0, 0.0, 0.0, 0.0, false, &mut rng);
        assert!(!c.is_crit);
        assert!((c.multiplier - 1.0).abs() < 1e-12);
    }

    #[test]
    fn guaranteed_crit_scales_by_weapon_and_officer_crit_damage_mult() {
        let mut rng = Rng::new(2);
        let c = resolve_vehicle_weapon_crit(1.0, 0.0, 2.0, 1.5, 0.0, 0.0, 0.0, 0.0, false, &mut rng);
        assert!(c.is_crit);
        assert!((c.multiplier - 3.0).abs() < 1e-9);
    }

    #[test]
    fn additive_crit_damage_bonus_adds_percentage_points_to_multiplier() {
        let mut rng = Rng::new(21);
        // weapon × officer = 2.0 × 1.0 = 2.0; additive +0.5 → 2.5 (vs ×1.5 = 3.0 for multiplicative).
        let c = resolve_vehicle_weapon_crit(1.0, 0.0, 2.0, 1.0, 0.5, 0.0, 0.0, 0.0, false, &mut rng);
        assert!(c.is_crit);
        assert!((c.multiplier - 2.5).abs() < 1e-9);
    }

    #[test]
    fn additive_crit_damage_bonus_ignored_on_non_crit() {
        let mut rng = Rng::new(22);
        let c = resolve_vehicle_weapon_crit(0.0, 0.0, 2.0, 1.0, 0.5, 0.0, 0.0, 0.0, false, &mut rng);
        assert!(!c.is_crit);
        assert!((c.multiplier - 1.0).abs() < 1e-12);
    }

    #[test]
    fn hull_breach_multiplies_crit_damage_bonus() {
        let mut rng = Rng::new(3);
        let c = resolve_vehicle_weapon_crit(1.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, true, &mut rng);
        assert!(c.is_crit);
        assert!((c.multiplier - 2.0 * HULL_BREACH_CRIT_BONUS).abs() < 1e-9);
    }

    #[test]
    fn same_seed_and_params_yields_identical_resolution() {
        let mut rng_a = Rng::new(42);
        let mut rng_b = Rng::new(42);
        let a = resolve_vehicle_weapon_crit(1.0, 0.0, 2.0, 1.2, 0.0, 0.0, 0.0, 0.0, false, &mut rng_a);
        let b = resolve_vehicle_weapon_crit(1.0, 0.0, 2.0, 1.2, 0.0, 0.0, 0.0, 0.0, false, &mut rng_b);
        assert_eq!(a.is_crit, b.is_crit);
        assert!((a.roll - b.roll).abs() < 1e-15);
        assert!((a.multiplier - b.multiplier).abs() < 1e-15);
    }

    #[test]
    fn effective_crit_chance_is_clamped_to_one() {
        let mut rng = Rng::new(7);
        // weapon 0.9 + bonus 0.5 = 1.4 → clamped to 1.0
        let c = resolve_vehicle_weapon_crit(0.9, 0.5, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, false, &mut rng);
        assert!(c.is_crit); // chance is 1.0, always crits
        assert!((c.effective_crit_chance - 1.0).abs() < 1e-12);
    }

    #[test]
    fn effective_crit_chance_is_clamped_to_zero() {
        let mut rng = Rng::new(8);
        // weapon 0.0 + bonus -0.5 = -0.5 → clamped to 0.0
        let c = resolve_vehicle_weapon_crit(0.0, -0.5, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, false, &mut rng);
        assert!(!c.is_crit);
        assert!((c.effective_crit_chance - 0.0).abs() < 1e-12);
    }

    #[test]
    fn crit_chance_bonus_adds_to_weapon_crit_chance() {
        let mut rng = Rng::new(9);
        // 25% from weapon + 75% from bonus = 100% → always crits
        let c = resolve_vehicle_weapon_crit(0.25, 0.75, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, false, &mut rng);
        assert!(c.is_crit);
        assert!((c.effective_crit_chance - 1.0).abs() < 1e-12);
    }

    #[test]
    fn attacker_crit_reduction_additive_subtracts_percentage_points() {
        let mut rng = Rng::new(31);
        // raw 10.0 (+900% bonus); −5.0 points from bonus → 1 + (9 − 5) = 5.0
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 2.0, 5.0, 0.0, 5.0, 0.0, 0.0, false, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 5.0).abs() < 1e-9);
    }

    #[test]
    fn attacker_crit_reduction_additive_only_eats_bonus_not_base_one() {
        let mut rng = Rng::new(34);
        // raw 2.5 (+150% bonus); −5.0 cannot pull below ×1.0 before floor
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 1.0, 2.5, 0.0, 5.0, 0.0, 0.0, false, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 1.0).abs() < 1e-9);
    }

    #[test]
    fn attacker_crit_reduction_mult_fraction_on_outbound_after_additive() {
        let mut rng = Rng::new(32);
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.50, 0.0, false, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 1.0).abs() < 1e-9);
    }

    #[test]
    fn attacker_crit_reduction_shrinks_base_multiplier_when_no_floor() {
        let mut rng = Rng::new(11);
        // legacy mult-only path via attacker_crit_reduction_mult
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.50, 0.0, false, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 1.0).abs() < 1e-9);
    }

    #[test]
    fn crit_damage_floor_clamps_after_additive_xindi_debuff() {
        let mut rng = Rng::new(33);
        // raw 12.0; −10.0 additive would give 2.0; floor 6.0 protects
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 2.0, 6.0, 0.0, 10.0, 0.0, 6.0, false, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 6.0).abs() < 1e-9);
    }
    #[test]
    fn crit_damage_floor_clamps_below_reduced_multiplier() {
        let mut rng = Rng::new(12);
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.80, 1.5, false, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 1.5).abs() < 1e-9);
    }

    #[test]
    fn crit_damage_floor_does_not_clamp_when_reduced_exceeds_floor() {
        let mut rng = Rng::new(13);
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 2.5, 1.0, 0.0, 0.0, 0.20, 1.0, false, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 2.0).abs() < 1e-9);
    }

    #[test]
    fn floor_protects_against_hull_breach_compounding_on_clamped_base() {
        let mut rng = Rng::new(14);
        let c = resolve_vehicle_weapon_crit(
            1.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.90, 1.0, true, &mut rng,
        );
        assert!(c.is_crit);
        assert!((c.multiplier - 1.0 * HULL_BREACH_CRIT_BONUS).abs() < 1e-9);
    }

    #[test]
    fn floor_is_a_noop_when_not_crit() {
        let mut rng = Rng::new(15);
        // Even with a high floor, non-crit returns unit multiplier 1.0.
        let c = resolve_vehicle_weapon_crit(0.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.0, 5.0, false, &mut rng);
        assert!(!c.is_crit);
        assert!((c.multiplier - 1.0).abs() < 1e-12);
    }
}
