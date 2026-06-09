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

/// Whether CombatEffectSpec-based research routing is considered “on” for diagnostics (HTTP debug, etc.).
/// Research-derived attack-phase seats **always** use the spec adapter + compiler in this codebase.
/// Set `KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE=0` / `false` / `no` to report disabled for tooling.
pub fn combat_effect_spec_enabled() -> bool {
    if let Ok(v) = std::env::var("KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE") {
        if env_falsy(&v) {
            return false;
        }
    }
    true
}

/// When set, the API exposes `GET /api/debug/combat-effect-spec/officers/:id` (see server routes).
/// **Off by default** — enable only for local investigation (`KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG=1`).
pub fn combat_effect_spec_debug_http_enabled() -> bool {
    std::env::var("KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG")
        .ok()
        .as_deref()
        .is_some_and(env_truthy)
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
    /// Opponent captain + bridge officers only (canonical `EnemyBridge`, e.g. Kras OA).
    #[serde(alias = "enemy_bridge")]
    EnemyBridgeOfficers,
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
    /// LCARS `armor`; serde wire remains `"armor"` for existing combat specs.
    #[serde(rename = "armor")]
    MitigationAdditive,
    /// Research/catalog `shield_deflection`; compiles identically to [`MitigationAdditive`](AbilityModifierSpec::MitigationAdditive).
    ShieldDeflection,
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
    /// LCARS `extra_attack` — crew-aggregate proc that triggers an additional weapon shot at
    /// `chance` with damage `multiplier`. Aggregates across all officers into
    /// [`crate::combat::types::BuffSet::proc_chance`] / `proc_multiplier`
    /// (highest-chance-wins, tiebroken by highest multiplier). Compiled via
    /// [`crate::combat::effect_spec_compile::compile_officer_buffset_proc`] rather than the
    /// per-round [`crate::combat::effect_spec_compile::compile_officer_combat_spec`] path —
    /// `compile_officer_combat_spec` returns `Err` for this modifier.
    ExtraAttackProc,
    HostileCritDamageReduction,
    CumulativeOpponentShieldMitigationDebuff,
    /// LCARS `shield_regen` / `shield_hp_repair` → [`crate::combat::abilities::AbilityEffect::ShieldRegen`].
    OfficerShieldRegenFlat,
    /// LCARS `shield_regen_max_fraction` / `shield_hp_repair_max_fraction` → restore a fraction of max shield HP.
    OfficerShieldRegenMaxFraction,
    /// LCARS `hull_repair` / `hull_hp_repair` (non-kill timings) → [`crate::combat::abilities::AbilityEffect::HullRegen`].
    OfficerHullRegenFlat,
    /// LCARS `hull_repair_max_fraction` / `hull_hp_repair_max_fraction` → restore a fraction of max hull HP.
    OfficerHullRegenMaxFraction,
    /// LCARS `hull_hp_repair_prev_round` (engine timing: round start).
    OfficerHullRegenPrevRoundFraction,
    /// LCARS `shield_hp_repair_prev_round` (engine timing: round start).
    OfficerShieldRegenPrevRoundFraction,
    /// LCARS `officerstathealth` / `attack` (officer-rating axis) — buffs the per-side officer Attack
    /// rating before breakpoint lookup. Defined in Phase 1; consumed by ability handlers in Phase 3.
    /// See `docs/OFFICER_STAT_FORMULA.md` §2b.
    OfficerAttack,
    /// LCARS `defense` (officer-rating axis) — buffs the per-side officer Defense rating.
    /// Defined in Phase 1; consumed in Phase 3. See `docs/OFFICER_STAT_FORMULA.md` §2c.
    OfficerDefense,
    /// LCARS `health` (officer-rating axis) — buffs the per-side officer Health rating.
    /// Defined in Phase 1; consumed in Phase 3. See `docs/OFFICER_STAT_FORMULA.md` §2d.
    OfficerHealth,
    /// LCARS `officerstatall` — buffs the per-side officer Attack / Defense / Health ratings
    /// simultaneously (the "all three stats" widget). See `docs/OFFICER_STAT_FORMULA.md` §3.
    OfficerStatAll,
    /// LCARS `allreloadspeed:enemy_delay` / `allloadspeed` enemy-target Add — skip defender counter-fire.
    DefenderFireDelay,
    /// LCARS `allreloadspeed:self_recharge` — self-target Sub reload at combat start (Kuron-style).
    AttackerWeaponRecharge,
    /// LCARS `addrandomstate` on round start — apply morale, burning, or hull breach at random.
    RandomDefenderState,
    /// LCARS `cptmaneuvereffect` — scale opponent captain maneuver seat effects (PvP-shaped).
    OpponentCaptainManeuverEffect,
    /// LCARS `offabilityeffect` at combat begin (Pike / McCoy / Picard captain) — additive bonus to
    /// bridge officer ability magnitudes: `effective = min(1, base × (1 + bonus))` for capped stats.
    BridgeAbilityEffectiveness,
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
    /// Officer-stat scaling. When set, the resolved scalar is `coefficient_per_rank[rank]/100 *
    /// officer.<stat>` at the chosen officer level. Captures the raw scaling for traceability;
    /// `scalar` carries the already-resolved final value the engine will consume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub officer_stat_scaling: Option<OfficerStatScaling>,
}

/// Officer's own stat (Attack / Defense / Health) used as a multiplier source for
/// [`OfficerStatScaling`]. STFC convention used by upstream canonical attribute encoding
/// (`officer_stat=1` ⇒ Attack, `=2` ⇒ Defense, `=3` ⇒ Health).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficerStat {
    Attack,
    Defense,
    Health,
}

/// Coefficients-per-rank reference to one of the officer's own stats. Coefficients are interpreted
/// as percentages: `15.0` means `+15% of officer.<stat>`. Resolution: index 0 → rank 1, last entry
/// → highest known rank; out-of-range ranks clamp to the table edges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficerStatScaling {
    pub stat: OfficerStat,
    pub coefficient_per_rank: Vec<f64>,
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
    /// Matches [`crate::combat::abilities::AbilityCondition::AttackerOfficerTalNotOnBridge`].
    AttackerOfficerTalNotOnBridge,
    /// Matches [`crate::combat::abilities::AbilityCondition::LiteralBool`].
    LiteralBool {
        value: bool,
    },
    DefenderShipTypeIs {
        ship_type: String,
    },
    AttackerShipTypeIs {
        ship_type: String,
    },
    /// Player ship identity gate ([`crate::combat::abilities::AbilityCondition::AttackerShipIdIs`]).
    AttackerShipIdIs {
        ship_id: String,
    },
    DefenderFactionIs {
        faction: String,
    },
    /// Player hull owner faction gate (research `attacker_faction` / `attacker_factions`).
    AttackerOwnerFactionIs {
        faction: String,
    },
    DefenderHullFactionIdIs {
        faction_id: i64,
    },
    RoundRange {
        min: u32,
        max: u32,
    },
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
    /// Engagement category gate (LCARS `engagement_includes` / `engagement_has`); `enemy_type` is a
    /// snake_case slug resolved like [`crate::combat::types::enemy_type_from_engagement_slug`].
    EngagementIncludes {
        enemy_type: String,
    },
    /// STFC combat battle-type id allow-list (LCARS `combat_battle_type_any`).
    CombatBattleTypeAny {
        battle_types: Vec<u32>,
    },
    /// Defender level ceiling (LCARS `defender_level_at_most` / `target_max_level`).
    DefenderLevelAtMost {
        max_level: u32,
    },
    /// stfc.cc / upstream token with no [`crate::combat::abilities::AbilityCondition`] mapping yet.
    /// [`crate::combat::effect_spec_compile::compile_condition`] returns an error for this variant.
    StfcCcToken {
        token: String,
    },
}

/// Per-round multiplicative decay applied to an [`AbilityModifierSpec::WeaponDamage`] effect.
/// Compiled into [`crate::combat::abilities::AbilityEffect::DecayingAttackMultiplier`]:
/// `initial = (1 + value.scalar)`; per-round value `-= amount`, clamped at `floor`.
///
/// **Schema rule**: only valid when `modifier == WeaponDamage`. Other modifiers + decay are
/// recorded as `unsupported_decay_on_stat:<stat>` drops by the adapter and the effect is
/// discarded (vs the pre-Phase-3 behaviour of silent loss via attribute-pack lookup).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DecaySpec {
    pub amount: f64,
    pub floor: f64,
}

/// Per-round multiplicative accumulation applied to an [`AbilityModifierSpec::WeaponDamage`]
/// effect. Compiled into [`crate::combat::abilities::AbilityEffect::AccumulatingAttackMultiplier`]:
/// `initial = (1 + value.scalar)`; per-round value `+= amount`, clamped at `ceiling`.
///
/// **Schema rule**: same `modifier == WeaponDamage` restriction as [`DecaySpec`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AccumulateSpec {
    pub amount: f64,
    pub ceiling: f64,
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

fn is_false(b: &bool) -> bool {
    !*b
}

/// Typed officer-specific carry that previously lived untyped in [`CombatEffectSpec::attributes`].
/// Only the LCARS officer adapter populates these; every other producer leaves them at default,
/// and `skip_serializing_if` keeps them out of the serialized form when empty (so non-officer
/// specs serialize unchanged). Promoting them to typed fields makes officer specs self-describing
/// and lets the spec validator reason about them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OfficerSpecAttrs {
    /// Raw LCARS operator string after dash/underscore normalization (resolver `normalize_operator`).
    /// Preserves multiply-family variants (`mul_add`, `multiply_base_sub`, …) that the coarser
    /// [`AbilityOperationSpec`] collapses into `Add`. `None` → the compiler derives it from `operation`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lcars_op: Option<String>,
    /// When true, the effect only procs on critical hits (e.g. Chang's bridge reload delay).
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_critical: bool,
    /// STFC state ids from canonical `multi_state=[…]` (each id is also its own weight). Empty →
    /// the compiler falls back to `DEFAULT_RANDOM_STATE_WEIGHTS`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub random_state_weights: Vec<u32>,
}

impl OfficerSpecAttrs {
    /// True when no officer-specific carry is set (used to skip serialization).
    pub fn is_empty(&self) -> bool {
        self.lcars_op.is_none() && !self.requires_critical && self.random_state_weights.is_empty()
    }
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
    /// Per-round multiplicative decay applied to a `WeaponDamage` effect. See [`DecaySpec`].
    /// `decay` and `accumulate` are mutually exclusive on the same spec; the adapter / compiler
    /// rejects both being set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<DecaySpec>,
    /// Per-round multiplicative accumulation applied to a `WeaponDamage` effect. See [`AccumulateSpec`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accumulate: Option<AccumulateSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<AbilityConditionSpec>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    /// Typed officer-specific carry (formerly stuffed into `attributes`). See [`OfficerSpecAttrs`].
    #[serde(default, skip_serializing_if = "OfficerSpecAttrs::is_empty")]
    pub officer: OfficerSpecAttrs,
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
                officer_stat_scaling: None,
            }),
            chance: None,
            duration: None,
            decay: None,
            accumulate: None,
            conditions: vec![AbilityConditionSpec::DefenderBurning],
            attributes: serde_json::Map::new(),
            officer: OfficerSpecAttrs::default(),
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

    #[test]
    fn serde_stfc_cc_token_roundtrip() {
        let c = AbilityConditionSpec::StfcCcToken {
            token: "TargetNotASB".into(),
        };
        let j = serde_json::to_string(&c).unwrap();
        let back: AbilityConditionSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn serde_literal_bool_roundtrip() {
        let c = AbilityConditionSpec::LiteralBool { value: false };
        let j = serde_json::to_string(&c).unwrap();
        let back: AbilityConditionSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c);
    }
}
