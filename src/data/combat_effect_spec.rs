//! Canonical combat effect IR (`CombatEffectSpec`): shared schema for officers (LCARS), research,
//! ship/hostile abilities, and optional cheat-sheet ingestion before compiling into engine types
//! (`AbilityEffect`, `AbilityCondition`, `TimingWindow`).
//!
//! Terminology (stfc.cc–style vocabulary, adapted as serde names on this IR):
//! - **Modifier** — what the ability changes (stat, proc, state, …).
//! - **Attributes** — extra numeric or categorical parameters (`attributes` map); resource targets,
//!   faction ids, round caps, durations, etc.
//! - **Conditions** — gates; all must pass (AND) unless expressed as `or` / `not` nodes.
//! - **Trigger** — evaluation timing; [`AbilityTriggerSpec::ShipLaunched`] matches passive “ship
//!   is in combat” defaults (compiled to [`crate::combat::TimingWindow::CombatBegin`]).
//! - **Target** — who receives the effect (`SelfShip` / [`AbilityTargetSpec::AttackerSelf`] vs enemy).
//! - **Operation** — how the modifier combines with formulas (`add`, `multiply`, …).

use serde::{Deserialize, Serialize};

/// Whether the CombatEffectSpec adapter + compiler path is used for supported flows (research-derived
/// attack-phase seats). **Default: enabled.** Set `KOBAYASHI_COMBAT_EFFECT_SPEC_DISABLE=1` (or `true`/`yes`)
/// to force the legacy Rust path, or `KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE=0` / `false` / `no` to disable.
pub fn combat_effect_spec_enabled() -> bool {
    if std::env::var("KOBAYASHI_COMBAT_EFFECT_SPEC_DISABLE")
        .ok()
        .as_deref()
        .is_some_and(env_truthy)
    {
        return false;
    }
    if let Ok(v) = std::env::var("KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE") {
        if env_falsy(&v) {
            return false;
        }
    }
    true
}

fn env_truthy(s: &str) -> bool {
    s == "1" || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes")
}

fn env_falsy(s: &str) -> bool {
    s == "0" || s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("no")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectSource {
    LcarsOfficer,
    ResearchCatalog,
    ShipAbilityCatalog,
    HostileAbilityCatalog,
    StfcCcCheatSheet,
    Manual,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub officer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ability_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rid: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fid: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ship_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loca_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityTriggerSpec {
    CombatBegin,
    /// Passive / generic ship bonuses (“ship launched” / stfc.cc default) — same timing as combat begin.
    ShipLaunched,
    RoundStart,
    AttackPhase,
    AfterSubround,
    DefensePhase,
    RoundEnd,
    ShieldBreak,
    SelfShieldBreak,
    Kill,
    HullBreach,
    ReceiveDamage,
    CombatEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityTargetSpec {
    /// Player / attacking ship receiving the buff.
    AttackerSelf,
    /// Alias for [`Self::AttackerSelf`] (SelfShip terminology).
    #[serde(alias = "self_ship")]
    SelfShip,
    /// Opponent hull the ability references (enemy ship).
    DefenderOpponent,
    #[serde(alias = "enemy_ship")]
    EnemyShip,
    AttackerTeam,
    DefenderTeam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityModifierSpec {
    WeaponDamage,
    HullHp,
    ShieldHp,
    CritChance,
    CritDamage,
    Pierce,
    ShieldMitigation,
    Armor,
    Dodge,
    DamageReduction,
    Accuracy,
    IsolyticDamage,
    IsolyticDefense,
    IsolyticCascadeDamage,
    ApexShred,
    ApexBarrier,
    StateMorale,
    StateBurning,
    StateHullBreach,
    StateAssimilated,
    ShotsBonus,
    ProcAttackMultiplier,
    ProcPierceBonus,
    HostileCritDamageReduction,
    CumulativeOpponentShieldMitigationDebuff,
    TagOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityOperationSpec {
    Add,
    Multiply,
    Set,
    Min,
    Max,
    ChanceApply,
    StateApply,
    StateExtend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueUnit {
    Fraction,
    Flat,
    Rounds,
    Stacks,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalar: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_rank: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<ValueUnit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChanceSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalar: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_rank: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DurationSpec {
    Permanent,
    Rounds { rounds: u32 },
    Stacks { stacks: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AbilityConditionSpec {
    MoraleActive,
    DefenderBurning,
    DefenderHullBreach,
    AttackerBurning,
    AttackerHullBreach,
    DefenderAssimilated,
    DefenderIsNpcHostile,
    DefenderIsPlayerShip,
    DefenderShipTypeIs { ship_type: String },
    AttackerShipTypeIs { ship_type: String },
    DefenderFactionIs { faction: String },
    DefenderHullFactionIdIs { faction_id: i64 },
    RoundRange { min: u32, max: u32 },
    StatBelow {
        stat: String,
        threshold_pct: f64,
    },
    StatAbove {
        stat: String,
        threshold_pct: f64,
    },
    Not {
        inner: Box<AbilityConditionSpec>,
    },
    And {
        all: Vec<AbilityConditionSpec>,
    },
    Or {
        any: Vec<AbilityConditionSpec>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackingPolicySpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additive_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiplicative_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_stacks: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectCategory {
    Combat,
    NonCombat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectConfidence {
    Authoritative,
    Inferred,
    Heuristic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatEffectSpec {
    pub id: String,
    pub source: EffectSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<SourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<EffectText>,
    pub trigger: AbilityTriggerSpec,
    pub target: AbilityTargetSpec,
    pub modifier: AbilityModifierSpec,
    pub operation: AbilityOperationSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chance: Option<ChanceSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<DurationSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<AbilityConditionSpec>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stacking: Option<StackingPolicySpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<EffectCategory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<EffectConfidence>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_trigger_target() {
        let t = AbilityTriggerSpec::ShipLaunched;
        let j = serde_json::to_string(&t).unwrap();
        let back: AbilityTriggerSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn serde_combat_effect_spec_json_example() {
        let spec = CombatEffectSpec {
            id: "research:365419690:l1".into(),
            source: EffectSource::ResearchCatalog,
            source_ref: Some(SourceRef {
                rid: Some(365419690),
                buff_id: Some(1_898_558_353),
                loca_id: Some(70106),
                ..Default::default()
            }),
            text: None,
            trigger: AbilityTriggerSpec::AttackPhase,
            target: AbilityTargetSpec::AttackerSelf,
            modifier: AbilityModifierSpec::WeaponDamage,
            operation: AbilityOperationSpec::Add,
            value: Some(ValueSpec {
                scalar: Some(0.01),
                by_rank: None,
                unit: Some(ValueUnit::Fraction),
            }),
            chance: None,
            duration: None,
            conditions: vec![AbilityConditionSpec::DefenderBurning],
            attributes: serde_json::Map::new(),
            stacking: None,
            category: Some(EffectCategory::Combat),
            confidence: Some(EffectConfidence::Authoritative),
        };
        let json = serde_json::to_value(&spec).unwrap();
        let back: CombatEffectSpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn condition_tree_not_and_roundtrip() {
        let c = AbilityConditionSpec::And {
            all: vec![
                AbilityConditionSpec::RoundRange { min: 1, max: 2 },
                AbilityConditionSpec::Not {
                    inner: Box::new(AbilityConditionSpec::DefenderBurning),
                },
            ],
        };
        let j = serde_json::to_string(&c).unwrap();
        let back: AbilityConditionSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c);
    }
}
