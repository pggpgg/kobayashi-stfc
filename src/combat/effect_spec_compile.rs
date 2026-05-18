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

/// Raw LCARS operator string after dash/underscore normalization (matches resolver `normalize_operator`).
/// Set by [`crate::lcars::effect_spec_adapter`] so the compiler can preserve multiply-family
/// variants (`mul_add`, etc.) that the coarser `spec.operation` enum collapses into `Add`.
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
        AbilityConditionSpec::LiteralBool { value } => Ok(AbilityCondition::LiteralBool(*value)),
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
        AbilityConditionSpec::AttackerOwnerFactionIs { faction } => {
            let tag = OpponentFactionTag::from_data_slug(faction)
                .ok_or_else(|| EffectSpecCompileError::UnknownFactionSlug(faction.clone()))?;
            Ok(AbilityCondition::AttackerOwnerFactionIs(tag))
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
            let et = enemy_type_from_engagement_slug(enemy_type)
                .ok_or_else(|| EffectSpecCompileError::UnknownEngagementSlug(enemy_type.clone()))?;
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

/// Compile LCARS-authored [`CombatEffectSpec`] into runtime [`AbilityEffect`] + [`TimingWindow`] +
/// optional AND-combined [`AbilityCondition`] from `spec.conditions`.
pub fn compile_officer_combat_spec(
    spec: &CombatEffectSpec,
) -> Result<(TimingWindow, AbilityEffect, Option<AbilityCondition>), EffectSpecCompileError> {
    let timing = compile_trigger(spec.trigger)?;
    let compiled_condition = compile_conditions_and(&spec.conditions)?;
    let op = lcars_op_from_officer_spec(spec);
    let op = op.as_str();

    match spec.modifier {
        AbilityModifierSpec::WeaponDamage => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            if let Some(decay) = spec.decay {
                return Ok((
                    timing,
                    AbilityEffect::DecayingAttackMultiplier {
                        initial: v,
                        decay_per_round: decay.amount,
                        floor: decay.floor,
                    },
                    compiled_condition.clone(),
                ));
            }
            if let Some(acc) = spec.accumulate {
                return Ok((
                    timing,
                    AbilityEffect::AccumulatingAttackMultiplier {
                        initial: v,
                        growth_per_round: acc.amount,
                        ceiling: acc.ceiling,
                    },
                    compiled_condition.clone(),
                ));
            }
            let mult = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                | "multiplybaseadd" => v,
                "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub" | "multiplybasesub" => {
                    1.0 - v
                }
                "set" => v,
                _ => 1.0 + v,
            };
            Ok((
                timing,
                AbilityEffect::AttackMultiplier(mult),
                compiled_condition.clone(),
            ))
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
            Ok((
                timing,
                AbilityEffect::PierceBonus(add),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::CritChance => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                | "multiplybaseadd" => {
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
            Ok((
                timing,
                AbilityEffect::CritChanceBonus(add),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::CritDamage => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let mult = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                | "multiplybaseadd" => v,
                "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub" | "multiplybasesub" => {
                    (1.0 - v).max(0.0)
                }
                "set" => v.max(0.0),
                _ => 1.0 + v,
            };
            if mult.is_finite() && mult > 0.0 {
                Ok((
                    timing,
                    AbilityEffect::CritDamageMultiplier(mult),
                    compiled_condition.clone(),
                ))
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
            Ok((
                timing,
                AbilityEffect::ApexShredBonus(v),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::ApexBarrier => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((
                timing,
                AbilityEffect::ApexBarrierBonus(v),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::OfficerShieldRegenFlat => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((
                timing,
                AbilityEffect::ShieldRegen(v),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::OfficerShieldRegenMaxFraction => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((
                timing,
                AbilityEffect::ShieldRegenMaxFraction(v),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::OfficerHullRegenFlat => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            if timing == TimingWindow::Kill {
                Ok((
                    timing,
                    AbilityEffect::OnKillHullRegen(v),
                    compiled_condition.clone(),
                ))
            } else {
                Ok((
                    timing,
                    AbilityEffect::HullRegen(v),
                    compiled_condition.clone(),
                ))
            }
        }
        AbilityModifierSpec::OfficerHullRegenMaxFraction => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            Ok((
                timing,
                AbilityEffect::HullRegenMaxFraction(v),
                compiled_condition.clone(),
            ))
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
            Ok((
                timing,
                AbilityEffect::HullRegenPrevRoundFraction(v),
                compiled_condition.clone(),
            ))
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
            Ok((
                timing,
                AbilityEffect::ShieldRegenPrevRoundFraction(v),
                compiled_condition.clone(),
            ))
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
            Ok((
                timing,
                AbilityEffect::IsolyticDamageBonus(add),
                compiled_condition.clone(),
            ))
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
            Ok((
                timing,
                AbilityEffect::IsolyticDefenseBonus(add),
                compiled_condition.clone(),
            ))
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
            Ok((
                timing,
                AbilityEffect::IsolyticCascadeDamageBonus(add),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::ShieldMitigation => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            // target=DefenderOpponent → **multiplicative** bypass on defender (Harrison-style
            // "ignores X% of opponent shield" — canonical `op: MultiplySub`). Engine applies as
            // `mitigation × (1 - bypass)` (see `engine.rs` shield_mitigation_bypass clamp).
            // The generator drops `MultiplySub` when ShieldMitigation falls through the
            // `:unmapped` tag path (YAML lands with `operator: null, value: 0.7`), so the
            // target field is what flags the multiplicative semantic. A single source may
            // exceed 100% (clamped at consume); we clamp to [0, 1] here as a guard.
            if matches!(spec.target, AbilityTargetSpec::DefenderOpponent) {
                let bypass = v.clamp(0.0, 1.0);
                return Ok((
                    timing,
                    AbilityEffect::ShieldMitigationBypassFraction(bypass),
                    compiled_condition.clone(),
                ));
            }
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" => v - 1.0,
                "sub" | "mul_sub" | "multiplysub" => -v,
                _ => v,
            };
            // target=AttackerSelf: buff the *attacker's* mitigation on counter-fire (engine
            // adds via `effective_incoming_shield_mitigation`). Without this routing the
            // bonus leaks into the `ShieldMitigationBonus` accumulator that the outbound
            // path adds to `defender.shield_mitigation` — i.e. it would buff the **defender**
            // and hurt the attacker. target=DefenderOpponent (and the default) keep emitting
            // additive `ShieldMitigationBonus`; multiplicative bypass for the opponent path
            // is handled separately (see `ShieldMitigationBypassFraction`).
            if matches!(spec.target, AbilityTargetSpec::AttackerSelf) {
                return Ok((
                    timing,
                    AbilityEffect::AttackerShieldMitigationBonus(add),
                    compiled_condition.clone(),
                ));
            }
            Ok((
                timing,
                AbilityEffect::ShieldMitigationBonus(add),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::MitigationAdditive | AbilityModifierSpec::ShieldDeflection => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                | "multiplybaseadd" => v - 1.0,
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
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::ShotsBonus => {
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
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::StateMorale => {
            let chance = spec
                .chance
                .as_ref()
                .and_then(|c| c.scalar)
                .filter(|c| c.is_finite())
                .unwrap_or(0.0);
            Ok((
                timing,
                AbilityEffect::Morale(chance),
                compiled_condition.clone(),
            ))
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
                compiled_condition.clone(),
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
                compiled_condition.clone(),
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
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::Dodge => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            let add = match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                | "multiplybaseadd" => v - 1.0,
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
                AbilityEffect::DodgeBonus(mitigation_fraction_from_lcars_armor_value(add)),
                compiled_condition.clone(),
            ))
        }
        AbilityModifierSpec::ShieldHp => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                | "multiplybaseadd" => {
                    let bonus = v - 1.0;
                    if bonus.is_finite() && bonus > 0.0 {
                        Ok((
                            timing,
                            AbilityEffect::ShieldRegenMaxFraction(bonus),
                            compiled_condition.clone(),
                        ))
                    } else {
                        Err(EffectSpecCompileError::UnsupportedModifierOperation {
                            modifier: spec.modifier,
                            operation: spec.operation,
                        })
                    }
                }
                _ => Err(EffectSpecCompileError::UnsupportedModifierOperation {
                    modifier: spec.modifier,
                    operation: spec.operation,
                }),
            }
        }
        AbilityModifierSpec::HullHp => {
            let v = scalar_fraction(
                spec.value
                    .as_ref()
                    .ok_or(EffectSpecCompileError::MissingScalarValue)?,
            )?;
            match op {
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                | "multiplybaseadd" => {
                    let bonus = v - 1.0;
                    if bonus.is_finite() && bonus > 0.0 {
                        Ok((
                            timing,
                            AbilityEffect::HullRegenMaxFraction(bonus),
                            compiled_condition.clone(),
                        ))
                    } else {
                        Err(EffectSpecCompileError::UnsupportedModifierOperation {
                            modifier: spec.modifier,
                            operation: spec.operation,
                        })
                    }
                }
                _ => Err(EffectSpecCompileError::UnsupportedModifierOperation {
                    modifier: spec.modifier,
                    operation: spec.operation,
                }),
            }
        }
        AbilityModifierSpec::Accuracy => {
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
            Ok((
                timing,
                AbilityEffect::AccuracyBonus(add),
                compiled_condition.clone(),
            ))
        }
        _ => Err(EffectSpecCompileError::UnsupportedModifierOperation {
            modifier: spec.modifier,
            operation: spec.operation,
        }),
    }
}

/// Crew-aggregate `extra_attack` proc contribution compiled from an
/// [`AbilityModifierSpec::ExtraAttackProc`] spec. Consumers (the LCARS resolver) fold these
/// across all crewed officers into [`crate::combat::types::BuffSet::proc_chance`] /
/// `proc_multiplier`. The aggregation rule (highest-chance-wins, tiebroken by highest
/// multiplier) lives in the consumer, not here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuffSetProcContribution {
    pub chance: f64,
    pub multiplier: f64,
}

/// How a [`StaticBuffContribution`] combines with the existing accumulator value in
/// `BuffSet.static_buffs`. The two LCARS sources are passive-permanent `stat_modify` /
/// mapped-tag effects (resolver Loop A) and combat-begin `accuracy` mods (Loop B). Both
/// emit `Multiply` when the underlying LCARS operator is from the multiply family, else `Add`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticBuffOp {
    Add,
    Multiply,
}

/// A single LCARS effect compiled into a (stat_key, op, value) tuple ready to fold into
/// [`crate::combat::types::BuffSet::static_buffs`]. The stat key is LCARS-side (e.g.
/// `"weapon_damage"`, `"accuracy"`, or the synthetic `"accuracy_cb_mult"` for multiplicative
/// accuracy combat-begin mods) — chosen by the caller, since the dispatch lives in the resolver.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticBuffContribution {
    pub stat_key: String,
    pub op: StaticBuffOp,
    pub value: f64,
}

impl StaticBuffContribution {
    /// Fold this contribution into a `BuffSet.static_buffs`-style map. New keys take the value
    /// directly (`*=` against an absent multiplicative entry would zero everything).
    pub fn apply(self, buffs: &mut std::collections::HashMap<String, f64>) {
        match self.op {
            StaticBuffOp::Multiply => {
                buffs
                    .entry(self.stat_key)
                    .and_modify(|x| *x *= self.value)
                    .or_insert(self.value);
            }
            StaticBuffOp::Add => {
                buffs
                    .entry(self.stat_key)
                    .and_modify(|x| *x += self.value)
                    .or_insert(self.value);
            }
        }
    }
}

/// Classify a normalized LCARS operator string into [`StaticBuffOp`]. The multiply family
/// includes `multiply`, `mul_add` / `multiplyadd`, and `multiply_base_add` / `multiplybaseadd`;
/// everything else (including unset / `add` / `sub` / `set`) folds as additive.
pub fn static_buff_op_from_lcars_op(normalized_op: &str) -> StaticBuffOp {
    if matches!(
        normalized_op,
        "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add" | "multiplybaseadd"
    ) {
        StaticBuffOp::Multiply
    } else {
        StaticBuffOp::Add
    }
}

/// Compile a passive-permanent `stat_modify` / mapped-tag [`CombatEffectSpec`] into a
/// [`StaticBuffContribution`]. Returns `None` when the spec has no resolvable scalar value.
///
/// The op classification reads the raw normalized LCARS op stashed by the adapter in
/// `attributes[OFFICER_SPEC_ATTR_LCARS_OP]` so multiply-family variants (`mul_add` etc.) are
/// honoured — the coarser `spec.operation` enum collapses them all into `Add`.
///
/// `stat_key` is the LCARS-side stat string the caller uses as the
/// [`crate::combat::types::BuffSet::static_buffs`] hash-map key.
pub fn compile_officer_static_buff(
    spec: &CombatEffectSpec,
    stat_key: &str,
) -> Option<StaticBuffContribution> {
    let value = spec.value.as_ref()?.scalar?;
    let op = static_buff_op_from_lcars_op(&lcars_op_from_officer_spec(spec));
    Some(StaticBuffContribution {
        stat_key: stat_key.to_string(),
        op,
        value,
    })
}

/// Compile an [`AbilityModifierSpec::ExtraAttackProc`] spec into a
/// [`BuffSetProcContribution`]. Returns `None` for any other modifier (the caller is expected
/// to dispatch on `spec.modifier` before calling this).
///
/// Chance is read from `spec.chance.scalar` (clamped to `[0.0, 1.0]`); multiplier from
/// `spec.value.scalar` (floored at `1.0`). Defaults match the legacy LCARS walker:
/// `chance = 0.0`, `multiplier = 2.0`.
pub fn compile_officer_buffset_proc(spec: &CombatEffectSpec) -> Option<BuffSetProcContribution> {
    if spec.modifier != AbilityModifierSpec::ExtraAttackProc {
        return None;
    }
    let chance = spec
        .chance
        .as_ref()
        .and_then(|c| c.scalar)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let multiplier = spec
        .value
        .as_ref()
        .and_then(|v| v.scalar)
        .unwrap_or(2.0)
        .max(1.0);
    Some(BuffSetProcContribution { chance, multiplier })
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
        AbilityModifierSpec::ApexShred => Ok(AbilityEffect::ApexShredBonus(v)),
        AbilityModifierSpec::ApexBarrier => Ok(AbilityEffect::ApexBarrierBonus(v)),
        AbilityModifierSpec::HostileCritDamageReduction => {
            Ok(AbilityEffect::HostileCritDamageReduction {
                reduction: v.clamp(0.0, 0.95),
                duration_rounds: crate::combat::types::MAX_COMBAT_ROUNDS,
            })
        }
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

    fn shield_mitigation_spec(target: AbilityTargetSpec, value: f64) -> CombatEffectSpec {
        CombatEffectSpec {
            id: "lcars:test:shield_mitigation".into(),
            source: EffectSource::LcarsOfficer,
            source_ref: None,
            text: None,
            trigger: AbilityTriggerSpec::CombatBegin,
            target,
            modifier: AbilityModifierSpec::ShieldMitigation,
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
            conditions: vec![],
            attributes: serde_json::Map::new(),
            stacking: None,
            category: Some(EffectCategory::Combat),
            confidence: Some(EffectConfidence::Authoritative),
        }
    }

    #[test]
    fn shield_mitigation_compile_attacker_self_emits_attacker_bonus_variant() {
        // target=AttackerSelf shieldmitigation effects buff the attacker's own mitigation on
        // counter-fire. They must NOT compile to `ShieldMitigationBonus`, which the engine
        // adds to the *defender's* mitigation (would buff the defender and hurt the attacker).
        let spec = shield_mitigation_spec(AbilityTargetSpec::AttackerSelf, 0.18);
        let (_, effect, _) = compile_officer_combat_spec(&spec).expect("self mitigation compiles");
        assert!(
            matches!(effect, AbilityEffect::AttackerShieldMitigationBonus(v) if (v - 0.18).abs() < 1e-12),
            "AttackerSelf target must emit AttackerShieldMitigationBonus, got {effect:?}"
        );
    }

    #[test]
    fn shield_mitigation_compile_defender_opponent_emits_bypass_fraction() {
        // Harrison's "Sabotage": canonical `op: MultiplySub, value 0.7 (rank 2), target EnemyShip`
        // → multiplicative bypass of defender's shield_mitigation. Engine math:
        // `mitigation × (1 - 0.7)`. Regression test for the `harrison-56cc6c` fidelity gap.
        let spec = shield_mitigation_spec(AbilityTargetSpec::DefenderOpponent, 0.7);
        let (_, effect, _) =
            compile_officer_combat_spec(&spec).expect("defender shield mitigation");
        assert!(
            matches!(effect, AbilityEffect::ShieldMitigationBypassFraction(v) if (v - 0.7).abs() < 1e-12),
            "DefenderOpponent target should emit ShieldMitigationBypassFraction(0.7), got {effect:?}"
        );
    }

    #[test]
    fn shield_mitigation_compile_defender_opponent_clamps_bypass_at_100pct() {
        // Belt-and-suspenders clamp: a single source with value > 1.0 must not bypass more than
        // 100% of the defender's mitigation. (The engine also clamps the *total* across sources.)
        let spec = shield_mitigation_spec(AbilityTargetSpec::DefenderOpponent, 1.4);
        let (_, effect, _) =
            compile_officer_combat_spec(&spec).expect("defender shield mitigation > 100%");
        assert!(
            matches!(effect, AbilityEffect::ShieldMitigationBypassFraction(v) if (v - 1.0).abs() < 1e-12),
            "Bypass > 100% must clamp to 1.0; got {effect:?}"
        );
    }

    #[test]
    fn officer_max_fraction_regen_compiles_to_distinct_effects() {
        let mut spec = CombatEffectSpec {
            id: "lcars:test:max_shield_regen".into(),
            source: EffectSource::LcarsOfficer,
            source_ref: None,
            text: None,
            trigger: AbilityTriggerSpec::RoundStart,
            target: AbilityTargetSpec::SelfShip,
            modifier: AbilityModifierSpec::OfficerShieldRegenMaxFraction,
            operation: AbilityOperationSpec::Add,
            value: Some(ValueSpec {
                scalar: Some(0.12),
                by_rank: None,
                unit: None,
                officer_stat_scaling: None,
            }),
            chance: None,
            duration: None,
            decay: None,
            accumulate: None,
            conditions: vec![],
            attributes: serde_json::Map::new(),
            stacking: None,
            category: Some(EffectCategory::Combat),
            confidence: Some(EffectConfidence::Authoritative),
        };
        let (_, effect, _) = compile_officer_combat_spec(&spec).expect("shield max fraction");
        assert!(
            matches!(effect, AbilityEffect::ShieldRegenMaxFraction(v) if (v - 0.12).abs() < 1e-12)
        );

        spec.id = "lcars:test:max_hull_regen".into();
        spec.modifier = AbilityModifierSpec::OfficerHullRegenMaxFraction;
        spec.value.as_mut().unwrap().scalar = Some(0.25);
        let (_, effect, _) = compile_officer_combat_spec(&spec).expect("hull max fraction");
        assert!(
            matches!(effect, AbilityEffect::HullRegenMaxFraction(v) if (v - 0.25).abs() < 1e-12)
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
                officer_stat_scaling: None,
            }),
            chance: None,
            duration: None,
            decay: None,
            accumulate: None,
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
