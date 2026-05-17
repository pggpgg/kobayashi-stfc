//! Resolves parsed LCARS abilities into a [BuffSet] (static buffs + crew config for the engine).

use std::collections::{HashMap, HashSet};

use crate::combat::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, Combatant, CrewConfiguration,
    CrewOfficerStatTotals, CrewSeat, CrewSeatContext, TimingWindow,
};
use crate::data::combat_effect_spec::{AbilityConditionSpec, AbilityOperationSpec};
use crate::data::profile;
use crate::lcars::parser::{LcarsAbility, LcarsEffect, LcarsOfficer};
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
    /// Per-side sum of crewed-officer Attack / Defense / Health stats (each crewed officer at
    /// weight 1.0, deduped by officer id). Populated in Phase 1; consumed in Phase 2/3.
    /// See `docs/OFFICER_STAT_FORMULA.md`.
    pub officer_stat_totals: CrewOfficerStatTotals,
    /// Officer-rating ability contributions whose effect on the per-side multiplier depends on
    /// fight-setup state that isn't known at resolve time (e.g. `attacker_ship_type_is`,
    /// `defender_is_player_ship`). Evaluated by
    /// [`crate::data::profile::compute_officer_stat_runtime_bonus`] against a setup context;
    /// entries whose conditions are all true are added to the per-axis multiplier.
    ///
    /// Officer-stat effects with **no** conditions are accumulated directly into [`static_buffs`]
    /// under their stat key (the simpler Phase 3 path); the pending list is only used when
    /// conditions need late evaluation.
    pub pending_officer_stat_contributions: Vec<PendingOfficerStatContribution>,
}

/// Officer-rating buff contribution whose application depends on conditions that can only be
/// evaluated once fight-setup state is known. See [`BuffSet::pending_officer_stat_contributions`].
#[derive(Debug, Clone)]
pub struct PendingOfficerStatContribution {
    /// Engine stat key the bonus targets: `officer_attack`, `officer_defense`, `officer_health`,
    /// or the synthetic `officer_stat_all` (boosts all three axes).
    pub stat_key: String,
    /// Bonus value (e.g. `0.20` for +20%). Added into the matching per-axis multiplier on top
    /// of the profile bonus and any unconditional static_buffs entries.
    pub value: f64,
    /// `true` for attacker-side buffs (the default `target: self`); `false` for defender-side
    /// debuffs (`target: enemy`). Phase 4b/4a both filter target-enemy contributions out of the
    /// attacker's compute path; Phase 4c will use this for PvP defender-side debuffs.
    pub target_attacker: bool,
    /// All conditions must evaluate to true at fight setup for the contribution to apply. If
    /// any condition evaluates to `Some(false)` or `None` (undecidable / dynamic), the
    /// contribution is silently dropped.
    pub conditions: Vec<AbilityConditionSpec>,
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

// LCARS condition resolution is now handled entirely through the canonical CombatEffectSpec path:
// lcars_condition_to_spec → compile_condition.

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
///
/// This is the **single source of truth** for LCARS trigger → engine timing. The runtime adapter
/// at [`crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec`] calls into this
/// function (do not reintroduce a string-keyed variant — it will silently diverge from the
/// `target`-aware disambiguation above).
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
/// Phase 4d: classify a condition as "dynamic" (must be evaluated each round by the engine)
/// vs static (decidable at fight setup). Mirrors the deferred-list in
/// [`crate::data::profile::eval_static_condition`]; composites are dynamic if any child is
/// dynamic.
fn condition_is_dynamic(cond: &AbilityConditionSpec) -> bool {
    use AbilityConditionSpec as C;
    match cond {
        C::MoraleActive
        | C::DefenderBurning
        | C::AttackerBurning
        | C::DefenderHullBreach
        | C::AttackerHullBreach
        | C::DefenderAssimilated
        | C::AttackerOfficerTalNotOnBridge
        | C::RoundRange { .. }
        | C::StatBelow { .. }
        | C::StatAbove { .. } => true,
        C::And { all } => all.iter().any(condition_is_dynamic),
        C::Or { any } => any.iter().any(condition_is_dynamic),
        C::Not { inner } => condition_is_dynamic(inner),
        _ => false,
    }
}

/// Phase 4d: expand officer-stat effects with dynamic conditions into substituted ship-stat
/// effects so the engine's existing per-round dynamic ability path can fire them when the
/// condition becomes true.
///
/// **Only the Attack axis is wired here**, via [`crate::combat::AbilityEffect::AttackMultiplier`].
/// The Health axis (`hull_hp` / `shield_hp`) is intentionally not emitted because those engine
/// values are fight-setup-only — there is no per-round `AbilityEffect` that multiplies the
/// shield/hull bars mid-fight. The Defense axis is similarly skipped because channel routing
/// (armor / shield_deflection / dodge) needs ship-class context the resolver doesn't have.
///
/// **Approximation v1**: this routes the Attack axis through a direct `weapon_damage`
/// multiplier instead of the §2 rating-via-breakpoint pipeline. Magnitude is correct for
/// crews far from breakpoint tiers; near a tier boundary the breakpoint nuance is lost. The
/// alternative (full per-round officer-stat-rating recomputation in the combat engine) is
/// deferred to future work. Static-conditional contributions still go through the proper
/// pipeline ([`PendingOfficerStatContribution`] + breakpoint lookup at compute time).
///
/// Production-data impact: kirk-1323b6 (`officerstatall +40% morale_active on_round_start`)
/// is the only dynamic-conditional officer-stat effect in current data. After this pass it
/// fires the Attack axis during morale-active rounds; Health and Defense axes are dropped
/// (limitation documented above).
///
/// `target: enemy` contributions are dropped (Phase 4c territory: PvP defender-side compute).
fn expand_dynamic_officer_stat_effects(
    officer: &LcarsOfficer,
    ability: &LcarsAbility,
    seat: CrewSeat,
    class: AbilityClass,
    options: &ResolveOptions,
    contribution_batch: u32,
) -> Vec<CrewSeatContext> {
    let mut out = Vec::new();
    for effect in &ability.effects {
        if effect.effect_type != "tag" {
            continue;
        }
        let Some(stat) = crate::lcars::combat_tag_to_stat(effect.tag.as_deref().unwrap_or(""))
        else {
            continue;
        };
        if !matches!(
            stat,
            "officer_attack" | "officer_defense" | "officer_health" | "officer_stat_all"
        ) {
            continue;
        }
        let Some(ref cond_obj) = effect.condition else {
            // No condition: handled by static path.
            continue;
        };
        let Ok(cond_spec) = crate::lcars::effect_spec_adapter::lcars_condition_to_spec(cond_obj)
        else {
            continue;
        };
        if !condition_is_dynamic(&cond_spec) {
            continue;
        }
        // Target filter: skip target:enemy entirely (Phase 4c).
        if effect
            .target
            .as_deref()
            .map(|t| t.trim().to_ascii_lowercase())
            .as_deref()
            == Some("enemy")
        {
            continue;
        }
        // See doc comment: only Attack axis has a per-round AbilityEffect. Health (max
        // hull/shield) and Defense (channel routing) need engine work that's out of scope
        // for this approximation.
        let substitute_stats: &[&str] = match stat {
            "officer_attack" | "officer_stat_all" => &["weapon_damage"],
            _ => &[],
        };
        for &sub_stat in substitute_stats {
            let synth_effect = LcarsEffect {
                effect_type: "stat_modify".to_string(),
                stat: Some(sub_stat.to_string()),
                target: effect.target.clone(),
                operator: effect.operator.clone(),
                value: effect.value,
                trigger: effect.trigger.clone(),
                duration: effect.duration.clone(),
                scaling: effect.scaling.clone(),
                condition: effect.condition.clone(),
                chance: None,
                multiplier: None,
                tag: None,
                accumulate: None,
                decay: None,
            };
            let synth_ability = LcarsAbility {
                name: ability.name.clone(),
                effects: vec![synth_effect],
            };
            let seats = resolve_officer_ability(
                officer,
                &synth_ability,
                seat,
                class,
                options,
                contribution_batch,
            );
            out.extend(seats);
        }
    }
    out
}

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
    let mut pending_officer_stat_contributions: Vec<PendingOfficerStatContribution> = Vec::new();
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
        let stats_row = officer
            .resolve_level(options.level_for(&officer.id), officer_tier)
            .and_then(|lvl| officer.stats_at_level(lvl));
        for (effect_idx, effect) in ability.effects.iter().enumerate() {
            // Determine the effective stat for passive permanent effects.
            // stat_modify reads from effect.stat; mapped tag effects use the tag→stat table.
            let effective_stat = if effect.effect_type == "stat_modify" {
                effect.stat.as_deref().filter(|s| !s.trim().is_empty())
            } else if effect.effect_type == "tag" {
                crate::lcars::combat_tag_to_stat(effect.tag.as_deref().unwrap_or(""))
            } else {
                None
            };
            // Phase 4a: officer-rating accumulator keys (officer_attack / officer_defense /
            // officer_health / officer_stat_all) accept on_combat_start and on_round_start
            // triggers in addition to passive+permanent, because their semantic effect on
            // per-side ratings is "constant for the duration of combat" regardless of the
            // exact trigger phase (see docs/OFFICER_STAT_FORMULA.md §3). All other stat keys
            // remain gated to passive+permanent.
            let is_officer_stat_key = matches!(
                effective_stat,
                Some("officer_attack")
                    | Some("officer_defense")
                    | Some("officer_health")
                    | Some("officer_stat_all")
            );
            let trigger_str = effect.trigger.as_deref().map(str::trim).unwrap_or("");
            let trigger_ok = trigger_str == "passive"
                || (is_officer_stat_key
                    && matches!(trigger_str, "on_combat_start" | "on_round_start"));
            let duration_ok = effect
                .duration
                .as_ref()
                .map(|d| d.is_permanent())
                .unwrap_or(is_officer_stat_key);
            if effective_stat.is_none() || !trigger_ok || !duration_ok {
                continue;
            }
            let stat = effective_stat.unwrap();
            if stat.eq_ignore_ascii_case("accuracy") {
                // Folded into `accuracy` / `accuracy_cb_mult` in the combat-begin accuracy loop below.
                continue;
            }

            // Route passive-permanent stat_modify/mapped-tag through the canonical CombatEffectSpec IR.
            let stable_id = format!("lcars:{}:{}:static:{effect_idx}", officer.id, ability.name);
            let spec = crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec(
                effect,
                &stable_id,
                &officer.id,
                &ability.name,
                officer_tier,
                stats_row,
            );
            let Some(spec) = spec else {
                continue;
            };
            let Some(ref value_spec) = spec.value else {
                continue;
            };
            let Some(v) = value_spec.scalar else {
                continue;
            };

            let target_attacker = matches!(
                spec.target,
                crate::data::combat_effect_spec::AbilityTargetSpec::AttackerSelf
                    | crate::data::combat_effect_spec::AbilityTargetSpec::SelfShip
            );

            // Phase 4b: officer-stat effects with conditions go into pending_contributions for
            // fight-setup-time evaluation (TOS McCoy attacker_ship_type_is, Dezoc engagement gate,
            // Strike Team Una composite, Kras defender_is_player_ship). Effects with no
            // conditions and target:self take the simpler static_buffs path.
            //
            // target:enemy debuffs still skip the attacker's static_buffs path (Phase 4a target
            // filter); the pending list preserves them with target_attacker=false so a future
            // Phase 4c can route them through PvP defender-side compute.
            if !spec.conditions.is_empty() {
                pending_officer_stat_contributions.push(PendingOfficerStatContribution {
                    stat_key: stat.to_string(),
                    value: v,
                    target_attacker,
                    conditions: spec.conditions.clone(),
                });
                continue;
            }
            if !target_attacker {
                // No conditions, but target:enemy: keep in pending for Phase 4c rather than
                // discard. Attacker-side compute will silently ignore non-attacker entries.
                pending_officer_stat_contributions.push(PendingOfficerStatContribution {
                    stat_key: stat.to_string(),
                    value: v,
                    target_attacker,
                    conditions: Vec::new(),
                });
                continue;
            }

            if spec.operation == AbilityOperationSpec::Multiply {
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
            let Some(value) = crate::lcars::effect_spec_adapter::lcars_effect_resolved_value(
                effect,
                officer_tier,
                stats_row,
            ) else {
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
        // Phase 4d: synthetic per-round seats for officer-stat effects with dynamic conditions
        // (e.g. Kirk-1323b6 `morale_active`). The original officerstat* spec fails to compile
        // because the engine has no officer-rating modifier handler; the substituted ship-stat
        // specs (weapon_damage / hull_hp / shield_hp) compile cleanly and fire per round
        // through the existing dynamic ability path.
        let dynamic_extras = expand_dynamic_officer_stat_effects(
            officer,
            ability,
            seat,
            class,
            options,
            contribution_batch,
        );
        seats.extend(dynamic_extras);
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

    // Per-side officer-stat totals (Phase 1 of officer A/D/H runtime — see
    // `docs/OFFICER_STAT_FORMULA.md` §1). Each crewed officer contributes their A/D/H from
    // `stats_at_level(resolved_level)` at weight 1.0, deduped by officer id so a captain who
    // also appears in `bridge` is only counted once.
    let mut officer_stat_totals = CrewOfficerStatTotals::default();
    let mut counted_for_totals: HashSet<String> = HashSet::new();
    let mut add_officer_stats = |officer: &LcarsOfficer| {
        if !counted_for_totals.insert(officer.id.clone()) {
            return;
        }
        let officer_tier = options.tier_for(&officer.id);
        let Some(level) = officer.resolve_level(options.level_for(&officer.id), officer_tier)
        else {
            return;
        };
        let Some(stats) = officer.stats_at_level(level) else {
            return;
        };
        officer_stat_totals.attack += stats.attack;
        officer_stat_totals.defense += stats.defense;
        officer_stat_totals.health += stats.health;
    };
    if let Some(o) = officers.get(captain_id) {
        add_officer_stats(o);
    }
    for id in bridge {
        if let Some(o) = officers.get(id.as_str()) {
            add_officer_stats(o);
        }
    }
    for id in below_decks {
        if let Some(o) = officers.get(id.as_str()) {
            add_officer_stats(o);
        }
    }

    BuffSet {
        static_buffs,
        crew: CrewConfiguration { seats },
        proc_chance,
        proc_multiplier,
        officer_stat_totals,
        pending_officer_stat_contributions,
    }
}

/// Build a map of officer id -> LcarsOfficer from a list (e.g. from load_lcars_dir).
pub fn index_lcars_officers_by_id(officers: Vec<LcarsOfficer>) -> HashMap<String, LcarsOfficer> {
    officers.into_iter().map(|o| (o.id.clone(), o)).collect()
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::combat::{AbilityClass, AbilityEffect, TimingWindow};
    use crate::lcars::parser::{
        load_lcars_file, LcarsAbility, LcarsDuration, LcarsEffect, LcarsOfficer, LcarsScaling,
    };
    use std::path::Path;

    fn officer_with_stats(
        id: &str,
        stats: Vec<crate::lcars::parser::LcarsLevelStats>,
        max_level_by_rank: Vec<u32>,
    ) -> LcarsOfficer {
        LcarsOfficer {
            id: id.to_string(),
            name: id.to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats,
            max_level_by_rank,
        }
    }

    fn level_stats(
        level: u32,
        attack: f64,
        defense: f64,
        health: f64,
    ) -> crate::lcars::parser::LcarsLevelStats {
        crate::lcars::parser::LcarsLevelStats {
            level,
            attack,
            defense,
            health,
        }
    }

    #[test]
    fn officer_stat_totals_sum_captain_bridge_and_below_decks_at_resolved_level() {
        // Per docs/OFFICER_STAT_FORMULA.md §1: every crewed officer contributes A/D/H at weight 1.0,
        // resolved via officer.stats_at_level(officer.resolve_level(...)).
        let cap = officer_with_stats(
            "cap",
            vec![level_stats(15, 1704.0, 4320.0, 824.0)],
            vec![15],
        );
        let bridge = officer_with_stats(
            "b1",
            vec![
                level_stats(10, 800.0, 900.0, 1000.0),
                level_stats(30, 90206.0, 92060.0, 90091.0),
            ],
            vec![10, 30],
        );
        let bd = officer_with_stats("bd1", vec![level_stats(5, 10.0, 20.0, 30.0)], vec![5]);
        let mut officers = HashMap::new();
        officers.insert("cap".to_string(), cap);
        officers.insert("b1".to_string(), bridge);
        officers.insert("bd1".to_string(), bd);
        let opts = ResolveOptions::default();
        let buff = resolve_crew_to_buff_set(
            "cap",
            &["b1".to_string()],
            &["bd1".to_string()],
            &officers,
            &opts,
        );
        // cap at level 15: 1704/4320/824
        // b1 at max level 30: 90206/92060/90091
        // bd1 at level 5: 10/20/30
        assert_eq!(buff.officer_stat_totals.attack, 1704.0 + 90206.0 + 10.0);
        assert_eq!(buff.officer_stat_totals.defense, 4320.0 + 92060.0 + 20.0);
        assert_eq!(buff.officer_stat_totals.health, 824.0 + 90091.0 + 30.0);
    }

    #[test]
    fn officer_stat_totals_dedup_by_officer_id_when_captain_also_on_bridge() {
        // A captain who also appears in `bridge[]` must count once.
        let cap = officer_with_stats("dup", vec![level_stats(20, 50.0, 60.0, 70.0)], vec![20]);
        let mut officers = HashMap::new();
        officers.insert("dup".to_string(), cap);
        let opts = ResolveOptions::default();
        let buff = resolve_crew_to_buff_set("dup", &["dup".to_string()], &[], &officers, &opts);
        assert_eq!(buff.officer_stat_totals.attack, 50.0);
        assert_eq!(buff.officer_stat_totals.defense, 60.0);
        assert_eq!(buff.officer_stat_totals.health, 70.0);
    }

    #[test]
    fn officer_stat_totals_skip_officers_without_stats() {
        // Officers with empty stats arrays contribute nothing; no panic.
        let cap = officer_with_stats("nostat", Vec::new(), Vec::new());
        let mut officers = HashMap::new();
        officers.insert("nostat".to_string(), cap);
        let opts = ResolveOptions::default();
        let buff = resolve_crew_to_buff_set("nostat", &[], &[], &officers, &opts);
        assert_eq!(buff.officer_stat_totals.attack, 0.0);
        assert_eq!(buff.officer_stat_totals.defense, 0.0);
        assert_eq!(buff.officer_stat_totals.health, 0.0);
    }

    #[test]
    fn officer_stat_totals_honor_per_officer_level_override() {
        // ResolveOptions.officer_levels overrides resolve_level(), so totals reflect the chosen tier.
        let cap = officer_with_stats(
            "lvl",
            vec![
                level_stats(1, 6.0, 6.0, 12.0),
                level_stats(30, 90091.0, 92060.0, 90206.0),
            ],
            vec![30],
        );
        let mut officers = HashMap::new();
        officers.insert("lvl".to_string(), cap);
        let mut levels = HashMap::new();
        levels.insert("lvl".to_string(), 1u32);
        let opts = ResolveOptions {
            officer_levels: Some(levels),
            ..Default::default()
        };
        let buff = resolve_crew_to_buff_set("lvl", &[], &[], &officers, &opts);
        assert_eq!(buff.officer_stat_totals.attack, 6.0);
        assert_eq!(buff.officer_stat_totals.defense, 6.0);
        assert_eq!(buff.officer_stat_totals.health, 12.0);
    }

    fn lcars_effect_officerstat_tag(
        tag: &str,
        value: f64,
        trigger: &str,
        target: &str,
        condition_type: Option<&str>,
    ) -> LcarsEffect {
        let duration = Some(LcarsDuration::Permanent("permanent".to_string()));
        let condition = condition_type.map(|ct| crate::lcars::LcarsCondition {
            condition_type: ct.to_string(),
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
        });
        LcarsEffect {
            effect_type: "tag".to_string(),
            stat: None,
            target: Some(target.to_string()),
            operator: None,
            value: Some(value),
            trigger: Some(trigger.to_string()),
            duration,
            scaling: None,
            condition,
            chance: None,
            multiplier: None,
            tag: Some(tag.to_string()),
            accumulate: None,
            decay: None,
        }
    }

    fn officer_with_officerstat_captain(
        id: &str,
        tag: &str,
        value: f64,
        trigger: &str,
        target: &str,
        condition_type: Option<&str>,
    ) -> LcarsOfficer {
        LcarsOfficer {
            id: id.to_string(),
            name: id.to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(LcarsAbility {
                name: "Motivational".to_string(),
                effects: vec![lcars_effect_officerstat_tag(
                    tag,
                    value,
                    trigger,
                    target,
                    condition_type,
                )],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: vec![level_stats(1, 100.0, 100.0, 100.0)],
            max_level_by_rank: vec![1],
        }
    }

    #[test]
    fn officerstat_passive_permanent_accumulates_into_static_buffs() {
        // Baseline: cadet-kirk pattern (passive, no condition, target:self) — Phase 3 behavior,
        // unchanged by Phase 4a.
        let o = officer_with_officerstat_captain(
            "k",
            "officerstatall:unmapped",
            0.08,
            "passive",
            "self",
            None,
        );
        let mut officers = HashMap::new();
        officers.insert("k".to_string(), o);
        let buff = resolve_crew_to_buff_set("k", &[], &[], &officers, &ResolveOptions::default());
        assert_eq!(
            buff.static_buffs.get("officer_stat_all").copied(),
            Some(0.08)
        );
    }

    #[test]
    fn officerstat_on_round_start_now_accumulates_into_static_buffs() {
        // Phase 4a: Kumak pattern — on_round_start trigger with no condition. Previously
        // dropped from the static-buffs path because trigger != "passive"; now accumulated
        // because the effective stat is an officer-rating key.
        let o = officer_with_officerstat_captain(
            "kumak",
            "officerstatall:unmapped",
            0.05,
            "on_round_start",
            "self",
            None,
        );
        let mut officers = HashMap::new();
        officers.insert("kumak".to_string(), o);
        let buff =
            resolve_crew_to_buff_set("kumak", &[], &[], &officers, &ResolveOptions::default());
        assert_eq!(
            buff.static_buffs.get("officer_stat_all").copied(),
            Some(0.05)
        );
    }

    #[test]
    fn officerstat_on_combat_start_target_enemy_does_not_self_buff() {
        // Phase 4a: Kras pattern — on_combat_start with target:enemy. PvP defender-side support
        // is deferred; in the meantime the effect must NOT incorrectly accumulate into the
        // attacker's static_buffs (which would have it buffing the player instead of debuffing
        // the enemy).
        let o = officer_with_officerstat_captain(
            "kras",
            "officerstatall:unmapped",
            0.20,
            "on_combat_start",
            "enemy",
            Some("defender_is_player_ship"),
        );
        let mut officers = HashMap::new();
        officers.insert("kras".to_string(), o);
        let buff =
            resolve_crew_to_buff_set("kras", &[], &[], &officers, &ResolveOptions::default());
        assert!(
            !buff.static_buffs.contains_key("officer_stat_all"),
            "target:enemy must not self-apply"
        );
    }

    #[test]
    fn officerstat_passive_literal_false_does_not_apply() {
        // Phase 4a: mitchell-0217f7 pattern — passive permanent with `literal_false` clause.
        // Pre-Phase-4a the static-buffs accumulator ignored conditions entirely and would
        // incorrectly add the bonus despite the always-false gate.
        let o = officer_with_officerstat_captain(
            "mitchell",
            "officerstatall:unmapped",
            0.20,
            "passive",
            "self",
            Some("literal_false"),
        );
        let mut officers = HashMap::new();
        officers.insert("mitchell".to_string(), o);
        let buff =
            resolve_crew_to_buff_set("mitchell", &[], &[], &officers, &ResolveOptions::default());
        assert!(
            !buff.static_buffs.contains_key("officer_stat_all"),
            "literal_false must skip the bonus"
        );
    }

    #[test]
    fn phase4d_dynamic_morale_active_emits_attack_multiplier_seat() {
        // Kirk-1323b6 pattern: officerstatall on_round_start morale_active target:self val=0.4.
        // Phase 4d (Attack-axis only): the spec compile errors out for OfficerStatAll modifier,
        // BUT the dynamic expansion emits a synthetic weapon_damage seat that the engine fires
        // per round when MoraleActive becomes true. The Health and Defense axes are dropped
        // because they have no per-round AbilityEffect equivalent (limitation documented on
        // `expand_dynamic_officer_stat_effects`).
        let o = officer_with_officerstat_captain(
            "kirk-dyn",
            "officerstatall:unmapped",
            0.40,
            "on_round_start",
            "self",
            Some("morale_active"),
        );
        let mut officers = HashMap::new();
        officers.insert("kirk-dyn".to_string(), o);
        let buff =
            resolve_crew_to_buff_set("kirk-dyn", &[], &[], &officers, &ResolveOptions::default());
        let attack_seats: Vec<_> = buff
            .crew
            .seats
            .iter()
            .filter(|s| {
                matches!(
                    s.ability.effect,
                    crate::combat::AbilityEffect::AttackMultiplier(_)
                )
            })
            .collect();
        assert_eq!(
            attack_seats.len(),
            1,
            "expected one synthetic AttackMultiplier seat; seats: {:?}",
            buff.crew
                .seats
                .iter()
                .map(|s| format!("{:?}", s.ability.effect))
                .collect::<Vec<_>>()
        );
        let seat = attack_seats[0];
        // Condition must be preserved so the engine evaluates it per round.
        assert!(
            matches!(
                seat.ability.condition,
                Some(crate::combat::AbilityCondition::MoraleActive)
            ),
            "expected MoraleActive condition; got: {:?}",
            seat.ability.condition
        );
        // Trigger must remain RoundStart so the engine evaluates at the start of each round.
        assert_eq!(seat.ability.timing, crate::combat::TimingWindow::RoundStart);
    }

    #[test]
    fn phase4d_static_condition_does_not_emit_synthetic_seats() {
        // TOS McCoy pattern: passive + attacker_ship_type_is(explorer). Static condition →
        // Phase 4b path → pending_officer_stat_contributions only; the dynamic expansion
        // must NOT emit synthetic seats for static-only conditions.
        let mut effect = lcars_effect_officerstat_tag(
            "officerstatall:unmapped",
            0.20,
            "passive",
            "self",
            Some("attacker_ship_type_is"),
        );
        // attacker_ship_type_is needs the ship_type field populated to compile.
        if let Some(ref mut c) = effect.condition {
            c.ship_type = Some("explorer".to_string());
        }
        let o = LcarsOfficer {
            id: "tos-mccoy".to_string(),
            name: "tos-mccoy".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(LcarsAbility {
                name: "Bones".to_string(),
                effects: vec![effect],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: vec![level_stats(1, 100.0, 100.0, 100.0)],
            max_level_by_rank: vec![1],
        };
        let mut officers = HashMap::new();
        officers.insert("tos-mccoy".to_string(), o);
        let buff =
            resolve_crew_to_buff_set("tos-mccoy", &[], &[], &officers, &ResolveOptions::default());
        let synthetic_count = buff
            .crew
            .seats
            .iter()
            .filter(|s| {
                matches!(
                    s.ability.effect,
                    crate::combat::AbilityEffect::AttackMultiplier(_)
                )
            })
            .count();
        assert_eq!(
            synthetic_count, 0,
            "static-only condition must not trigger dynamic expansion"
        );
        // Static-conditional contribution goes to the pending list instead.
        assert_eq!(
            buff.pending_officer_stat_contributions.len(),
            1,
            "static conditional contribution should land in pending"
        );
    }

    #[test]
    fn officerstat_on_attack_trigger_still_skipped() {
        // Phase 4a opens on_combat_start / on_round_start specifically; other triggers
        // (on_attack, on_round_end, …) are still treated as dynamic and skipped by the
        // static-buffs accumulator. Document via a guard test so we don't over-broaden later.
        let o = officer_with_officerstat_captain(
            "ondefense",
            "officerstatall:unmapped",
            0.10,
            "on_attack",
            "self",
            None,
        );
        let mut officers = HashMap::new();
        officers.insert("ondefense".to_string(), o);
        let buff =
            resolve_crew_to_buff_set("ondefense", &[], &[], &officers, &ResolveOptions::default());
        assert!(!buff.static_buffs.contains_key("officer_stat_all"));
    }

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
        // target defaults to AttackerSelf (the helper sets `target: None`), so
        // shield_mitigation routes to the attacker-side counter-fire channel rather than
        // the additive `ShieldMitigationBonus` channel that the engine consumes against
        // the defender.
        assert!(
            matches!(contexts_shield[0].ability.effect, AbilityEffect::AttackerShieldMitigationBonus(v) if (v - 0.05).abs() < 1e-12)
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
    fn static_passive_applies_officer_stat_scaling_when_stats_present() {
        use crate::data::combat_effect_spec::OfficerStat;
        use crate::lcars::parser::LcarsLevelStats;

        let scaling_effect = LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some("armor".to_string()),
            target: None,
            operator: Some("add".to_string()),
            value: None,
            trigger: Some("passive".to_string()),
            duration: Some(LcarsDuration::Permanent("permanent".to_string())),
            scaling: Some(LcarsScaling {
                base: None,
                per_rank: None,
                max_rank: Some(3),
                base_chance: None,
                values: Some(vec![10.0]),
                chance_values: None,
                officer_stat: Some(OfficerStat::Defense),
            }),
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        };
        let officer = LcarsOfficer {
            id: "stat_scale_officer".to_string(),
            name: "StatScale".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(LcarsAbility {
                name: "passive_armor".to_string(),
                effects: vec![scaling_effect],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: vec![LcarsLevelStats {
                level: 30,
                attack: 0.0,
                defense: 200.0,
                health: 0.0,
            }],
            max_level_by_rank: vec![30],
        };
        let mut officers = HashMap::new();
        officers.insert("stat_scale_officer".to_string(), officer);
        let options = ResolveOptions {
            tier: None,
            officer_tiers: Some(
                [("stat_scale_officer".to_string(), 1u8)]
                    .into_iter()
                    .collect(),
            ),
            officer_levels: None,
        };
        let buff = resolve_crew_to_buff_set("stat_scale_officer", &[], &[], &officers, &options);
        let armor = buff.static_buffs.get("armor").copied().unwrap_or(0.0);
        assert!(
            (armor - 20.0).abs() < 1e-9,
            "expected 10% coeff × defense 200 / 100 = 20, got {armor}"
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
    fn production_on_shield_break_effects_all_resolve_to_target_specific_timing() {
        let path = Path::new("data/officers/officers.lcars.yaml");
        if !path.exists() {
            return; // skip in minimal checkouts (mirrors resolve_bundled_lcars_yaml_*)
        }
        let file = load_lcars_file(path).unwrap();
        let mut total = 0usize;
        let mut self_count = 0usize;
        let mut enemy_count = 0usize;
        let mut unresolved: Vec<String> = Vec::new();
        for officer in &file.officers {
            for ability in [
                officer.captain_ability.as_ref(),
                officer.bridge_ability.as_ref(),
                officer.below_decks_ability.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                for effect in &ability.effects {
                    let trig = effect
                        .trigger
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .to_lowercase();
                    if trig != "on_shield_break" {
                        continue;
                    }
                    total += 1;
                    match effect_trigger_timing(effect) {
                        Some(TimingWindow::SelfShieldBreak) => self_count += 1,
                        Some(TimingWindow::ShieldBreak) => enemy_count += 1,
                        other => unresolved
                            .push(format!("{}::{} → {:?}", officer.id, ability.name, other)),
                    }
                }
            }
        }
        assert!(
            unresolved.is_empty(),
            "on_shield_break effects must all resolve via target field; unresolved: {unresolved:?}"
        );
        assert!(
            total > 0,
            "expected at least one on_shield_break effect in bundled LCARS"
        );
        assert_eq!(
            total,
            self_count + enemy_count,
            "self_count + enemy_count should equal total"
        );
    }
}
