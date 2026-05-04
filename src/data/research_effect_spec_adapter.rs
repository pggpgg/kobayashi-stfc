//! Research catalog → [`CombatEffectSpec`] → compiled [`crate::combat::CrewSeatContext`].
//!
//! Canonical overrides (`data/research_canonical.json`) take priority over the auto-generated
//! catalog. When a researched RID has a canonical override entry, its effects are compiled
//! directly from the override definition; otherwise the existing catalog-based path is used.

use std::collections::HashMap;

use crate::combat::abilities::{Ability, AbilityClass, CrewSeat, NO_EXPLICIT_CONTRIBUTION_BATCH};
use crate::combat::effect_spec_compile::{
    compile_officer_combat_spec, compile_research_attack_phase_spec_to_seat,
};
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
    cumulative_conditional_research_bonuses, ResearchBonusConditionKey, ResearchCanonicalOverride,
    ResearchCatalog,
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
        "hull_hp" => Some(AbilityModifierSpec::HullHp),
        "shield_hp" => Some(AbilityModifierSpec::ShieldHp),
        "crit_chance" => Some(AbilityModifierSpec::CritChance),
        "crit_damage" => Some(AbilityModifierSpec::CritDamage),
        "pierce" | "armor_pierce" | "shield_pierce" => Some(AbilityModifierSpec::Pierce),
        "shield_mitigation" => Some(AbilityModifierSpec::ShieldMitigation),
        "armor" => Some(AbilityModifierSpec::Armor),
        "dodge" => Some(AbilityModifierSpec::Dodge),
        "damage_reduction" => Some(AbilityModifierSpec::DamageReduction),
        "accuracy" => Some(AbilityModifierSpec::Accuracy),
        "isolytic_damage" | "isolytic_damage_morale" => Some(AbilityModifierSpec::IsolyticDamage),
        "isolytic_defense" => Some(AbilityModifierSpec::IsolyticDefense),
        "isolytic_cascade_damage" | "isolytic_cascade" => {
            Some(AbilityModifierSpec::IsolyticCascadeDamage)
        }
        "apex_shred" => Some(AbilityModifierSpec::ApexShred),
        "apex_barrier" => Some(AbilityModifierSpec::ApexBarrier),
        _ => None,
    }
}

/// Compile canonical override effects for a single researched RID.
fn compile_canonical_override_seats(
    override_entry: &ResearchCanonicalOverride,
    player_level: u32,
    idx: &mut u32,
) -> Vec<CrewSeatContext> {
    let mut out: Vec<CrewSeatContext> = Vec::new();
    let clamped_level = player_level.min(
        override_entry
            .effects
            .first()
            .map_or(0, |e| e.by_level.len() as u32),
    );
    if clamped_level == 0 {
        return out;
    }

    for effect in &override_entry.effects {
        // Sum by_level[0..clamped_level] for the cumulative scalar.
        let cumulative: f64 = effect.by_level.iter().take(clamped_level as usize).sum();
        if !cumulative.is_finite() || cumulative == 0.0 {
            continue;
        }

        let name_idx = idx.saturating_add(1);
        let spec = CombatEffectSpec {
            id: format!("canonical_{}_{name_idx}", effect.id),
            source: EffectSource::ResearchCatalog,
            source_ref: effect.source_ref.clone(),
            text: None,
            trigger: AbilityTriggerSpec::AttackPhase,
            target: AbilityTargetSpec::AttackerSelf,
            modifier: effect.modifier,
            operation: effect.operation,
            value: Some(ValueSpec {
                scalar: Some(cumulative),
                by_rank: None,
                unit: None,
                officer_stat_scaling: None,
            }),
            chance: None,
            duration: None,
            conditions: effect.conditions.clone(),
            attributes: serde_json::Map::new(),
            stacking: None,
            category: effect.category,
            confidence: effect.confidence,
        };

        if validate_combat_effect_spec(&spec).is_err() {
            continue;
        }
        if let Ok(ctx) = compile_research_attack_phase_spec_to_seat(&spec) {
            *idx = name_idx;
            out.push(ctx);
        }
    }
    out
}

/// Build research-derived attack-phase seats. **Canonical overrides take priority**: for each
/// researched RID with a canonical entry, effects are compiled directly from the override.
/// Remaining RIDs fall back to the auto-generated catalog + conditional bonus aggregation.
pub fn research_derived_attack_phase_seats_from_spec(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    canonical_overrides: &HashMap<i64, ResearchCanonicalOverride>,
) -> Vec<CrewSeatContext> {
    if imported_research.is_empty() {
        return Vec::new();
    }
    let levels_by_rid = research_levels_by_rid_from_import(imported_research);
    if levels_by_rid.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<CrewSeatContext> = Vec::new();
    let mut idx = 0u32;

    // ── Canonical overrides (priority path) ─────────────────────────────────────
    // Track which RIDs were handled canonically so they are excluded from the
    // catalog fallback path.
    let mut canonical_handled_rids: Vec<i64> = Vec::new();
    for (&rid, &player_level) in &levels_by_rid {
        if let Some(ov) = canonical_overrides.get(&rid) {
            let seats = compile_canonical_override_seats(ov, player_level, &mut idx);
            if !seats.is_empty() {
                canonical_handled_rids.push(rid);
                out.extend(seats);
            }
        }
    }

    // ── Catalog fallback (auto-generated path, excluding RIDs with overrides) ──
    if !catalog.items.is_empty() {
        let excluded: std::collections::HashSet<i64> = canonical_handled_rids.into_iter().collect();

        let mut remaining_levels: HashMap<i64, u32> = HashMap::new();
        for (&rid, &level) in &levels_by_rid {
            if !excluded.contains(&rid) {
                remaining_levels.insert(rid, level);
            }
        }

        if !remaining_levels.is_empty() {
            let records: Vec<&crate::data::research::ResearchRecord> = catalog
                .items
                .iter()
                .filter(|r| remaining_levels.contains_key(&r.rid))
                .collect();

            if !records.is_empty() {
                let conditional =
                    cumulative_conditional_research_bonuses(&records, &remaining_levels);
                for ((key, stat), value) in conditional {
                    if !value.is_finite() || value == 0.0 {
                        continue;
                    }
                    let Some(norm) = normalize_profile_combat_stat(&stat) else {
                        continue;
                    };
                    let Some(modifier) = norm_to_modifier(norm) else {
                        continue;
                    };

                    // isolytic_cascade_damage always uses attack-phase timing (cascade stacks
                    // applied during the isolytic damage leg). isolytic_damage_morale uses
                    // round-start timing with a morale gate.
                    let is_cascade = norm == "isolytic_cascade_damage";
                    let is_morale_gated = norm == "isolytic_damage_morale";

                    let condition_specs = if is_morale_gated {
                        Some(vec![AbilityConditionSpec::MoraleActive])
                    } else {
                        research_bonus_key_to_condition_specs(&key)
                    };
                    let has_conditions = condition_specs.is_some();

                    let trigger = if is_cascade {
                        AbilityTriggerSpec::AttackPhase
                    } else if is_morale_gated {
                        AbilityTriggerSpec::RoundStart
                    } else if has_conditions {
                        AbilityTriggerSpec::AttackPhase
                    } else {
                        AbilityTriggerSpec::CombatBegin
                    };

                    let name_idx = idx.saturating_add(1);
                    let spec = CombatEffectSpec {
                        id: format!("research_{norm}_{name_idx}"),
                        source: EffectSource::ResearchCatalog,
                        source_ref: None,
                        text: None,
                        trigger,
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
                        conditions: condition_specs.unwrap_or_default(),
                        attributes: serde_json::Map::new(),
                        stacking: None,
                        category: Some(EffectCategory::Combat),
                        confidence: Some(EffectConfidence::Authoritative),
                    };
                    if validate_combat_effect_spec(&spec).is_err() {
                        continue;
                    }

                    let use_attack_phase_path = is_cascade || (!is_morale_gated && has_conditions);
                    if use_attack_phase_path {
                        let Ok(ctx) = compile_research_attack_phase_spec_to_seat(&spec) else {
                            continue;
                        };
                        idx = name_idx;
                        out.push(ctx);
                    } else {
                        let Ok((timing, effect, condition)) = compile_officer_combat_spec(&spec)
                        else {
                            continue;
                        };
                        idx = name_idx;
                        out.push(CrewSeatContext {
                            seat: CrewSeat::Ship,
                            ability: Ability {
                                name: spec.id.clone(),
                                class: AbilityClass::ShipAbility,
                                timing,
                                boostable: false,
                                effect,
                                condition,
                            },
                            boosted: false,
                            officer_id: None,
                            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                        });
                    }
                }
            }
        }
    }

    out
}
