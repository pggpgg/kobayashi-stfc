//! Stat-level perturbation hook for sensitivity analysis.
//!
//! Mutates resolved combat state by a per-stat delta. Applied immediately before
//! [`crate::combat::build_combat_setup`] / [`crate::combat::simulate_combat_from_setup`]
//! so the engine sees the perturbed values for the entire fight.
//!
//! Two pieces of state are touched depending on the stat:
//!
//! - [`Combatant`] (attacker) for HP, crit, the aggregated mitigation scalar, isolytic,
//!   apex, shield_mitigation, the universal `crit_damage_reduction_bonus` (additive on
//!   top of the crew-derived reduction at engine call time), etc.
//! - [`crate::combat::types::HostileMitigationParams::base_attacker_stats`] inside the
//!   defender's `Combatant` for piercing / accuracy (these feed the component-based
//!   mitigation calc in [`crate::combat::mitigation::mitigation_breakdown`]).
//!
//! The four mitigation components (`armor`, `shield_deflection`, `dodge`, `damage_reduction`)
//! are tracked separately on [`Combatant`] post-resolution and weighted by ship-type
//! coefficients in the engine's inbound counter-fire path. Each is exposed as its own
//! `StatKey` variant; the aggregated [`StatKey::Mitigation`] is no longer surfaced.
//!
//! `crit_damage_floor` is exposed via [`StatKey::CritDamageFloor`]: a defensive clamp
//! enforced at the per-shot crit resolution site after opponent-side crit-damage
//! reductions are applied (see [`crate::combat::crit::resolve_vehicle_weapon_crit`]).

use serde::{Deserialize, Serialize};

use crate::combat::Combatant;

/// Identifier for a player-facing combat stat that can be perturbed by the sensitivity
/// analyzer. Serialized as snake_case for JSON / API compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatKey {
    WeaponDamage,
    CritChance,
    CritDamage,
    ArmorPiercing,
    ShieldPiercing,
    Accuracy,
    ApexShred,
    IsolyticDamage,
    Armor,
    ShieldDeflection,
    Dodge,
    DamageReduction,
    ApexBarrier,
    IsolyticDefense,
    CritDamageFloor,
    HullHp,
    ShieldHp,
    ShieldMitigation,
}

impl StatKey {
    /// Every stat key in the sensitivity set. Stable iteration order — UI surfaces
    /// follow this order until they sort by measured Δ.
    pub const ALL: &'static [StatKey] = &[
        StatKey::WeaponDamage,
        StatKey::CritChance,
        StatKey::CritDamage,
        StatKey::ArmorPiercing,
        StatKey::ShieldPiercing,
        StatKey::Accuracy,
        StatKey::ApexShred,
        StatKey::IsolyticDamage,
        StatKey::Armor,
        StatKey::ShieldDeflection,
        StatKey::Dodge,
        StatKey::DamageReduction,
        StatKey::ApexBarrier,
        StatKey::IsolyticDefense,
        StatKey::CritDamageFloor,
        StatKey::HullHp,
        StatKey::ShieldHp,
        StatKey::ShieldMitigation,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StatKey::WeaponDamage => "weapon_damage",
            StatKey::CritChance => "crit_chance",
            StatKey::CritDamage => "crit_damage",
            StatKey::ArmorPiercing => "armor_piercing",
            StatKey::ShieldPiercing => "shield_piercing",
            StatKey::Accuracy => "accuracy",
            StatKey::ApexShred => "apex_shred",
            StatKey::IsolyticDamage => "isolytic_damage",
            StatKey::Armor => "armor",
            StatKey::ShieldDeflection => "shield_deflection",
            StatKey::Dodge => "dodge",
            StatKey::DamageReduction => "damage_reduction",
            StatKey::ApexBarrier => "apex_barrier",
            StatKey::IsolyticDefense => "isolytic_defense",
            StatKey::CritDamageFloor => "crit_damage_floor",
            StatKey::HullHp => "hull_hp",
            StatKey::ShieldHp => "shield_hp",
            StatKey::ShieldMitigation => "shield_mitigation",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|k| k.as_str() == s).copied()
    }

    /// Default per-stat delta sized to "one realistic step of in-game investment."
    ///
    /// Multiplicative stats (HP, weapon damage, piercing) return a fractional bump
    /// (`0.05` → `×1.05`). Additive stats return a percentage-point bump
    /// (`0.01` → `+1pp` on a `[0,1]` stat, `0.10` → `+10pp` on a crit multiplier).
    /// Users can override every value via the API.
    pub fn default_delta(self) -> f64 {
        match self {
            StatKey::WeaponDamage => 0.05,
            StatKey::CritChance => 0.01,
            StatKey::CritDamage => 0.10,
            StatKey::ArmorPiercing => 0.05,
            StatKey::ShieldPiercing => 0.05,
            StatKey::Accuracy => 0.01,
            StatKey::ApexShred => 0.01,
            StatKey::IsolyticDamage => 0.01,
            StatKey::Armor => 0.01,
            StatKey::ShieldDeflection => 0.01,
            StatKey::Dodge => 0.01,
            StatKey::DamageReduction => 0.01,
            StatKey::ApexBarrier => 0.01,
            StatKey::IsolyticDefense => 0.05,
            // Same units as crit_damage (raw multiplier). +0.10 = floor moves up by 0.10×
            // damage. The clamp matters only when an attacker-outbound CDR is reducing the
            // crit multiplier below the floor — dormant otherwise.
            StatKey::CritDamageFloor => 0.10,
            StatKey::HullHp => 0.05,
            StatKey::ShieldHp => 0.05,
            StatKey::ShieldMitigation => 0.01,
        }
    }

    /// Whether the default delta is multiplicative (`value *= 1 + δ`) or additive
    /// (`value += δ`). The UI uses this to format the override input.
    pub fn is_multiplicative(self) -> bool {
        matches!(
            self,
            StatKey::WeaponDamage
                | StatKey::ArmorPiercing
                | StatKey::ShieldPiercing
                | StatKey::HullHp
                | StatKey::ShieldHp
        )
    }
}

/// Rebuild the aggregated `mitigation` scalar from the four components, clamped to [0, 1].
/// Used by the per-component `StatKey` arms to keep the back-compat scalar in sync after
/// a perturbation; the engine's inbound counter-fire path consults the components first.
#[inline]
fn sum_mitigation_components(c: &Combatant) -> f64 {
    (c.armor + c.shield_deflection + c.dodge + c.damage_reduction).clamp(0.0, 1.0)
}

/// Apply a per-stat perturbation to resolved combat state.
///
/// Mutates the relevant field by `delta`. A zero delta is a no-op (modulo floating
/// point noise — `value *= 1.0` and `value += 0.0` are exact for finite operands).
/// This property is exercised by the determinism test in
/// `tests/sensitivity_engine_tests.rs`.
pub fn apply_perturbation(
    attacker: &mut Combatant,
    defender: &mut Combatant,
    stat: StatKey,
    delta: f64,
) {
    if delta == 0.0 {
        return;
    }
    match stat {
        StatKey::WeaponDamage => {
            let factor = 1.0 + delta;
            attacker.attack *= factor;
            for w in &mut attacker.weapons {
                w.attack *= factor;
            }
        }
        StatKey::CritChance => {
            attacker.crit_chance = (attacker.crit_chance + delta).clamp(0.0, 1.0);
        }
        StatKey::CritDamage => {
            attacker.crit_multiplier = (attacker.crit_multiplier + delta).max(0.0);
        }
        StatKey::ArmorPiercing => {
            if let Some(p) = defender.hostile_mitigation_params.as_mut() {
                p.base_attacker_stats.armor_piercing *= 1.0 + delta;
            }
        }
        StatKey::ShieldPiercing => {
            if let Some(p) = defender.hostile_mitigation_params.as_mut() {
                p.base_attacker_stats.shield_piercing *= 1.0 + delta;
            }
        }
        StatKey::Accuracy => {
            if let Some(p) = defender.hostile_mitigation_params.as_mut() {
                p.base_attacker_stats.accuracy += delta;
            }
        }
        StatKey::ApexShred => {
            attacker.apex_shred = (attacker.apex_shred + delta).max(0.0);
        }
        StatKey::IsolyticDamage => {
            attacker.isolytic_damage = (attacker.isolytic_damage + delta).max(0.0);
        }
        StatKey::Armor => {
            attacker.armor = (attacker.armor + delta).max(0.0);
            attacker.mitigation = sum_mitigation_components(attacker);
        }
        StatKey::ShieldDeflection => {
            attacker.shield_deflection = (attacker.shield_deflection + delta).max(0.0);
            attacker.mitigation = sum_mitigation_components(attacker);
        }
        StatKey::Dodge => {
            attacker.dodge = (attacker.dodge + delta).max(0.0);
            attacker.mitigation = sum_mitigation_components(attacker);
        }
        StatKey::DamageReduction => {
            attacker.damage_reduction = (attacker.damage_reduction + delta).max(0.0);
            attacker.mitigation = sum_mitigation_components(attacker);
        }
        StatKey::ApexBarrier => {
            attacker.apex_barrier = (attacker.apex_barrier + delta).max(0.0);
        }
        StatKey::IsolyticDefense => {
            attacker.isolytic_defense = (attacker.isolytic_defense + delta).max(0.0);
        }
        StatKey::CritDamageFloor => {
            attacker.crit_damage_floor = (attacker.crit_damage_floor + delta).max(0.0);
        }
        StatKey::HullHp => {
            attacker.hull_health *= 1.0 + delta;
        }
        StatKey::ShieldHp => {
            attacker.shield_health *= 1.0 + delta;
        }
        StatKey::ShieldMitigation => {
            attacker.shield_mitigation = (attacker.shield_mitigation + delta).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Combatant;

    fn mk_combatant() -> Combatant {
        Combatant {
            id: "test".into(),
            attack: 100.0,
            mitigation: 0.3,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.5,
            crit_multiplier: 2.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 0.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 500.0,
            shield_mitigation: 0.8,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            weapons: Vec::new(),
            hostile_mitigation_params: None,
        }
    }

    #[test]
    fn zero_delta_is_noop() {
        let baseline = mk_combatant();
        let mut a = baseline.clone();
        let mut d = baseline.clone();
        for stat in StatKey::ALL {
            apply_perturbation(&mut a, &mut d, *stat, 0.0);
        }
        assert_eq!(a, baseline);
        assert_eq!(d, baseline);
    }

    #[test]
    fn weapon_damage_scales_attack_and_weapons() {
        let mut a = mk_combatant();
        let mut d = mk_combatant();
        a.weapons.push(crate::combat::WeaponStats {
            weapon_type: Default::default(),
            attack: 50.0,
            shots: Some(1),
            pierce: None,
            crit_chance: None,
            crit_multiplier: None,
            proc_chance: None,
            proc_multiplier: None,
        });
        apply_perturbation(&mut a, &mut d, StatKey::WeaponDamage, 0.05);
        assert!((a.attack - 105.0).abs() < 1e-9);
        assert!((a.weapons[0].attack - 52.5).abs() < 1e-9);
    }

    #[test]
    fn crit_chance_clamps_to_one() {
        let mut a = mk_combatant();
        let mut d = mk_combatant();
        a.crit_chance = 0.99;
        apply_perturbation(&mut a, &mut d, StatKey::CritChance, 0.05);
        assert!((a.crit_chance - 1.0).abs() < 1e-9);
    }

    #[test]
    fn stat_key_from_str_roundtrip() {
        for stat in StatKey::ALL {
            assert_eq!(StatKey::parse_str(stat.as_str()), Some(*stat));
        }
        assert_eq!(StatKey::parse_str("nonexistent_stat"), None);
    }

    #[test]
    fn default_deltas_are_finite_and_nonzero() {
        for stat in StatKey::ALL {
            let d = stat.default_delta();
            assert!(d.is_finite(), "{} delta not finite", stat.as_str());
            assert!(d > 0.0, "{} delta not positive", stat.as_str());
        }
    }

    #[test]
    fn crit_damage_floor_perturbation_is_additive_and_clamped_at_zero() {
        let mut a = mk_combatant();
        let mut d = mk_combatant();
        a.crit_damage_floor = 0.5;
        apply_perturbation(&mut a, &mut d, StatKey::CritDamageFloor, 0.25);
        assert!(
            (a.crit_damage_floor - 0.75).abs() < 1e-9,
            "expected 0.75, got {}",
            a.crit_damage_floor
        );
        // Defender is untouched.
        assert!(d.crit_damage_floor == 0.0);
        // Negative delta clamps the field at zero.
        apply_perturbation(&mut a, &mut d, StatKey::CritDamageFloor, -10.0);
        assert!(a.crit_damage_floor == 0.0);
    }

    #[test]
    fn stat_key_all_contains_18_entries() {
        // 18 = 19 (pre-CritDamageReduction-removal) − 1. `crit_damage_reduction` was
        // dropped from the sensitivity catalog because it had no natural home as a
        // resolved-once Combatant scalar; the bookkeeping for it (a perturb-only field
        // on Combatant + a special engine branch) was sensitivity infrastructure
        // leaking into the combat engine for one stat. See PR notes for the removal.
        assert_eq!(StatKey::ALL.len(), 18);
        assert!(StatKey::ALL.contains(&StatKey::CritDamageFloor));
        assert_eq!(
            StatKey::parse_str("crit_damage_floor"),
            Some(StatKey::CritDamageFloor)
        );
        // The removed stat must not round-trip through parse_str.
        assert_eq!(StatKey::parse_str("crit_damage_reduction"), None);
    }
}
