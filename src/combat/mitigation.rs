//! Pre-combat mitigation and pierce formulas.

use crate::combat::types::{self, AttackerStats, DefenderStats, ShipType, EPSILON};

/// Detailed mitigation decomposition for one attacker/defender/ship-type tuple.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MitigationBreakdown {
    pub c_armor: f64,
    pub c_shield: f64,
    pub c_dodge: f64,
    pub armor_ratio: f64,
    pub shield_ratio: f64,
    pub dodge_ratio: f64,
    pub f_armor: f64,
    pub f_shield: f64,
    pub f_dodge: f64,
    pub weighted_armor: f64,
    pub weighted_shield: f64,
    pub weighted_dodge: f64,
    pub mystery_mitigation_factor: f64,
    pub one_minus_mystery: f64,
    pub raw_mitigation: f64,
}

#[inline]
fn component_ratio(defense: f64, piercing: f64) -> f64 {
    let safe_defense = defense.max(0.0);
    let safe_piercing = piercing.max(EPSILON);
    safe_defense / safe_piercing
}

/// Compute component mitigation f(x) = 1 / (1 + 4^(1.1 - x)).
pub fn component_mitigation(defense: f64, piercing: f64) -> f64 {
    let x = component_ratio(defense, piercing);
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

/// Return the full mitigation decomposition before hostile floor/ceiling clamping.
pub fn mitigation_breakdown(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    mystery_mitigation_factor: f64,
) -> MitigationBreakdown {
    let (c_armor, c_shield, c_dodge) = ship_type.coefficients();
    let armor_ratio = component_ratio(defender.armor, attacker.armor_piercing);
    let shield_ratio = component_ratio(defender.shield_deflection, attacker.shield_piercing);
    let dodge_ratio = component_ratio(defender.dodge, attacker.accuracy);
    let f_armor = 1.0 / (1.0 + 4.0_f64.powf(1.1 - armor_ratio));
    let f_shield = 1.0 / (1.0 + 4.0_f64.powf(1.1 - shield_ratio));
    let f_dodge = 1.0 / (1.0 + 4.0_f64.powf(1.1 - dodge_ratio));
    let weighted_armor = c_armor * f_armor;
    let weighted_shield = c_shield * f_shield;
    let weighted_dodge = c_dodge * f_dodge;
    let one_minus_mystery = (1.0 - mystery_mitigation_factor).max(0.0);
    let raw_mitigation = 1.0
        - one_minus_mystery
            * (1.0 - weighted_armor)
            * (1.0 - weighted_shield)
            * (1.0 - weighted_dodge);
    MitigationBreakdown {
        c_armor,
        c_shield,
        c_dodge,
        armor_ratio,
        shield_ratio,
        dodge_ratio,
        f_armor,
        f_shield,
        f_dodge,
        weighted_armor,
        weighted_shield,
        weighted_dodge,
        mystery_mitigation_factor,
        one_minus_mystery,
        raw_mitigation,
    }
}

/// Raw mitigation with optional "mystery" factor X. Formula:
/// `1 - (1 - X) * (1 - cA*fA) * (1 - cS*fS) * (1 - cD*fD)`.
pub fn mitigation_with_mystery(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    mystery_mitigation_factor: f64,
) -> f64 {
    mitigation_breakdown(defender, attacker, ship_type, mystery_mitigation_factor).raw_mitigation
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
        apply_morale_piercing(attacker)
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

/// Morale piercing bonus: all piercing stats (armor piercing, shield piercing, accuracy) are
/// increased by [`types::MORALE_PIERCING_BONUS`] for the weapon attack, applied at the end,
/// after all other bonuses (so call this on fully-resolved stats).
pub fn apply_morale_piercing(attacker: AttackerStats) -> AttackerStats {
    use types::MORALE_PIERCING_BONUS;
    AttackerStats {
        armor_piercing: attacker.armor_piercing * (1.0 + MORALE_PIERCING_BONUS),
        shield_piercing: attacker.shield_piercing * (1.0 + MORALE_PIERCING_BONUS),
        accuracy: attacker.accuracy * (1.0 + MORALE_PIERCING_BONUS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn def(armor: f64, shield: f64, dodge: f64) -> DefenderStats {
        DefenderStats {
            armor,
            shield_deflection: shield,
            dodge,
        }
    }

    fn att(armor_piercing: f64, shield_piercing: f64, accuracy: f64) -> AttackerStats {
        AttackerStats {
            armor_piercing,
            shield_piercing,
            accuracy,
        }
    }

    // ── constants pinned as intentional-change tripwires ──

    #[test]
    fn formula_constants_are_pinned() {
        assert_eq!(PIERCE_CAP, 0.25);
        assert_eq!(MITIGATION_FLOOR, 0.16);
        assert_eq!(MITIGATION_CEILING, 0.72);
        assert_eq!(ShipType::Survey.coefficients(), (0.3, 0.3, 0.3));
        assert_eq!(ShipType::Armada.coefficients(), (0.3, 0.3, 0.3));
        assert_eq!(ShipType::Battleship.coefficients(), (0.55, 0.2, 0.2));
        assert_eq!(ShipType::Explorer.coefficients(), (0.2, 0.55, 0.2));
        assert_eq!(ShipType::Interceptor.coefficients(), (0.2, 0.2, 0.55));
    }

    // ── component_mitigation curve ──

    #[test]
    fn component_mitigation_is_half_at_ratio_one_point_one() {
        // f(x) = 1 / (1 + 4^(1.1 - x)) hits exactly 0.5 when x = 1.1.
        let f = component_mitigation(110.0, 100.0);
        assert!((f - 0.5).abs() < 1e-12, "got {f}");
    }

    #[test]
    fn component_mitigation_stays_in_open_unit_interval_for_moderate_ratios() {
        // Above ratio ≈ 27 the 4^(1.1 - x) term drops below f64 epsilon and f saturates to
        // exactly 1.0 (covered by the epsilon-guard test); the curve is strictly inside (0, 1)
        // only for moderate ratios.
        for (d, p) in [(0.0, 100.0), (1.0, 1.0), (20.0, 1.0), (1.0, 1e9)] {
            let f = component_mitigation(d, p);
            assert!(f > 0.0 && f < 1.0, "f({d}, {p}) = {f} out of (0, 1)");
        }
    }

    #[test]
    fn component_mitigation_monotonic_in_defense_and_piercing() {
        assert!(component_mitigation(200.0, 100.0) > component_mitigation(100.0, 100.0));
        assert!(component_mitigation(100.0, 200.0) < component_mitigation(100.0, 100.0));
    }

    // ── input guards ──

    #[test]
    fn zero_piercing_uses_epsilon_guard_and_stays_finite() {
        let f = component_mitigation(100.0, 0.0);
        assert!(f.is_finite());
        // Ratio explodes toward +inf, so 4^(1.1 - x) underflows to 0 and f saturates at 1.0
        // exactly — the asymptotic bound of the curve, not a NaN/inf escape.
        assert!(f > 0.999 && f <= 1.0, "got {f}");
    }

    #[test]
    fn negative_defense_clamps_to_zero_defense() {
        assert_eq!(
            component_mitigation(-500.0, 100.0),
            component_mitigation(0.0, 100.0)
        );
        let m_neg = mitigation(
            def(-1.0, -2.0, -3.0),
            att(100.0, 100.0, 100.0),
            ShipType::Survey,
        );
        let m_zero = mitigation(
            def(0.0, 0.0, 0.0),
            att(100.0, 100.0, 100.0),
            ShipType::Survey,
        );
        assert_eq!(m_neg, m_zero);
    }

    /// `f64::max` returns the non-NaN operand, so NaN defense behaves as 0 and NaN piercing
    /// behaves as EPSILON — the formula sanitizes NaN inputs by construction. Pinned so a
    /// refactor that swaps the comparison (e.g. to `if defense < 0.0`) doesn't silently start
    /// propagating NaN into combat math.
    #[test]
    fn nan_inputs_are_sanitized_not_propagated() {
        let f_nan_def = component_mitigation(f64::NAN, 100.0);
        assert!(f_nan_def.is_finite());
        assert_eq!(f_nan_def, component_mitigation(0.0, 100.0));

        let f_nan_pierce = component_mitigation(100.0, f64::NAN);
        assert!(f_nan_pierce.is_finite());
        assert_eq!(f_nan_pierce, component_mitigation(100.0, 0.0));

        let m = mitigation(
            def(f64::NAN, f64::NAN, f64::NAN),
            att(f64::NAN, f64::NAN, f64::NAN),
            ShipType::Explorer,
        );
        assert!(m.is_finite() && (0.0..=1.0).contains(&m));
    }

    /// NaN mystery factor: `(1.0 - NaN).max(0.0)` → 0.0, so it degenerates to mystery = 1
    /// (full mitigation), finite. Unreachable from JSON data (serde_json rejects NaN) but
    /// pinned for the same refactor-safety reason as above.
    #[test]
    fn nan_mystery_factor_degenerates_to_full_mitigation() {
        let m = mitigation_with_mystery(
            def(100.0, 100.0, 100.0),
            att(100.0, 100.0, 100.0),
            ShipType::Survey,
            f64::NAN,
        );
        assert_eq!(m, 1.0);
    }

    // ── ship-class channel routing ──

    #[test]
    fn primary_channel_dominates_per_ship_class() {
        // Defense concentrated in one channel; the class whose primary weight (0.55) matches
        // that channel mitigates the most.
        let a = att(100.0, 100.0, 100.0);
        let armor_heavy = def(1000.0, 0.0, 0.0);
        let shield_heavy = def(0.0, 1000.0, 0.0);
        let dodge_heavy = def(0.0, 0.0, 1000.0);

        let m_bs = mitigation(armor_heavy, a, ShipType::Battleship);
        assert!(m_bs > mitigation(armor_heavy, a, ShipType::Explorer));
        assert!(m_bs > mitigation(armor_heavy, a, ShipType::Interceptor));

        let m_ex = mitigation(shield_heavy, a, ShipType::Explorer);
        assert!(m_ex > mitigation(shield_heavy, a, ShipType::Battleship));
        assert!(m_ex > mitigation(shield_heavy, a, ShipType::Interceptor));

        let m_in = mitigation(dodge_heavy, a, ShipType::Interceptor);
        assert!(m_in > mitigation(dodge_heavy, a, ShipType::Battleship));
        assert!(m_in > mitigation(dodge_heavy, a, ShipType::Explorer));
    }

    // ── mystery factor ──

    #[test]
    fn mystery_zero_matches_plain_mitigation_and_one_saturates() {
        let d = def(150.0, 150.0, 150.0);
        let a = att(100.0, 100.0, 100.0);
        assert_eq!(
            mitigation_with_mystery(d, a, ShipType::Survey, 0.0),
            mitigation(d, a, ShipType::Survey)
        );
        assert_eq!(mitigation_with_mystery(d, a, ShipType::Survey, 1.0), 1.0);
        // Negative mystery can push raw below zero; the public `mitigation` clamp holds [0, 1].
        let m = mitigation(d, a, ShipType::Survey);
        assert!((0.0..=1.0).contains(&m));
    }

    // ── hostile floor/ceiling clamp ──

    #[test]
    fn hostile_mitigation_clamps_both_sides() {
        let a = att(100.0, 100.0, 100.0);
        // Tiny defense → raw mitigation below the floor → clamped up.
        let weak = mitigation_for_hostile(
            def(0.0, 0.0, 0.0),
            a,
            ShipType::Survey,
            0.0,
            MITIGATION_FLOOR,
            MITIGATION_CEILING,
        );
        assert_eq!(weak, MITIGATION_FLOOR);
        // Huge defense + mystery → raw above the ceiling → clamped down.
        let strong = mitigation_for_hostile(
            def(1e7, 1e7, 1e7),
            a,
            ShipType::Survey,
            0.9,
            MITIGATION_FLOOR,
            MITIGATION_CEILING,
        );
        assert_eq!(strong, MITIGATION_CEILING);
    }

    // ── pierce ──

    #[test]
    fn pierce_bonus_is_quarter_of_unmitigated_share() {
        let d = def(110.0, 110.0, 110.0);
        let a = att(100.0, 100.0, 100.0);
        let mit = mitigation(d, a, ShipType::Survey);
        let pierce = pierce_damage_through_bonus(d, a, ShipType::Survey);
        assert!((pierce - PIERCE_CAP * (1.0 - mit)).abs() < 1e-12);
        assert!((0.0..=PIERCE_CAP).contains(&pierce));
    }

    // ── morale piercing ──

    #[test]
    fn morale_boosts_all_piercing_stats_by_ten_percent() {
        let base = att(100.0, 200.0, 300.0);
        let got = apply_morale_piercing(base);
        for (g, w) in [
            (got.armor_piercing, 110.0),
            (got.shield_piercing, 220.0),
            (got.accuracy, 330.0),
        ] {
            assert!((g - w).abs() < 1e-9, "expected {w}, got {g}");
        }
    }

    #[test]
    fn morale_never_increases_mitigation() {
        let d = def(150.0, 150.0, 150.0);
        let a = att(100.0, 100.0, 100.0);
        for st in [
            ShipType::Battleship,
            ShipType::Explorer,
            ShipType::Interceptor,
            ShipType::Survey,
            ShipType::Armada,
        ] {
            let with = mitigation_with_morale(d, a, st, true);
            let without = mitigation_with_morale(d, a, st, false);
            assert!(
                with <= without + 1e-12,
                "{st:?}: morale raised mitigation {without} -> {with}"
            );
        }
    }

    // ── isolytic ──

    #[test]
    fn isolytic_damage_formula_and_negative_clamp() {
        // bonus only: 1000 * 0.2
        assert!((isolytic_damage(1000.0, 0.2, 0.0) - 200.0).abs() < 1e-12);
        // cascade compounds on (1 + bonus): 1000 * (0.2 + 1.2 * 0.1)
        assert!((isolytic_damage(1000.0, 0.2, 0.1) - 320.0).abs() < 1e-12);
        // negative regular damage clamps to zero contribution
        assert_eq!(isolytic_damage(-500.0, 0.2, 0.1), 0.0);
    }

    // ── property-based: pure formula, fast ──

    fn stat() -> impl Strategy<Value = f64> {
        0.0..1e9f64
    }

    proptest! {
        #[test]
        fn mitigation_always_in_unit_interval_and_finite(
            da in stat(), ds in stat(), dd in stat(),
            pa in stat(), ps in stat(), pc in stat(),
            mystery in 0.0..=1.0f64,
        ) {
            for st in [
                ShipType::Battleship,
                ShipType::Explorer,
                ShipType::Interceptor,
                ShipType::Survey,
                ShipType::Armada,
            ] {
                let m = mitigation(def(da, ds, dd), att(pa, ps, pc), st);
                prop_assert!(m.is_finite() && (0.0..=1.0).contains(&m));
                let raw = mitigation_with_mystery(def(da, ds, dd), att(pa, ps, pc), st, mystery);
                prop_assert!(raw.is_finite() && (0.0..=1.0).contains(&raw));
                let breakdown = mitigation_breakdown(def(da, ds, dd), att(pa, ps, pc), st, mystery);
                prop_assert_eq!(breakdown.raw_mitigation, raw);
            }
        }

        #[test]
        fn more_defense_never_lowers_and_more_piercing_never_raises_mitigation(
            da in stat(), ds in stat(), dd in stat(),
            pa in 1.0..1e6f64, ps in 1.0..1e6f64, pc in 1.0..1e6f64,
            bump in 1.0..1e6f64,
        ) {
            let a = att(pa, ps, pc);
            let base = mitigation(def(da, ds, dd), a, ShipType::Survey);
            prop_assert!(mitigation(def(da + bump, ds, dd), a, ShipType::Survey) >= base - 1e-12);
            prop_assert!(mitigation(def(da, ds + bump, dd), a, ShipType::Survey) >= base - 1e-12);
            prop_assert!(mitigation(def(da, ds, dd + bump), a, ShipType::Survey) >= base - 1e-12);
            prop_assert!(
                mitigation(def(da, ds, dd), att(pa + bump, ps, pc), ShipType::Survey)
                    <= base + 1e-12
            );
            prop_assert!(
                mitigation(def(da, ds, dd), att(pa, ps + bump, pc), ShipType::Survey)
                    <= base + 1e-12
            );
            prop_assert!(
                mitigation(def(da, ds, dd), att(pa, ps, pc + bump), ShipType::Survey)
                    <= base + 1e-12
            );
        }

        #[test]
        fn hostile_mitigation_always_within_floor_and_ceiling(
            da in stat(), ds in stat(), dd in stat(),
            pa in stat(), ps in stat(), pc in stat(),
            mystery in 0.0..=1.0f64,
            floor in 0.0..=0.5f64,
            span in 0.0..=0.5f64,
        ) {
            let ceiling = floor + span;
            let m = mitigation_for_hostile(
                def(da, ds, dd),
                att(pa, ps, pc),
                ShipType::Battleship,
                mystery,
                floor,
                ceiling,
            );
            prop_assert!((floor..=ceiling).contains(&m));
        }
    }
}
