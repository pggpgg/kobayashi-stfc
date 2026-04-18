//! Compile [`crate::data::combat_effect_spec::CombatEffectSpec`] into engine runtime structs.

use crate::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::combat::condition::combine_optional_and;
use crate::combat::types::{OpponentFactionTag, ShipType};
use crate::combat::TimingWindow;
use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, CombatEffectSpec, ValueSpec,
};
use crate::data::ship_ability_resolve;

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
