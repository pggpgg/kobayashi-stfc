//! Resolves parsed LCARS abilities into a [BuffSet] (static buffs + crew config for the engine).

use std::collections::{HashMap, HashSet};

use crate::combat::types::enemy_type_from_engagement_slug;
use crate::combat::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, OpponentFactionTag, ShipType, TimingWindow,
};
use crate::data::profile;
use crate::lcars::parser::{LcarsAbility, LcarsCondition, LcarsEffect, LcarsOfficer};
use serde::Serialize;

/// Options when resolving officer abilities (e.g. officer tier for scaling).
#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    /// Fallback officer tier (1-based) when per-officer tier is not set.
    pub tier: Option<u8>,
    /// Per-officer tier (canonical_officer_id → tier). When set, each officer uses their tier for
    /// scaling via [crate::lcars::parser::LcarsScaling::value_at_rank] / `chance_at_rank` (discrete
    /// `values` / `chance_values` when present, else `base` + `per_rank`).
    pub officer_tiers: Option<HashMap<String, u8>>,
    /// Per-officer level override for officer-stat scaling lookups (canonical_officer_id → level).
    /// When unset, [`crate::lcars::parser::LcarsOfficer::resolve_level`] picks the rank's max level.
    pub officer_levels: Option<HashMap<String, u32>>,
}

impl ResolveOptions {
    /// Tier to use for the given officer: per-officer tier if available, else fallback [ResolveOptions::tier].
    pub fn tier_for(&self, officer_id: &str) -> Option<u8> {
        self.officer_tiers
            .as_ref()
            .and_then(|m| m.get(officer_id).copied())
            .or(self.tier)
    }

    /// Per-officer level override for stat-scaling lookups; [`None`] when not specified (resolver
    /// falls back to the officer's max level for the resolved rank).
    pub fn level_for(&self, officer_id: &str) -> Option<u32> {
        self.officer_levels
            .as_ref()
            .and_then(|m| m.get(officer_id).copied())
    }
}

/// Resolved set of buffs: static modifiers (applied once) and dynamic crew config (per-round/triggered).
/// Per docs/DESIGN.md: "LCARS definitions are collapsed into a BuffSet" before combat.
#[derive(Debug, Clone, Default)]
pub struct BuffSet {
    /// Stat modifiers applied once at combat start (e.g. passive permanent stat_modify).
    /// Keys are engine stat names (weapon_damage, shield_pierce, etc.); values are the resolved delta.
    pub static_buffs: HashMap<String, f64>,
    /// Per-round and triggered effects: the crew configuration the engine evaluates each round.
    pub crew: CrewConfiguration,
    /// Extra attack proc chance (0.0–1.0). When set, engine rolls per attack; on success, damage × proc_multiplier.
    pub proc_chance: f64,
    /// Extra attack proc multiplier (e.g. 2.0 for double shot). Applied when proc triggers.
    pub proc_multiplier: f64,
}

impl BuffSet {
    /// Convert this BuffSet into the crew configuration for the existing combat API.
    /// Static buffs are intended to be applied to ship/attacker stats before simulation;
    /// callers can do that in a follow-up. This returns the dynamic part.
    pub fn to_crew_config(&self) -> &CrewConfiguration {
        &self.crew
    }

    /// Apply this BuffSet's static_buffs to a Combatant (isolytic_damage, isolytic_defense, shield_mitigation).
    /// Call this when building a Combatant from ship/hostile + crew resolved via [resolve_crew_to_buff_set].
    pub fn apply_static_buffs_to_combatant(&self, combatant: Combatant) -> Combatant {
        profile::apply_static_buffs_to_combatant(combatant, &self.static_buffs)
    }
}

/// Resolve an LCARS condition tree into an engine [`AbilityCondition`].
///
/// Used for LCARS validation and parity tests; officer ability runtime conditions on dynamic
/// effects come from [`crate::combat::effect_spec_compile::compile_officer_combat_spec`]
/// (spec IR) after [`lcars_effect_to_combat_effect_spec`].
/// Returns [`Err`] with a short message when the YAML is unsupported or incomplete (unknown `type`,
/// missing `ship_type` / `faction`, unknown slugs, empty `and`/`or`, or invalid child conditions).
pub fn resolve_lcars_condition(c: &LcarsCondition) -> Result<AbilityCondition, String> {
    let ty = c.condition_type.trim().to_lowercase().replace('-', "_");
    match ty.as_str() {
        "stat_below" => Ok(AbilityCondition::StatBelow {
            stat: c.stat.clone().unwrap_or_else(|| "hull_hp".to_string()),
            threshold_pct: c.threshold_pct.unwrap_or(0.5),
        }),
        "stat_above" => Ok(AbilityCondition::StatAbove {
            stat: c.stat.clone().unwrap_or_else(|| "hull_hp".to_string()),
            threshold_pct: c.threshold_pct.unwrap_or(0.8),
        }),
        "round_range" => Ok(AbilityCondition::RoundRange {
            min: c.min.unwrap_or(1),
            max: c.max.unwrap_or(100),
        }),
        "morale_active" | "attacker_morale" | "morale" => Ok(AbilityCondition::MoraleActive),
        "defender_is_npc_hostile" | "defender_npc_hostile" | "enemy_hostile" => {
            Ok(AbilityCondition::DefenderIsNpcHostile)
        }
        "defender_is_player_ship" | "defender_player_ship" | "enemy_player" => {
            Ok(AbilityCondition::DefenderIsPlayerShip)
        }
        "defender_burning" | "target_burning" | "burning" => Ok(AbilityCondition::DefenderBurning),
        "defender_hull_breach" | "target_hull_breach" | "hull_breach_active" => {
            Ok(AbilityCondition::DefenderHullBreach)
        }
        "attacker_burning" | "self_burning" | "player_burning" => {
            Ok(AbilityCondition::AttackerBurning)
        }
        "attacker_hull_breach" | "self_hull_breach" | "player_hull_breach" => {
            Ok(AbilityCondition::AttackerHullBreach)
        }
        "defender_assimilated" | "target_assimilated" => Ok(AbilityCondition::DefenderAssimilated),
        "attacker_officer_tal_not_on_bridge" | "self_officer_tal_not_on_bridge" => {
            Ok(AbilityCondition::AttackerOfficerTalNotOnBridge)
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
            let tag = OpponentFactionTag::from_data_slug(slug).ok_or_else(|| {
                format!("unknown faction slug '{slug}' for condition type '{ty}'")
            })?;
            Ok(AbilityCondition::DefenderFactionIs(tag))
        }
        "defender_hull_faction_id" | "enemy_hull_faction" | "enemy_hull_faction_id" => {
            let id = c.faction_id.ok_or_else(|| {
                format!(
                    "{ty} condition requires integer `faction_id` (upstream hostile faction.id)"
                )
            })?;
            Ok(AbilityCondition::DefenderHullFactionIdIs(id))
        }
        "not" => {
            let children = c
                .conditions
                .as_ref()
                .ok_or_else(|| "`not` condition requires a `conditions` array".to_string())?;
            if children.len() != 1 {
                return Err("`not` condition must include exactly one sub-condition".to_string());
            }
            Ok(AbilityCondition::Not(Box::new(resolve_lcars_condition(
                &children[0],
            )?)))
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
            let st = ShipType::from_data_slug(slug).ok_or_else(|| {
                format!("unknown ship class slug '{slug}' for condition type '{ty}'")
            })?;
            Ok(AbilityCondition::DefenderShipTypeIs(st))
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
            let st = ShipType::from_data_slug(slug).ok_or_else(|| {
                format!("unknown ship class slug '{slug}' for condition type '{ty}'")
            })?;
            Ok(AbilityCondition::AttackerShipTypeIs(st))
        }
        "attacker_ship_id_is" | "self_ship_id_is" => {
            let id = c
                .ship_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    format!(
                        "{ty} condition requires non-empty `ship_id` (Kobayashi ships_extended id)"
                    )
                })?;
            Ok(AbilityCondition::AttackerShipIdIs(id.to_string()))
        }
        "engagement_includes" | "engagement_has" => {
            let slug = c
                .enemy_type
                .as_deref()
                .or(c.stat.as_deref())
                .or(c.tag.as_deref())
                .ok_or_else(|| {
                    format!("{ty} requires `enemy_type` (snake_case tag, e.g. group_armadas)")
                })?;
            let et = enemy_type_from_engagement_slug(slug).ok_or_else(|| {
                format!("unknown engagement `enemy_type` slug '{slug}' for condition type '{ty}'")
            })?;
            Ok(AbilityCondition::EngagementIncludes(et))
        }
        "combat_battle_type_any" | "combat_battle_type" => {
            let mut values = c
                .battle_types
                .clone()
                .ok_or_else(|| format!("{ty} requires non-empty `battle_types` list"))?;
            values.sort_unstable();
            values.dedup();
            if values.is_empty() {
                return Err(format!("{ty} requires non-empty `battle_types` list"));
            }
            Ok(AbilityCondition::CombatBattleTypeAny(values))
        }
        "defender_level_at_most" | "target_max_level" => {
            let max_level = c
                .max
                .ok_or_else(|| format!("{ty} requires integer `max` level"))?;
            Ok(AbilityCondition::DefenderLevelAtMost(max_level))
        }
        "literal_true" => Ok(AbilityCondition::LiteralBool(true)),
        "literal_false" => Ok(AbilityCondition::LiteralBool(false)),
        "and" => {
            let children = c.conditions.as_ref().ok_or_else(|| {
                "`and` condition requires non-empty `conditions` array".to_string()
            })?;
            if children.is_empty() {
                return Err("`and` condition must include at least one sub-condition".to_string());
            }
            let mut conds = Vec::with_capacity(children.len());
            for child in children {
                conds.push(resolve_lcars_condition(child)?);
            }
            Ok(AbilityCondition::And(conds))
        }
        "or" => {
            let children = c.conditions.as_ref().ok_or_else(|| {
                "`or` condition requires non-empty `conditions` array".to_string()
            })?;
            if children.is_empty() {
                return Err("`or` condition must include at least one sub-condition".to_string());
            }
            let mut conds = Vec::with_capacity(children.len());
            for child in children {
                conds.push(resolve_lcars_condition(child)?);
            }
            Ok(AbilityCondition::Or(conds))
        }
        _ => Err(format!(
            "unknown LCARS condition type '{}'",
            c.condition_type.trim()
        )),
    }
}

fn normalize_trigger(trigger: &str) -> String {
    trigger.trim().to_ascii_lowercase().replace('-', "_")
}

fn normalize_operator(op: Option<&str>) -> String {
    op.unwrap_or("add")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
}

/// Map LCARS `trigger` (+ `target` for legacy `on_shield_break`) to engine timing. Unknown → None.
///
/// Shield semantics: **enemy** shields down → [`TimingWindow::ShieldBreak`]; **your** shields down
/// → [`TimingWindow::SelfShieldBreak`]. Prefer explicit `on_enemy_shield_break` / `on_own_shield_break`;
/// legacy `on_shield_break` uses `target: enemy` vs `target: self` (default `self` if omitted).
pub(crate) fn effect_trigger_timing(effect: &LcarsEffect) -> Option<TimingWindow> {
    let t = effect.trigger.as_deref().map(normalize_trigger)?;
    match t.as_str() {
        "on_own_shield_break" | "self_shields_depleted" | "own_shields_depleted" => {
            Some(TimingWindow::SelfShieldBreak)
        }
        "on_enemy_shield_break"
        | "enemy_shields_depleted"
        | "target_shields_depleted"
        | "targetshieldsdepleted" => Some(TimingWindow::ShieldBreak),
        "shieldsdepleted" | "on_shield_break" => {
            let who = effect
                .target
                .as_deref()
                .map(|s| s.trim().to_ascii_lowercase())
                .unwrap_or_else(|| "self".to_string());
            if who == "enemy" {
                Some(TimingWindow::ShieldBreak)
            } else {
                Some(TimingWindow::SelfShieldBreak)
            }
        }
        "passive" => Some(TimingWindow::CombatBegin),
        "combatstart" | "on_combat_start" => Some(TimingWindow::CombatBegin),
        "roundstart" | "on_round_start" => Some(TimingWindow::RoundStart),
        "criticalshotfired" | "enemytakeshit" | "on_attack" | "on_hit" | "on_critical" => {
            Some(TimingWindow::AttackPhase)
        }
        "after_shot" | "on_after_shot" | "subround_end" | "on_subround_end" | "after_weapon"
        | "on_after_weapon" => Some(TimingWindow::AfterSubround),
        "hittaken" | "on_defense" => Some(TimingWindow::DefensePhase),
        "roundend" | "on_round_end" => Some(TimingWindow::RoundEnd),
        "battlewon" | "on_kill" => Some(TimingWindow::Kill),
        "hulldamagetaken" | "on_hull_breach" => Some(TimingWindow::HullBreach),
        "shielddamagetaken" | "on_receive_damage" => Some(TimingWindow::ReceiveDamage),
        "on_combat_end" => Some(TimingWindow::CombatEnd),
        _ => None,
    }
}

/// True if this effect is passive and permanent (should go only into static_buffs, not crew).
fn is_static_effect(effect: &LcarsEffect) -> bool {
    let passive = effect.trigger.as_deref().map(str::trim) == Some("passive");
    let permanent = effect
        .duration
        .as_ref()
        .map(|d| d.is_permanent())
        .unwrap_or(false);
    if !passive || !permanent {
        return false;
    }
    if effect.effect_type == "stat_modify" {
        return true;
    }
    if effect.effect_type == "tag" {
        let tag_str = effect.tag.as_deref().unwrap_or("");
        return crate::lcars::combat_tag_to_stat(tag_str).is_some();
    }
    false
}

/// Resolve a single LCARS effect into timing, effect body, and optional AND-combined condition
/// from the CombatEffectSpec compile path when supported.
///
/// `officer` (when provided) supplies the per-level stat row used to resolve
/// [`crate::lcars::parser::LcarsScaling::officer_stat`] scaling. When `None`, officer-stat scaling
/// passes through unchanged (the rank coefficient is used as a flat value).
///
/// Implementation: [`crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec`] →
/// [`crate::combat::effect_spec_compile::compile_officer_combat_spec`].
fn resolve_effect(
    effect: &LcarsEffect,
    ability_name: &str,
    options: &ResolveOptions,
    officer_id: &str,
    officer: Option<&LcarsOfficer>,
    effect_index: usize,
) -> Option<(TimingWindow, AbilityEffect, Option<AbilityCondition>)> {
    if is_static_effect(effect) {
        return None;
    }
    let tier = options.tier_for(officer_id);
    let stats_row = officer.and_then(|o| {
        let level = o.resolve_level(options.level_for(officer_id), tier)?;
        o.stats_at_level(level)
    });
    let stable_id = format!("lcars:{officer_id}:{ability_name}:{effect_index}");
    let spec = crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec(
        effect,
        &stable_id,
        officer_id,
        ability_name,
        tier,
        stats_row,
    )?;
    crate::combat::effect_spec_compile::compile_officer_combat_spec(&spec).ok()
}

/// Coarse coverage tier for `/api/mechanics/coverage` (LCARS effects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MechanicCoverageTier {
    Implemented,
    Partial,
    Ignored,
}

/// How a single LCARS effect is treated when resolving a crew (static buff, dynamic seat, proc, or skipped).
#[derive(Debug, Clone, Serialize)]
pub struct LcarsEffectCoverage {
    pub tier: MechanicCoverageTier,
    /// Short machine-readable reason / pathway.
    pub pathway: String,
}

/// Classify one LCARS effect for mechanics coverage reports. Uses the same rules as [resolve_effect] and static/proc paths in [resolve_crew_to_buff_set].
pub fn lcars_effect_coverage(
    effect: &LcarsEffect,
    officer_id: &str,
    options: &ResolveOptions,
) -> LcarsEffectCoverage {
    if effect.effect_type == "tag" {
        // Mapped combat tags → treat like stat_modify for coverage reporting.
        let tag_str = effect.tag.as_deref().unwrap_or("");
        if crate::lcars::combat_tag_to_stat(tag_str).is_some() {
            // Fall through to the same checks as stat_modify below.
        } else {
            return LcarsEffectCoverage {
                tier: MechanicCoverageTier::Ignored,
                pathway: format!("tag_unmapped:{tag_str}"),
            };
        }
    }
    if effect.effect_type == "extra_attack" {
        return LcarsEffectCoverage {
            tier: MechanicCoverageTier::Implemented,
            pathway: "proc_extra_attack".to_string(),
        };
    }
    if is_static_effect(effect) {
        return LcarsEffectCoverage {
            tier: MechanicCoverageTier::Implemented,
            pathway: "static_passive_stat_modify".to_string(),
        };
    }
    // For mapped tags, we also check for static effects (passive + permanent).
    if effect.effect_type == "tag" {
        let passive = effect.trigger.as_deref().map(str::trim) == Some("passive");
        let permanent = effect
            .duration
            .as_ref()
            .map(|d| d.is_permanent())
            .unwrap_or(false);
        if passive && permanent {
            return LcarsEffectCoverage {
                tier: MechanicCoverageTier::Implemented,
                pathway: "static_passive_tag_mapped".to_string(),
            };
        }
    }
    if effect.effect_type == "stat_modify" || effect.effect_type == "tag" {
        let stat = if effect.effect_type == "tag" {
            let tag_str = effect.tag.as_deref().unwrap_or("");
            crate::lcars::combat_tag_to_stat(tag_str).unwrap_or("")
        } else {
            effect.stat.as_deref().unwrap_or("").trim()
        };
        if stat.eq_ignore_ascii_case("accuracy") {
            let pathway = if effect_trigger_timing(effect) == Some(TimingWindow::CombatBegin) {
                "combat_begin_accuracy_static"
            } else {
                "accuracy_non_combat_begin_skipped"
            };
            return LcarsEffectCoverage {
                tier: if pathway.starts_with("combat_begin") {
                    MechanicCoverageTier::Implemented
                } else {
                    MechanicCoverageTier::Partial
                },
                pathway: pathway.to_string(),
            };
        }
    }

    if effect_trigger_timing(effect).is_none() {
        let tr = effect
            .trigger
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("(missing)");
        return LcarsEffectCoverage {
            tier: MechanicCoverageTier::Ignored,
            pathway: format!("unknown_trigger:{tr}"),
        };
    }

    if resolve_effect(effect, "", options, officer_id, None, 0).is_some() {
        return LcarsEffectCoverage {
            tier: MechanicCoverageTier::Implemented,
            pathway: "dynamic_crew_ability".to_string(),
        };
    }

    if effect.effect_type == "stat_modify"
        || (effect.effect_type == "tag"
            && crate::lcars::combat_tag_to_stat(effect.tag.as_deref().unwrap_or("")).is_some())
    {
        return LcarsEffectCoverage {
            tier: MechanicCoverageTier::Partial,
            pathway: "stat_modify_not_modeled_or_timing".to_string(),
        };
    }

    LcarsEffectCoverage {
        tier: MechanicCoverageTier::Ignored,
        pathway: format!("effect_type_not_modeled:{}", effect.effect_type),
    }
}

/// Resolve one officer ability block (captain, bridge, or below decks) into seat contexts.
pub fn resolve_officer_ability(
    officer: &LcarsOfficer,
    ability: &LcarsAbility,
    seat: CrewSeat,
    class: AbilityClass,
    options: &ResolveOptions,
    contribution_batch: u32,
) -> Vec<CrewSeatContext> {
    let mut contexts = Vec::new();
    for (idx, effect) in ability.effects.iter().enumerate() {
        if let Some((timing, effect_effect, condition)) = resolve_effect(
            effect,
            &ability.name,
            options,
            &officer.id,
            Some(officer),
            idx,
        ) {
            contexts.push(CrewSeatContext {
                seat,
                ability: Ability {
                    name: ability.name.clone(),
                    class,
                    timing,
                    boostable: true,
                    effect: effect_effect,
                    condition,
                },
                boosted: false,
                officer_id: Some(officer.id.clone()),
                contribution_batch,
            });
        }
    }
    contexts
}

/// Build a BuffSet for a crew: captain_id, bridge_ids, below_deck_ids.
///
/// Slot rules (aligned with STFC seating):
/// - **Captain:** [LcarsOfficer::captain_ability] and [LcarsOfficer::bridge_ability] both apply; seat is [CrewSeat::Captain].
/// - **Bridge:** only [LcarsOfficer::bridge_ability]; seat is [CrewSeat::Bridge].
/// - **Below decks:** only [LcarsOfficer::below_decks_ability]; seat is [CrewSeat::BelowDeck].
///
/// Officers are looked up from the provided map (id -> LcarsOfficer).
/// Static buffs are accumulated from passive permanent stat_modify effects;
/// all resolved effects that have a timing go into crew.
///
pub fn resolve_crew_to_buff_set(
    captain_id: &str,
    bridge: &[String],
    below_decks: &[String],
    officers: &HashMap<String, LcarsOfficer>,
    options: &ResolveOptions,
) -> BuffSet {
    let mut static_buffs: HashMap<String, f64> = HashMap::new();
    let mut seats: Vec<CrewSeatContext> = Vec::new();
    let mut proc_chance = 0.0_f64;
    let mut proc_multiplier = 1.0_f64;
    let mut next_batch: u32 = 0;
    let mut seen_slots: HashSet<String> = HashSet::new();

    let mut add_ability = |officer: &LcarsOfficer,
                           ability: &LcarsAbility,
                           seat: CrewSeat,
                           class: AbilityClass,
                           contribution_batch: u32| {
        let officer_tier = options.tier_for(&officer.id);
        for effect in &ability.effects {
            // Determine the effective stat for passive permanent effects.
            // stat_modify reads from effect.stat; mapped tag effects use the tag→stat table.
            let effective_stat = if effect.effect_type == "stat_modify" {
                effect.stat.as_deref().filter(|s| !s.trim().is_empty())
            } else if effect.effect_type == "tag" {
                crate::lcars::combat_tag_to_stat(effect.tag.as_deref().unwrap_or(""))
            } else {
                None
            };
            if effective_stat.is_none()
                || effect.trigger.as_deref().map(str::trim) != Some("passive")
                || effect.duration.as_ref().is_some_and(|d| !d.is_permanent())
            {
                continue;
            }
            let stat = effective_stat.unwrap();
            let value = effect.value.or_else(|| {
                effect
                    .scaling
                    .as_ref()
                    .map(|s| s.value_at_rank(officer_tier))
            });
            if let Some(v) = value {
                if stat.eq_ignore_ascii_case("accuracy") {
                    // Folded into `accuracy` / `accuracy_cb_mult` in the combat-begin accuracy loop below.
                    continue;
                }
                if effect.operator.as_deref() == Some("multiply") {
                    static_buffs
                        .entry(stat.to_string())
                        .and_modify(|x| *x *= v)
                        .or_insert(v);
                } else {
                    static_buffs
                        .entry(stat.to_string())
                        .and_modify(|x| *x += v)
                        .or_insert(v);
                }
            }
        }
        // Combat-begin `stat_modify` / mapped-tag accuracy: stacks into pre-mitigation attacker
        // stats (scenario), not a crew seat. Multiplicative entries use key `accuracy_cb_mult`.
        for effect in &ability.effects {
            let is_accuracy = if effect.effect_type == "stat_modify" {
                effect
                    .stat
                    .as_deref()
                    .map(|s| s.trim())
                    .is_some_and(|s: &str| s.eq_ignore_ascii_case("accuracy"))
            } else if effect.effect_type == "tag" {
                crate::lcars::combat_tag_to_stat(effect.tag.as_deref().unwrap_or(""))
                    .is_some_and(|s: &str| s.eq_ignore_ascii_case("accuracy"))
            } else {
                false
            };
            if !is_accuracy {
                continue;
            }
            if effect_trigger_timing(effect) != Some(TimingWindow::CombatBegin) {
                continue;
            }
            let Some(value) = effect.value.or_else(|| {
                effect
                    .scaling
                    .as_ref()
                    .map(|s| s.value_at_rank(officer_tier))
            }) else {
                continue;
            };
            let op = normalize_operator(effect.operator.as_deref());
            if matches!(
                op.as_str(),
                "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add" | "multiplybaseadd"
            ) {
                static_buffs
                    .entry("accuracy_cb_mult".to_string())
                    .and_modify(|x| *x *= value)
                    .or_insert(value);
            } else {
                static_buffs
                    .entry("accuracy".to_string())
                    .and_modify(|x| *x += value)
                    .or_insert(value);
            }
        }
        let contexts =
            resolve_officer_ability(officer, ability, seat, class, options, contribution_batch);
        seats.extend(contexts);
    };

    if let Some(o) = officers.get(captain_id) {
        seen_slots.insert(captain_id.to_string());
        if let Some(ref a) = o.captain_ability {
            let b = next_batch;
            next_batch = next_batch.saturating_add(1);
            add_ability(o, a, CrewSeat::Captain, AbilityClass::CaptainManeuver, b);
        }
        if let Some(ref a) = o.bridge_ability {
            let b = next_batch;
            next_batch = next_batch.saturating_add(1);
            add_ability(o, a, CrewSeat::Captain, AbilityClass::BridgeAbility, b);
        }
    }

    for id in bridge {
        let Some(o) = officers.get(id.as_str()) else {
            continue;
        };
        if seen_slots.contains(id.as_str()) {
            continue;
        }
        seen_slots.insert(id.clone());
        if let Some(ref a) = o.bridge_ability {
            let b = next_batch;
            next_batch = next_batch.saturating_add(1);
            add_ability(o, a, CrewSeat::Bridge, AbilityClass::BridgeAbility, b);
        }
    }

    for id in below_decks {
        let Some(o) = officers.get(id.as_str()) else {
            continue;
        };
        if seen_slots.contains(id.as_str()) {
            continue;
        }
        seen_slots.insert(id.clone());
        if let Some(ref a) = o.below_decks_ability {
            let b = next_batch;
            next_batch = next_batch.saturating_add(1);
            add_ability(o, a, CrewSeat::BelowDeck, AbilityClass::BelowDeck, b);
        }
    }

    let mut accumulate_proc = |officer: &LcarsOfficer, ability: &LcarsAbility| {
        let officer_tier = options.tier_for(&officer.id);
        for effect in &ability.effects {
            if effect.effect_type == "extra_attack" {
                let chance = effect
                    .chance
                    .or_else(|| {
                        effect
                            .scaling
                            .as_ref()
                            .map(|s| s.chance_at_rank(officer_tier))
                    })
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let mult = effect.multiplier.unwrap_or(2.0).max(1.0);
                if chance > proc_chance || (chance == proc_chance && mult > proc_multiplier) {
                    proc_chance = chance;
                    proc_multiplier = mult;
                }
            }
        }
    };

    let mut seen_proc: HashSet<String> = HashSet::new();
    if let Some(o) = officers.get(captain_id) {
        seen_proc.insert(captain_id.to_string());
        if let Some(ref a) = o.captain_ability {
            accumulate_proc(o, a);
        }
        if let Some(ref a) = o.bridge_ability {
            accumulate_proc(o, a);
        }
    }
    for id in bridge {
        let Some(o) = officers.get(id.as_str()) else {
            continue;
        };
        if seen_proc.contains(id.as_str()) {
            continue;
        }
        seen_proc.insert(id.clone());
        if let Some(ref a) = o.bridge_ability {
            accumulate_proc(o, a);
        }
    }
    for id in below_decks {
        let Some(o) = officers.get(id.as_str()) else {
            continue;
        };
        if seen_proc.contains(id.as_str()) {
            continue;
        }
        seen_proc.insert(id.clone());
        if let Some(ref a) = o.below_decks_ability {
            accumulate_proc(o, a);
        }
    }

    BuffSet {
        static_buffs,
        crew: CrewConfiguration { seats },
        proc_chance,
        proc_multiplier,
    }
}

/// Build a map of officer id -> LcarsOfficer from a list (e.g. from load_lcars_dir).
pub fn index_lcars_officers_by_id(officers: Vec<LcarsOfficer>) -> HashMap<String, LcarsOfficer> {
    officers.into_iter().map(|o| (o.id.clone(), o)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::CombatContext;
    use crate::combat::{
        AbilityClass, AbilityCondition, AbilityEffect, OpponentFactionTag, ShipType, TimingWindow,
    };
    use crate::lcars::parser::{
        load_lcars_file, LcarsAbility, LcarsCondition, LcarsDuration, LcarsEffect, LcarsOfficer,
        LcarsScaling,
    };
    use std::path::Path;

    fn lcars_effect_stat_modify(stat: &str, value: f64, trigger: &str) -> LcarsEffect {
        LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some(stat.to_string()),
            target: None,
            operator: Some("add".to_string()),
            value: Some(value),
            trigger: Some(trigger.to_string()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        }
    }

    fn lcars_condition(ty: &str) -> LcarsCondition {
        LcarsCondition {
            condition_type: ty.to_string(),
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
        }
    }

    fn lcars_effect_stat_modify_with_condition(
        stat: &str,
        value: f64,
        trigger: &str,
        condition: LcarsCondition,
    ) -> LcarsEffect {
        LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some(stat.to_string()),
            target: None,
            operator: Some("add".to_string()),
            value: Some(value),
            trigger: Some(trigger.to_string()),
            duration: None,
            scaling: None,
            condition: Some(condition),
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        }
    }

    #[test]
    fn resolve_lcars_condition_maps_morale_burning_hull_breach_and_faction() {
        let officer = LcarsOfficer {
            id: "cond_officer".to_string(),
            name: "Cond".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let mut fc = lcars_condition("defender_faction_is");
        fc.faction = Some("klingon".to_string());
        let compound = LcarsCondition {
            condition_type: "and".to_string(),
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
            conditions: Some(vec![lcars_condition("morale_active"), fc.clone()]),
        };
        let ability = LcarsAbility {
            name: "predicates".to_string(),
            effects: vec![
                lcars_effect_stat_modify_with_condition(
                    "weapon_damage",
                    0.01,
                    "on_round_start",
                    lcars_condition("morale_active"),
                ),
                lcars_effect_stat_modify_with_condition(
                    "weapon_damage",
                    0.02,
                    "on_round_start",
                    lcars_condition("defender_burning"),
                ),
                lcars_effect_stat_modify_with_condition(
                    "weapon_damage",
                    0.03,
                    "on_round_start",
                    lcars_condition("defender_hull_breach"),
                ),
                lcars_effect_stat_modify_with_condition(
                    "weapon_damage",
                    0.04,
                    "on_round_start",
                    fc,
                ),
                lcars_effect_stat_modify_with_condition(
                    "weapon_damage",
                    0.05,
                    "on_round_start",
                    compound,
                ),
            ],
        };
        let contexts = resolve_officer_ability(
            &officer,
            &ability,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &ResolveOptions::default(),
            0,
        );
        assert_eq!(contexts.len(), 5);
        assert_eq!(
            contexts[0].ability.condition,
            Some(AbilityCondition::MoraleActive)
        );
        assert_eq!(
            contexts[1].ability.condition,
            Some(AbilityCondition::DefenderBurning)
        );
        assert_eq!(
            contexts[2].ability.condition,
            Some(AbilityCondition::DefenderHullBreach)
        );
        assert_eq!(
            contexts[3].ability.condition,
            Some(AbilityCondition::DefenderFactionIs(
                OpponentFactionTag::Klingon
            ))
        );
        let and_cond = contexts[4].ability.condition.clone().unwrap();
        let ctx_ok = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: true,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Klingon,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
            combat_battle_type_id: None,
            defender_level: None,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            attacker_ship_id: String::new(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        assert!(and_cond.evaluate(&ctx_ok));
        let ctx_no_morale = CombatContext {
            attacker_morale_active: false,
            ..ctx_ok
        };
        assert!(!and_cond.evaluate(&ctx_no_morale));
    }

    #[test]
    fn resolve_lcars_condition_maps_defender_hull_faction_id_and_evaluates() {
        let c = resolve_lcars_condition(&LcarsCondition {
            condition_type: "defender_hull_faction_id".to_string(),
            stat: None,
            threshold_pct: None,
            min: None,
            max: None,
            faction: None,
            group: None,
            min_members: None,
            tag: None,
            ship_type: None,
            faction_id: Some(1750120904),
            ship_id: None,
            enemy_type: None,
            battle_types: None,
            conditions: None,
        })
        .unwrap();
        assert_eq!(c, AbilityCondition::DefenderHullFactionIdIs(1750120904));
        let mut ctx = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 1750120904,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
            combat_battle_type_id: None,
            defender_level: None,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            attacker_ship_id: String::new(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        assert!(c.evaluate(&ctx));
        ctx.defender_hull_faction_id = 0;
        assert!(!c.evaluate(&ctx));
    }

    #[test]
    fn resolve_lcars_condition_maps_attacker_burning_and_self_hull_breach() {
        let burn = resolve_lcars_condition(&LcarsCondition {
            condition_type: "attacker_burning".to_string(),
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
        })
        .unwrap();
        assert_eq!(burn, AbilityCondition::AttackerBurning);
        let hb = resolve_lcars_condition(&LcarsCondition {
            condition_type: "self_hull_breach".to_string(),
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
        })
        .unwrap();
        assert_eq!(hb, AbilityCondition::AttackerHullBreach);
    }

    #[test]
    fn resolve_lcars_condition_maps_defender_assimilated() {
        let c = resolve_lcars_condition(&LcarsCondition {
            condition_type: "defender_assimilated".to_string(),
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
        })
        .unwrap();
        assert_eq!(c, AbilityCondition::DefenderAssimilated);
        let alias = resolve_lcars_condition(&LcarsCondition {
            condition_type: "target_assimilated".to_string(),
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
        })
        .unwrap();
        assert_eq!(alias, c);
    }

    #[test]
    fn resolve_lcars_condition_maps_defender_ship_type() {
        let c = LcarsCondition {
            condition_type: "defender_ship_type_is".to_string(),
            stat: None,
            threshold_pct: None,
            min: None,
            max: None,
            faction: None,
            group: None,
            min_members: None,
            tag: None,
            ship_type: Some("explorer".to_string()),
            faction_id: None,
            ship_id: None,
            enemy_type: None,
            battle_types: None,
            conditions: None,
        };
        let ac = resolve_lcars_condition(&c).expect("maps");
        assert_eq!(ac, AbilityCondition::DefenderShipTypeIs(ShipType::Explorer));
    }

    #[test]
    fn resolve_lcars_condition_maps_attacker_ship_id_is_and_evaluates() {
        let c = LcarsCondition {
            condition_type: "attacker_ship_id_is".to_string(),
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
            ship_id: Some("uss_discovery".to_string()),
            enemy_type: None,
            battle_types: None,
            conditions: None,
        };
        let ac = resolve_lcars_condition(&c).expect("maps");
        assert_eq!(
            ac,
            AbilityCondition::AttackerShipIdIs("uss_discovery".into())
        );
        let ctx_match = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
            combat_battle_type_id: None,
            defender_level: None,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            attacker_ship_id: "uss_discovery".into(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        assert!(ac.evaluate(&ctx_match));
        let ctx_other = CombatContext {
            attacker_ship_id: "uss_voyager".into(),
            ..ctx_match
        };
        assert!(!ac.evaluate(&ctx_other));
    }

    #[test]
    fn resolve_lcars_condition_maps_attacker_ship_type_and_evaluates() {
        let c = LcarsCondition {
            condition_type: "self_ship_class_is".to_string(),
            stat: None,
            threshold_pct: None,
            min: None,
            max: None,
            faction: None,
            group: None,
            min_members: None,
            tag: None,
            ship_type: Some("battleship".to_string()),
            faction_id: None,
            ship_id: None,
            enemy_type: None,
            battle_types: None,
            conditions: None,
        };
        let ac = resolve_lcars_condition(&c).expect("maps");
        assert_eq!(
            ac,
            AbilityCondition::AttackerShipTypeIs(ShipType::Battleship)
        );
        let ctx_bb = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
            combat_battle_type_id: None,
            defender_level: None,
            defender_ship_type: ShipType::Explorer,
            attacker_ship_type: ShipType::Battleship,
            attacker_ship_id: String::new(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        assert!(ac.evaluate(&ctx_bb));
        let ctx_int = CombatContext {
            attacker_ship_type: ShipType::Interceptor,
            ..ctx_bb
        };
        assert!(!ac.evaluate(&ctx_int));
    }

    #[test]
    fn resolve_lcars_condition_maps_defender_opponent_kind_and_evaluates() {
        let npc = resolve_lcars_condition(&LcarsCondition {
            condition_type: "defender_is_npc_hostile".to_string(),
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
        })
        .expect("npc");
        let player = resolve_lcars_condition(&LcarsCondition {
            condition_type: "defender_is_player_ship".to_string(),
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
        })
        .expect("player");
        let ctx_pve = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
            combat_battle_type_id: None,
            defender_level: None,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            attacker_ship_id: String::new(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        assert!(npc.evaluate(&ctx_pve));
        assert!(!player.evaluate(&ctx_pve));
        let ctx_pvp = CombatContext {
            defender_is_npc_hostile: false,
            defender_is_player_ship: true,
            ..ctx_pve
        };
        assert!(!npc.evaluate(&ctx_pvp));
        assert!(player.evaluate(&ctx_pvp));
    }

    #[test]
    fn resolve_lcars_condition_not_defender_armada_evaluates() {
        let c = LcarsCondition {
            condition_type: "not".to_string(),
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
            conditions: Some(vec![LcarsCondition {
                condition_type: "defender_ship_type_is".to_string(),
                stat: None,
                threshold_pct: None,
                min: None,
                max: None,
                faction: None,
                group: None,
                min_members: None,
                tag: None,
                ship_type: Some("armada".to_string()),
                faction_id: None,
                ship_id: None,
                enemy_type: None,
                battle_types: None,
                conditions: None,
            }]),
        };
        let ac = resolve_lcars_condition(&c).expect("maps");
        let ctx_bb = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
            combat_battle_type_id: None,
            defender_level: None,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            attacker_ship_id: String::new(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        assert!(ac.evaluate(&ctx_bb));
        let ctx_armada = CombatContext {
            defender_ship_type: ShipType::Armada,
            ..ctx_bb
        };
        assert!(!ac.evaluate(&ctx_armada));
    }

    #[test]
    fn resolve_lcars_condition_maps_tal_not_on_bridge_and_evaluates() {
        let c = LcarsCondition {
            condition_type: "attacker_officer_tal_not_on_bridge".to_string(),
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
        let ac = resolve_lcars_condition(&c).expect("maps");
        assert_eq!(ac, AbilityCondition::AttackerOfficerTalNotOnBridge);
        let alias = resolve_lcars_condition(&LcarsCondition {
            condition_type: "self_officer_tal_not_on_bridge".to_string(),
            ..c.clone()
        })
        .expect("alias maps");
        assert_eq!(alias, ac);
        let ctx_free = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
            combat_battle_type_id: None,
            defender_level: None,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            attacker_ship_id: String::new(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        assert!(ac.evaluate(&ctx_free));
        let ctx_blocked = CombatContext {
            attacker_tal_assigned_captain_or_bridge: true,
            ..ctx_free
        };
        assert!(!ac.evaluate(&ctx_blocked));
    }

    #[test]
    fn captain_applies_captain_and_bridge_blocks() {
        let officer = LcarsOfficer {
            id: "cap_dual".to_string(),
            name: "CapDual".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(LcarsAbility {
                name: "Cap".to_string(),
                effects: vec![lcars_effect_stat_modify(
                    "isolytic_damage",
                    0.11,
                    "on_round_start",
                )],
            }),
            bridge_ability: Some(LcarsAbility {
                name: "Bridge".to_string(),
                effects: vec![lcars_effect_stat_modify(
                    "isolytic_cascade_damage",
                    0.22,
                    "on_combat_start",
                )],
            }),
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let mut officers = HashMap::new();
        officers.insert("cap_dual".to_string(), officer);
        let opts = ResolveOptions {
            tier: Some(5),
            ..Default::default()
        };
        let buff = resolve_crew_to_buff_set("cap_dual", &[], &[], &officers, &opts);
        assert_eq!(buff.crew.seats.len(), 2);
        assert!(buff.crew.seats.iter().all(|s| s.seat == CrewSeat::Captain));
        let classes: Vec<_> = buff.crew.seats.iter().map(|s| s.ability.class).collect();
        assert!(classes.contains(&AbilityClass::CaptainManeuver));
        assert!(classes.contains(&AbilityClass::BridgeAbility));
    }

    #[test]
    fn below_decks_does_not_apply_bridge_when_no_below_block() {
        let bridge = LcarsAbility {
            name: "Bd Bridge Only (Bridge)".to_string(),
            effects: vec![lcars_effect_stat_modify("shield_pierce", 10.0, "on_hit")],
        };
        let officer = LcarsOfficer {
            id: "bd_only_bridge".to_string(),
            name: "BdTest".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: Some(bridge),
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let mut officers = HashMap::new();
        officers.insert("bd_only_bridge".to_string(), officer);
        let opts = ResolveOptions {
            tier: Some(5),
            ..Default::default()
        };
        let buff =
            resolve_crew_to_buff_set("", &[], &["bd_only_bridge".to_string()], &officers, &opts);
        assert!(buff.crew.seats.is_empty());
    }

    #[test]
    fn resolve_effect_maps_isolytic_and_shield_mitigation_to_ability_effects() {
        let officer = LcarsOfficer {
            id: "test".to_string(),
            name: "Test".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let options = ResolveOptions {
            tier: Some(5),
            officer_tiers: None,
            officer_levels: None,
        };
        let ability_iso = LcarsAbility {
            name: "iso".to_string(),
            effects: vec![lcars_effect_stat_modify(
                "isolytic_damage",
                0.15,
                "on_round_start",
            )],
        };
        let contexts = resolve_officer_ability(
            &officer,
            &ability_iso,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &options,
            0,
        );
        assert_eq!(contexts.len(), 1);
        assert!(
            matches!(contexts[0].ability.effect, AbilityEffect::IsolyticDamageBonus(v) if (v - 0.15).abs() < 1e-12)
        );

        let ability_def = LcarsAbility {
            name: "def".to_string(),
            effects: vec![lcars_effect_stat_modify(
                "isolytic_defense",
                20.0,
                "on_round_start",
            )],
        };
        let contexts_def = resolve_officer_ability(
            &officer,
            &ability_def,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &options,
            0,
        );
        assert_eq!(contexts_def.len(), 1);
        assert!(
            matches!(contexts_def[0].ability.effect, AbilityEffect::IsolyticDefenseBonus(v) if (v - 20.0).abs() < 1e-12)
        );

        let ability_shield = LcarsAbility {
            name: "shield".to_string(),
            effects: vec![lcars_effect_stat_modify(
                "shield_mitigation",
                0.05,
                "on_combat_start",
            )],
        };
        let contexts_shield = resolve_officer_ability(
            &officer,
            &ability_shield,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &options,
            0,
        );
        assert_eq!(contexts_shield.len(), 1);
        assert!(
            matches!(contexts_shield[0].ability.effect, AbilityEffect::ShieldMitigationBonus(v) if (v - 0.05).abs() < 1e-12)
        );

        let ability_cascade = LcarsAbility {
            name: "cascade".to_string(),
            effects: vec![lcars_effect_stat_modify(
                "isolytic_cascade_damage",
                0.2,
                "on_round_start",
            )],
        };
        let contexts_cascade = resolve_officer_ability(
            &officer,
            &ability_cascade,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &options,
            0,
        );
        assert_eq!(contexts_cascade.len(), 1);
        assert!(
            matches!(contexts_cascade[0].ability.effect, AbilityEffect::IsolyticCascadeDamageBonus(v) if (v - 0.2).abs() < 1e-12)
        );
    }

    #[test]
    fn resolve_bundled_lcars_yaml_discrete_scaling_smoke() {
        let path = Path::new("data/officers/officers.lcars.yaml");
        if !path.exists() {
            return; // skip if data not present (e.g. in minimal checkouts)
        }
        let file = load_lcars_file(path).unwrap();
        let officers = index_lcars_officers_by_id(file.officers);
        let options = ResolveOptions {
            tier: Some(5),
            officer_tiers: None,
            officer_levels: None,
        };
        // Khan (Independent): bridge passive crit_chance uses `scaling.values` at officer tier.
        let khan = resolve_crew_to_buff_set("khan-3f1d1e", &[], &[], &officers, &options);
        let cc = khan
            .static_buffs
            .get("crit_chance")
            .copied()
            .expect("expected khan-3f1d1e passive crit_chance from bundled LCARS");
        assert!((cc - 0.05).abs() < 1e-12);

        // Scotty: passive bridge hull_hp multiplier rank 5 from discrete table.
        let scotty = resolve_crew_to_buff_set("scotty-a83cb5", &[], &[], &officers, &options);
        let hull = scotty
            .static_buffs
            .get("hull_hp")
            .copied()
            .expect("expected scotty-a83cb5 passive hull_hp from bundled LCARS");
        assert!((hull - 1.2).abs() < 1e-12);
    }

    #[test]
    fn resolve_options_level_for_returns_per_officer_override_or_none() {
        let mut levels = HashMap::new();
        levels.insert("a".to_string(), 25u32);
        levels.insert("b".to_string(), 30u32);
        let opts = ResolveOptions {
            tier: None,
            officer_tiers: None,
            officer_levels: Some(levels),
        };
        assert_eq!(opts.level_for("a"), Some(25));
        assert_eq!(opts.level_for("b"), Some(30));
        assert_eq!(opts.level_for("missing"), None);

        let none = ResolveOptions::default();
        assert_eq!(none.level_for("anyone"), None);
    }

    #[test]
    fn resolve_officer_ability_applies_officer_stat_scaling_end_to_end() {
        use crate::data::combat_effect_spec::OfficerStat;
        use crate::lcars::parser::{LcarsLevelStats, LcarsScaling};

        // Mbenga-style on-combat-start ability: armor += <coeff>% of officer.health. Resolver
        // emits a CombatBegin crew seat carrying [`AbilityEffect::MitigationAdditive`] whose value
        // is derived from the officer-stat-scaled armor value.
        let scaling_effect = LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some("armor".to_string()),
            target: None,
            operator: Some("add".to_string()),
            value: None,
            trigger: Some("on_combat_start".to_string()),
            duration: None,
            scaling: Some(LcarsScaling {
                base: None,
                per_rank: None,
                max_rank: Some(3),
                base_chance: None,
                values: Some(vec![15.0, 15.0, 25.0]),
                chance_values: None,
                officer_stat: Some(OfficerStat::Health),
            }),
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let ability = LcarsAbility {
            name: "scale_armor".to_string(),
            effects: vec![scaling_effect],
        };
        let officer = LcarsOfficer {
            id: "scaling_officer".to_string(),
            name: "Scale".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(ability.clone()),
            bridge_ability: None,
            below_decks_ability: None,
            stats: vec![
                LcarsLevelStats {
                    level: 1,
                    attack: 0.0,
                    defense: 0.0,
                    health: 100.0,
                },
                LcarsLevelStats {
                    level: 30,
                    attack: 0.0,
                    defense: 0.0,
                    health: 400.0,
                },
            ],
            max_level_by_rank: vec![5, 10, 15, 25, 30],
        };

        let armor_for = |level: u32| -> f64 {
            let opts = ResolveOptions {
                tier: None,
                officer_tiers: Some([("scaling_officer".to_string(), 3u8)].into_iter().collect()),
                officer_levels: Some(
                    [("scaling_officer".to_string(), level)]
                        .into_iter()
                        .collect(),
                ),
            };
            let contexts = resolve_officer_ability(
                &officer,
                &ability,
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                &opts,
                0,
            );
            assert_eq!(
                contexts.len(),
                1,
                "expected one resolved seat for level {level}"
            );
            match contexts[0].ability.effect {
                AbilityEffect::MitigationAdditive(v) => v,
                ref e => panic!("expected MitigationAdditive, got {e:?}"),
            }
        };
        let v_l1 = armor_for(1);
        let v_l30 = armor_for(30);
        assert!(
            v_l1 > 0.0,
            "scaled armor should be positive at level 1, got {v_l1}"
        );
        // Level 30 health is 4× level 1 health → mitigation-additive armor should scale upward.
        assert!(
            v_l30 > v_l1,
            "officer-stat scaling must move with level: l1={v_l1}, l30={v_l30}"
        );

        // Sanity check: when no per-level stats are wired, the rank coefficient passes through
        // as a flat value (much smaller than the stat-scaled output above).
        let stripped = LcarsOfficer {
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
            ..officer.clone()
        };
        let opts_no_stats = ResolveOptions {
            tier: None,
            officer_tiers: Some([("scaling_officer".to_string(), 3u8)].into_iter().collect()),
            officer_levels: None,
        };
        let no_stat_contexts = resolve_officer_ability(
            &stripped,
            &ability,
            CrewSeat::Captain,
            AbilityClass::CaptainManeuver,
            &opts_no_stats,
            0,
        );
        let no_stat_armor = match no_stat_contexts[0].ability.effect {
            AbilityEffect::MitigationAdditive(v) => v,
            ref e => panic!("expected MitigationAdditive, got {e:?}"),
        };
        assert!(v_l30 > no_stat_armor, "stats must amplify rank coefficient");
    }

    #[test]
    fn resolve_options_tier_for_uses_per_officer_tier_then_fallback() {
        let mut officer_tiers = HashMap::new();
        officer_tiers.insert("officer_a".to_string(), 1u8);
        officer_tiers.insert("officer_b".to_string(), 5u8);
        let options = ResolveOptions {
            tier: Some(3),
            officer_tiers: Some(officer_tiers),
            officer_levels: None,
        };
        assert_eq!(options.tier_for("officer_a"), Some(1));
        assert_eq!(options.tier_for("officer_b"), Some(5));
        assert_eq!(options.tier_for("unknown"), Some(3));
        let options_no_fallback = ResolveOptions {
            tier: None,
            officer_tiers: Some([("x".to_string(), 2u8)].into_iter().collect()),
            officer_levels: None,
        };
        assert_eq!(options_no_fallback.tier_for("x"), Some(2));
        assert_eq!(options_no_fallback.tier_for("y"), None);
    }

    #[test]
    fn per_officer_tier_affects_resolved_static_buffs() {
        // Effect with scaling only (no fixed value): value_at_rank(1) = 0.1, value_at_rank(5) = 0.1 + 0.05*4 = 0.3
        let scaling_effect = LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some("weapon_damage".to_string()),
            target: None,
            operator: Some("add".to_string()),
            value: None,
            trigger: Some("passive".to_string()),
            duration: Some(LcarsDuration::Permanent("permanent".to_string())),
            scaling: Some(LcarsScaling {
                base: Some(0.1),
                per_rank: Some(0.05),
                max_rank: Some(5),
                base_chance: None,
                values: None,
                chance_values: None,
                officer_stat: None,
            }),
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let officer = LcarsOfficer {
            id: "tiered_officer".to_string(),
            name: "Tiered".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(LcarsAbility {
                name: "scaling".to_string(),
                effects: vec![scaling_effect],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let mut officers = HashMap::new();
        officers.insert("tiered_officer".to_string(), officer.clone());
        let options_tier1 = ResolveOptions {
            tier: None,
            officer_tiers: Some([("tiered_officer".to_string(), 1u8)].into_iter().collect()),
            officer_levels: None,
        };
        let options_tier5 = ResolveOptions {
            tier: None,
            officer_tiers: Some([("tiered_officer".to_string(), 5u8)].into_iter().collect()),
            officer_levels: None,
        };
        let buff_tier1 =
            resolve_crew_to_buff_set("tiered_officer", &[], &[], &officers, &options_tier1);
        let buff_tier5 =
            resolve_crew_to_buff_set("tiered_officer", &[], &[], &officers, &options_tier5);
        let v1 = buff_tier1
            .static_buffs
            .get("weapon_damage")
            .copied()
            .unwrap_or(0.0);
        let v5 = buff_tier5
            .static_buffs
            .get("weapon_damage")
            .copied()
            .unwrap_or(0.0);
        assert!(
            (v5 - v1).abs() > 1e-6,
            "per-officer tier should change resolved static_buffs: tier1={v1}, tier5={v5}"
        );
    }

    #[test]
    fn per_officer_tier_uses_discrete_scaling_values_not_linear_endpoints() {
        // Game-style table (e.g. Alok Sahar apex shred): not colinear with base..last linear fit.
        let table = vec![0.15, 0.25, 0.35, 0.5, 0.7];
        let linear_rank2: f64 = table[0] + (table[4] - table[0]) / 4.0;
        assert!(
            (table[1] - linear_rank2).abs() > 1e-6,
            "test table rank2 must differ from linear endpoint fit"
        );

        let scaling_effect = LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some("apex_shred".to_string()),
            target: None,
            operator: Some("add".to_string()),
            value: None,
            trigger: Some("passive".to_string()),
            duration: Some(LcarsDuration::Permanent("permanent".to_string())),
            scaling: Some(LcarsScaling {
                base: Some(999.0),
                per_rank: Some(999.0),
                max_rank: Some(5),
                base_chance: None,
                values: Some(table),
                chance_values: None,
                officer_stat: None,
            }),
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let officer = LcarsOfficer {
            id: "table_officer".to_string(),
            name: "Table".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(LcarsAbility {
                name: "scaling".to_string(),
                effects: vec![scaling_effect],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let mut officers = HashMap::new();
        officers.insert("table_officer".to_string(), officer);
        let options_tier2 = ResolveOptions {
            tier: None,
            officer_tiers: Some([("table_officer".to_string(), 2u8)].into_iter().collect()),
            officer_levels: None,
        };
        let buff = resolve_crew_to_buff_set("table_officer", &[], &[], &officers, &options_tier2);
        let v2 = buff.static_buffs.get("apex_shred").copied().unwrap_or(0.0);
        assert!(
            (v2 - 0.25).abs() < 1e-9,
            "expected discrete rank-2 value 0.25, got {v2}"
        );
    }

    #[test]
    fn resolve_effect_supports_trigger_aliases_and_duration_rounds() {
        let officer = LcarsOfficer {
            id: "trigger_officer".to_string(),
            name: "Trigger Officer".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let ability = LcarsAbility {
            name: "aliases".to_string(),
            effects: vec![
                LcarsEffect {
                    effect_type: "hull_breach".to_string(),
                    stat: None,
                    target: None,
                    operator: None,
                    value: None,
                    trigger: Some("CriticalShotFired".to_string()),
                    duration: Some(LcarsDuration::Rounds { rounds: 3 }),
                    scaling: None,
                    condition: None,
                    chance: Some(1.0),
                    multiplier: None,
                    tag: None,
                    accumulate: None,
                    decay: None,
                },
                LcarsEffect {
                    effect_type: "assimilated".to_string(),
                    stat: None,
                    target: None,
                    operator: None,
                    value: None,
                    trigger: Some("RoundStart".to_string()),
                    duration: Some(LcarsDuration::Stacks { stacks: 2 }),
                    scaling: None,
                    condition: None,
                    chance: Some(1.0),
                    multiplier: None,
                    tag: None,
                    accumulate: None,
                    decay: None,
                },
            ],
        };

        let contexts = resolve_officer_ability(
            &officer,
            &ability,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &ResolveOptions::default(),
            0,
        );
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].ability.timing, TimingWindow::AttackPhase);
        assert!(matches!(
            contexts[0].ability.effect,
            AbilityEffect::HullBreach {
                duration_rounds: 3,
                ..
            }
        ));
        assert_eq!(contexts[1].ability.timing, TimingWindow::RoundStart);
        assert!(matches!(
            contexts[1].ability.effect,
            AbilityEffect::Assimilated {
                duration_rounds: 2,
                ..
            }
        ));
    }

    #[test]
    fn resolve_effect_supports_operator_and_shots_aliases() {
        let officer = LcarsOfficer {
            id: "op_officer".to_string(),
            name: "Operator Officer".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let ability = LcarsAbility {
            name: "ops".to_string(),
            effects: vec![
                LcarsEffect {
                    effect_type: "stat_modify".to_string(),
                    stat: Some("weapon_damage".to_string()),
                    target: None,
                    operator: Some("sub".to_string()),
                    value: Some(0.2),
                    trigger: Some("on_round_start".to_string()),
                    duration: None,
                    scaling: None,
                    condition: None,
                    chance: None,
                    multiplier: None,
                    tag: None,
                    accumulate: None,
                    decay: None,
                },
                LcarsEffect {
                    effect_type: "stat_modify".to_string(),
                    stat: Some("shots_per_attack".to_string()),
                    target: None,
                    operator: Some("add".to_string()),
                    value: Some(0.5),
                    trigger: Some("on_round_start".to_string()),
                    duration: Some(LcarsDuration::Rounds { rounds: 2 }),
                    scaling: None,
                    condition: None,
                    chance: None,
                    multiplier: None,
                    tag: None,
                    accumulate: None,
                    decay: None,
                },
            ],
        };

        let contexts = resolve_officer_ability(
            &officer,
            &ability,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &ResolveOptions::default(),
            0,
        );
        assert_eq!(contexts.len(), 2);
        assert!(matches!(
            contexts[0].ability.effect,
            AbilityEffect::AttackMultiplier(v) if (v - 0.8).abs() < 1e-12
        ));
        assert!(matches!(
            contexts[1].ability.effect,
            AbilityEffect::ShotsBonus {
                bonus_pct,
                duration_rounds: 2,
                ..
            } if (bonus_pct - 0.5).abs() < 1e-12
        ));
    }

    #[test]
    fn effect_trigger_timing_on_shield_break_targets_self_vs_enemy() {
        let mut e = lcars_effect_stat_modify("weapon_damage", 0.1, "on_shield_break");
        e.target = Some("self".to_string());
        assert_eq!(
            effect_trigger_timing(&e),
            Some(TimingWindow::SelfShieldBreak)
        );
        e.target = Some("enemy".to_string());
        assert_eq!(effect_trigger_timing(&e), Some(TimingWindow::ShieldBreak));
        e.target = None;
        assert_eq!(
            effect_trigger_timing(&e),
            Some(TimingWindow::SelfShieldBreak)
        );
    }

    #[test]
    fn effect_trigger_timing_explicit_own_and_enemy_shield_triggers() {
        let mut e = lcars_effect_stat_modify("weapon_damage", 0.1, "on_own_shield_break");
        assert_eq!(
            effect_trigger_timing(&e),
            Some(TimingWindow::SelfShieldBreak)
        );
        e.trigger = Some("on_enemy_shield_break".to_string());
        assert_eq!(effect_trigger_timing(&e), Some(TimingWindow::ShieldBreak));
    }

    #[test]
    fn resolve_lcars_condition_maps_combat_battle_type_any() {
        let c = resolve_lcars_condition(&LcarsCondition {
            condition_type: "combat_battle_type_any".to_string(),
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
            battle_types: Some(vec![9, 4, 4]),
            conditions: None,
        })
        .expect("battle types map");
        assert_eq!(c, AbilityCondition::CombatBattleTypeAny(vec![4, 9]));
    }

    #[test]
    fn resolve_lcars_condition_maps_defender_level_at_most() {
        let c = resolve_lcars_condition(&LcarsCondition {
            condition_type: "defender_level_at_most".to_string(),
            stat: None,
            threshold_pct: None,
            min: None,
            max: Some(51),
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
        })
        .expect("level maps");
        assert_eq!(c, AbilityCondition::DefenderLevelAtMost(51));
    }
}
