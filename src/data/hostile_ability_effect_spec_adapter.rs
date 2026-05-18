//! Hostile ability catalog → [`CombatEffectSpec`] (canonical IR).
//!
//! Maps hostile abilities from the catalog into [`crate::data::combat_effect_spec::CombatEffectSpec`]
//! rows. Proc effects (attack_multiplier, pierce_bonus) carry both a `chance` and a `value`.

use crate::data::combat_effect_spec::{
    AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec, AbilityTriggerSpec, ChanceSpec,
    CombatEffectSpec, EffectCategory, EffectConfidence, EffectSource, ValueSpec,
};
use crate::data::hostile_ability_resolve::HostileAbilityCatalogEntry;

/// Map a hostile ability catalog `effect_type` string to an [`AbilityModifierSpec`].
pub fn hostile_ability_effect_type_to_modifier(effect_type: &str) -> Option<AbilityModifierSpec> {
    match effect_type.trim().to_lowercase().replace('-', "_").as_str() {
        "combat_noop" | "unmodeled" | "not_applicable" => None,
        "attack_multiplier" | "weapon_damage" | "attack" => {
            Some(AbilityModifierSpec::ProcAttackMultiplier)
        }
        "pierce_bonus" | "armor_pierce" | "shield_pierce" => {
            Some(AbilityModifierSpec::ProcPierceBonus)
        }
        "hostile_crit_damage_reduction" | "reduce_attacker_crit_damage" => None,
        _ => None,
    }
}

/// Convert a hostile ability catalog entry + resolved value/chance into a [`CombatEffectSpec`].
/// `ability_id` is the upstream numeric id string; `chance` is the raw probability (0–100 scale).
pub fn hostile_ability_to_combat_effect_spec(
    ability_id: &str,
    entry: &HostileAbilityCatalogEntry,
    chance: f64,
    value: f64,
) -> Option<CombatEffectSpec> {
    let modifier = hostile_ability_effect_type_to_modifier(&entry.effect_type)?;

    let timing = crate::data::ship_ability_resolve::parse_ship_ability_timing(&entry.timing)?;
    let trigger_spec = match timing {
        crate::combat::TimingWindow::CombatBegin => AbilityTriggerSpec::CombatBegin,
        crate::combat::TimingWindow::RoundStart => AbilityTriggerSpec::RoundStart,
        crate::combat::TimingWindow::AttackPhase => AbilityTriggerSpec::AttackPhase,
        crate::combat::TimingWindow::AfterSubround => AbilityTriggerSpec::AfterSubround,
        crate::combat::TimingWindow::DefensePhase => AbilityTriggerSpec::DefensePhase,
        crate::combat::TimingWindow::RoundEnd => AbilityTriggerSpec::RoundEnd,
        crate::combat::TimingWindow::ShieldBreak => AbilityTriggerSpec::ShieldBreak,
        crate::combat::TimingWindow::SelfShieldBreak => AbilityTriggerSpec::SelfShieldBreak,
        crate::combat::TimingWindow::Kill => AbilityTriggerSpec::Kill,
        crate::combat::TimingWindow::HullBreach => AbilityTriggerSpec::HullBreach,
        crate::combat::TimingWindow::ReceiveDamage => AbilityTriggerSpec::ReceiveDamage,
        crate::combat::TimingWindow::CombatEnd => AbilityTriggerSpec::CombatEnd,
    };

    let p = crate::data::ship_ability_resolve::normalize_probability(chance);

    Some(CombatEffectSpec {
        id: ability_id.to_string(),
        source: EffectSource::HostileAbilityCatalog,
        source_ref: None,
        text: None,
        trigger: trigger_spec,
        target: AbilityTargetSpec::AttackerSelf,
        modifier,
        operation: AbilityOperationSpec::ChanceApply,
        value: Some(ValueSpec {
            scalar: Some(value),
            by_rank: None,
            unit: None,
            officer_stat_scaling: None,
        }),
        chance: Some(ChanceSpec {
            scalar: Some(p),
            by_rank: None,
        }),
        duration: None,
        decay: None,
        accumulate: None,
        conditions: Vec::new(),
        attributes: serde_json::Map::new(),
        stacking: None,
        category: Some(EffectCategory::Combat),
        confidence: Some(EffectConfidence::Authoritative),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::hostile_ability_resolve::HostileAbilityCatalogEntry;

    #[test]
    fn hostile_ability_effect_type_map_covers_resolved_types() {
        for effect_type in ["attack_multiplier", "pierce_bonus"] {
            assert!(
                hostile_ability_effect_type_to_modifier(effect_type).is_some(),
                "hostile ability effect_type '{effect_type}' should map"
            );
        }
    }

    #[test]
    fn unmapped_effect_type_returns_none() {
        assert!(hostile_ability_effect_type_to_modifier("combat_noop").is_none());
        assert!(hostile_ability_effect_type_to_modifier("unknown_xyz").is_none());
    }

    #[test]
    fn hostile_ability_maps_to_spec() {
        let entry = HostileAbilityCatalogEntry {
            timing: "round_start".into(),
            effect_type: "attack_multiplier".into(),
            value_is_percentage: true,
            ignore_upstream_value_is_percentage: false,
            value_override: None,
            duration_rounds: None,
        };
        let spec =
            hostile_ability_to_combat_effect_spec("123", &entry, 100.0, 0.15).expect("should map");
        assert_eq!(spec.modifier, AbilityModifierSpec::ProcAttackMultiplier);
        assert_eq!(spec.trigger, AbilityTriggerSpec::RoundStart);
        let v = spec.value.as_ref().and_then(|v| v.scalar).unwrap();
        assert!((v - 0.15).abs() < 1e-12);
        let c = spec.chance.as_ref().and_then(|c| c.scalar).unwrap();
        assert!((c - 1.0).abs() < 1e-12);
    }
}
