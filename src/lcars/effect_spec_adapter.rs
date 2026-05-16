//! LCARS YAML → [`crate::data::combat_effect_spec::CombatEffectSpec`] (canonical IR).
//!
//! Officer dynamic effects resolve through this adapter plus
//! [`crate::combat::effect_spec_compile::compile_officer_combat_spec`] (see
//! [`crate::lcars::resolver::resolve_effect`]). HTTP debug and parity tests use the same path.

use crate::combat::effect_spec_compile::{
    OFFICER_SPEC_ATTR_LCARS_OP, OFFICER_SPEC_ATTR_WEAPON_DAMAGE_ACCUMULATE,
    OFFICER_SPEC_ATTR_WEAPON_DAMAGE_DECAY,
};
use crate::combat::TimingWindow;
use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, ChanceSpec, CombatEffectSpec, DurationSpec, EffectSource,
    OfficerStatScaling, ValueSpec,
};
use crate::lcars::parser::{
    LcarsCondition, LcarsDuration, LcarsEffect, LcarsLevelStats, LcarsOfficer,
};
use crate::lcars::resolver::effect_trigger_timing;
use serde::Serialize;
use serde_json::json;

/// A single LCARS effect that was silently dropped during YAML→IR conversion.
///
/// Populated by [`lcars_effect_to_combat_effect_spec_with_report`] at each adapter early-return
/// site. See [`LcarsDropReport`] for the aggregator. The `reason` field uses short discriminators
/// like `unknown_trigger:<raw>`, `unmapped_tag:<base>`, `unmapped_stat:<name>`,
/// `unmapped_condition:<type>`, `extra_attack_unsupported`, or `unknown_effect_type:<type>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DroppedLcarsEffect {
    pub officer_id: String,
    pub ability_name: String,
    pub effect_index: usize,
    pub reason: String,
}

/// Aggregator that collects effects silently dropped by the LCARS→IR adapter.
///
/// Records are only pushed at the load/resolve boundary
/// ([`lcars_effect_to_combat_effect_spec_with_report`]). Nothing inside `src/combat/` may
/// write to this — enforced by an architectural test.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LcarsDropReport {
    pub drops: Vec<DroppedLcarsEffect>,
}

impl LcarsDropReport {
    /// Number of drops collected so far.
    pub fn len(&self) -> usize {
        self.drops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.drops.is_empty()
    }

    fn record(
        &mut self,
        officer_id: &str,
        ability_name: &str,
        effect_index: usize,
        reason: impl Into<String>,
    ) {
        self.drops.push(DroppedLcarsEffect {
            officer_id: officer_id.to_string(),
            ability_name: ability_name.to_string(),
            effect_index,
            reason: reason.into(),
        });
    }

    /// Drop reason prefix (before the first `:`). E.g. `"unmapped_tag:allreloadspeed"` →
    /// `"unmapped_tag"`. Reasons without a colon (e.g. `"extra_attack_unsupported"`) return
    /// themselves.
    pub fn reason_category(reason: &str) -> &str {
        reason.split_once(':').map(|(c, _)| c).unwrap_or(reason)
    }

    /// `(category, count, distinct_officer_count)` triples sorted by count descending.
    pub fn category_counts(&self) -> Vec<(String, usize, usize)> {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, (usize, std::collections::HashSet<&str>)> = HashMap::new();
        for d in &self.drops {
            let cat = Self::reason_category(&d.reason);
            let entry = counts.entry(cat).or_default();
            entry.0 += 1;
            entry.1.insert(d.officer_id.as_str());
        }
        let mut out: Vec<(String, usize, usize)> = counts
            .into_iter()
            .map(|(k, (c, set))| (k.to_string(), c, set.len()))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// `(reason, count)` pairs sorted by count descending.
    pub fn reasons_by_count(&self) -> Vec<(String, usize)> {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for d in &self.drops {
            *counts.entry(d.reason.as_str()).or_default() += 1;
        }
        let mut out: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// `(officer_id, count, top_reason)` triples sorted by count descending.
    pub fn officers_by_count(&self) -> Vec<(String, usize, String)> {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, (usize, HashMap<&str, usize>)> = HashMap::new();
        for d in &self.drops {
            let entry = counts.entry(d.officer_id.as_str()).or_default();
            entry.0 += 1;
            *entry.1.entry(d.reason.as_str()).or_default() += 1;
        }
        let mut out: Vec<(String, usize, String)> = counts
            .into_iter()
            .map(|(officer, (total, reasons))| {
                let top = reasons
                    .into_iter()
                    .max_by_key(|(_, c)| *c)
                    .map(|(r, _)| r.to_string())
                    .unwrap_or_default();
                (officer.to_string(), total, top)
            })
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }
}

/// Walk an iterator of officers and feed every effect through
/// [`lcars_effect_to_combat_effect_spec_with_report`] to populate a fresh [`LcarsDropReport`].
///
/// Officer tier / stats are passed as `None` because none of the drop categories depend on
/// scaling resolution; this keeps the helper free of [`crate::lcars::resolver::ResolveOptions`].
pub fn collect_lcars_drops<'a, I>(officers: I) -> LcarsDropReport
where
    I: IntoIterator<Item = &'a LcarsOfficer>,
{
    let mut report = LcarsDropReport::default();
    for officer in officers {
        for ability in [
            officer.captain_ability.as_ref(),
            officer.bridge_ability.as_ref(),
            officer.below_decks_ability.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            for (idx, effect) in ability.effects.iter().enumerate() {
                let stable_id = format!("{}::{}::{}", officer.id, ability.name, idx);
                let _ = lcars_effect_to_combat_effect_spec_with_report(
                    effect,
                    &stable_id,
                    &officer.id,
                    &ability.name,
                    None,
                    None,
                    idx,
                    Some(&mut report),
                );
            }
        }
    }
    report
}

/// Push a drop record when the caller asked for one; no-op otherwise.
fn maybe_record_drop(
    drop_report: &mut Option<&mut LcarsDropReport>,
    officer_id: &str,
    ability_name: &str,
    effect_index: usize,
    reason: impl Into<String>,
) {
    if let Some(r) = drop_report.as_deref_mut() {
        r.record(officer_id, ability_name, effect_index, reason);
    }
}

fn normalize_operator(op: Option<&str>) -> String {
    op.unwrap_or("add")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
}

/// Map a combat-intent LCARS `tag` string (e.g. `shieldmitigation:unmapped`) to an engine stat
/// name when the tag represents a modifier directly expressible as `stat_modify`.
/// Returns [`None`] for economy / non-combat tags or tags without a direct stat mapping.
pub fn combat_tag_to_stat(tag: &str) -> Option<&'static str> {
    let base = tag
        .trim()
        .split(':')
        .next()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match base.as_str() {
        "shieldmitigation" => Some("shield_mitigation"),
        "shipdodge" => Some("dodge"),
        "shieldpiercing" => Some("pierce"),
        "accuracy" => Some("accuracy"),
        "shields" => Some("shield_hp"),
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
        "literal_true" => Ok(AbilityConditionSpec::LiteralBool { value: true }),
        "literal_false" => Ok(AbilityConditionSpec::LiteralBool { value: false }),
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
        "armor" => Some(AbilityModifierSpec::MitigationAdditive),
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
        "shield_regen_max_fraction"
        | "shield_hp_repair_max_fraction"
        | "shield_regen_max_pct"
        | "shield_hp_repair_max_pct" => Some(AbilityModifierSpec::OfficerShieldRegenMaxFraction),
        "hull_repair" | "hull_hp_repair" => Some(AbilityModifierSpec::OfficerHullRegenFlat),
        "hull_repair_max_fraction"
        | "hull_hp_repair_max_fraction"
        | "hull_repair_max_pct"
        | "hull_hp_repair_max_pct" => Some(AbilityModifierSpec::OfficerHullRegenMaxFraction),
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

/// Resolve numeric value for LCARS `stat_modify` scaling, including `scaling.officer_stat` when set.
/// Matches [`effect_value_with_officer_stat`] / dynamic CombatEffectSpec compile — used for passive
/// `static_buffs` and combat-begin accuracy accumulation in [`crate::lcars::resolver::resolve_crew_to_buff_set`].
pub fn lcars_effect_resolved_value(
    effect: &LcarsEffect,
    tier: Option<u8>,
    officer_stats: Option<&LcarsLevelStats>,
) -> Option<f64> {
    effect_value_with_officer_stat(effect, tier, officer_stats).0
}

/// Resolve effect value for officer-stat scaling. When `effect.scaling.officer_stat` is set and
/// per-level stats are available, the rank coefficient is multiplied by the officer's stat divided
/// by 100. When the officer-stat reference is set but stats are missing, the rank coefficient
/// passes through unchanged (no-op fallback) — equivalent to the historical behavior, plus the
/// scaling clause is preserved on `ValueSpec.officer_stat_scaling` for traceability.
fn effect_value_with_officer_stat(
    effect: &LcarsEffect,
    tier: Option<u8>,
    officer_stats: Option<&LcarsLevelStats>,
) -> (Option<f64>, Option<OfficerStatScaling>) {
    let base_value = effect_value_at_officer_tier(effect, tier);
    let Some(ref scaling) = effect.scaling else {
        return (base_value, None);
    };
    let Some(stat) = scaling.officer_stat else {
        return (base_value, None);
    };
    let coefficients_per_rank = scaling
        .values
        .clone()
        .or_else(|| {
            let max_rank = scaling.max_rank.unwrap_or(5).max(1) as usize;
            let base = scaling.base?;
            let per = scaling.per_rank.unwrap_or(0.0);
            Some((0..max_rank).map(|i| base + per * i as f64).collect())
        })
        .unwrap_or_default();
    let scaling_spec = OfficerStatScaling {
        stat,
        coefficient_per_rank: coefficients_per_rank,
    };
    let rank_value = base_value;
    let resolved = match (rank_value, officer_stats) {
        (Some(rv), Some(stats)) => Some(rv * stats.value_for(stat) / 100.0),
        (Some(rv), None) => Some(rv),
        (None, _) => None,
    };
    (resolved, Some(scaling_spec))
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

/// `None` when YAML has a `condition` block that cannot be represented in [`AbilityConditionSpec`]
/// (caller must not emit a spec row without that gate — see [`lcars_effect_to_combat_effect_spec`]).
fn try_officer_conditions_from_effect(effect: &LcarsEffect) -> Option<Vec<AbilityConditionSpec>> {
    match effect.condition.as_ref() {
        None => Some(Vec::new()),
        Some(c) => match lcars_condition_to_spec(c) {
            Ok(s) => Some(vec![s]),
            Err(_) => None,
        },
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
/// Returns [`None`] for `extra_attack`, `accuracy` at CombatBegin,
/// unknown triggers, or a present `condition` that [`lcars_condition_to_spec`] cannot encode
/// (aligned with [`crate::lcars::resolver::resolve_effect`]).
///
/// Passive-permanent `stat_modify` / mapped-tag effects now emit `CombatBegin`-timed specs
/// routed through the canonical IR in [`crate::lcars::resolver::resolve_crew_to_buff_set`].
///
/// `officer_stats` is the per-level stat row used to resolve officer-stat scaling (see
/// [`LcarsScaling::officer_stat`]). Pass `None` when stats are unknown — the rank coefficient
/// passes through unchanged and the scaling clause is preserved on
/// [`ValueSpec::officer_stat_scaling`] for traceability.
pub fn lcars_effect_to_combat_effect_spec(
    effect: &LcarsEffect,
    stable_id: &str,
    officer_id: &str,
    ability_name: &str,
    officer_tier: Option<u8>,
    officer_stats: Option<&LcarsLevelStats>,
) -> Option<CombatEffectSpec> {
    lcars_effect_to_combat_effect_spec_with_report(
        effect,
        stable_id,
        officer_id,
        ability_name,
        officer_tier,
        officer_stats,
        0,
        None,
    )
}

/// Same as [`lcars_effect_to_combat_effect_spec`] but pushes a [`DroppedLcarsEffect`] record
/// into `drop_report` at every silent early-return. Designed for offline tooling
/// (`validate_data --coverage`, the officer scorecard) — must never be invoked from the
/// combat hot loop.
///
/// `effect_index` is the position of `effect` within its parent ability's `effects` array;
/// callers (e.g. a scorecard walker) supply it for diagnostic provenance.
#[allow(clippy::too_many_arguments)]
pub fn lcars_effect_to_combat_effect_spec_with_report(
    effect: &LcarsEffect,
    stable_id: &str,
    officer_id: &str,
    ability_name: &str,
    officer_tier: Option<u8>,
    officer_stats: Option<&LcarsLevelStats>,
    effect_index: usize,
    mut drop_report: Option<&mut LcarsDropReport>,
) -> Option<CombatEffectSpec> {
    if effect.effect_type == "extra_attack" {
        maybe_record_drop(
            &mut drop_report,
            officer_id,
            ability_name,
            effect_index,
            "extra_attack_unsupported",
        );
        return None;
    }

    // If this is a tag effect, try to map it to an engine stat. Unmapped combat tags
    // (and all non-combat tags) remain unsupported here.
    let tag_mapped_stat: Option<&'static str> = if effect.effect_type == "tag" {
        let tag_str = effect.tag.as_deref().unwrap_or("");
        combat_tag_to_stat(tag_str)
    } else {
        None
    };
    if effect.effect_type == "tag" && tag_mapped_stat.is_none() {
        let tag_str = effect.tag.as_deref().unwrap_or("");
        // `:non_combat` tags are explicitly documented as non-combat in the YAML —
        // returning None is intentional, so we don't record them as drops.
        if !tag_str.to_ascii_lowercase().contains(":non_combat") {
            let tag_base = tag_str.split(':').next().unwrap_or("").trim();
            maybe_record_drop(
                &mut drop_report,
                officer_id,
                ability_name,
                effect_index,
                format!("unmapped_tag:{tag_base}"),
            );
        }
        return None;
    }

    // Resolve the effective stat name: for stat_modify it comes from effect.stat; for
    // mapped tags it comes from the tag→stat table.
    let effective_stat: &str = if let Some(s) = tag_mapped_stat {
        s
    } else {
        effect.stat.as_deref().unwrap_or("").trim()
    };

    // Passive + permanent stat_modify and mapped tag effects now emit CombatBegin-timed specs
    // (routed through the canonical CombatEffectSpec IR in resolve_crew_to_buff_set).

    let timing = match effect_trigger_timing(effect) {
        Some(t) => t,
        None => {
            let raw = effect.trigger.as_deref().unwrap_or("").trim();
            maybe_record_drop(
                &mut drop_report,
                officer_id,
                ability_name,
                effect_index,
                format!("unknown_trigger:{raw}"),
            );
            return None;
        }
    };

    // Accuracy at combat-begin is folded into static buffs; skip dynamic spec.
    // Non-combat-begin accuracy (e.g. round-start) should produce a dynamic spec.
    // Not recorded as a drop: handled via static_buffs in resolve_crew_to_buff_set.
    if effective_stat.eq_ignore_ascii_case("accuracy") && timing == TimingWindow::CombatBegin {
        return None;
    }

    let trigger = timing_window_to_trigger_spec(timing);
    let target = officer_target_from_effect(effect);
    let conditions = match try_officer_conditions_from_effect(effect) {
        Some(c) => c,
        None => {
            let cond_type = effect
                .condition
                .as_ref()
                .map(|c| c.condition_type.trim())
                .unwrap_or("");
            maybe_record_drop(
                &mut drop_report,
                officer_id,
                ability_name,
                effect_index,
                format!("unmapped_condition:{cond_type}"),
            );
            return None;
        }
    };
    let duration = effect.duration.as_ref().and_then(lcars_duration_to_spec);

    let op_norm = normalize_operator(effect.operator.as_deref());
    let mut attributes = serde_json::Map::new();
    attributes.insert(
        OFFICER_SPEC_ATTR_LCARS_OP.into(),
        serde_json::Value::String(op_norm.clone()),
    );
    let operation = op_to_spec(op_norm.as_str());

    // stat_modify and mapped-tag effects share the same spec-building logic.
    if effect.effect_type == "stat_modify" || tag_mapped_stat.is_some() {
        let (resolved_value, scaling_spec) =
            effect_value_with_officer_stat(effect, officer_tier, officer_stats);
        let value = resolved_value?;
        let stat = effective_stat;

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
                        officer_stat_scaling: scaling_spec.clone(),
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
                        officer_stat_scaling: scaling_spec.clone(),
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

        let modifier = match stat_to_officer_modifier(stat) {
            Some(m) => m,
            None => {
                maybe_record_drop(
                    &mut drop_report,
                    officer_id,
                    ability_name,
                    effect_index,
                    format!("unmapped_stat:{stat}"),
                );
                return None;
            }
        };
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
            modifier,
            operation,
            value: Some(ValueSpec {
                scalar: Some(value),
                by_rank: None,
                unit: None,
                officer_stat_scaling: scaling_spec,
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

    match effect.effect_type.as_str() {
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
        other => {
            maybe_record_drop(
                &mut drop_report,
                officer_id,
                ability_name,
                effect_index,
                format!("unknown_effect_type:{}", other.trim()),
            );
            None
        }
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
        let spec =
            lcars_effect_to_combat_effect_spec(&e, "test:id", "gorkon", "cm", None, None).unwrap();
        assert_eq!(spec.modifier, AbilityModifierSpec::WeaponDamage);
        assert_eq!(spec.trigger, AbilityTriggerSpec::AttackPhase);
    }

    #[test]
    fn lcars_max_fraction_regen_stats_map_to_distinct_specs() {
        let e = LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some("shield_regen_max_fraction".into()),
            target: None,
            operator: Some("add".into()),
            value: Some(0.12),
            trigger: Some("on_round_start".into()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let spec =
            lcars_effect_to_combat_effect_spec(&e, "test:id", "seska", "ba", None, None).unwrap();
        assert_eq!(
            spec.modifier,
            AbilityModifierSpec::OfficerShieldRegenMaxFraction
        );
        assert_eq!(spec.trigger, AbilityTriggerSpec::RoundStart);

        let mut e = e;
        e.stat = Some("hull_hp_repair_max_fraction".into());
        let spec = lcars_effect_to_combat_effect_spec(&e, "test:id", "pic-hugh", "bd", None, None)
            .unwrap();
        assert_eq!(
            spec.modifier,
            AbilityModifierSpec::OfficerHullRegenMaxFraction
        );
    }

    #[test]
    fn passive_permanent_emits_combat_begin_spec() {
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
        let spec = lcars_effect_to_combat_effect_spec(&e, "x", "o", "a", None, None)
            .expect("passive+permanent should now emit a CombatBegin spec");
        assert_eq!(spec.trigger, AbilityTriggerSpec::CombatBegin);
        assert_eq!(spec.modifier, AbilityModifierSpec::WeaponDamage);
        assert_eq!(spec.operation, AbilityOperationSpec::Add);
        let v = spec.value.as_ref().and_then(|v| v.scalar).unwrap();
        assert!((v - 0.1).abs() < 1e-12);
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
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
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

    #[test]
    fn lcars_effect_with_unmapped_condition_yields_no_spec_row() {
        let bad_c = LcarsCondition {
            condition_type: "totally_unknown_condition_xyz".into(),
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
        let e_bad = LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some("weapon_damage".into()),
            target: None,
            operator: Some("add".into()),
            value: Some(0.1),
            trigger: Some("on_attack".into()),
            duration: None,
            scaling: None,
            condition: Some(bad_c),
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        assert!(
            lcars_effect_to_combat_effect_spec(&e_bad, "x", "o", "a", None, None).is_none(),
            "must not emit spec without encoding the YAML condition"
        );
        let good_c = LcarsCondition {
            condition_type: "defender_burning".into(),
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
        let e_ok = LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some("weapon_damage".into()),
            target: None,
            operator: Some("add".into()),
            value: Some(0.1),
            trigger: Some("on_attack".into()),
            duration: None,
            scaling: None,
            condition: Some(good_c),
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        assert!(lcars_effect_to_combat_effect_spec(&e_ok, "x", "o", "a", None, None).is_some());
    }

    fn officer_stat_scaling_effect(
        stat: &str,
        coefficient_per_rank: Vec<f64>,
        officer_stat: crate::data::combat_effect_spec::OfficerStat,
    ) -> LcarsEffect {
        LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some(stat.into()),
            target: None,
            operator: Some("add".into()),
            value: None,
            trigger: Some("on_combat_start".into()),
            duration: None,
            scaling: Some(crate::lcars::parser::LcarsScaling {
                base: None,
                per_rank: None,
                max_rank: Some(coefficient_per_rank.len() as u8),
                base_chance: None,
                values: Some(coefficient_per_rank),
                chance_values: None,
                officer_stat: Some(officer_stat),
            }),
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        }
    }

    #[test]
    fn officer_stat_scaling_resolves_coefficient_times_stat_over_100() {
        // Mbenga-style: armor += <coeff>% of officer.health. At rank 3, coefficient = 25%.
        let e = officer_stat_scaling_effect(
            "armor",
            vec![15.0, 15.0, 25.0],
            crate::data::combat_effect_spec::OfficerStat::Health,
        );
        let stats = LcarsLevelStats {
            level: 30,
            attack: 800.0,
            defense: 400.0,
            health: 400.0,
        };
        let spec = lcars_effect_to_combat_effect_spec(&e, "tid", "o", "ab", Some(3), Some(&stats))
            .expect("spec");
        let v = spec.value.as_ref().and_then(|v| v.scalar).expect("scalar");
        // 25 * 400 / 100 = 100.
        assert!((v - 100.0).abs() < 1e-9, "got {v}");
        let scaling = spec
            .value
            .as_ref()
            .and_then(|v| v.officer_stat_scaling.as_ref())
            .expect("officer_stat_scaling preserved");
        assert_eq!(scaling.coefficient_per_rank, vec![15.0, 15.0, 25.0]);
        assert_eq!(
            scaling.stat,
            crate::data::combat_effect_spec::OfficerStat::Health
        );
    }

    #[test]
    fn officer_stat_scaling_clamps_rank_beyond_table() {
        // Tier 7 with a 3-entry table → use the highest entry.
        let e = officer_stat_scaling_effect(
            "armor",
            vec![10.0, 20.0, 30.0],
            crate::data::combat_effect_spec::OfficerStat::Defense,
        );
        let stats = LcarsLevelStats {
            level: 1,
            attack: 0.0,
            defense: 200.0,
            health: 0.0,
        };
        let spec = lcars_effect_to_combat_effect_spec(&e, "tid", "o", "ab", Some(7), Some(&stats))
            .expect("spec");
        let v = spec.value.as_ref().and_then(|v| v.scalar).expect("scalar");
        assert!((v - 60.0).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn officer_stat_scaling_no_op_when_stats_missing() {
        // Without per-level stats, the rank coefficient passes through unchanged. Scaling clause
        // is preserved on ValueSpec for traceability.
        let e = officer_stat_scaling_effect(
            "armor",
            vec![15.0, 15.0, 25.0],
            crate::data::combat_effect_spec::OfficerStat::Health,
        );
        let spec =
            lcars_effect_to_combat_effect_spec(&e, "tid", "o", "ab", Some(3), None).expect("spec");
        let v = spec.value.as_ref().and_then(|v| v.scalar).expect("scalar");
        assert!(
            (v - 25.0).abs() < 1e-12,
            "expected raw rank coefficient, got {v}"
        );
        assert!(spec
            .value
            .as_ref()
            .and_then(|v| v.officer_stat_scaling.as_ref())
            .is_some());
    }

    #[test]
    fn officer_stat_scaling_falls_through_when_clause_absent() {
        // No officer_stat clause → unchanged behavior; no scaling spec on ValueSpec.
        let e = LcarsEffect {
            effect_type: "stat_modify".into(),
            stat: Some("armor".into()),
            target: None,
            operator: Some("add".into()),
            value: Some(0.04),
            trigger: Some("on_combat_start".into()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let stats = LcarsLevelStats {
            level: 30,
            attack: 0.0,
            defense: 0.0,
            health: 999.0,
        };
        let spec = lcars_effect_to_combat_effect_spec(&e, "tid", "o", "ab", Some(3), Some(&stats))
            .expect("spec");
        let v = spec.value.as_ref().and_then(|v| v.scalar).expect("scalar");
        assert!((v - 0.04).abs() < 1e-12);
        assert!(spec
            .value
            .as_ref()
            .and_then(|v| v.officer_stat_scaling.as_ref())
            .is_none());
    }

    #[test]
    fn combat_tag_to_stat_maps_expected_tags_and_returns_none_for_others() {
        assert_eq!(
            combat_tag_to_stat("shieldmitigation:unmapped"),
            Some("shield_mitigation")
        );
        assert_eq!(
            combat_tag_to_stat("shieldmitigation"),
            Some("shield_mitigation")
        );
        assert_eq!(combat_tag_to_stat("shipdodge:unmapped"), Some("dodge"));
        assert_eq!(
            combat_tag_to_stat("shieldpiercing:unmapped"),
            Some("pierce")
        );
        assert_eq!(combat_tag_to_stat("accuracy:unmapped"), Some("accuracy"));
        assert_eq!(combat_tag_to_stat("shields:unmapped"), Some("shield_hp"));
        // Economy / unmapped tags
        assert_eq!(combat_tag_to_stat("cargoprotection:unmapped"), None);
        assert_eq!(combat_tag_to_stat("impulsespeed:unmapped"), None);
        assert_eq!(combat_tag_to_stat("officerstathealth:unmapped"), None);
        assert_eq!(combat_tag_to_stat("miningrate:non_combat"), None);
    }

    #[test]
    fn lcars_mapped_tag_produces_spec_like_stat_modify() {
        let e = LcarsEffect {
            effect_type: "tag".into(),
            stat: None,
            target: None,
            operator: Some("add".into()),
            value: Some(0.06),
            trigger: Some("on_round_start".into()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: Some("shieldmitigation:unmapped".into()),
            accumulate: None,
            decay: None,
        };
        let spec = lcars_effect_to_combat_effect_spec(&e, "test:tag:sm", "o", "ab", None, None)
            .expect("should produce spec for shieldmitigation tag");
        assert_eq!(spec.modifier, AbilityModifierSpec::ShieldMitigation);
        assert_eq!(spec.trigger, AbilityTriggerSpec::RoundStart);
        let v = spec.value.as_ref().and_then(|v| v.scalar).unwrap();
        assert!((v - 0.06).abs() < 1e-12);
    }

    #[test]
    fn unmapped_combat_tag_still_returns_none() {
        let e = LcarsEffect {
            effect_type: "tag".into(),
            stat: None,
            target: None,
            operator: Some("add".into()),
            value: Some(0.1),
            trigger: Some("on_round_start".into()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: Some("allreloadspeed:unmapped".into()),
            accumulate: None,
            decay: None,
        };
        assert!(lcars_effect_to_combat_effect_spec(&e, "x", "o", "a", None, None).is_none());
    }

    #[test]
    fn mapped_tag_passive_permanent_emits_combat_begin_spec() {
        let e = LcarsEffect {
            effect_type: "tag".into(),
            stat: None,
            target: None,
            operator: Some("add".into()),
            value: Some(0.2),
            trigger: Some("passive".into()),
            duration: Some(LcarsDuration::Permanent("permanent".into())),
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: Some("shieldmitigation:unmapped".into()),
            accumulate: None,
            decay: None,
        };
        let spec = lcars_effect_to_combat_effect_spec(&e, "x", "o", "a", None, None)
            .expect("passive+permanent mapped tag should now emit a CombatBegin spec");
        assert_eq!(spec.trigger, AbilityTriggerSpec::CombatBegin);
        assert_eq!(spec.modifier, AbilityModifierSpec::ShieldMitigation);
        let v = spec.value.as_ref().and_then(|v| v.scalar).unwrap();
        assert!((v - 0.2).abs() < 1e-12);
    }
}
