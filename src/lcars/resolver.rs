//! Resolves parsed LCARS abilities into a [BuffSet] (static buffs + crew config for the engine).

use std::collections::{HashMap, HashSet};

use crate::combat::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, Combatant, CrewConfiguration,
    CrewOfficerStatTotals, CrewSeat, CrewSeatContext, TimingWindow,
};
use crate::data::combat_effect_spec::AbilityConditionSpec;
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
    /// Captain + bridge officer A/D/H (subset of [`Self::officer_stat_totals`]); used for
    /// `EnemyBridge`-scoped opponent debuffs.
    pub bridge_officer_stat_totals: CrewOfficerStatTotals,
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
    /// Phase 4d: officer-stat rows whose conditions depend on round state (morale, burning, …).
    /// Evaluated each round via [`crate::data::officer_stat_round::OfficerStatRoundContext`].
    pub dynamic_officer_stat_contributions: Vec<DynamicOfficerStatContribution>,
}

/// Which opponent crewed officers receive a `target: enemy` officer-stat modifier (Phase 4c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OfficerStatOpponentScope {
    /// Every crewed officer on the opponent side (default `target: enemy`).
    #[default]
    AllCrewed,
    /// Captain + bridge slots only (canonical `EnemyBridge`, e.g. Kras "Know Your Enemy").
    BridgeOfficers,
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
    /// debuffs (`target: enemy`). Phase 4b/4a filter these out of the owning crew's self compute;
    /// Phase 4c applies them to the opponent's officer ratings in PvP via
    /// [`crate::data::profile::compute_officer_stat_runtime_bonus`] `opponent_enemy_target_pending`.
    pub target_attacker: bool,
    /// All conditions must evaluate to true at fight setup for the contribution to apply. If
    /// any condition evaluates to `Some(false)` or `None` (undecidable / dynamic), the
    /// contribution is silently dropped.
    pub conditions: Vec<AbilityConditionSpec>,
    /// When [`Self::target_attacker`] is false, limits which opponent officers receive the
    /// debuff. Ignored for `target: self` rows (always crew-wide on the owning side).
    pub opponent_scope: OfficerStatOpponentScope,
}

/// Phase 4d: officer-rating contribution evaluated each combat round when its compiled
/// [`AbilityCondition`] passes (morale-gated Kirk, burning-gated Tyler, …).
#[derive(Debug, Clone)]
pub struct DynamicOfficerStatContribution {
    pub stat_key: String,
    pub value: f64,
    pub target_attacker: bool,
    pub opponent_scope: OfficerStatOpponentScope,
    pub runtime_condition: Option<crate::combat::AbilityCondition>,
    pub timing: TimingWindow,
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
        return crate::lcars::effect_spec_adapter::combat_tag_to_stat_for_effect(effect).is_some();
    }
    false
}

/// Effective combat stat key for `stat_modify` or mapped `tag` effects.
fn effective_stat_for_effect(effect: &LcarsEffect) -> Option<&str> {
    if effect.effect_type == "stat_modify" {
        effect.stat.as_deref().filter(|s| !s.trim().is_empty())
    } else if effect.effect_type == "tag" {
        crate::lcars::effect_spec_adapter::combat_tag_to_stat_for_effect(effect)
    } else {
        None
    }
}

fn is_officer_stat_key(stat: &str) -> bool {
    matches!(
        stat,
        "officer_attack" | "officer_defense" | "officer_health" | "officer_stat_all"
    )
}

fn officer_stat_trigger_ok(trigger_str: &str, officer_stat_key: bool) -> bool {
    trigger_str == "passive"
        || (officer_stat_key && matches!(trigger_str, "on_combat_start" | "on_round_start"))
}

fn officer_stat_duration_ok(effect: &LcarsEffect, officer_stat_key: bool) -> bool {
    effect
        .duration
        .as_ref()
        .map(|d| d.is_permanent())
        .unwrap_or(officer_stat_key)
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
        if let Some(pathway) = officer_rating_tag_coverage_path(effect, officer_id, options) {
            return LcarsEffectCoverage {
                tier: MechanicCoverageTier::Implemented,
                pathway,
            };
        }
        if let Some(pathway) = accuracy_combat_begin_coverage_path(effect, officer_id, options) {
            return LcarsEffectCoverage {
                tier: MechanicCoverageTier::Implemented,
                pathway,
            };
        }
        if let Some(pathway) = random_defender_state_coverage_path(effect, officer_id, options) {
            return LcarsEffectCoverage {
                tier: MechanicCoverageTier::Implemented,
                pathway,
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

/// Phase 4d dynamic officer-stat path (e.g. Kirk `morale_active`); mirrors
/// [`expand_dynamic_officer_stat_effects`].
fn officer_stat_dynamic_coverage_path(effect: &LcarsEffect) -> Option<&'static str> {
    if effect.effect_type != "tag" {
        return None;
    }
    let stat = effective_stat_for_effect(effect)?;
    if !is_officer_stat_key(stat) {
        return None;
    }
    let cond_obj = effect.condition.as_ref()?;
    let cond_spec = crate::lcars::effect_spec_adapter::lcars_condition_to_spec(cond_obj).ok()?;
    if !condition_is_dynamic(&cond_spec) {
        return None;
    }
    if effect
        .target
        .as_deref()
        .map(|t| t.trim().to_ascii_lowercase())
        .as_deref()
        == Some("enemy")
    {
        return None;
    }
    match stat {
        "officer_attack" | "officer_defense" | "officer_health" | "officer_stat_all" => {
            Some("officer_stat_dynamic_all_axes")
        }
        _ => None,
    }
}

/// Officer-rating tag pathways used by [`resolve_crew_to_buff_set`] static / pending loops.
fn officer_rating_tag_coverage_path(
    effect: &LcarsEffect,
    officer_id: &str,
    options: &ResolveOptions,
) -> Option<String> {
    if effect.effect_type != "tag" {
        return None;
    }
    if let Some(path) = officer_stat_dynamic_coverage_path(effect) {
        return Some(path.to_string());
    }
    let stat = effective_stat_for_effect(effect)?;
    if !is_officer_stat_key(stat) {
        return None;
    }
    if stat.eq_ignore_ascii_case("accuracy") {
        return None;
    }
    let trigger_str = effect.trigger.as_deref().map(str::trim).unwrap_or("");
    if !officer_stat_trigger_ok(trigger_str, true) || !officer_stat_duration_ok(effect, true) {
        return None;
    }
    let tier = options.tier_for(officer_id);
    let spec = crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec(
        effect,
        "coverage:officer_stat",
        officer_id,
        "coverage",
        tier,
        None,
    )?;
    spec.value.as_ref().and_then(|v| v.scalar)?;
    let target_attacker = matches!(
        spec.target,
        crate::data::combat_effect_spec::AbilityTargetSpec::AttackerSelf
            | crate::data::combat_effect_spec::AbilityTargetSpec::SelfShip
    );
    if !spec.conditions.is_empty() || !target_attacker {
        return Some("officer_stat_pending".to_string());
    }
    Some("officer_stat_static_buff".to_string())
}

/// Combat-begin accuracy from mapped tags (Kang bridge pattern).
fn accuracy_combat_begin_coverage_path(
    effect: &LcarsEffect,
    officer_id: &str,
    options: &ResolveOptions,
) -> Option<String> {
    let is_accuracy =
        effective_stat_for_effect(effect).is_some_and(|s| s.eq_ignore_ascii_case("accuracy"));
    if !is_accuracy {
        return None;
    }
    if effect_trigger_timing(effect) != Some(TimingWindow::CombatBegin) {
        return None;
    }
    let tier = options.tier_for(officer_id);
    crate::lcars::effect_spec_adapter::lcars_effect_resolved_value(effect, tier, None)?;
    Some("accuracy_combat_begin".to_string())
}

/// Round-start weighted defender states from mapped `addrandomstate` tags (T'Ana / Zeph).
fn random_defender_state_coverage_path(
    effect: &LcarsEffect,
    officer_id: &str,
    options: &ResolveOptions,
) -> Option<String> {
    let is_random_state =
        effective_stat_for_effect(effect).is_some_and(|s| s == "random_defender_state");
    if !is_random_state {
        return None;
    }
    if effect_trigger_timing(effect) != Some(TimingWindow::RoundStart) {
        return None;
    }
    let tier = options.tier_for(officer_id);
    let spec = crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec(
        effect,
        "coverage:random_defender_state",
        officer_id,
        "coverage",
        tier,
        None,
    )?;
    crate::combat::effect_spec_compile::compile_officer_combat_spec(&spec).ok()?;
    Some("random_defender_state_round_start".to_string())
}

/// Phase 4d: collect officer-stat effects with dynamic conditions for per-round breakpoint
/// evaluation in the combat loop ([`crate::data::officer_stat_round`]).
///
/// All three axes (Attack / Defense / Health) flow through the proper §2 breakpoint pipeline
/// when their compiled runtime condition passes. Static-conditional contributions still use
/// [`PendingOfficerStatContribution`] + fight-setup evaluation.
///
/// `target: enemy` contributions are skipped here (Phase 4c applies them on the opponent side in PvP).
fn collect_dynamic_officer_stat_contributions(
    officer: &LcarsOfficer,
    ability: &LcarsAbility,
    options: &ResolveOptions,
    effect_idx_base: usize,
) -> Vec<DynamicOfficerStatContribution> {
    let mut out = Vec::new();
    let officer_tier = options.tier_for(&officer.id);
    let stats_row = options
        .level_for(&officer.id)
        .and_then(|lvl| officer.stats_at_level(lvl));
    for (idx, effect) in ability.effects.iter().enumerate() {
        if effect.effect_type != "tag" && effect.effect_type != "stat_modify" {
            continue;
        }
        let Some(stat) = effective_stat_for_effect(effect) else {
            continue;
        };
        if !is_officer_stat_key(stat) {
            continue;
        }
        let Some(ref cond_obj) = effect.condition else {
            continue;
        };
        let Ok(cond_spec) = crate::lcars::effect_spec_adapter::lcars_condition_to_spec(cond_obj)
        else {
            continue;
        };
        if !condition_is_dynamic(&cond_spec) {
            continue;
        }
        if effect
            .target
            .as_deref()
            .map(|t| t.trim().to_ascii_lowercase())
            .as_deref()
            == Some("enemy")
        {
            continue;
        }
        let stable_id = format!(
            "lcars:{}:{}:dynamic:{}{}",
            officer.id, ability.name, effect_idx_base, idx
        );
        let Some(spec) = crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec(
            effect,
            &stable_id,
            &officer.id,
            &ability.name,
            officer_tier,
            stats_row,
        ) else {
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
        let opponent_scope = match spec.target {
            crate::data::combat_effect_spec::AbilityTargetSpec::EnemyBridgeOfficers => {
                OfficerStatOpponentScope::BridgeOfficers
            }
            _ => OfficerStatOpponentScope::AllCrewed,
        };
        let timing = match crate::combat::effect_spec_compile::compile_trigger(spec.trigger) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let Ok(compiled) =
            crate::combat::effect_spec_compile::compile_conditions_and(&spec.conditions)
        else {
            continue;
        };
        let runtime_condition =
            crate::combat::effect_spec_compile::merge_duration_round_condition(compiled, &spec);
        out.push(DynamicOfficerStatContribution {
            stat_key: stat.to_string(),
            value: v,
            target_attacker,
            opponent_scope,
            runtime_condition,
            timing,
        });
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
    let mut dynamic_officer_stat_contributions: Vec<DynamicOfficerStatContribution> = Vec::new();
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
            let Some(stat) = effective_stat_for_effect(effect) else {
                continue;
            };
            // Phase 4a: officer-rating keys accept on_combat_start / on_round_start in addition
            // to passive+permanent (see docs/OFFICER_STAT_FORMULA.md §3).
            let officer_stat_key = is_officer_stat_key(stat);
            let trigger_str = effect.trigger.as_deref().map(str::trim).unwrap_or("");
            if !officer_stat_trigger_ok(trigger_str, officer_stat_key)
                || !officer_stat_duration_ok(effect, officer_stat_key)
            {
                continue;
            }
            if stat.eq_ignore_ascii_case("accuracy") {
                // Folded into `accuracy` / `accuracy_cb_mult` in the combat-begin accuracy loop below.
                continue;
            }

            // Route passive-permanent stat_modify/mapped-tag through the canonical CombatEffectSpec IR.
            let stable_id = format!("lcars:{}:{}:static:{effect_idx}", officer.id, ability.name);
            let Some(spec) = crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec(
                effect,
                &stable_id,
                &officer.id,
                &ability.name,
                officer_tier,
                stats_row,
            ) else {
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
            let opponent_scope = match spec.target {
                crate::data::combat_effect_spec::AbilityTargetSpec::EnemyBridgeOfficers => {
                    OfficerStatOpponentScope::BridgeOfficers
                }
                _ => OfficerStatOpponentScope::AllCrewed,
            };

            // Phase 4b: officer-stat effects with conditions go into pending_contributions for
            // fight-setup-time evaluation (TOS McCoy attacker_ship_type_is, Dezoc engagement gate,
            // Strike Team Una composite, Kras defender_is_player_ship). Effects with no
            // conditions and target:self take the simpler static_buffs path.
            //
            // target:enemy debuffs still skip the attacker's static_buffs path (Phase 4a target
            // filter); the pending list preserves them with target_attacker=false so a future
            // Phase 4c can route them through PvP defender-side compute.
            if !spec.conditions.is_empty() {
                // Dynamic (per-round) conditions — morale / burning / hull-breach / round-range /
                // stat thresholds — are collected for Phase 4d per-round breakpoint evaluation;
                // keeping them in pending would double-count. Only static fight-setup conditions
                // (faction, ship class, engagement, defender_is_player_ship, …) belong in pending.
                if spec.conditions.iter().any(condition_is_dynamic) {
                    continue;
                }
                pending_officer_stat_contributions.push(PendingOfficerStatContribution {
                    stat_key: stat.to_string(),
                    value: v,
                    target_attacker,
                    conditions: spec.conditions.clone(),
                    opponent_scope,
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
                    opponent_scope,
                });
                continue;
            }

            if let Some(contrib) =
                crate::combat::effect_spec_compile::compile_officer_static_buff(&spec, stat)
            {
                contrib.apply(&mut static_buffs);
            }
        }
        // Combat-begin `stat_modify` / mapped-tag accuracy: stacks into pre-mitigation attacker
        // stats (scenario), not a crew seat. Multiplicative entries use key `accuracy_cb_mult`.
        for effect in &ability.effects {
            let is_accuracy = effective_stat_for_effect(effect)
                .is_some_and(|s| s.eq_ignore_ascii_case("accuracy"));
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
            let op = crate::combat::effect_spec_compile::static_buff_op_from_lcars_op(
                &normalize_operator(effect.operator.as_deref()),
            );
            let stat_key = match op {
                crate::combat::effect_spec_compile::StaticBuffOp::Multiply => "accuracy_cb_mult",
                crate::combat::effect_spec_compile::StaticBuffOp::Add => "accuracy",
            };
            crate::combat::effect_spec_compile::StaticBuffContribution {
                stat_key: stat_key.to_string(),
                op,
                value,
            }
            .apply(&mut static_buffs);
        }
        let contexts =
            resolve_officer_ability(officer, ability, seat, class, options, contribution_batch);
        seats.extend(contexts);
        dynamic_officer_stat_contributions.extend(collect_dynamic_officer_stat_contributions(
            officer,
            ability,
            options,
            contribution_batch as usize,
        ));
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
        let stats_row = officer
            .resolve_level(options.level_for(&officer.id), officer_tier)
            .and_then(|lvl| officer.stats_at_level(lvl));
        for (idx, effect) in ability.effects.iter().enumerate() {
            if effect.effect_type != "extra_attack" {
                continue;
            }
            let stable_id = format!("lcars:{}:{}:proc:{idx}", officer.id, ability.name);
            let Some(spec) = crate::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec(
                effect,
                &stable_id,
                &officer.id,
                &ability.name,
                officer_tier,
                stats_row,
            ) else {
                continue;
            };
            let Some(contrib) =
                crate::combat::effect_spec_compile::compile_officer_buffset_proc(&spec)
            else {
                continue;
            };
            if contrib.chance > proc_chance
                || (contrib.chance == proc_chance && contrib.multiplier > proc_multiplier)
            {
                proc_chance = contrib.chance;
                proc_multiplier = contrib.multiplier;
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
    let mut bridge_officer_stat_totals = CrewOfficerStatTotals::default();
    let mut counted_for_totals: HashSet<String> = HashSet::new();
    let mut counted_for_bridge: HashSet<String> = HashSet::new();
    let mut add_officer_stats = |officer: &LcarsOfficer, bridge_only: bool| {
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
        if bridge_only && counted_for_bridge.insert(officer.id.clone()) {
            bridge_officer_stat_totals.attack += stats.attack;
            bridge_officer_stat_totals.defense += stats.defense;
            bridge_officer_stat_totals.health += stats.health;
        }
    };
    if let Some(o) = officers.get(captain_id) {
        add_officer_stats(o, true);
    }
    for id in bridge {
        if let Some(o) = officers.get(id.as_str()) {
            add_officer_stats(o, true);
        }
    }
    for id in below_decks {
        if let Some(o) = officers.get(id.as_str()) {
            add_officer_stats(o, false);
        }
    }

    BuffSet {
        static_buffs,
        crew: CrewConfiguration { seats },
        proc_chance,
        proc_multiplier,
        officer_stat_totals,
        bridge_officer_stat_totals,
        pending_officer_stat_contributions,
        dynamic_officer_stat_contributions,
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
        LcarsAbility, LcarsDuration, LcarsEffect, LcarsOfficer, LcarsScaling,
    };

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
        assert_eq!(buff.bridge_officer_stat_totals.attack, 1704.0 + 90206.0);
        assert_eq!(buff.bridge_officer_stat_totals.defense, 4320.0 + 92060.0);
        assert_eq!(buff.bridge_officer_stat_totals.health, 824.0 + 90091.0);
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
    fn officerstat_enemy_bridge_pending_scope_is_bridge_officers_only() {
        use super::OfficerStatOpponentScope;
        let o = officer_with_officerstat_captain(
            "kras",
            "officerstatall:unmapped",
            0.20,
            "on_combat_start",
            "enemy_bridge",
            Some("defender_is_player_ship"),
        );
        let mut officers = HashMap::new();
        officers.insert("kras".to_string(), o);
        let buff =
            resolve_crew_to_buff_set("kras", &[], &[], &officers, &ResolveOptions::default());
        assert_eq!(buff.pending_officer_stat_contributions.len(), 1);
        let c = &buff.pending_officer_stat_contributions[0];
        assert!(!c.target_attacker);
        assert_eq!(c.opponent_scope, OfficerStatOpponentScope::BridgeOfficers);
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
    fn phase4d_dynamic_morale_active_emits_dynamic_contribution() {
        // Kirk-1323b6 pattern: officerstatall on_round_start morale_active target:self val=0.4.
        // Phase 4d: dynamic rows feed the per-round breakpoint path (all three axes).
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
        assert_eq!(buff.dynamic_officer_stat_contributions.len(), 1);
        let row = &buff.dynamic_officer_stat_contributions[0];
        assert_eq!(row.stat_key, "officer_stat_all");
        assert!((row.value - 0.40).abs() < 1e-9);
        assert_eq!(row.timing, TimingWindow::RoundStart);
        assert!(
            row.runtime_condition
                .as_ref()
                .is_some_and(|c| matches!(c, AbilityCondition::MoraleActive)),
            "expected MoraleActive condition; got: {:?}",
            row.runtime_condition
        );
        assert!(
            buff.crew
                .seats
                .iter()
                .all(|s| { !matches!(s.ability.effect, AbilityEffect::AttackMultiplier(_)) }),
            "must not emit synthetic AttackMultiplier seats"
        );
    }

    #[test]
    fn phase4d_production_kirk_resolves_dynamic_without_pending_duplicate() {
        let Ok(file) = super::super::build_officer_model_file_default() else {
            return;
        };
        let officers = index_lcars_officers_by_id(file.officers);
        let buff = resolve_crew_to_buff_set(
            "kirk-1323b6",
            &[],
            &[],
            &officers,
            &ResolveOptions {
                tier: Some(1),
                ..ResolveOptions::default()
            },
        );
        assert_eq!(
            buff.dynamic_officer_stat_contributions.len(),
            1,
            "production Kirk should emit one Leader dynamic row"
        );
        assert!(
            buff.pending_officer_stat_contributions.is_empty(),
            "dynamic officer-stat must not also land in pending_officer_stat_contributions"
        );
    }

    #[test]
    fn phase4d_static_condition_does_not_emit_dynamic_rows() {
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

    fn assert_coverage_implemented(effect: &LcarsEffect, officer_id: &str, pathway: &str) {
        let cov = lcars_effect_coverage(effect, officer_id, &ResolveOptions::default());
        assert_eq!(cov.tier, MechanicCoverageTier::Implemented, "{:?}", cov);
        assert_eq!(cov.pathway, pathway);
    }

    fn assert_coverage_partial(effect: &LcarsEffect, officer_id: &str) {
        let cov = lcars_effect_coverage(effect, officer_id, &ResolveOptions::default());
        assert_eq!(cov.tier, MechanicCoverageTier::Partial, "{:?}", cov);
    }

    #[test]
    fn officer_rating_tag_coverage_kumak_static_buff() {
        let eff = lcars_effect_officerstat_tag(
            "officerstatall:unmapped",
            0.05,
            "on_round_start",
            "self",
            None,
        );
        assert_coverage_implemented(&eff, "kumak-c5b0db", "officer_stat_static_buff");
    }

    #[test]
    fn officer_rating_tag_coverage_dezoc_pending() {
        let mut eff = lcars_effect_officerstat_tag(
            "officerstatall:unmapped",
            0.10,
            "on_combat_start",
            "self",
            Some("and"),
        );
        if let Some(ref mut c) = eff.condition {
            c.conditions = Some(vec![
                crate::lcars::LcarsCondition {
                    condition_type: "engagement_includes".to_string(),
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
                    enemy_type: Some("solo_armadas".to_string()),
                    battle_types: None,
                    conditions: None,
                },
                crate::lcars::LcarsCondition {
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
                    faction_id: Some(2943562711),
                    ship_id: None,
                    enemy_type: None,
                    battle_types: None,
                    conditions: None,
                },
            ]);
        }
        assert_coverage_implemented(&eff, "dezoc-381416", "officer_stat_pending");
    }

    #[test]
    fn officer_rating_tag_coverage_kras_enemy_bridge_pending() {
        let eff = lcars_effect_officerstat_tag(
            "officerstatall:unmapped",
            0.20,
            "on_combat_start",
            "enemy_bridge",
            Some("defender_is_player_ship"),
        );
        assert_coverage_implemented(&eff, "kras-a47042", "officer_stat_pending");
    }

    #[test]
    fn officer_rating_tag_coverage_kirk_dynamic_attack_axis() {
        let mut eff = lcars_effect_officerstat_tag(
            "officerstatall:unmapped",
            0.40,
            "on_round_start",
            "self",
            Some("morale_active"),
        );
        eff.duration = Some(crate::lcars::LcarsDuration::Rounds { rounds: 1 });
        assert_coverage_implemented(&eff, "kirk-1323b6", "officer_stat_dynamic_all_axes");
    }

    #[test]
    fn officer_rating_tag_coverage_on_attack_stays_partial() {
        let eff = lcars_effect_officerstat_tag(
            "officerstatall:unmapped",
            0.10,
            "on_attack",
            "self",
            None,
        );
        assert_coverage_partial(&eff, "ondefense");
    }

    #[test]
    fn accuracy_combat_begin_tag_coverage_kang_pattern() {
        let eff =
            lcars_effect_officerstat_tag("accuracy:unmapped", 1.0, "on_combat_start", "self", None);
        assert_coverage_implemented(&eff, "kang-55e67a", "accuracy_combat_begin");
    }

    #[test]
    fn random_defender_state_tag_coverage_tana_pattern() {
        let mut eff = lcars_effect_officerstat_tag(
            "addrandomstate:unmapped",
            1.0,
            "on_round_start",
            "enemy",
            Some("defender_is_player_ship"),
        );
        eff.duration = Some(crate::lcars::LcarsDuration::Rounds { rounds: 3 });
        eff.scaling = Some(crate::lcars::LcarsScaling {
            base: None,
            per_rank: None,
            max_rank: Some(5),
            base_chance: None,
            values: Some(vec![8.0, 4.0, 2.0]),
            chance_values: Some(vec![0.4, 0.45, 0.55, 0.75, 1.0]),
            officer_stat: None,
        });
        assert_coverage_implemented(
            &eff,
            "doctor-t-ana-b98f82",
            "random_defender_state_round_start",
        );
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
        let Ok(file) = super::super::build_officer_model_file_default() else {
            return; // skip if source data not present (e.g. in minimal checkouts)
        };
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
        let Ok(file) = super::super::build_officer_model_file_default() else {
            return; // skip in minimal checkouts (mirrors resolve_bundled_lcars_yaml_*)
        };
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
