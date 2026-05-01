//! Research catalog → [`CombatEffectSpec`] → compiled [`crate::combat::CrewSeatContext`].

use crate::combat::effect_spec_compile::compile_research_attack_phase_spec_to_seat;
use crate::combat::CrewSeatContext;
use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, CombatEffectSpec, EffectCategory, EffectConfidence, EffectSource,
    ValueSpec,
};
use crate::data::combat_effect_spec_validate::validate_combat_effect_spec;
use crate::data::import::ResearchEntry;
use crate::data::profile::{normalize_profile_combat_stat, research_levels_by_rid_from_import};
use crate::data::research::{
    cumulative_conditional_research_bonuses, ResearchBonusConditionKey, ResearchCatalog,
};

/// Map [`ResearchBonusConditionKey`] to spec condition nodes (same order as
/// [`crate::combat::condition::ability_condition_from_research_bonus_key`]).
pub fn research_bonus_key_to_condition_specs(
    key: &ResearchBonusConditionKey,
) -> Option<Vec<AbilityConditionSpec>> {
    let mut parts: Vec<AbilityConditionSpec> = Vec::new();
    if key.requires_morale {
        parts.push(AbilityConditionSpec::MoraleActive);
    }
    if key.requires_defender_burning {
        parts.push(AbilityConditionSpec::DefenderBurning);
    }
    if key.requires_defender_hull_breach {
        parts.push(AbilityConditionSpec::DefenderHullBreach);
    }
    if let Some(ref slug) = key.defender_faction {
        parts.push(AbilityConditionSpec::DefenderFactionIs {
            faction: slug.clone(),
        });
    }
    if let Some(ref slug) = key.defender_ship_class {
        parts.push(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: slug.clone(),
        });
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn norm_to_modifier(norm: &str) -> Option<AbilityModifierSpec> {
    match norm {
        "weapon_damage" => Some(AbilityModifierSpec::WeaponDamage),
        "crit_chance" => Some(AbilityModifierSpec::CritChance),
        "crit_damage" => Some(AbilityModifierSpec::CritDamage),
        "apex_barrier" => Some(AbilityModifierSpec::ApexBarrier),
        "apex_shred" => Some(AbilityModifierSpec::ApexShred),
        _ => None,
    }
}

/// Same selection rules as the historical inline research seat builder (conditional bonuses only;
/// `weapon_damage` / `crit_*` with [`ResearchBonusConditionKey`]).
/// implemented via `CombatEffectSpec` + [`compile_research_attack_phase_spec`].
pub fn research_derived_attack_phase_seats_from_spec(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
) -> Vec<CrewSeatContext> {
    if imported_research.is_empty() || catalog.items.is_empty() {
        return Vec::new();
    }
    let levels_by_rid = research_levels_by_rid_from_import(imported_research);
    if levels_by_rid.is_empty() {
        return Vec::new();
    }

    let records: Vec<&crate::data::research::ResearchRecord> = catalog
        .items
        .iter()
        .filter(|r| levels_by_rid.contains_key(&r.rid))
        .collect();
    if records.is_empty() {
        return Vec::new();
    }

    let conditional = cumulative_conditional_research_bonuses(&records, &levels_by_rid);
    let mut out: Vec<CrewSeatContext> = Vec::new();
    let mut idx = 0u32;
    for ((key, stat), value) in conditional {
        if !value.is_finite() || value == 0.0 {
            continue;
        }
        let Some(norm) = normalize_profile_combat_stat(&stat) else {
            continue;
        };
        if norm != "crit_chance" && norm != "crit_damage" && norm != "weapon_damage"
            && norm != "apex_barrier" && norm != "apex_shred"
        {
            continue;
        }
        let Some(condition_specs) = research_bonus_key_to_condition_specs(&key) else {
            continue;
        };
        let Some(modifier) = norm_to_modifier(norm) else {
            continue;
        };
        let name_idx = idx.saturating_add(1);
        let spec = CombatEffectSpec {
            id: format!("research_{norm}_{name_idx}"),
            source: EffectSource::ResearchCatalog,
            source_ref: None,
            text: None,
            trigger: AbilityTriggerSpec::AttackPhase,
            target: AbilityTargetSpec::AttackerSelf,
            modifier,
            operation: AbilityOperationSpec::Add,
            value: Some(ValueSpec {
                scalar: Some(value),
                by_rank: None,
                unit: None,
                officer_stat_scaling: None,
            }),
            chance: None,
            duration: None,
            conditions: condition_specs,
            attributes: serde_json::Map::new(),
            stacking: None,
            category: Some(EffectCategory::Combat),
            confidence: Some(EffectConfidence::Authoritative),
        };
        if validate_combat_effect_spec(&spec).is_err() {
            continue;
        }
        let Ok(ctx) = compile_research_attack_phase_spec_to_seat(&spec) else {
            continue;
        };
        idx = name_idx;
        out.push(ctx);
    }
    out
}
