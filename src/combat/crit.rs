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
pub(crate) fn resolve_vehicle_weapon_crit(
    weapon_crit_chance: f64,
    crit_chance_bonus: f64,
    weapon_crit_multiplier: f64,
    crit_damage_multiplier: f64,
    hull_breach_active: bool,
    rng: &mut Rng,
) -> VehicleCritResolution {
    let effective_crit_chance = (weapon_crit_chance + crit_chance_bonus).clamp(0.0, 1.0);
    let roll = uniform_open_01(rng);
    let is_crit = roll < effective_crit_chance;
    let base_crit_multiplier = weapon_crit_multiplier * crit_damage_multiplier;
    let multiplier = compute_crit_multiplier(is_crit, base_crit_multiplier, hull_breach_active);
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
        let c = resolve_vehicle_weapon_crit(0.0, 0.0, 2.0, 1.5, false, &mut rng);
        assert!(!c.is_crit);
        assert!((c.multiplier - 1.0).abs() < 1e-12);
    }

    #[test]
    fn guaranteed_crit_scales_by_weapon_and_officer_crit_damage_mult() {
        let mut rng = Rng::new(2);
        let c = resolve_vehicle_weapon_crit(1.0, 0.0, 2.0, 1.5, false, &mut rng);
        assert!(c.is_crit);
        assert!((c.multiplier - 3.0).abs() < 1e-9);
    }

    #[test]
    fn hull_breach_multiplies_crit_damage_bonus() {
        let mut rng = Rng::new(3);
        let c = resolve_vehicle_weapon_crit(1.0, 0.0, 2.0, 1.0, true, &mut rng);
        assert!(c.is_crit);
        assert!((c.multiplier - 2.0 * HULL_BREACH_CRIT_BONUS).abs() < 1e-9);
    }
}
