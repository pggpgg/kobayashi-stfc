//! Structural and light semantic validation for [`CombatEffectSpec`].

use crate::data::combat_effect_spec::{
    AbilityConditionSpec, ChanceSpec, CombatEffectSpec, ValueSpec,
};

#[derive(Debug, Clone, PartialEq)]
pub enum CombatEffectSpecValidationError {
    EmptyId,
    ChanceOutOfRange { field: &'static str, value: f64 },
    RoundRangeInverted { min: u32, max: u32 },
    EmptyAndOr { which: &'static str },
    EmptyCombatBattleTypeList,
    ValueSpecEmpty,
}

impl std::fmt::Display for CombatEffectSpecValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyId => write!(f, "CombatEffectSpec.id must be non-empty"),
            Self::ChanceOutOfRange { field, value } => {
                write!(f, "chance value out of [0,1] for {field}: {value}")
            }
            Self::RoundRangeInverted { min, max } => {
                write!(f, "round_range min ({min}) must be <= max ({max})")
            }
            Self::EmptyAndOr { which } => write!(f, "{which} must contain at least one condition"),
            Self::EmptyCombatBattleTypeList => {
                write!(f, "combat_battle_type_any requires non-empty battle_types")
            }
            Self::ValueSpecEmpty => write!(f, "value must set scalar or by_rank when present"),
        }
    }
}

impl std::error::Error for CombatEffectSpecValidationError {}

fn validate_chance_spec(
    ch: &ChanceSpec,
    field: &'static str,
) -> Result<(), CombatEffectSpecValidationError> {
    if let Some(s) = ch.scalar {
        if !s.is_finite() || !(0.0..=1.0).contains(&s) {
            return Err(CombatEffectSpecValidationError::ChanceOutOfRange { field, value: s });
        }
    }
    if let Some(ref table) = ch.by_rank {
        for &v in table {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err(CombatEffectSpecValidationError::ChanceOutOfRange { field, value: v });
            }
        }
    }
    Ok(())
}

fn validate_value_spec(v: &ValueSpec) -> Result<(), CombatEffectSpecValidationError> {
    if v.scalar.is_none() && v.by_rank.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
        return Err(CombatEffectSpecValidationError::ValueSpecEmpty);
    }
    Ok(())
}

fn validate_condition_tree(
    c: &AbilityConditionSpec,
) -> Result<(), CombatEffectSpecValidationError> {
    match c {
        AbilityConditionSpec::RoundRange { min, max } => {
            if min > max {
                return Err(CombatEffectSpecValidationError::RoundRangeInverted {
                    min: *min,
                    max: *max,
                });
            }
        }
        AbilityConditionSpec::And { all } => {
            if all.is_empty() {
                return Err(CombatEffectSpecValidationError::EmptyAndOr { which: "and" });
            }
            for child in all {
                validate_condition_tree(child)?;
            }
        }
        AbilityConditionSpec::Or { any } => {
            if any.is_empty() {
                return Err(CombatEffectSpecValidationError::EmptyAndOr { which: "or" });
            }
            for child in any {
                validate_condition_tree(child)?;
            }
        }
        AbilityConditionSpec::Not { inner } => validate_condition_tree(inner)?,
        AbilityConditionSpec::CombatBattleTypeAny { battle_types } => {
            if battle_types.is_empty() {
                return Err(CombatEffectSpecValidationError::EmptyCombatBattleTypeList);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Validate ids, chance ranges, round ranges, and non-empty `and`/`or` nodes.
pub fn validate_combat_effect_spec(
    spec: &CombatEffectSpec,
) -> Result<(), CombatEffectSpecValidationError> {
    if spec.id.trim().is_empty() {
        return Err(CombatEffectSpecValidationError::EmptyId);
    }
    if let Some(ref ch) = spec.chance {
        validate_chance_spec(ch, "chance")?;
    }
    if let Some(ref v) = spec.value {
        validate_value_spec(v)?;
    }
    for c in &spec.conditions {
        validate_condition_tree(c)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::combat_effect_spec::{
        AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec, AbilityTriggerSpec,
        ChanceSpec, CombatEffectSpec, EffectSource, ValueSpec,
    };

    fn minimal_spec() -> CombatEffectSpec {
        CombatEffectSpec {
            id: "t".into(),
            source: EffectSource::Manual,
            source_ref: None,
            text: None,
            trigger: AbilityTriggerSpec::AttackPhase,
            target: AbilityTargetSpec::AttackerSelf,
            modifier: AbilityModifierSpec::WeaponDamage,
            operation: AbilityOperationSpec::Add,
            value: Some(ValueSpec {
                scalar: Some(0.1),
                by_rank: None,
                unit: None,
            }),
            chance: None,
            duration: None,
            conditions: vec![],
            attributes: serde_json::Map::new(),
            stacking: None,
            category: None,
            confidence: None,
        }
    }

    #[test]
    fn rejects_empty_id() {
        let mut s = minimal_spec();
        s.id = "  ".into();
        assert!(matches!(
            validate_combat_effect_spec(&s),
            Err(CombatEffectSpecValidationError::EmptyId)
        ));
    }

    #[test]
    fn rejects_chance_above_one() {
        let mut s = minimal_spec();
        s.chance = Some(ChanceSpec {
            scalar: Some(1.5),
            by_rank: None,
        });
        assert!(validate_combat_effect_spec(&s).is_err());
    }

    #[test]
    fn rejects_inverted_round_range_in_condition() {
        use crate::data::combat_effect_spec::AbilityConditionSpec;
        let mut s = minimal_spec();
        s.conditions = vec![AbilityConditionSpec::RoundRange { min: 5, max: 1 }];
        assert!(validate_combat_effect_spec(&s).is_err());
    }

    #[test]
    fn rejects_empty_value_spec_when_present() {
        let mut s = minimal_spec();
        s.value = Some(ValueSpec {
            scalar: None,
            by_rank: None,
            unit: None,
        });
        assert!(matches!(
            validate_combat_effect_spec(&s),
            Err(CombatEffectSpecValidationError::ValueSpecEmpty)
        ));
    }
}
