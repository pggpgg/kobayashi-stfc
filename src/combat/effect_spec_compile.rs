//! Compile [`crate::data::combat_effect_spec::CombatEffectSpec`] into engine runtime structs.

use crate::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::combat::condition::combine_optional_and;
use crate::combat::types::{enemy_type_from_engagement_slug, OpponentFactionTag, ShipType};
use crate::combat::TimingWindow;
use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, CombatEffectSpec, DurationSpec, ValueSpec,
};
use crate::data::ship_ability_resolve;

/// Decaying `weapon_damage`: `{"amount": f64, "floor": f64}` on [`CombatEffectSpec::attributes`].
pub const OFFICER_SPEC_ATTR_WEAPON_DAMAGE_DECAY: &str = "kobayashi_officer_weapon_damage_decay";
/// Accumulating `weapon_damage`: `{"amount": f64, "ceiling": f64}` on [`CombatEffectSpec::attributes`].
pub const OFFICER_SPEC_ATTR_WEAPON_DAMAGE_ACCUMULATE: &str = "kobayashi_officer_weapon_damage_accumulate";
/// Raw LCARS operator string after dash/underscore normalization (matches resolver `normalize_operator`).
pub const OFFICER_SPEC_ATTR_LCARS_OP: &str = "kobayashi_lcars_normalize_op";

#[derive(Debug, Clone, PartialEq)]
pub enum EffectSpecCompileError {
    UnsupportedTrigger(AbilityTriggerSpec),
    UnsupportedModifierOperation {
        modifier: AbilityModifierSpec,
        operation: AbilityOperationSpec,
    },
    MissingScalarValue,
    UnsupportedValueShape,
    UnknownFactionSlug(String),
    UnknownShipTypeSlug(String),
    UnknownEngagementSlug(String),
    EmptyConditionParts,
    StfcCcTokenNotCompilable {
        token: String,
    },
}

impl std::fmt::Display for EffectSpecCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedTrigger(t) => {
                write!(f, "unsupported trigger {t:?} for this compile path")
            }
            Self::UnsupportedModifierOperation {
                modifier,
                operation,
            } => {
                write!(
                    f,
                    "unsupported modifier/operation pair: {modifier:?} + {operation:?}"
                )
            }
            Self::MissingScalarValue => write!(f, "value.scalar required for this compile path"),
            Self::UnsupportedValueShape => {
                write!(f, "by_rank / table values require LCARS resolution path")
            }
            Self::UnknownFactionSlug(s) => write!(f, "unknown defender faction slug '{s}'"),
            Self::UnknownShipTypeSlug(s) => write!(f, "unknown ship class slug '{s}'"),
            Self::UnknownEngagementSlug(s) => write!(f, "unknown engagement enemy_type slug '{s}'"),
            Self::EmptyConditionParts => write!(f, "condition list produced no runtime gates"),
            Self::StfcCcTokenNotCompilable { token } => write!(
                f,
                "stfc.cc condition token has no AbilityCondition mapping yet: {token}"
            ),
        }
    }
}

impl std::error::Error for EffectSpecCompileError {}

/// Map canonical trigger tokens to engine timing. [`AbilityTriggerSpec::ShipLaunched`] is the
/// cheat-sheet “default ship” trigger and matches [`TimingWindow::CombatBegin`].
pub fn compile_trigger(
    trigger: AbilityTriggerSpec,
) -> Result<TimingWindow, EffectSpecCompileError> {
    match trigger {
        AbilityTriggerSpec::CombatBegin | AbilityTriggerSpec::ShipLaunched => {
            Ok(TimingWindow::CombatBegin)
        }
        AbilityTriggerSpec::RoundStart => Ok(TimingWindow::RoundStart),
        AbilityTriggerSpec::AttackPhase => Ok(TimingWindow::AttackPhase),
        AbilityTriggerSpec::AfterSubround => Ok(TimingWindow::AfterSubround),
        AbilityTriggerSpec::DefensePhase => Ok(TimingWindow::DefensePhase),
        AbilityTriggerSpec::RoundEnd => Ok(TimingWindow::RoundEnd),
        AbilityTriggerSpec::ShieldBreak => Ok(TimingWindow::ShieldBreak),
        AbilityTriggerSpec::SelfShieldBreak => Ok(TimingWindow::SelfShieldBreak),
        AbilityTriggerSpec::Kill => Ok(TimingWindow::Kill),
        AbilityTriggerSpec::HullBreach => Ok(TimingWindow::HullBreach),
        AbilityTriggerSpec::ReceiveDamage => Ok(TimingWindow::ReceiveDamage),
        AbilityTriggerSpec::CombatEnd => Ok(TimingWindow::CombatEnd),
    }
}

fn scalar_fraction(value: &ValueSpec) -> Result<f64, EffectSpecCompileError> {
    value
        .scalar
        .filter(|v| v.is_finite())
        .ok_or(EffectSpecCompileError::MissingScalarValue)
}

/// Compile a condition tree used in JSON / research specs.
pub fn compile_condition(
    spec: &AbilityConditionSpec,
) -> Result<AbilityCondition, EffectSpecCompileError> {
    match spec {
        AbilityConditionSpec::MoraleActive => Ok(AbilityCondition::MoraleActive),
        AbilityConditionSpec::DefenderBurning => Ok(AbilityCondition::DefenderBurning),
        AbilityConditionSpec::DefenderHullBreach => Ok(AbilityCondition::DefenderHullBreach),
        AbilityConditionSpec::AttackerBurning => Ok(AbilityCondition::AttackerBurning),
        AbilityConditionSpec::AttackerHullBreach => Ok(AbilityCondition::AttackerHullBreach),
        AbilityConditionSpec::DefenderAssimilated => Ok(AbilityCondition::DefenderAssimilated),
        AbilityConditionSpec::DefenderIsNpcHostile => Ok(AbilityCondition::DefenderIsNpcHostile),
        AbilityConditionSpec::DefenderIsPlayerShip => Ok(AbilityCondition::DefenderIsPlayerShip),
        AbilityConditionSpec::AttackerOfficerTalNotOnBridge => {
            Ok(AbilityCondition::AttackerOfficerTalNotOnBridge)
        }
        AbilityConditionSpec::DefenderShipTypeIs { ship_type } => {
            let st = ShipType::from_data_slug(ship_type)
                .ok_or_else(|| EffectSpecCompileError::UnknownShipTypeSlug(ship_type.clone()))?;
            Ok(AbilityCondition::DefenderShipTypeIs(st))
        }
        AbilityConditionSpec::AttackerShipTypeIs { ship_type } => {
            let st = ShipType::from_data_slug(ship_type)
                .ok_or_else(|| EffectSpecCompileError::UnknownShipTypeSlug(ship_type.clone()))?;
            Ok(AbilityCondition::AttackerShipTypeIs(st))
        }
        AbilityConditionSpec::AttackerShipIdIs { ship_id } => {
            Ok(AbilityCondition::AttackerShipIdIs(ship_id.clone()))
        }
        AbilityConditionSpec::DefenderFactionIs { faction } => {
            let tag = OpponentFactionTag::from_data_slug(faction)
                .ok_or_else(|| EffectSpecCompileError::UnknownFactionSlug(faction.clone()))?;
            Ok(AbilityCondition::DefenderFactionIs(tag))
        }
        AbilityConditionSpec::DefenderHullFactionIdIs { faction_id } => {
            Ok(AbilityCondition::DefenderHullFactionIdIs(*faction_id))
        }
        AbilityConditionSpec::RoundRange { min, max } => Ok(AbilityCondition::RoundRange {
            min: *min,
            max: *max,
        }),
        AbilityConditionSpec::StatBelow {
            stat,
            threshold_pct,
        } => Ok(AbilityCondition::StatBelow {
            stat: stat.clone(),
            threshold_pct: *threshold_pct,
        }),
        AbilityConditionSpec::StatAbove {
            stat,
            threshold_pct,
        } => Ok(AbilityCondition::StatAbove {
            stat: stat.clone(),
            threshold_pct: *threshold_pct,
        }),
        AbilityConditionSpec::Not { inner } => {
            Ok(AbilityCondition::Not(Box::new(compile_condition(inner)?)))
        }
        AbilityConditionSpec::And { all } => {
            let mut parts = Vec::with_capacity(all.len());
            for c in all {
                parts.push(compile_condition(c)?);
            }
            combine_optional_and(parts).ok_or(EffectSpecCompileError::EmptyConditionParts)
        }
        AbilityConditionSpec::Or { any } => {
            if any.is_empty() {
                return Err(EffectSpecCompileError::EmptyConditionParts);
            }
            let mut parts = Vec::with_capacity(any.len());
            for c in any {
                parts.push(compile_condition(c)?);
            }
            Ok(AbilityCondition::Or(parts))
        }
        AbilityConditionSpec::EngagementIncludes { enemy_type } => {
            let et = enemy_type_from_engagement_slug(enemy_type).ok_or_else(|| {
                EffectSpecCompileError::UnknownEngagementSlug(enemy_type.clone())
            })?;
            Ok(AbilityCondition::EngagementIncludes(et))
        }
        AbilityConditionSpec::CombatBattleTypeAny { battle_types } => {
            if battle_types.is_empty() {
                return Err(EffectSpecCompileError::EmptyConditionParts);
            }
            let mut v = battle_types.clone();
            v.sort_unstable();
            v.dedup();
            Ok(AbilityCondition::CombatBattleTypeAny(v))
        }
        AbilityConditionSpec::DefenderLevelAtMost { max_level } => {
            Ok(AbilityCondition::DefenderLevelAtMost(*max_level))
        }
        AbilityConditionSpec::StfcCcToken { token } => {
            Err(EffectSpecCompileError::StfcCcTokenNotCompilable {
                token: token.clone(),
            })
        }
    }
}

/// AND-combine top-level condition specs (empty → None).
pub fn compile_conditions_and(
    specs: &[AbilityConditionSpec],
) -> Result<Option<AbilityCondition>, EffectSpecCompileError> {
    if specs.is_empty() {
        return Ok(None);
    }
    let mut parts = Vec::with_capacity(specs.len());
    for s in specs {
        parts.push(compile_condition(s)?);
    }
    Ok(combine_optional_and(parts))
}

fn mitigation_fraction_from_lcars_armor_value(raw: f64) -> f64 {
    if raw.abs() > 1.0 {
        raw / 100.0
    } else {
        raw
    }
}

fn officer_spec_duration_rounds(spec: &CombatEffectSpec, fallback: u32) -> u32 {
    match spec.duration.as_ref() {
        Some(DurationSpec::Rounds { rounds }) => (*rounds).max(1),
        Some(DurationSpec::Stacks { stacks }) => (*stacks).max(1),
        Some(DurationSpec::Permanent) => fallback,
        None => fallback,
    }
}

fn lcars_op_from_officer_spec(spec: &CombatEffectSpec) -> String {
    spec.attributes
        .get(OFFICER_SPEC_ATTR_LCARS_OP)
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| match spec.operation {
            AbilityOperationSpec::Multiply => "multiply".to_string(),
            AbilityOperationSpec::Set => "set".to_string(),
            AbilityOperationSpec::Min => "min".to_string(),
            AbilityOperationSpec::Max => "max".to_string(),
            _ => "add".to_string(),
        })
}

/// Compile LCARS-authored [`CombatEffectSpec`] into runtime [`AbilityEffect`] + [`TimingWindow`].
pub fn compile_officer_combat_spec(
    spec: &CombatEffectSpec,
) -> Result<(TimingWindow, AbilityEffect), EffectSpecCompileError> {
    let timing = compile_trigger(spec.trigger)?;
    let op = lcars_op_from_officer_spec(spec);
    let op = op.as_str();

    match spec.modifier {
        AbilityModifierSpec::WeaponDamage => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            if let Some(obj) = spec
                .attributes
                .get(OFFICER_SPEC_ATTR_WEAPON_DAMAGE_DECAY)
                .and_then(|x| x.as_object())
            {
                let decay_per_round = obj.get("amount").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let floor = obj.get("floor").and_then(|x| x.as_f64()).unwrap_or(1.0);
                return Ok((
                    timing,
                    AbilityEffect::DecayingAttackMultiplier {
                        initial: v,
                        decay_per_round,
                        floor,
                    },
                ));
            }
            if let Some(obj) = spec
                .attributes
                .get(OFFICER_SPEC_ATTR_WEAPON_DAMAGE_ACCUMULATE)
                .and_then(|x| x.as_object())
            {
                let growth_per_round = obj.get("amount").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let ceiling = obj.get("ceiling").and_then(|x| x.as_f64()).unwrap_or(2.0);
                return Ok((
                    timing,
                    AbilityEffect::AccumulatingAttackMultiplier {
                        initial: v,
                        growth_per_round,
                        ceiling,
                    },
                ));
            }
            let mult = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add" | "multiplybaseadd" => v,
                "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub" | "multiplybasesub" => {
                    1.0 - v
                }
                "set" => v,
                _ => 1.0 + v,
            };
            Ok((timing, AbilityEffect::AttackMultiplier(mult)))
        }
        AbilityModifierSpec::Pierce => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" => v - 1.0,
                "sub" | "mul_sub" | "multiplysub" => -v,
                "set" => v,
                _ => v,
            };
            Ok((timing, AbilityEffect::PierceBonus(add)))
        }
        AbilityModifierSpec::CritChance => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add" | "multiplybaseadd" => {
                    return Err(EffectSpecCompileError::UnsupportedModifierOperation {
                        modifier: spec.modifier,
                        operation: spec.operation,
                    });
                }
                "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub" | "multiplybasesub" => -v,
                "set" => {
                    return Err(EffectSpecCompileError::UnsupportedModifierOperation {
                        modifier: spec.modifier,
                        operation: spec.operation,
                    });
                }
                _ => v,
            };
            Ok((timing, AbilityEffect::CritChanceBonus(add)))
        }
        AbilityModifierSpec::CritDamage => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let mult = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add" | "multiplybaseadd" => v,
                "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub" | "multiplybasesub" => {
                    (1.0 - v).max(0.0)
                }
                "set" => v.max(0.0),
                _ => 1.0 + v,
            };
            if mult.is_finite() && mult > 0.0 {
                Ok((timing, AbilityEffect::CritDamageMultiplier(mult)))
            } else {
                Err(EffectSpecCompileError::UnsupportedModifierOperation {
                    modifier: spec.modifier,
                    operation: spec.operation,
                })
            }
        }
        AbilityModifierSpec::ApexShred => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((timing, AbilityEffect::ApexShredBonus(v)))
        }
        AbilityModifierSpec::ApexBarrier => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((timing, AbilityEffect::ApexBarrierBonus(v)))
        }
        AbilityModifierSpec::OfficerShieldRegenFlat => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((timing, AbilityEffect::ShieldRegen(v)))
        }
        AbilityModifierSpec::OfficerHullRegenFlat => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            if timing == TimingWindow::Kill {
                Ok((timing, AbilityEffect::OnKillHullRegen(v)))
            } else {
                Ok((timing, AbilityEffect::HullRegen(v)))
            }
        }
        AbilityModifierSpec::OfficerHullRegenPrevRoundFraction => {
            if timing != TimingWindow::RoundStart {
                return Err(EffectSpecCompileError::UnsupportedTrigger(spec.trigger));
            }
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((timing, AbilityEffect::HullRegenPrevRoundFraction(v)))
        }
        AbilityModifierSpec::OfficerShieldRegenPrevRoundFraction => {
            if timing != TimingWindow::RoundStart {
                return Err(EffectSpecCompileError::UnsupportedTrigger(spec.trigger));
            }
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((timing, AbilityEffect::ShieldRegenPrevRoundFraction(v)))
        }
        AbilityModifierSpec::IsolyticDamage => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" => v - 1.0,
                "sub" | "mul_sub" | "multiplysub" => -v,
                _ => v,
            };
            Ok((timing, AbilityEffect::IsolyticDamageBonus(add)))
        }
        AbilityModifierSpec::IsolyticDefense => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" => v - 1.0,
                "sub" | "mul_sub" | "multiplysub" => -v,
                _ => v,
            };
            Ok((timing, AbilityEffect::IsolyticDefenseBonus(add)))
        }
        AbilityModifierSpec::IsolyticCascadeDamage => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" => v - 1.0,
                "sub" | "mul_sub" | "multiplysub" => -v,
                _ => v,
            };
            Ok((timing, AbilityEffect::IsolyticCascadeDamageBonus(add)))
        }
        AbilityModifierSpec::ShieldMitigation => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" => v - 1.0,
                "sub" | "mul_sub" | "multiplysub" => -v,
                _ => v,
            };
            Ok((timing, AbilityEffect::ShieldMitigationBonus(add)))
        }
        AbilityModifierSpec::Armor => {
            if !matches!(timing, TimingWindow::CombatBegin | TimingWindow::RoundStart) {
                return Err(EffectSpecCompileError::UnsupportedTrigger(spec.trigger));
            }
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add" | "multiplybaseadd" => {
                    v - 1.0
                }
                "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub" | "multiplybasesub" => -v,
                "set" => {
                    return Err(EffectSpecCompileError::UnsupportedModifierOperation {
                        modifier: spec.modifier,
                        operation: spec.operation,
                    });
                }
                _ => v,
            };
            Ok((
                timing,
                AbilityEffect::MitigationAdditive(mitigation_fraction_from_lcars_armor_value(add)),
            ))
        }
        AbilityModifierSpec::ShotsBonus => {
            if !matches!(timing, TimingWindow::RoundStart | TimingWindow::CombatBegin) {
                return Err(EffectSpecCompileError::UnsupportedTrigger(spec.trigger));
            }
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let bonus_pct = match op {
                "multiply" | "mul_add" | "multiplyadd" => v - 1.0,
                "sub" | "mul_sub" | "multiplysub" => -v,
                "set" => v,
                _ => v,
            };
            let duration_rounds = officer_spec_duration_rounds(spec, 1);
            Ok((
                timing,
                AbilityEffect::ShotsBonus {
                    chance: 1.0,
                    bonus_pct,
                    duration_rounds,
                },
            ))
        }
        AbilityModifierSpec::StateMorale => {
            let chance = spec
                .chance
                .as_ref()
                .and_then(|c| c.scalar)
                .filter(|c| c.is_finite())
                .unwrap_or(0.0);
            Ok((timing, AbilityEffect::Morale(chance)))
        }
        AbilityModifierSpec::StateAssimilated => {
            let chance = spec
                .chance
                .as_ref()
                .and_then(|c| c.scalar)
                .filter(|c| c.is_finite())
                .unwrap_or(0.0);
            let duration_rounds = officer_spec_duration_rounds(spec, 1);
            Ok((
                timing,
                AbilityEffect::Assimilated {
                    chance,
                    duration_rounds,
                },
            ))
        }
        AbilityModifierSpec::StateHullBreach => {
            let chance = spec
                .chance
                .as_ref()
                .and_then(|c| c.scalar)
                .filter(|c| c.is_finite())
                .unwrap_or(0.0);
            let duration_rounds = officer_spec_duration_rounds(spec, 1);
            Ok((
                timing,
                AbilityEffect::HullBreach {
                    chance,
                    duration_rounds,
                    requires_critical: false,
                },
            ))
        }
        AbilityModifierSpec::StateBurning => {
            let chance = spec
                .chance
                .as_ref()
                .and_then(|c| c.scalar)
                .filter(|c| c.is_finite())
                .unwrap_or(0.0);
            let duration_rounds = officer_spec_duration_rounds(spec, 1);
            Ok((
                timing,
                AbilityEffect::Burning {
                    chance,
                    duration_rounds,
                },
            ))
        }
        _ => Err(EffectSpecCompileError::UnsupportedModifierOperation {
            modifier: spec.modifier,
            operation: spec.operation,
        }),
    }
}

/// Research conditional attack-phase row: `weapon_damage` / `crit_*`, `add`, scalar fraction.
pub fn compile_research_attack_effect(
    modifier: AbilityModifierSpec,
    operation: AbilityOperationSpec,
    value: &ValueSpec,
) -> Result<AbilityEffect, EffectSpecCompileError> {
    if operation != AbilityOperationSpec::Add {
        return Err(EffectSpecCompileError::UnsupportedModifierOperation {
            modifier,
            operation,
        });
    }
    if value.by_rank.is_some() {
        return Err(EffectSpecCompileError::UnsupportedValueShape);
    }
    let v = scalar_fraction(value)?;
    match modifier {
        AbilityModifierSpec::WeaponDamage => Ok(AbilityEffect::AttackMultiplier(v)),
        AbilityModifierSpec::CritChance => Ok(AbilityEffect::CritChanceBonus(
            ship_ability_resolve::normalize_probability(v),
        )),
        AbilityModifierSpec::CritDamage => Ok(AbilityEffect::CritDamageMultiplier(
            (1.0 + v).max(crate::combat::EPSILON),
        )),
        _ => Err(EffectSpecCompileError::UnsupportedModifierOperation {
            modifier,
            operation,
        }),
    }
}

/// Whether the target applies to the attacking ship (self-buff semantics).
pub fn target_is_attacker_self(t: AbilityTargetSpec) -> bool {
    matches!(
        t,
        AbilityTargetSpec::AttackerSelf | AbilityTargetSpec::SelfShip
    )
}

/// Full compile for a research-derived attack-phase [`CombatEffectSpec`] row.
pub fn compile_research_attack_phase_spec(
    spec: &CombatEffectSpec,
) -> Result<Ability, EffectSpecCompileError> {
    if !target_is_attacker_self(spec.target) {
        // Research attack rows are attacker self-buffs; other targets are unsupported here.
        return Err(EffectSpecCompileError::UnsupportedModifierOperation {
            modifier: spec.modifier,
            operation: spec.operation,
        });
    }
    let timing = compile_trigger(spec.trigger)?;
    if timing != TimingWindow::AttackPhase {
        return Err(EffectSpecCompileError::UnsupportedTrigger(spec.trigger));
    }
    let effect = compile_research_attack_effect(
        spec.modifier,
        spec.operation,
        spec.value
            .as_ref()
            .ok_or(EffectSpecCompileError::MissingScalarValue)?,
    )?;
    let condition = compile_conditions_and(&spec.conditions)?;
    let Some(cond) = condition else {
        return Err(EffectSpecCompileError::EmptyConditionParts);
    };
    Ok(Ability {
        name: spec.id.clone(),
        class: AbilityClass::ShipAbility,
        timing,
        boostable: false,
        effect,
        condition: Some(cond),
    })
}

/// [`compile_research_attack_phase_spec`] wrapped as a ship [`CrewSeatContext`] (research conditional rows).
pub fn compile_research_attack_phase_spec_to_seat(
    spec: &CombatEffectSpec,
) -> Result<CrewSeatContext, EffectSpecCompileError> {
    let ability = compile_research_attack_phase_spec(spec)?;
    Ok(CrewSeatContext {
        seat: CrewSeat::Ship,
        ability,
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::AbilityCondition;
    use crate::data::combat_effect_spec::{
        CombatEffectSpec, EffectCategory, EffectConfidence, EffectSource,
    };

    #[test]
    fn compile_condition_tal_not_on_bridge() {
        let c = compile_condition(&AbilityConditionSpec::AttackerOfficerTalNotOnBridge).unwrap();
        assert!(matches!(c, AbilityCondition::AttackerOfficerTalNotOnBridge));
    }

    #[test]
    fn compile_condition_attacker_ship_id_is() {
        let c = compile_condition(&AbilityConditionSpec::AttackerShipIdIs {
            ship_id: "borg_cube".into(),
        })
        .unwrap();
        assert_eq!(c, AbilityCondition::AttackerShipIdIs("borg_cube".into()));
    }

    #[test]
    fn compile_condition_stfc_cc_token_errors() {
        let e = compile_condition(&AbilityConditionSpec::StfcCcToken {
            token: "TargetMaxLevel".into(),
        })
        .unwrap_err();
        assert!(matches!(
            e,
            EffectSpecCompileError::StfcCcTokenNotCompilable { .. }
        ));
    }

    #[test]
    fn ship_launched_maps_to_combat_begin() {
        assert_eq!(
            compile_trigger(AbilityTriggerSpec::ShipLaunched).unwrap(),
            TimingWindow::CombatBegin
        );
    }

    #[test]
    fn research_attack_phase_compiles_ns_burning_example() {
        let spec = CombatEffectSpec {
            id: "research:wd:1".into(),
            source: EffectSource::ResearchCatalog,
            source_ref: None,
            text: None,
            trigger: AbilityTriggerSpec::AttackPhase,
            target: AbilityTargetSpec::SelfShip,
            modifier: AbilityModifierSpec::WeaponDamage,
            operation: AbilityOperationSpec::Add,
            value: Some(ValueSpec {
                scalar: Some(0.01),
                by_rank: None,
                unit: None,
            }),
            chance: None,
            duration: None,
            conditions: vec![AbilityConditionSpec::DefenderBurning],
            attributes: serde_json::Map::new(),
            stacking: None,
            category: Some(EffectCategory::Combat),
            confidence: Some(EffectConfidence::Authoritative),
        };
        let a = compile_research_attack_phase_spec(&spec).unwrap();
        assert_eq!(a.timing, TimingWindow::AttackPhase);
        assert!(matches!(a.effect, AbilityEffect::AttackMultiplier(x) if (x - 0.01).abs() < 1e-12));
    }
}
