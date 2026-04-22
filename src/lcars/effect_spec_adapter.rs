//! LCARS YAML → [`crate::data::combat_effect_spec::CombatEffectSpec`] (canonical IR).
//!
//! Officer dynamic effects resolve through this adapter plus
//! [`crate::combat::effect_spec_compile::compile_officer_combat_spec`] (see
//! [`crate::lcars::resolver::resolve_effect`]). HTTP debug and parity tests use the same path.

use crate::combat::effect_spec_compile::{
    OFFICER_SPEC_ATTR_LCARS_OP, OFFICER_SPEC_ATTR_WEAPON_DAMAGE_ACCUMULATE,
    OFFICER_SPEC_ATTR_WEAPON_DAMAGE_DECAY,
};
use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, ChanceSpec, CombatEffectSpec, DurationSpec, EffectSource, ValueSpec,
};
use crate::lcars::parser::{LcarsCondition, LcarsDuration, LcarsEffect};
use crate::lcars::resolver::effect_trigger_timing;
use serde_json::json;

fn normalize_trigger(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace('-', "_")
}

fn normalize_operator(op: Option<&str>) -> String {
    op.unwrap_or("add")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
}

/// Map LCARS `trigger` string to canonical [`AbilityTriggerSpec`]. Unknown → [`None`].
pub fn lcars_trigger_str_to_spec(trigger: &str) -> Option<AbilityTriggerSpec> {
    let t = normalize_trigger(trigger);
    match t.as_str() {
        "on_own_shield_break" | "self_shields_depleted" | "own_shields_depleted" => {
            Some(AbilityTriggerSpec::SelfShieldBreak)
        }
        "on_enemy_shield_break"
        | "enemy_shields_depleted"
        | "target_shields_depleted"
        | "targetshieldsdepleted" => Some(AbilityTriggerSpec::ShieldBreak),
        "shieldsdepleted" | "on_shield_break" => None,
        "passive" => Some(AbilityTriggerSpec::CombatBegin),
        "combatstart" | "on_combat_start" => Some(AbilityTriggerSpec::CombatBegin),
        "ship_launched" | "shiplaunched" => Some(AbilityTriggerSpec::ShipLaunched),
        "roundstart" | "on_round_start" => Some(AbilityTriggerSpec::RoundStart),
        "criticalshotfired" | "enemytakeshit" | "on_attack" | "on_hit" | "on_critical" => {
            Some(AbilityTriggerSpec::AttackPhase)
        }
        "after_shot" | "on_after_shot" | "subround_end" | "on_subround_end" | "after_weapon"
        | "on_after_weapon" => Some(AbilityTriggerSpec::AfterSubround),
        "hittaken" | "on_defense" => Some(AbilityTriggerSpec::DefensePhase),
        "roundend" | "on_round_end" => Some(AbilityTriggerSpec::RoundEnd),
        "battlewon" | "on_kill" => Some(AbilityTriggerSpec::Kill),
        "hulldamagetaken" | "on_hull_breach" => Some(AbilityTriggerSpec::HullBreach),
        "shielddamagetaken" | "on_receive_damage" => Some(AbilityTriggerSpec::ReceiveDamage),
        "on_combat_end" => Some(AbilityTriggerSpec::CombatEnd),
        _ => None,
    }
}

/// LCARS condition → spec IR (same coverage as [`crate::lcars::resolver::resolve_lcars_condition`]).
pub fn lcars_condition_to_spec(c: &LcarsCondition) -> Result<AbilityConditionSpec, String> {
    let ty = c.condition_type.trim().to_lowercase().replace('-', "_");
    match ty.as_str() {
        "stat_below" => Ok(AbilityConditionSpec::StatBelow {
            stat: c.stat.clone().unwrap_or_else(|| "hull_hp".to_string()),
            threshold_pct: c.threshold_pct.unwrap_or(0.5),
        }),
        "stat_above" => Ok(AbilityConditionSpec::StatAbove {
            stat: c.stat.clone().unwrap_or_else(|| "hull_hp".to_string()),
            threshold_pct: c.threshold_pct.unwrap_or(0.8),
        }),
        "round_range" => Ok(AbilityConditionSpec::RoundRange {
            min: c.min.unwrap_or(1),
            max: c.max.unwrap_or(100),
        }),
        "morale_active" | "attacker_morale" | "morale" => Ok(AbilityConditionSpec::MoraleActive),
        "defender_is_npc_hostile" | "defender_npc_hostile" | "enemy_hostile" => {
            Ok(AbilityConditionSpec::DefenderIsNpcHostile)
        }
        "defender_is_player_ship" | "defender_player_ship" | "enemy_player" => {
            Ok(AbilityConditionSpec::DefenderIsPlayerShip)
        }
        "defender_burning" | "target_burning" | "burning" => {
            Ok(AbilityConditionSpec::DefenderBurning)
        }
        "defender_hull_breach" | "target_hull_breach" | "hull_breach_active" => {
            Ok(AbilityConditionSpec::DefenderHullBreach)
        }
        "attacker_burning" | "self_burning" | "player_burning" => {
            Ok(AbilityConditionSpec::AttackerBurning)
        }
        "attacker_hull_breach" | "self_hull_breach" | "player_hull_breach" => {
            Ok(AbilityConditionSpec::AttackerHullBreach)
        }
        "defender_assimilated" | "target_assimilated" => {
            Ok(AbilityConditionSpec::DefenderAssimilated)
        }
        "attacker_officer_tal_not_on_bridge" | "self_officer_tal_not_on_bridge" => {
            Ok(AbilityConditionSpec::AttackerOfficerTalNotOnBridge)
        }
        "defender_faction_is"
        | "defender_faction"
        | "opponent_faction_is"
        | "opponent_faction"
        | "faction_is" => {
            let slug = c.faction.as_deref().or(c.tag.as_deref()).ok_or_else(|| {
                "faction condition requires `faction` or `tag` with a known faction slug"
                    .to_string()
            })?;
            Ok(AbilityConditionSpec::DefenderFactionIs {
                faction: slug.to_string(),
            })
        }
        "defender_hull_faction_id" | "enemy_hull_faction" | "enemy_hull_faction_id" => {
            let id = c.faction_id.ok_or_else(|| {
                format!(
                    "{ty} condition requires integer `faction_id` (upstream hostile faction.id)"
                )
            })?;
            Ok(AbilityConditionSpec::DefenderHullFactionIdIs { faction_id: id })
        }
        "not" => {
            let children = c
                .conditions
                .as_ref()
                .ok_or_else(|| "`not` condition requires a `conditions` array".to_string())?;
            if children.len() != 1 {
                return Err("`not` condition must include exactly one sub-condition".to_string());
            }
            Ok(AbilityConditionSpec::Not {
                inner: Box::new(lcars_condition_to_spec(&children[0])?),
            })
        }
        "defender_ship_type_is"
        | "defender_ship_class_is"
        | "opponent_ship_type_is"
        | "opponent_ship_class_is" => {
            let slug = c
                .ship_type
                .as_deref()
                .or(c.stat.as_deref())
                .ok_or_else(|| {
                    "defender/opponent ship class condition requires `ship_type` or `stat` slug"
                        .to_string()
                })?;
            Ok(AbilityConditionSpec::DefenderShipTypeIs {
                ship_type: slug.to_string(),
            })
        }
        "attacker_ship_type_is"
        | "attacker_ship_class_is"
        | "self_ship_type_is"
        | "self_ship_class_is" => {
            let slug = c
                .ship_type
                .as_deref()
                .or(c.stat.as_deref())
                .ok_or_else(|| {
                    "attacker/self ship class condition requires `ship_type` or `stat` slug"
                        .to_string()
                })?;
            Ok(AbilityConditionSpec::AttackerShipTypeIs {
                ship_type: slug.to_string(),
            })
        }
        "attacker_ship_id_is" | "self_ship_id_is" => {
            let id = c
                .ship_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{ty} condition requires non-empty `ship_id`"))?;
            Ok(AbilityConditionSpec::AttackerShipIdIs {
                ship_id: id.to_string(),
            })
        }
        "and" => {
            let children = c.conditions.as_ref().ok_or_else(|| {
                "`and` condition requires non-empty `conditions` array".to_string()
            })?;
            if children.is_empty() {
                return Err("`and` condition must include at least one sub-condition".to_string());
            }
            let mut all = Vec::with_capacity(children.len());
            for child in children {
                all.push(lcars_condition_to_spec(child)?);
            }
            Ok(AbilityConditionSpec::And { all })
        }
        "or" => {
            let children = c.conditions.as_ref().ok_or_else(|| {
                "`or` condition requires non-empty `conditions` array".to_string()
            })?;
            if children.is_empty() {
                return Err("`or` condition must include at least one sub-condition".to_string());
            }
            let mut any = Vec::with_capacity(children.len());
            for child in children {
                any.push(lcars_condition_to_spec(child)?);
            }
            Ok(AbilityConditionSpec::Or { any })
        }
        "engagement_includes" | "engagement_has" => {
            let slug = c
                .enemy_type
                .as_deref()
                .or(c.stat.as_deref())
                .or(c.tag.as_deref())
                .ok_or_else(|| {
                    "engagement_includes requires `enemy_type` (snake_case tag, e.g. group_armadas)"
                        .to_string()
                })?;
            Ok(AbilityConditionSpec::EngagementIncludes {
                enemy_type: slug.to_string(),
            })
        }
        "combat_battle_type_any" | "combat_battle_type" => {
            let mut values = c.battle_types.clone().ok_or_else(|| {
                "combat_battle_type_any requires non-empty `battle_types` list".to_string()
            })?;
            values.sort_unstable();
            values.dedup();
            if values.is_empty() {
                return Err(
                    "combat_battle_type_any requires non-empty `battle_types` list".to_string(),
                );
            }
            Ok(AbilityConditionSpec::CombatBattleTypeAny {
                battle_types: values,
            })
        }
        "defender_level_at_most" | "target_max_level" => {
            let max_level = c
                .max
                .ok_or_else(|| "defender_level_at_most requires integer `max` level".to_string())?;
            Ok(AbilityConditionSpec::DefenderLevelAtMost { max_level })
        }
        _ => Err(format!(
            "unknown LCARS condition type '{}'",
            c.condition_type.trim()
        )),
    }
}

fn stat_to_officer_modifier(stat: &str) -> Option<AbilityModifierSpec> {
    match stat.trim() {
        "weapon_damage" | "attack" => Some(AbilityModifierSpec::WeaponDamage),
        "hull_hp" | "hull" => Some(AbilityModifierSpec::HullHp),
        "shield_hp" | "shield" => Some(AbilityModifierSpec::ShieldHp),
        "crit_chance" => Some(AbilityModifierSpec::CritChance),
        "crit_damage" => Some(AbilityModifierSpec::CritDamage),
        "pierce" | "armor_pierce" | "shield_pierce" => Some(AbilityModifierSpec::Pierce),
        "shield_mitigation" => Some(AbilityModifierSpec::ShieldMitigation),
        "armor" => Some(AbilityModifierSpec::Armor),
        "dodge" => Some(AbilityModifierSpec::Dodge),
        "damage_reduction" => Some(AbilityModifierSpec::DamageReduction),
        "accuracy" => Some(AbilityModifierSpec::Accuracy),
        "isolytic_damage" => Some(AbilityModifierSpec::IsolyticDamage),
        "isolytic_defense" => Some(AbilityModifierSpec::IsolyticDefense),
        "isolytic_cascade" | "isolytic_cascade_damage" => {
            Some(AbilityModifierSpec::IsolyticCascadeDamage)
        }
        "apex_shred" => Some(AbilityModifierSpec::ApexShred),
        "apex_barrier" => Some(AbilityModifierSpec::ApexBarrier),
        "shield_regen" | "shield_hp_repair" => Some(AbilityModifierSpec::OfficerShieldRegenFlat),
        "hull_repair" | "hull_hp_repair" => Some(AbilityModifierSpec::OfficerHullRegenFlat),
        "hull_hp_repair_prev_round" | "hull_repair_prev_round" => {
            Some(AbilityModifierSpec::OfficerHullRegenPrevRoundFraction)
        }
        "shield_hp_repair_prev_round" | "shield_repair_prev_round" => {
            Some(AbilityModifierSpec::OfficerShieldRegenPrevRoundFraction)
        }
        "shots" | "weapon_shots" | "shots_per_weapon" | "shots_per_attack" => {
            Some(AbilityModifierSpec::ShotsBonus)
        }
        _ => None,
    }
}

fn timing_window_to_trigger_spec(tw: crate::combat::TimingWindow) -> AbilityTriggerSpec {
    match tw {
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
    }
}

fn effect_value_at_officer_tier(effect: &LcarsEffect, tier: Option<u8>) -> Option<f64> {
    effect
        .value
        .or_else(|| effect.scaling.as_ref().map(|s| s.value_at_rank(tier)))
}

fn effect_chance_at_officer_tier(effect: &LcarsEffect, tier: Option<u8>) -> f64 {
    effect
        .chance
        .or_else(|| effect.scaling.as_ref().map(|s| s.chance_at_rank(tier)))
        .unwrap_or(0.0)
}

fn lcars_duration_to_spec(d: &LcarsDuration) -> Option<DurationSpec> {
    match d {
        LcarsDuration::Rounds { rounds } => Some(DurationSpec::Rounds { rounds: *rounds }),
        LcarsDuration::Stacks { stacks } => Some(DurationSpec::Rounds { rounds: *stacks }),
        LcarsDuration::Permanent(_) => None,
    }
}

fn officer_conditions_from_effect(effect: &LcarsEffect) -> Vec<AbilityConditionSpec> {
    match effect.condition.as_ref() {
        Some(c) => match lcars_condition_to_spec(c) {
            Ok(s) => vec![s],
            Err(_) => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn officer_target_from_effect(effect: &LcarsEffect) -> AbilityTargetSpec {
    match effect
        .target
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
    {
        Some(ref t) if t == "enemy" => AbilityTargetSpec::DefenderOpponent,
        Some(ref t) if t == "self" || t.is_empty() => AbilityTargetSpec::AttackerSelf,
        None => AbilityTargetSpec::AttackerSelf,
        _ => AbilityTargetSpec::AttackerSelf,
    }
}

fn op_to_spec(op: &str) -> AbilityOperationSpec {
    match op {
        "multiply" | "mul" | "mult" => AbilityOperationSpec::Multiply,
        "set" => AbilityOperationSpec::Set,
        "min" => AbilityOperationSpec::Min,
        "max" => AbilityOperationSpec::Max,
        _ => AbilityOperationSpec::Add,
    }
}

/// Convert one LCARS officer effect to a [`CombatEffectSpec`] when the row maps cleanly.
/// Returns [`None`] for static-only `stat_modify`, `tag`, `extra_attack`, `accuracy` non-CB timings,
/// and unknown triggers (aligned with [`crate::lcars::resolver::resolve_effect`]).
pub fn lcars_effect_to_combat_effect_spec(
    effect: &LcarsEffect,
    stable_id: &str,
    officer_id: &str,
    ability_name: &str,
    officer_tier: Option<u8>,
) -> Option<CombatEffectSpec> {
    if effect.effect_type == "tag" || effect.effect_type == "extra_attack" {
        return None;
    }

    if effect.effect_type == "stat_modify" {
        let passive = effect.trigger.as_deref().map(str::trim) == Some("passive");
        let permanent = effect
            .duration
            .as_ref()
            .map(|d| d.is_permanent())
            .unwrap_or(false);
        if passive && permanent {
            return None;
        }
    }

    let timing = effect_trigger_timing(effect)?;
    if effect.effect_type == "stat_modify" {
        let stat = effect.stat.as_deref().unwrap_or("").trim();
        if stat.eq_ignore_ascii_case("accuracy") {
            return None;
        }
    }

    let trigger = timing_window_to_trigger_spec(timing);
    let target = officer_target_from_effect(effect);
    let conditions = officer_conditions_from_effect(effect);
    let duration = effect.duration.as_ref().and_then(lcars_duration_to_spec);

    let op_norm = normalize_operator(effect.operator.as_deref());
    let mut attributes = serde_json::Map::new();
    attributes.insert(
        OFFICER_SPEC_ATTR_LCARS_OP.into(),
        serde_json::Value::String(op_norm.clone()),
    );
    let operation = op_to_spec(op_norm.as_str());

    match effect.effect_type.as_str() {
        "stat_modify" => {
            let value = effect_value_at_officer_tier(effect, officer_tier)?;
            let stat = effect.stat.as_deref().unwrap_or("").trim();

            if stat == "weapon_damage" || stat == "attack" {
                if let Some(ref decay) = effect.decay {
                    attributes.insert(
                        OFFICER_SPEC_ATTR_WEAPON_DAMAGE_DECAY.into(),
                        json!({
                            "amount": decay.amount.unwrap_or(0.0),
                            "floor": decay.floor.unwrap_or(1.0),
                        }),
                    );
                    return Some(CombatEffectSpec {
                        id: stable_id.to_string(),
                        source: EffectSource::LcarsOfficer,
                        source_ref: Some(crate::data::combat_effect_spec::SourceRef {
                            officer_id: Some(officer_id.to_string()),
                            ability_id: Some(ability_name.to_string()),
                            ..Default::default()
                        }),
                        text: None,
                        trigger,
                        target,
                        modifier: AbilityModifierSpec::WeaponDamage,
                        operation,
                        value: Some(ValueSpec {
                            scalar: Some(value),
                            by_rank: None,
                            unit: None,
                        }),
                        chance: None,
                        duration,
                        conditions,
                        attributes,
                        stacking: None,
                        category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
                        confidence: None,
                    });
                }
                if let Some(ref acc) = effect.accumulate {
                    attributes.insert(
                        OFFICER_SPEC_ATTR_WEAPON_DAMAGE_ACCUMULATE.into(),
                        json!({
                            "amount": acc.amount.unwrap_or(0.0),
                            "ceiling": acc.ceiling.unwrap_or(2.0),
                        }),
                    );
                    return Some(CombatEffectSpec {
                        id: stable_id.to_string(),
                        source: EffectSource::LcarsOfficer,
                        source_ref: Some(crate::data::combat_effect_spec::SourceRef {
                            officer_id: Some(officer_id.to_string()),
                            ability_id: Some(ability_name.to_string()),
                            ..Default::default()
                        }),
                        text: None,
                        trigger,
                        target,
                        modifier: AbilityModifierSpec::WeaponDamage,
                        operation,
                        value: Some(ValueSpec {
                            scalar: Some(value),
                            by_rank: None,
                            unit: None,
                        }),
                        chance: None,
                        duration,
                        conditions,
                        attributes,
                        stacking: None,
                        category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
                        confidence: None,
                    });
                }
            }

            let modifier = stat_to_officer_modifier(stat)?;
            Some(CombatEffectSpec {
                id: stable_id.to_string(),
                source: EffectSource::LcarsOfficer,
                source_ref: Some(crate::data::combat_effect_spec::SourceRef {
                    officer_id: Some(officer_id.to_string()),
                    ability_id: Some(ability_name.to_string()),
                    ..Default::default()
                }),
                text: None,
                trigger,
                target,
                modifier,
                operation,
                value: Some(ValueSpec {
                    scalar: Some(value),
                    by_rank: None,
                    unit: None,
                }),
                chance: None,
                duration,
                conditions,
                attributes,
                stacking: None,
                category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
                confidence: None,
            })
        }
        "morale" => {
            let chance = effect_chance_at_officer_tier(effect, officer_tier);
            Some(CombatEffectSpec {
                id: stable_id.to_string(),
                source: EffectSource::LcarsOfficer,
                source_ref: Some(crate::data::combat_effect_spec::SourceRef {
                    officer_id: Some(officer_id.to_string()),
                    ability_id: Some(ability_name.to_string()),
                    ..Default::default()
                }),
                text: None,
                trigger,
                target,
                modifier: AbilityModifierSpec::StateMorale,
                operation: AbilityOperationSpec::Add,
                value: None,
                chance: Some(ChanceSpec {
                    scalar: Some(chance),
                    by_rank: None,
                }),
                duration,
                conditions,
                attributes,
                stacking: None,
                category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
                confidence: None,
            })
        }
        "assimilated" => {
            let chance = effect_chance_at_officer_tier(effect, officer_tier);
            Some(CombatEffectSpec {
                id: stable_id.to_string(),
                source: EffectSource::LcarsOfficer,
                source_ref: Some(crate::data::combat_effect_spec::SourceRef {
                    officer_id: Some(officer_id.to_string()),
                    ability_id: Some(ability_name.to_string()),
                    ..Default::default()
                }),
                text: None,
                trigger,
                target,
                modifier: AbilityModifierSpec::StateAssimilated,
                operation: AbilityOperationSpec::Add,
                value: None,
                chance: Some(ChanceSpec {
                    scalar: Some(chance),
                    by_rank: None,
                }),
                duration,
                conditions,
                attributes,
                stacking: None,
                category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
                confidence: None,
            })
        }
        "hull_breach" => {
            let chance = effect_chance_at_officer_tier(effect, officer_tier);
            Some(CombatEffectSpec {
                id: stable_id.to_string(),
                source: EffectSource::LcarsOfficer,
                source_ref: Some(crate::data::combat_effect_spec::SourceRef {
                    officer_id: Some(officer_id.to_string()),
                    ability_id: Some(ability_name.to_string()),
                    ..Default::default()
                }),
                text: None,
                trigger,
                target,
                modifier: AbilityModifierSpec::StateHullBreach,
                operation: AbilityOperationSpec::Add,
                value: None,
                chance: Some(ChanceSpec {
                    scalar: Some(chance),
                    by_rank: None,
                }),
                duration,
                conditions,
                attributes,
                stacking: None,
                category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
                confidence: None,
            })
        }
        "burning" => {
            let chance = effect_chance_at_officer_tier(effect, officer_tier);
            Some(CombatEffectSpec {
                id: stable_id.to_string(),
                source: EffectSource::LcarsOfficer,
                source_ref: Some(crate::data::combat_effect_spec::SourceRef {
                    officer_id: Some(officer_id.to_string()),
                    ability_id: Some(ability_name.to_string()),
                    ..Default::default()
                }),
                text: None,
                trigger,
                target,
                modifier: AbilityModifierSpec::StateBurning,
                operation: AbilityOperationSpec::Add,
                value: None,
                chance: Some(ChanceSpec {
                    scalar: Some(chance),
                    by_rank: None,
                }),
                duration,
                conditions,
                attributes,
                stacking: None,
                category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
                confidence: None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::{AbilityClass, AbilityEffect, CrewSeat};
    use crate::lcars::parser::{LcarsAbility, LcarsCondition, LcarsOfficer};
    use crate::lcars::resolver::{resolve_officer_ability, ResolveOptions};

    #[test]
    fn lcars_self_officer_tal_not_on_bridge_maps_to_spec() {
        let c = LcarsCondition {
            condition_type: "self_officer_tal_not_on_bridge".into(),
            stat: None,
            threshold_pct: None,
            min: None,
            max: None,
            faction: None,
            group: None,
            min_members: None,
            tag: None,
            ship_type: None,
            faction_id: None,
            ship_id: None,
            enemy_type: None,
            battle_types: None,
            conditions: None,
        };
        let spec = lcars_condition_to_spec(&c).expect("spec");
        assert_eq!(spec, AbilityConditionSpec::AttackerOfficerTalNotOnBridge);
    }

    #[test]
    fn lcars_stat_modify_maps_to_spec() {
        let e = LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some("weapon_damage".into()),
            target: None,
            operator: Some("add".into()),
            value: Some(0.12),
            trigger: Some("on_attack".into()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let spec = lcars_effect_to_combat_effect_spec(&e, "test:id", "gorkon", "cm", None).unwrap();
        assert_eq!(spec.modifier, AbilityModifierSpec::WeaponDamage);
        assert_eq!(spec.trigger, AbilityTriggerSpec::AttackPhase);
    }

    #[test]
    fn passive_permanent_skipped() {
        let e = LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some("weapon_damage".into()),
            target: None,
            operator: Some("add".into()),
            value: Some(0.1),
            trigger: Some("passive".into()),
            duration: Some(crate::lcars::parser::LcarsDuration::Permanent(
                "permanent".into(),
            )),
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        assert!(lcars_effect_to_combat_effect_spec(&e, "x", "o", "a", None).is_none());
    }

    /// [`lcars_effect_to_combat_effect_spec`] scalar + modifier must stay aligned with
    /// [`crate::lcars::resolver::resolve_effect`] `stat_modify` / `weapon_damage` / `add` semantics (`1 + value` → [`AbilityEffect::AttackMultiplier`]).
    #[test]
    fn lcars_spec_weapon_damage_scalar_matches_resolver_attack_multiplier() {
        let officer = LcarsOfficer {
            id: "parity_officer".into(),
            name: "Parity".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
        };
        let effect = LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some("weapon_damage".into()),
            target: None,
            operator: Some("add".into()),
            value: Some(0.12),
            trigger: Some("on_attack".into()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let ability = LcarsAbility {
            name: "strike".into(),
            effects: vec![effect.clone()],
        };
        let spec = lcars_effect_to_combat_effect_spec(
            &effect,
            "parity:id",
            "parity_officer",
            "strike",
            None,
        )
        .expect("spec");
        let raw = spec.value.as_ref().and_then(|v| v.scalar).expect("scalar");
        let contexts = resolve_officer_ability(
            &officer,
            &ability,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &ResolveOptions::default(),
            0,
        );
        assert_eq!(contexts.len(), 1);
        let m = match contexts[0].ability.effect {
            AbilityEffect::AttackMultiplier(x) => x,
            ref e => panic!("expected AttackMultiplier, got {e:?}"),
        };
        assert!(
            (m - (1.0 + raw)).abs() < 1e-12,
            "resolver mult {m} vs 1+spec_scalar {}",
            1.0 + raw
        );
    }
}
