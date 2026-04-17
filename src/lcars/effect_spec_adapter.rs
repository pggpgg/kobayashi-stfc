//! LCARS YAML → [`crate::data::combat_effect_spec::CombatEffectSpec`] (canonical IR).
//!
//! This is a **lossy** view used for tooling, JSON export, and parity experiments; the combat
//! runtime still resolves through [`crate::lcars::resolver::resolve_officer_ability`] today.

use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, CombatEffectSpec, EffectSource, ValueSpec,
};
use crate::lcars::parser::{LcarsCondition, LcarsEffect};
use crate::lcars::resolver::effect_trigger_timing;

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
        "defender_burning" | "target_burning" | "burning" => Ok(AbilityConditionSpec::DefenderBurning),
        "defender_hull_breach" | "target_hull_breach" | "hull_breach_active" => {
            Ok(AbilityConditionSpec::DefenderHullBreach)
        }
        "attacker_burning" | "self_burning" | "player_burning" => Ok(AbilityConditionSpec::AttackerBurning),
        "attacker_hull_breach" | "self_hull_breach" | "player_hull_breach" => {
            Ok(AbilityConditionSpec::AttackerHullBreach)
        }
        "defender_assimilated" | "target_assimilated" => Ok(AbilityConditionSpec::DefenderAssimilated),
        "attacker_officer_tal_not_on_bridge" | "self_officer_tal_not_on_bridge" => Err(
            "tal_not_on_bridge is engine-resolved; not represented in CombatEffectSpec yet".into(),
        ),
        "defender_faction_is"
        | "defender_faction"
        | "opponent_faction_is"
        | "opponent_faction"
        | "faction_is" => {
            let slug = c.faction.as_deref().or(c.tag.as_deref()).ok_or_else(|| {
                "faction condition requires `faction` or `tag` with a known faction slug".to_string()
            })?;
            Ok(AbilityConditionSpec::DefenderFactionIs {
                faction: slug.to_string(),
            })
        }
        "defender_hull_faction_id" | "enemy_hull_faction" | "enemy_hull_faction_id" => {
            let id = c.faction_id.ok_or_else(|| {
                format!("{ty} condition requires integer `faction_id` (upstream hostile faction.id)")
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
        "and" => {
            let children = c
                .conditions
                .as_ref()
                .ok_or_else(|| "`and` condition requires non-empty `conditions` array".to_string())?;
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
            let children = c
                .conditions
                .as_ref()
                .ok_or_else(|| "`or` condition requires non-empty `conditions` array".to_string())?;
            if children.is_empty() {
                return Err("`or` condition must include at least one sub-condition".to_string());
            }
            let mut any = Vec::with_capacity(children.len());
            for child in children {
                any.push(lcars_condition_to_spec(child)?);
            }
            Ok(AbilityConditionSpec::Or { any })
        }
        _ => Err(format!("unknown LCARS condition type '{}'", c.condition_type.trim())),
    }
}

fn stat_to_modifier(stat: &str) -> Option<AbilityModifierSpec> {
    match stat {
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
        "isolytic_cascade_damage" => Some(AbilityModifierSpec::IsolyticCascadeDamage),
        "apex_shred" => Some(AbilityModifierSpec::ApexShred),
        "apex_barrier" => Some(AbilityModifierSpec::ApexBarrier),
        _ => None,
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

/// Convert one LCARS effect to a [`CombatEffectSpec`] when the row maps cleanly. Returns [`None`] for
/// static-only rows, unsupported types, or unknown triggers (same coarse cuts as the resolver).
pub fn lcars_effect_to_combat_effect_spec(
    effect: &LcarsEffect,
    stable_id: &str,
    officer_id: &str,
    ability_name: &str,
) -> Option<CombatEffectSpec> {
    if effect.effect_type != "stat_modify" {
        return None;
    }
    let passive = effect.trigger.as_deref().map(str::trim) == Some("passive");
    let permanent = effect
        .duration
        .as_ref()
        .map(|d| d.is_permanent())
        .unwrap_or(false);
    if passive && permanent {
        return None;
    }
    let timing = effect_trigger_timing(effect)?;
    let trigger = match timing {
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
    let stat = effect.stat.as_deref().unwrap_or("");
    let modifier = stat_to_modifier(stat)?;
    let op = normalize_operator(effect.operator.as_deref());
    let operation = op_to_spec(op.as_str());
    let value = if let Some(v) = effect.value {
        v
    } else if let Some(ref sc) = effect.scaling {
        sc.value_at_rank(None)
    } else {
        return None;
    };
    let conditions: Vec<AbilityConditionSpec> = match effect.condition.as_ref() {
        Some(c) => vec![lcars_condition_to_spec(c).ok()?],
        None => Vec::new(),
    };
    let target = match effect.target.as_deref().map(|s| s.trim().to_ascii_lowercase()) {
        Some(ref t) if t == "enemy" => AbilityTargetSpec::DefenderOpponent,
        Some(ref t) if t == "self" || t.is_empty() => AbilityTargetSpec::AttackerSelf,
        None => AbilityTargetSpec::AttackerSelf,
        _ => AbilityTargetSpec::AttackerSelf,
    };
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
        duration: None,
        conditions,
        attributes: serde_json::Map::new(),
        stacking: None,
        category: Some(crate::data::combat_effect_spec::EffectCategory::Combat),
        confidence: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::{AbilityClass, AbilityEffect, CrewSeat};
    use crate::lcars::parser::{LcarsAbility, LcarsOfficer};
    use crate::lcars::resolver::{resolve_officer_ability, ResolveOptions};

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
        let spec = lcars_effect_to_combat_effect_spec(&e, "test:id", "gorkon", "cm").unwrap();
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
            duration: Some(crate::lcars::parser::LcarsDuration::Permanent("permanent".into())),
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        assert!(lcars_effect_to_combat_effect_spec(&e, "x", "o", "a").is_none());
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
        let spec = lcars_effect_to_combat_effect_spec(&effect, "parity:id", "parity_officer", "strike")
            .expect("spec");
        let raw = spec
            .value
            .as_ref()
            .and_then(|v| v.scalar)
            .expect("scalar");
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
