//! Pre-combat mitigation and pierce formulas.

use crate::combat::types::{self, AttackerStats, DefenderStats, ShipType, EPSILON};

/// Compute component mitigation f(x) = 1 / (1 + 4^(1.1 - x)).
pub fn component_mitigation(defense: f64, piercing: f64) -> f64 {
    let safe_defense = defense.max(0.0);
    let safe_piercing = piercing.max(EPSILON);
    let x = safe_defense / safe_piercing;
    1.0 / (1.0 + 4.0_f64.powf(1.1 - x))
}

/// Maximum pierce damage-through bonus (additive to (1 - mitigation)).
pub const PIERCE_CAP: f64 = 0.25;

/// Pierce damage-through bonus derived from defender/attacker stats and ship type.
/// Uses the same defense/piercing ratios as mitigation (STFC Toolbox). Formula:
/// `pierce = 0.25 * (1 - mitigation(defender, attacker, ship_type))`, clamped to [0, PIERCE_CAP].
pub fn pierce_damage_through_bonus(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
) -> f64 {
    let mit = mitigation(defender, attacker, ship_type);
    (PIERCE_CAP * (1.0 - mit)).clamp(0.0, PIERCE_CAP)
}

/// Default mitigation floor for hostile fights (game clamp). Some hostiles may override.
pub const MITIGATION_FLOOR: f64 = 0.16;
/// Default mitigation ceiling for hostile fights (game clamp). Some hostiles may override.
pub const MITIGATION_CEILING: f64 = 0.72;

/// Compute total mitigation using weighted multiplicative composition.
/// No hostile-specific clamp; use [`mitigation_for_hostile`] when the defender is a hostile.
pub fn mitigation(defender: DefenderStats, attacker: AttackerStats, ship_type: ShipType) -> f64 {
    mitigation_with_mystery(defender, attacker, ship_type, 0.0).clamp(0.0, 1.0)
}

/// Raw mitigation with optional "mystery" factor X. Formula:
/// `1 - (1 - X) * (1 - cA*fA) * (1 - cS*fS) * (1 - cD*fD)`.
pub fn mitigation_with_mystery(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    mystery_mitigation_factor: f64,
) -> f64 {
    let (c_armor, c_shield, c_dodge) = ship_type.coefficients();

    let f_armor = component_mitigation(defender.armor, attacker.armor_piercing);
    let f_shield = component_mitigation(defender.shield_deflection, attacker.shield_piercing);
    let f_dodge = component_mitigation(defender.dodge, attacker.accuracy);

    let one_minus_x = (1.0 - mystery_mitigation_factor).max(0.0);
    let total = 1.0
        - one_minus_x
            * (1.0 - c_armor * f_armor)
            * (1.0 - c_shield * f_shield)
            * (1.0 - c_dodge * f_dodge);
    total
}

/// Mitigation for hostile defenders: applies mystery factor X then clamps to [floor, ceiling].
pub fn mitigation_for_hostile(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    mystery_mitigation_factor: f64,
    floor: f64,
    ceiling: f64,
) -> f64 {
    let raw = mitigation_with_mystery(defender, attacker, ship_type, mystery_mitigation_factor);
    raw.clamp(floor, ceiling)
}

pub fn mitigation_with_morale(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    morale_active: bool,
) -> f64 {
    let attacker = if morale_active {
        apply_morale_primary_piercing(attacker, ship_type)
    } else {
        attacker
    };
    mitigation(defender, attacker, ship_type)
}

/// Compute isolytic damage from already-resolved regular attack damage.
pub fn isolytic_damage(
    regular_attack_damage: f64,
    isolytic_damage_bonus: f64,
    isolytic_cascade_damage_bonus: f64,
) -> f64 {
    regular_attack_damage.max(0.0)
        * (isolytic_damage_bonus + (1.0 + isolytic_damage_bonus) * isolytic_cascade_damage_bonus)
}

pub fn apply_morale_primary_piercing(
    attacker: AttackerStats,
    ship_type: ShipType,
) -> AttackerStats {
    use types::MORALE_PRIMARY_PIERCING_BONUS;
    let mut adjusted = attacker;
    match ship_type {
        ShipType::Battleship => {
            adjusted.shield_piercing *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
        }
        ShipType::Interceptor => {
            adjusted.armor_piercing *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
        }
        ShipType::Explorer => {
            adjusted.accuracy *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
        }
        ShipType::Survey => {}
        ShipType::Armada => {}
    }

    adjusted
}
