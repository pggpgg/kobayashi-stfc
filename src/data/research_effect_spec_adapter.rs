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
    if !key.attacker_factions.is_empty() {
        let mut owner_specs: Vec<AbilityConditionSpec> = Vec::new();
        for raw in &key.attacker_factions {
            let s = raw.trim();
            if !s.is_empty() {
                owner_specs.push(AbilityConditionSpec::AttackerOwnerFactionIs {
                    faction: s.to_string(),
                });
            }
        }
        match owner_specs.len() {
            0 => {}
            1 => parts.push(owner_specs.pop().expect("len checked")),
            _ => parts.push(AbilityConditionSpec::Or { any: owner_specs }),
        }
    } else if let Some(ref raw) = key.attacker_faction {
        let s = raw.trim();
        if !s.is_empty() {
            parts.push(AbilityConditionSpec::AttackerOwnerFactionIs {
                faction: s.to_string(),
            });
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

/// Catalog `isolytic_damage` / `apex_barrier` + `requires_morale` uses round-start timing; merge an explicit morale
/// gate with any other [`ResearchBonusConditionKey`] fields so dual gates are not dropped. Skips a
/// duplicate `MoraleActive` when the key also sets `requires_morale`.
fn morale_gated_research_condition_specs(
    key: &ResearchBonusConditionKey,
) -> Vec<AbilityConditionSpec> {
    let mut parts = vec![AbilityConditionSpec::MoraleActive];
    if let Some(from_key) = research_bonus_key_to_condition_specs(key) {
        for spec in from_key {
            if spec == AbilityConditionSpec::MoraleActive {
                continue;
            }
            parts.push(spec);
        }
    }
    parts
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
        "armor" => Some(AbilityModifierSpec::MitigationAdditive),
        "shield_deflection" => Some(AbilityModifierSpec::ShieldDeflection),
        "dodge" => Some(AbilityModifierSpec::Dodge),
        "damage_reduction" => Some(AbilityModifierSpec::DamageReduction),
        "accuracy" => Some(AbilityModifierSpec::Accuracy),
        "isolytic_damage" => Some(AbilityModifierSpec::IsolyticDamage),
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
    let max_by_level_len = override_entry
        .effects
        .iter()
        .map(|e| e.by_level.len())
        .max()
        .unwrap_or(0) as u32;
    let clamped_level = player_level.min(max_by_level_len);
    if clamped_level == 0 {
        return out;
    }

    for effect in &override_entry.effects {
        if effect.incoming_shield_mitigation_rounds.is_some() {
            // Handled in scenario → SimulationConfig (incoming damage / counter-fire only).
            continue;
        }
        // Default: sum by_level[0..clamped_level]. Snapshot: single tier total at by_level[level-1].
        let cumulative: f64 = if effect.snapshot_by_level {
            effect
                .by_level
                .get(clamped_level.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(0.0)
        } else {
            effect.by_level.iter().take(clamped_level as usize).sum()
        };
        if !cumulative.is_finite() || cumulative == 0.0 {
            continue;
        }

        let name_idx = idx.saturating_add(1);
        let trigger = effect.trigger.unwrap_or(AbilityTriggerSpec::AttackPhase);
        let spec = CombatEffectSpec {
            id: format!("canonical_{}_{name_idx}", effect.id),
            source: EffectSource::ResearchCatalog,
            source_ref: effect.source_ref.clone(),
            text: None,
            trigger,
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
            decay: None,
            accumulate: None,
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
        } else if let Ok((timing, ability_effect, condition)) = compile_officer_combat_spec(&spec) {
            *idx = name_idx;
            out.push(CrewSeatContext {
                seat: CrewSeat::Ship,
                ability: Ability {
                    name: spec.id.clone(),
                    class: AbilityClass::ShipAbility,
                    timing,
                    boostable: false,
                    effect: ability_effect,
                    condition,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            });
        }
    }
    out
}

/// KSG-style incoming shield mitigation: canonical effects marked with
/// [`ResearchCanonicalEffectEntry::incoming_shield_mitigation_rounds`]. Returns additive fraction
/// (same units as [`crate::combat::Combatant::shield_mitigation`]) and the maximum round count
/// across matching effects.
pub fn incoming_shield_mitigation_for_combat(
    imported_research: &[ResearchEntry],
    canonical_overrides: &HashMap<i64, ResearchCanonicalOverride>,
) -> (f64, u32) {
    let levels_by_rid = research_levels_by_rid_from_import(imported_research);
    let mut bonus = 0.0_f64;
    let mut rounds_out = 0_u32;
    for (&rid, &player_level) in &levels_by_rid {
        if player_level == 0 {
            continue;
        }
        let Some(ov) = canonical_overrides.get(&rid) else {
            continue;
        };
        for effect in &ov.effects {
            let Some(rounds) = effect.incoming_shield_mitigation_rounds else {
                continue;
            };
            if effect.modifier != AbilityModifierSpec::ShieldMitigation {
                continue;
            }
            let max_len = effect.by_level.len() as u32;
            if max_len == 0 {
                continue;
            }
            let clamped_level = player_level.min(max_len);
            let scalar = if effect.snapshot_by_level {
                effect
                    .by_level
                    .get(clamped_level.saturating_sub(1) as usize)
                    .copied()
                    .unwrap_or(0.0)
            } else {
                effect.by_level.iter().take(clamped_level as usize).sum()
            };
            if scalar.is_finite() && scalar != 0.0 {
                bonus += scalar;
                rounds_out = rounds_out.max(rounds);
            }
        }
    }
    (bonus, rounds_out)
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
            canonical_handled_rids.push(rid);
            out.extend(seats);
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
                    // applied during the isolytic damage leg). Morale-gated catalog `isolytic_damage`
                    // and `apex_barrier` (`requires_morale`) use round-start timing (matches officer seats).
                    let is_cascade = norm == "isolytic_cascade_damage";
                    let is_morale_gated_round_start = (norm == "isolytic_damage" || norm == "apex_barrier")
                        && key.requires_morale;

                    let condition_specs = if is_morale_gated_round_start {
                        Some(morale_gated_research_condition_specs(&key))
                    } else {
                        research_bonus_key_to_condition_specs(&key)
                    };
                    let has_conditions = condition_specs.is_some();

                    let trigger = if is_cascade {
                        AbilityTriggerSpec::AttackPhase
                    } else if is_morale_gated_round_start {
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
                        decay: None,
                        accumulate: None,
                        conditions: condition_specs.unwrap_or_default(),
                        attributes: serde_json::Map::new(),
                        stacking: None,
                        category: Some(EffectCategory::Combat),
                        confidence: Some(EffectConfidence::Authoritative),
                    };
                    if validate_combat_effect_spec(&spec).is_err() {
                        continue;
                    }

                    let use_attack_phase_path =
                        is_cascade || (!is_morale_gated_round_start && has_conditions);
                    if use_attack_phase_path {
                        if let Ok(ctx) = compile_research_attack_phase_spec_to_seat(&spec) {
                            idx = name_idx;
                            out.push(ctx);
                        } else if let Ok((timing, effect, condition)) =
                            compile_officer_combat_spec(&spec)
                        {
                            // Mitigation (`armor`/`dodge`/…) + `shield_deflection` compile
                            // via LCARS/officer rules; WD/crit narrow path may reject other modifiers.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::combat_effect_spec::{AbilityModifierSpec, AbilityOperationSpec};
    use crate::data::import::ResearchEntry;
    use crate::data::research::{ResearchCanonicalEffectEntry, ResearchCanonicalOverride};

    #[test]
    fn incoming_ksg_sm_is_snapshot_total_and_round_count() {
        let rid = 2392190200_i64;
        let mut overrides = HashMap::new();
        overrides.insert(
            rid,
            ResearchCanonicalOverride {
                rid,
                name: None,
                source_note: None,
                effects: vec![ResearchCanonicalEffectEntry {
                    id: "test_ksg".into(),
                    modifier: AbilityModifierSpec::ShieldMitigation,
                    operation: AbilityOperationSpec::Add,
                    by_level: vec![0.005, 0.01, 0.015, 0.02, 0.025],
                    conditions: vec![],
                    trigger: None,
                    category: None,
                    confidence: None,
                    source_ref: None,
                    snapshot_by_level: true,
                    incoming_shield_mitigation_rounds: Some(2),
                }],
            },
        );
        let imported = vec![ResearchEntry { rid, level: 5 }];
        let (bonus, rounds) = incoming_shield_mitigation_for_combat(&imported, &overrides);
        assert!(
            (bonus - 0.025).abs() < 1e-9,
            "expected 2.5% tier snapshot, got {bonus}"
        );
        assert_eq!(rounds, 2);
    }
}
