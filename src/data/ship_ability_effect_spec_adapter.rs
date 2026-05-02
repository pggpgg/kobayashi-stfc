//! Ship ability catalog → [`CombatEffectSpec`] (canonical IR).
//!
//! Maps [`crate::data::ship::ShipAbility`] records into [`crate::data::combat_effect_spec::CombatEffectSpec`]
//! rows that compile through [`crate::combat::effect_spec_compile::compile_officer_combat_spec`].

use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, ChanceSpec, CombatEffectSpec, EffectCategory, EffectConfidence, EffectSource,
    ValueSpec,
};
use crate::data::ship::ShipAbility;

/// Map a ship ability catalog `effect_type` string to an [`AbilityModifierSpec`].
pub fn ship_ability_effect_type_to_modifier(effect_type: &str) -> Option<AbilityModifierSpec> {
    match effect_type.trim().to_lowercase().replace('-', "_").as_str() {
        "combat_noop" | "unmodeled" | "not_applicable" => None,
        "hostile_crit_damage_reduction" | "reduce_hostile_crit_damage" => {
            Some(AbilityModifierSpec::HostileCritDamageReduction)
        }
        "pierce_bonus" | "armor_pierce" | "shield_pierce" => Some(AbilityModifierSpec::Pierce),
        "attack_multiplier" | "weapon_damage" | "attack" => Some(AbilityModifierSpec::WeaponDamage),
        "accumulating_attack_multiplier"
        | "cumulative_weapon_damage"
        | "additive_weapon_damage_growth"
        | "galaxy_additive_weapon_damage_growth"
        | "galaxy_class_weapon_damage_growth" => Some(AbilityModifierSpec::WeaponDamage),
        "crit_chance" => Some(AbilityModifierSpec::CritChance),
        "crit_damage" => Some(AbilityModifierSpec::CritDamage),
        "apex_shred" => Some(AbilityModifierSpec::ApexShred),
        "apex_barrier" => Some(AbilityModifierSpec::ApexBarrier),
        "conqueror_borg_beam_suppression" | "borg_conqueror_beam_suppression" => None,
        "shield_regen" | "shield_hp_repair" => Some(AbilityModifierSpec::OfficerShieldRegenFlat),
        "hull_regen" | "hull_hp_repair" | "hull_repair" => {
            Some(AbilityModifierSpec::OfficerHullRegenFlat)
        }
        "isolytic_damage" => Some(AbilityModifierSpec::IsolyticDamage),
        "isolytic_defense" => Some(AbilityModifierSpec::IsolyticDefense),
        "isolytic_cascade" | "isolytic_cascade_damage" => {
            Some(AbilityModifierSpec::IsolyticCascadeDamage)
        }
        "shield_mitigation" => Some(AbilityModifierSpec::ShieldMitigation),
        "morale" => Some(AbilityModifierSpec::StateMorale),
        "assimilated" => Some(AbilityModifierSpec::StateAssimilated),
        "hull_breach" => Some(AbilityModifierSpec::StateHullBreach),
        "burning" => Some(AbilityModifierSpec::StateBurning),
        "shots" | "weapon_shots" | "shots_per_weapon" | "shots_per_attack" | "shots_bonus" => {
            Some(AbilityModifierSpec::ShotsBonus)
        }
        "accuracy" | "accuracy_bonus" => Some(AbilityModifierSpec::Accuracy),
        _ => None,
    }
}

/// True when this effect_type represents a state application (morale, burning, hull_breach, assimilated)
/// that carries a chance rather than a numeric modifier value.
fn is_state_effect_type(effect_type: &str) -> bool {
    matches!(
        effect_type.trim().to_lowercase().replace('-', "_").as_str(),
        "morale" | "burning" | "hull_breach" | "assimilated"
    )
}

/// Build condition specs from a ship ability's condition fields.
fn ship_ability_condition_specs(ability: &ShipAbility) -> Vec<AbilityConditionSpec> {
    let mut parts: Vec<AbilityConditionSpec> = Vec::new();
    if ability.condition_morale {
        parts.push(AbilityConditionSpec::MoraleActive);
    }
    if ability.condition_defender_burning {
        parts.push(AbilityConditionSpec::DefenderBurning);
    }
    if ability.condition_defender_hull_breach {
        parts.push(AbilityConditionSpec::DefenderHullBreach);
    }
    if let Some(ref faction) = ability.condition_opponent_faction {
        parts.push(AbilityConditionSpec::DefenderFactionIs {
            faction: faction.clone(),
        });
    }
    if let Some(ref ship_class) = ability.condition_opponent_ship_class {
        parts.push(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: ship_class.clone(),
        });
    }
    if let Some(ref round_cap) = ability.round_cap {
        parts.push(AbilityConditionSpec::RoundRange {
            min: 1,
            max: *round_cap,
        });
    }
    parts
}

/// Convert a [`ShipAbility`] to a [`CombatEffectSpec`], or [`None`] when the effect type or
/// timing cannot be mapped.
pub fn ship_ability_to_combat_effect_spec(ability: &ShipAbility) -> Option<CombatEffectSpec> {
    let modifier = ship_ability_effect_type_to_modifier(&ability.effect_type)?;

    let trigger = crate::data::ship_ability_resolve::parse_ship_ability_timing(&ability.timing)?;
    let trigger_spec = match trigger {
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

    let conditions = ship_ability_condition_specs(ability);

    let value = if is_state_effect_type(&ability.effect_type) {
        None
    } else {
        Some(ValueSpec {
            scalar: Some(ability.value),
            by_rank: None,
            unit: None,
            officer_stat_scaling: None,
        })
    };

    let chance = if is_state_effect_type(&ability.effect_type) {
        let p = crate::data::ship_ability_resolve::normalize_probability(ability.value);
        Some(ChanceSpec {
            scalar: Some(p),
            by_rank: None,
        })
    } else {
        None
    };

    let operation = match modifier {
        AbilityModifierSpec::CritDamage => AbilityOperationSpec::Multiply,
        _ => AbilityOperationSpec::Add,
    };

    Some(CombatEffectSpec {
        id: ability.id.clone(),
        source: EffectSource::ShipAbilityCatalog,
        source_ref: None,
        text: None,
        trigger: trigger_spec,
        target: AbilityTargetSpec::AttackerSelf,
        modifier,
        operation,
        value,
        chance,
        duration: ability
            .duration_rounds
            .map(|r| crate::data::combat_effect_spec::DurationSpec::Rounds { rounds: r }),
        conditions,
        attributes: serde_json::Map::new(),
        stacking: None,
        category: Some(EffectCategory::Combat),
        confidence: Some(EffectConfidence::Authoritative),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::TimingWindow;

    #[test]
    fn ship_ability_effect_type_map_covers_all_resolved_types() {
        // Every effect_type that ship_ability_effect_from_catalog handles should map.
        for (effect_type, _timing) in [
            ("attack_multiplier", TimingWindow::CombatBegin),
            ("crit_chance", TimingWindow::RoundStart),
            ("crit_damage", TimingWindow::RoundStart),
            ("apex_shred", TimingWindow::CombatBegin),
            ("apex_barrier", TimingWindow::CombatBegin),
            ("pierce_bonus", TimingWindow::CombatBegin),
            ("shield_regen", TimingWindow::RoundStart),
            ("hull_regen", TimingWindow::RoundEnd),
            ("isolytic_damage", TimingWindow::CombatBegin),
            ("isolytic_defense", TimingWindow::CombatBegin),
            ("isolytic_cascade", TimingWindow::CombatBegin),
            ("shield_mitigation", TimingWindow::CombatBegin),
            ("morale", TimingWindow::CombatBegin),
            ("burning", TimingWindow::CombatBegin),
            ("hull_breach", TimingWindow::CombatBegin),
            ("assimilated", TimingWindow::CombatBegin),
            ("hostile_crit_damage_reduction", TimingWindow::CombatBegin),
        ] {
            assert!(
                ship_ability_effect_type_to_modifier(effect_type).is_some(),
                "ship ability effect_type '{effect_type}' should map"
            );
        }
    }

    #[test]
    fn ship_ability_with_conditions_maps_to_spec() {
        let ability = ShipAbility {
            id: "test_ability".into(),
            timing: "combat_begin".into(),
            effect_type: "attack_multiplier".into(),
            value: 0.15,
            duration_rounds: None,
            condition_morale: true,
            condition_defender_burning: true,
            condition_defender_hull_breach: false,
            condition_opponent_faction: Some("klingon".into()),
            condition_opponent_ship_class: None,
            condition_opponent_hostile_tags: None,
            round_cap: Some(5),
            level_scaled_values: None,
        };
        let spec = ship_ability_to_combat_effect_spec(&ability).expect("should map");
        assert_eq!(spec.modifier, AbilityModifierSpec::WeaponDamage);
        assert_eq!(spec.trigger, AbilityTriggerSpec::CombatBegin);
        assert_eq!(spec.conditions.len(), 4); // morale, burning, faction, round_cap
        assert!(spec
            .conditions
            .contains(&AbilityConditionSpec::MoraleActive));
        assert!(spec
            .conditions
            .contains(&AbilityConditionSpec::DefenderBurning));
        assert!(spec
            .conditions
            .contains(&AbilityConditionSpec::DefenderFactionIs {
                faction: "klingon".into()
            }));
        assert!(spec
            .conditions
            .contains(&AbilityConditionSpec::RoundRange { min: 1, max: 5 }));
        let v = spec.value.as_ref().and_then(|v| v.scalar).unwrap();
        assert!((v - 0.15).abs() < 1e-12);
    }

    #[test]
    fn unmapped_effect_type_returns_none() {
        let ability = ShipAbility {
            id: "unknown".into(),
            timing: "combat_begin".into(),
            effect_type: "unknown_effect_xyz".into(),
            value: 1.0,
            duration_rounds: None,
            condition_morale: false,
            condition_defender_burning: false,
            condition_defender_hull_breach: false,
            condition_opponent_faction: None,
            condition_opponent_ship_class: None,
            condition_opponent_hostile_tags: None,
            round_cap: None,
            level_scaled_values: None,
        };
        assert!(ship_ability_to_combat_effect_spec(&ability).is_none());
    }
}
