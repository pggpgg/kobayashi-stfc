//! Resolves parsed LCARS abilities into a [BuffSet] (static buffs + crew config for the engine).

use std::collections::{HashMap, HashSet};

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
}

impl ResolveOptions {
    /// Tier to use for the given officer: per-officer tier if available, else fallback [ResolveOptions::tier].
    pub fn tier_for(&self, officer_id: &str) -> Option<u8> {
        self.officer_tiers
            .as_ref()
            .and_then(|m| m.get(officer_id).copied())
            .or(self.tier)
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
/// Used by [`resolve_officer_ability`] (via [`lcars_condition_to_ability_condition`]) and LCARS validation.
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
        "defender_burning" | "target_burning" | "burning" => Ok(AbilityCondition::DefenderBurning),
        "defender_hull_breach" | "target_hull_breach" | "hull_breach_active" => {
            Ok(AbilityCondition::DefenderHullBreach)
        }
        "defender_faction_is"
        | "defender_faction"
        | "opponent_faction_is"
        | "opponent_faction"
        | "faction_is" => {
            let slug = c
                .faction
                .as_deref()
                .or(c.tag.as_deref())
                .ok_or_else(|| {
                    "faction condition requires `faction` or `tag` with a known faction slug".to_string()
                })?;
            let tag = OpponentFactionTag::from_data_slug(slug).ok_or_else(|| {
                format!("unknown faction slug '{slug}' for condition type '{ty}'")
            })?;
            Ok(AbilityCondition::DefenderFactionIs(tag))
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
                    "attacker/self ship class condition requires `ship_type` or `stat` slug".to_string()
                })?;
            let st = ShipType::from_data_slug(slug).ok_or_else(|| {
                format!("unknown ship class slug '{slug}' for condition type '{ty}'")
            })?;
            Ok(AbilityCondition::AttackerShipTypeIs(st))
        }
        "and" => {
            let children = c
                .conditions
                .as_ref()
                .ok_or_else(|| "`and` condition requires non-empty `conditions` array".to_string())?;
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
            let children = c
                .conditions
                .as_ref()
                .ok_or_else(|| "`or` condition requires non-empty `conditions` array".to_string())?;
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

fn lcars_condition_to_ability_condition(c: &LcarsCondition) -> Option<AbilityCondition> {
    resolve_lcars_condition(c).ok()
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

fn duration_rounds_or_default(effect: &LcarsEffect, fallback: u32) -> u32 {
    effect
        .duration
        .as_ref()
        .and_then(|d| match d {
            crate::lcars::parser::LcarsDuration::Rounds { rounds } => Some(*rounds),
            crate::lcars::parser::LcarsDuration::Stacks { stacks } => Some(*stacks),
            crate::lcars::parser::LcarsDuration::Permanent(_) => None,
        })
        .unwrap_or(fallback)
        .max(1)
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

/// LCARS `armor` values often follow sheet “percent of health” magnitudes (e.g. `8`, `25`).
/// Engine mitigation is a `0..1` fraction; treat `|v| > 1` as percent points (`v / 100`).
fn mitigation_fraction_from_lcars_armor_value(raw: f64) -> f64 {
    if raw.abs() > 1.0 {
        raw / 100.0
    } else {
        raw
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
    passive && permanent && effect.effect_type == "stat_modify"
}

/// Resolve a single LCARS effect into (TimingWindow, AbilityEffect) if supported.
/// Unknown effect types or stats are skipped (graceful degradation); returns None.
/// Static effects (passive + permanent stat_modify) return None so they are only in static_buffs.
fn resolve_effect(
    effect: &LcarsEffect,
    _ability_name: &str,
    options: &ResolveOptions,
    officer_id: &str,
) -> Option<(TimingWindow, AbilityEffect)> {
    if is_static_effect(effect) {
        return None;
    }
    let tier = options.tier_for(officer_id);
    let timing = effect_trigger_timing(effect)?;

    match effect.effect_type.as_str() {
        "stat_modify" => {
            let value = effect
                .value
                .or_else(|| effect.scaling.as_ref().map(|s| s.value_at_rank(tier)))?;
            let stat = effect.stat.as_deref().unwrap_or("");
            let op = normalize_operator(effect.operator.as_deref());

            // Map stat + operator to engine effect. Multiplicative damage -> AttackMultiplier; pierce -> PierceBonus.
            match stat {
                "weapon_damage" | "attack" => {
                    if let Some(ref decay) = effect.decay {
                        let initial = value;
                        let decay_per_round = decay.amount.unwrap_or(0.0);
                        let floor = decay.floor.unwrap_or(1.0);
                        Some((
                            timing,
                            AbilityEffect::DecayingAttackMultiplier {
                                initial,
                                decay_per_round,
                                floor,
                            },
                        ))
                    } else if let Some(ref acc) = effect.accumulate {
                        let initial = value;
                        let growth_per_round = acc.amount.unwrap_or(0.0);
                        let ceiling = acc.ceiling.unwrap_or(2.0);
                        Some((
                            timing,
                            AbilityEffect::AccumulatingAttackMultiplier {
                                initial,
                                growth_per_round,
                                ceiling,
                            },
                        ))
                    } else {
                        let mult = match op.as_str() {
                            // Best effort: map common canonical forms to additive/multiplicative behavior.
                            "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                            | "multiplybaseadd" => value,
                            "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub"
                            | "multiplybasesub" => 1.0 - value,
                            "set" => value,
                            _ => 1.0 + value,
                        };
                        Some((timing, AbilityEffect::AttackMultiplier(mult)))
                    }
                }
                "shield_pierce" | "armor_pierce" => {
                    let add = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" => value - 1.0,
                        "sub" | "mul_sub" | "multiplysub" => -value,
                        "set" => value,
                        _ => value,
                    };
                    Some((timing, AbilityEffect::PierceBonus(add)))
                }
                "crit_chance" => {
                    let add = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                        | "multiplybaseadd" => return None,
                        "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub"
                        | "multiplybasesub" => -value,
                        "set" => return None,
                        _ => value,
                    };
                    Some((timing, AbilityEffect::CritChanceBonus(add)))
                }
                "crit_damage" => {
                    let mult = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                        | "multiplybaseadd" => value,
                        "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub"
                        | "multiplybasesub" => (1.0 - value).max(0.0),
                        "set" => value.max(0.0),
                        _ => 1.0 + value,
                    };
                    if mult.is_finite() && mult > 0.0 {
                        Some((timing, AbilityEffect::CritDamageMultiplier(mult)))
                    } else {
                        None
                    }
                }
                "apex_shred" => Some((timing, AbilityEffect::ApexShredBonus(value))),
                "apex_barrier" => Some((timing, AbilityEffect::ApexBarrierBonus(value))),
                "shield_regen" | "shield_hp_repair" => {
                    Some((timing, AbilityEffect::ShieldRegen(value)))
                }
                "hull_repair" | "hull_hp_repair" => {
                    if timing == TimingWindow::Kill {
                        Some((timing, AbilityEffect::OnKillHullRegen(value)))
                    } else {
                        Some((timing, AbilityEffect::HullRegen(value)))
                    }
                }
                "isolytic_damage" => {
                    let add = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" => value - 1.0,
                        "sub" | "mul_sub" | "multiplysub" => -value,
                        _ => value,
                    };
                    Some((timing, AbilityEffect::IsolyticDamageBonus(add)))
                }
                "isolytic_defense" => {
                    let add = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" => value - 1.0,
                        "sub" | "mul_sub" | "multiplysub" => -value,
                        _ => value,
                    };
                    Some((timing, AbilityEffect::IsolyticDefenseBonus(add)))
                }
                "isolytic_cascade" | "isolytic_cascade_damage" => {
                    let add = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" => value - 1.0,
                        "sub" | "mul_sub" | "multiplysub" => -value,
                        _ => value,
                    };
                    Some((timing, AbilityEffect::IsolyticCascadeDamageBonus(add)))
                }
                "shield_mitigation" => {
                    let add = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" => value - 1.0,
                        "sub" | "mul_sub" | "multiplysub" => -value,
                        _ => value,
                    };
                    Some((timing, AbilityEffect::ShieldMitigationBonus(add)))
                }
                "armor" => {
                    if !matches!(
                        timing,
                        TimingWindow::CombatBegin | TimingWindow::RoundStart
                    ) {
                        return None;
                    }
                    let add = match op.as_str() {
                        "multiply" | "mul_add" | "multiplyadd" | "multiply_base_add"
                        | "multiplybaseadd" => value - 1.0,
                        "sub" | "mul_sub" | "multiplysub" | "multiply_base_sub"
                        | "multiplybasesub" => -value,
                        "set" => return None,
                        _ => value,
                    };
                    Some((
                        timing,
                        AbilityEffect::MitigationAdditive(
                            mitigation_fraction_from_lcars_armor_value(add),
                        ),
                    ))
                }
                // Combat-begin accuracy is merged in [resolve_crew_to_buff_set]; other timings are not modeled.
                "accuracy" => None,
                "shots" | "weapon_shots" | "shots_per_weapon" | "shots_per_attack" => {
                    // +X% shots for Y rounds (round half-even applied in engine). Only at round start or combat begin.
                    if matches!(timing, TimingWindow::RoundStart | TimingWindow::CombatBegin) {
                        let bonus_pct = match op.as_str() {
                            "multiply" | "mul_add" | "multiplyadd" => value - 1.0,
                            "sub" | "mul_sub" | "multiplysub" => -value,
                            "set" => value,
                            _ => value,
                        };
                        let duration_rounds = duration_rounds_or_default(effect, 1);
                        Some((
                            timing,
                            AbilityEffect::ShotsBonus {
                                chance: 1.0,
                                bonus_pct,
                                duration_rounds,
                            },
                        ))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        "extra_attack" => {
            // extra_attack is handled via BuffSet.proc_chance/proc_multiplier, not crew seats.
            // Return None so it's not added to crew; resolve_crew_to_buff_set accumulates proc separately.
            None
        }
        "morale" => {
            let chance = effect
                .chance
                .or_else(|| effect.scaling.as_ref().map(|s| s.chance_at_rank(tier)))
                .unwrap_or(0.0);
            Some((timing, AbilityEffect::Morale(chance)))
        }
        "assimilated" => {
            let chance = effect
                .chance
                .or_else(|| effect.scaling.as_ref().map(|s| s.chance_at_rank(tier)))
                .unwrap_or(0.0);
            let duration_rounds = duration_rounds_or_default(effect, 1);
            Some((
                timing,
                AbilityEffect::Assimilated {
                    chance,
                    duration_rounds,
                },
            ))
        }
        "hull_breach" => {
            let chance = effect
                .chance
                .or_else(|| effect.scaling.as_ref().map(|s| s.chance_at_rank(tier)))
                .unwrap_or(0.0);
            let duration_rounds = duration_rounds_or_default(effect, 1);
            Some((
                timing,
                AbilityEffect::HullBreach {
                    chance,
                    duration_rounds,
                    requires_critical: false,
                },
            ))
        }
        "burning" => {
            let chance = effect
                .chance
                .or_else(|| effect.scaling.as_ref().map(|s| s.chance_at_rank(tier)))
                .unwrap_or(0.0);
            let duration_rounds = duration_rounds_or_default(effect, 1);
            Some((
                timing,
                AbilityEffect::Burning {
                    chance,
                    duration_rounds,
                },
            ))
        }
        "tag" => None, // Non-combat; skip.
        _ => None,
    }
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
        return LcarsEffectCoverage {
            tier: MechanicCoverageTier::Ignored,
            pathway: "tag_non_combat".to_string(),
        };
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
    if effect.effect_type == "stat_modify" {
        let stat = effect.stat.as_deref().unwrap_or("").trim();
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

    if resolve_effect(effect, "", options, officer_id).is_some() {
        return LcarsEffectCoverage {
            tier: MechanicCoverageTier::Implemented,
            pathway: "dynamic_crew_ability".to_string(),
        };
    }

    if effect.effect_type == "stat_modify" {
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
    for effect in &ability.effects {
        if let Some((timing, effect_effect)) =
            resolve_effect(effect, &ability.name, options, &officer.id)
        {
            let condition = effect
                .condition
                .as_ref()
                .and_then(lcars_condition_to_ability_condition);
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
            if effect.effect_type != "stat_modify"
                || effect.trigger.as_deref().map(str::trim) != Some("passive")
                || effect.duration.as_ref().is_some_and(|d| !d.is_permanent())
            {
                continue;
            }
            let value = effect.value.or_else(|| {
                effect
                    .scaling
                    .as_ref()
                    .map(|s| s.value_at_rank(officer_tier))
            });
            if let (Some(stat), Some(v)) = (effect.stat.as_deref(), value) {
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
        // Combat-begin `stat_modify` accuracy: stacks into pre-mitigation attacker stats (scenario),
        // not a crew seat. Multiplicative entries use key `accuracy_cb_mult`.
        for effect in &ability.effects {
            if effect.effect_type != "stat_modify" {
                continue;
            }
            if effect_trigger_timing(effect) != Some(TimingWindow::CombatBegin) {
                continue;
            }
            let stat = effect.stat.as_deref().unwrap_or("").trim();
            if !stat.eq_ignore_ascii_case("accuracy") {
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
        AbilityClass, AbilityCondition, AbilityEffect, OpponentFactionTag, TimingWindow,
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
            defender_faction: OpponentFactionTag::Klingon,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
        };
        assert!(and_cond.evaluate(&ctx_ok));
        let ctx_no_morale = CombatContext {
            attacker_morale_active: false,
            ..ctx_ok
        };
        assert!(!and_cond.evaluate(&ctx_no_morale));
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
            conditions: None,
        };
        let ac = resolve_lcars_condition(&c).expect("maps");
        assert_eq!(ac, AbilityCondition::DefenderShipTypeIs(ShipType::Explorer));
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
            conditions: None,
        };
        let ac = resolve_lcars_condition(&c).expect("maps");
        assert_eq!(ac, AbilityCondition::AttackerShipTypeIs(ShipType::Battleship));
        let ctx_bb = CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_ship_type: ShipType::Explorer,
            attacker_ship_type: ShipType::Battleship,
        };
        assert!(ac.evaluate(&ctx_bb));
        let ctx_int = CombatContext {
            attacker_ship_type: ShipType::Interceptor,
            ..ctx_bb
        };
        assert!(!ac.evaluate(&ctx_int));
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
        };
        let options = ResolveOptions {
            tier: Some(5),
            officer_tiers: None,
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
    fn resolve_options_tier_for_uses_per_officer_tier_then_fallback() {
        let mut officer_tiers = HashMap::new();
        officer_tiers.insert("officer_a".to_string(), 1u8);
        officer_tiers.insert("officer_b".to_string(), 5u8);
        let options = ResolveOptions {
            tier: Some(3),
            officer_tiers: Some(officer_tiers),
        };
        assert_eq!(options.tier_for("officer_a"), Some(1));
        assert_eq!(options.tier_for("officer_b"), Some(5));
        assert_eq!(options.tier_for("unknown"), Some(3));
        let options_no_fallback = ResolveOptions {
            tier: None,
            officer_tiers: Some([("x".to_string(), 2u8)].into_iter().collect()),
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
        };
        let mut officers = HashMap::new();
        officers.insert("tiered_officer".to_string(), officer.clone());
        let options_tier1 = ResolveOptions {
            tier: None,
            officer_tiers: Some([("tiered_officer".to_string(), 1u8)].into_iter().collect()),
        };
        let options_tier5 = ResolveOptions {
            tier: None,
            officer_tiers: Some([("tiered_officer".to_string(), 5u8)].into_iter().collect()),
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
        };
        let mut officers = HashMap::new();
        officers.insert("table_officer".to_string(), officer);
        let options_tier2 = ResolveOptions {
            tier: None,
            officer_tiers: Some([("table_officer".to_string(), 2u8)].into_iter().collect()),
        };
        let buff =
            resolve_crew_to_buff_set("table_officer", &[], &[], &officers, &options_tier2);
        let v2 = buff
            .static_buffs
            .get("apex_shred")
            .copied()
            .unwrap_or(0.0);
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
}
