//! Combat loop orchestration. Types, mitigation, effects, events, and damage helpers live in sibling modules.

pub use crate::combat::events::serialize_events_json;
pub use crate::combat::mitigation::{
    apply_morale_primary_piercing, component_mitigation, isolytic_damage, mitigation,
    mitigation_breakdown, mitigation_for_hostile, mitigation_with_morale, mitigation_with_mystery,
    pierce_damage_through_bonus, MITIGATION_CEILING, MITIGATION_FLOOR, PIERCE_CAP,
};
pub use crate::combat::types::{
    effective_shots_for_weapon, round_half_even, AttackerStats, CombatEvent, Combatant,
    CrewOfficerStatTotals, DefenderStats, EnemyTypes, EventSource, HostileMitigationParams,
    OpponentFactionTag, ShipType, SimulationConfig, SimulationResult, TraceCollector, TraceMode,
    WeaponStats, BATTLESHIP_COEFFICIENTS, EPSILON, EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS,
    MAX_COMBAT_ROUNDS, MORALE_PRIMARY_PIERCING_BONUS, SURVEY_COEFFICIENTS,
};

use serde_json::{Map, Value};

use crate::combat::abilities::{
    active_effects_for_timing, apply_duplicate_officer_policy,
    attacker_crew_tal_assigned_captain_or_bridge, defender_shield_drain_per_round_from_crew,
    filter_effects_by_condition, hostile_counter_stat_debuff_from_crew,
    hostile_crit_damage_reduction_active_at_round,
    opponent_captain_maneuver_multiplier_from_effects, scale_crew_captain_maneuver_effects,
    sum_accuracy_bonus, sum_breach_cumulative_crit_chance_per_hit,
    sum_breach_cumulative_crit_damage_per_crit, sum_dodge_bonus,
    sum_hostile_engagement_defensive_bonus, sum_mitigation_additive, AbilityEffect,
    ActiveAbilityEffect, CombatContext, CrewConfiguration, TimingWindow,
};
use crate::combat::condition::round_in_inclusive_first_n;
use crate::combat::conqueror_borg_beams::{
    effective_conqueror_borg_beam_suppression, hyperthermic_resonance_beam_instant_loss,
    quantum_resonance_beam_instant_loss,
};
use crate::combat::crit::resolve_vehicle_weapon_crit;
use crate::combat::damage::{
    apply_shield_hull_split, compute_apex_damage_factor, compute_damage_through_factor,
    compute_isolytic_taken,
};
use crate::combat::effect_accumulator::{
    record_ability_activations, scale_effect, sum_on_kill_hull_regen, EffectAccumulator,
};
use crate::combat::events::round_f64;
use crate::combat::evolutionary_assimilation::evolutionary_assimilation_instant_loss;
use crate::combat::proc::{accumulate_proc_attack_effects, roll_weapon_intrinsic_proc};
use crate::combat::rng::Rng;
use crate::combat::simd_damage_kernel::{avx2_supported, compute_damage_after_apex_batch};
use crate::combat::snapshot::{
    build_combat_state_snapshot, state_snapshot_as_combat_event, SnapshotAnchor,
};
use crate::combat::types::BURNING_HULL_DAMAGE_PER_ROUND;

#[allow(clippy::too_many_arguments)]
fn apply_defender_fire_delay(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    round_index: u32,
    phase: &str,
    attacker_id: &str,
    ability_name: &str,
    chance: f64,
    delay_rounds: u32,
    requires_critical: bool,
    is_crit: bool,
    defender_weapon_fire_delayed_rounds: &mut u32,
) {
    if requires_critical && !is_crit {
        return;
    }
    let roll = (rng.next_u64() as f64) / (u64::MAX as f64);
    let triggered = roll < chance.clamp(0.0, 1.0);
    if triggered {
        *defender_weapon_fire_delayed_rounds =
            (*defender_weapon_fire_delayed_rounds).max(delay_rounds.max(1));
    }
    trace.record_if(|| CombatEvent {
        event_type: "defender_fire_delay_trigger".to_string(),
        round_index,
        phase: phase.to_string(),
        source: EventSource {
            officer_id: Some(attacker_id.to_string()),
            ship_ability_id: Some(ability_name.to_string()),
            ..EventSource::default()
        },
        weapon_index: None,
        values: Map::from_iter([
            ("roll".to_string(), Value::from(round_f64(roll))),
            ("triggered".to_string(), Value::Bool(triggered)),
            ("chance".to_string(), Value::from(round_f64(chance))),
            ("delay_rounds".to_string(), Value::from(delay_rounds)),
            (
                "requires_critical".to_string(),
                Value::Bool(requires_critical),
            ),
        ]),
    });
}

#[inline]
fn effective_incoming_shield_mitigation(
    base_sm: f64,
    config: &SimulationConfig,
    round_index: u32,
    attacker_self_bonus: f64,
) -> f64 {
    let extra = if config.incoming_shield_mitigation_bonus_rounds > 0
        && round_index > 0
        && round_index <= config.incoming_shield_mitigation_bonus_rounds
        && config.incoming_shield_mitigation_bonus.is_finite()
    {
        config.incoming_shield_mitigation_bonus
    } else {
        0.0
    };
    // attacker_self_bonus comes from `AttackerShieldMitigationBonus` accumulator: target=SelfShip
    // officer effects (e.g. shieldmitigation tag with target=self) that buff the attacker's
    // own mitigation when they take counter-fire. Folds in alongside the config-driven
    // `incoming_shield_mitigation_bonus`; final clamp keeps the total in [0, 1].
    (base_sm + extra + attacker_self_bonus).clamp(0.0, 1.0)
}

/// Immutable combat setup precomputed once per crew, reused across multiple trials with different seeds.
/// Avoids repeated `active_effects_for_timing` calls and crew resolution in Monte Carlo batch workloads.
pub struct PreCombatSetup {
    pub attacker: Combatant,
    pub defender: Combatant,
    pub config: SimulationConfig,
    pub attacker_crew: CrewConfiguration,
    pub defender_crew: CrewConfiguration,
    pub defender_faction: OpponentFactionTag,
    pub defender_ship_type: ShipType,
    pub attacker_ship_type: ShipType,
    pub defender_is_npc_hostile: bool,
    pub defender_is_player_ship: bool,
    pub attacker_tal_assigned_captain_or_bridge: bool,
    // Precomputed attacker effects by timing window
    pub combat_begin_effects: Vec<ActiveAbilityEffect>,
    pub round_start_effects: Vec<ActiveAbilityEffect>,
    pub attack_phase_effects: Vec<ActiveAbilityEffect>,
    pub defense_phase_effects: Vec<ActiveAbilityEffect>,
    pub round_end_effects: Vec<ActiveAbilityEffect>,
    pub after_subround_effects: Vec<ActiveAbilityEffect>,
    pub shield_break_effects: Vec<ActiveAbilityEffect>,
    pub self_shield_break_effects: Vec<ActiveAbilityEffect>,
    pub kill_effects: Vec<ActiveAbilityEffect>,
    pub hull_breach_effects: Vec<ActiveAbilityEffect>,
    pub receive_damage_effects: Vec<ActiveAbilityEffect>,
    pub combat_end_effects: Vec<ActiveAbilityEffect>,
    // Precomputed defender effects by timing window
    pub defender_combat_begin_effects: Vec<ActiveAbilityEffect>,
    pub defender_round_start_effects: Vec<ActiveAbilityEffect>,
    pub defender_attack_phase_effects: Vec<ActiveAbilityEffect>,
    pub defender_defense_phase_effects: Vec<ActiveAbilityEffect>,
    pub defender_shield_break_effects: Vec<ActiveAbilityEffect>,
    pub defender_round_end_effects: Vec<ActiveAbilityEffect>,
    /// Defender-owned [`TimingWindow::ReceiveDamage`] effects evaluated when the defender takes
    /// hull damage from outbound (attacker) weapon fire.
    pub defender_receive_damage_effects: Vec<ActiveAbilityEffect>,
    // Combat begin state (initial round 0, all precomputable)
    pub combat_begin_ctx: CombatContext,
    pub combat_begin_filtered: Vec<ActiveAbilityEffect>,
    pub attacker_mitigation_additive: f64,
    pub attacker_accuracy_bonus: f64,
    pub attacker_dodge_bonus: f64,
    pub conqueror_borg_beam_suppression: bool,
    pub effective_conqueror_borg_beam_suppression: bool,
    pub quantum_beam_instant_loss: bool,
    pub evo_assim_instant_loss: bool,
    /// Pre-allocated Arc for attacker ship id slug — avoids String clone per construction.
    pub attacker_ship_id_arc: std::sync::Arc<str>,
    /// Pre-allocated Arc for engagement enemy types — avoids EnemyTypes clone per construction.
    pub engagement_enemy_types_arc: std::sync::Arc<EnemyTypes>,
}

impl PreCombatSetup {
    /// Whether the fight ends instantly due to conqueror borg / evolutionary assimilation mechanics.
    pub fn would_end_instantly(&self) -> bool {
        self.quantum_beam_instant_loss || self.evo_assim_instant_loss
    }
}

/// Build a [`PreCombatSetup`] from the same inputs as [`simulate_combat_with_defender_faction_and_defender_crew`].
/// This precomputes all effects filtering and immutable context — useful for batch trials.
#[allow(clippy::too_many_arguments)]
pub fn build_combat_setup(
    attacker: &Combatant,
    defender: &Combatant,
    config: &SimulationConfig,
    attacker_crew: &CrewConfiguration,
    defender_faction: OpponentFactionTag,
    defender_ship_type: ShipType,
    attacker_ship_type: ShipType,
    defender_is_npc_hostile: bool,
    defender_is_player_ship: bool,
    defender_crew: &CrewConfiguration,
) -> PreCombatSetup {
    let mut attacker_crew = apply_duplicate_officer_policy(attacker_crew);
    let mut defender_crew = apply_duplicate_officer_policy(defender_crew);
    let attacker_tal_assigned_captain_or_bridge =
        attacker_crew_tal_assigned_captain_or_bridge(&attacker_crew);

    let attacker_ship_id_arc: std::sync::Arc<str> = attacker.id.clone().into();
    let engagement_enemy_types_arc: std::sync::Arc<EnemyTypes> =
        std::sync::Arc::new(config.engagement_enemy_types.clone());

    let combat_begin_pre_scale =
        active_effects_for_timing(&attacker_crew, TimingWindow::CombatBegin);
    // RoundRange gates on finite-duration combat-begin effects use min: 1 (see Harrison Sabotage).
    let combat_begin_ctx = CombatContext {
        round_index: 1,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: 1.0,
        attacker_shield_pct: 1.0,
        attacker_morale_active: false,
        defender_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction,
        attacker_owner_faction: config.attacker_owner_faction,
        defender_hull_faction_id: config.defender_hull_faction_id,
        defender_ship_type,
        attacker_ship_type,
        attacker_ship_id: std::sync::Arc::clone(&attacker_ship_id_arc),
        defender_is_npc_hostile,
        defender_is_player_ship,
        attacker_tal_assigned_captain_or_bridge,
        defender_hostile_tag_mask: config.defender_hostile_tag_mask,
        engagement_enemy_types: std::sync::Arc::clone(&engagement_enemy_types_arc),
        combat_battle_type_id: None,
        defender_level: config.defender_level,
    };
    let combat_begin_pre_scale_filtered =
        filter_effects_by_condition(&combat_begin_pre_scale, &combat_begin_ctx);
    let bridge_ability_effectiveness_add =
        crate::combat::abilities::sum_bridge_ability_effectiveness_add(
            &combat_begin_pre_scale_filtered,
        );
    crate::combat::abilities::scale_crew_bridge_ability_effects(
        &mut attacker_crew,
        bridge_ability_effectiveness_add,
    );
    // Re-read combat-begin seats after Pike-style bridge scaling so bypass fractions match crew.
    let combat_begin_effects = active_effects_for_timing(&attacker_crew, TimingWindow::CombatBegin);
    let combat_begin_filtered =
        filter_effects_by_condition(&combat_begin_effects, &combat_begin_ctx);
    let conqueror_borg_beam_suppression = combat_begin_filtered
        .iter()
        .any(|e| matches!(e.effect, AbilityEffect::ConquerorBorgBeamSuppression));
    let effective_conqueror_borg_beam_suppression =
        effective_conqueror_borg_beam_suppression(conqueror_borg_beam_suppression, &attacker.id);
    let quantum_beam_instant_loss = quantum_resonance_beam_instant_loss(
        defender_is_npc_hostile,
        config.defender_hostile_tag_mask,
        effective_conqueror_borg_beam_suppression,
        &attacker.id,
    );
    let evo_assim_instant_loss = evolutionary_assimilation_instant_loss(
        defender_is_npc_hostile,
        config.defender_hostile_tag_mask,
        conqueror_borg_beam_suppression,
        &config.attacker_roster_officer_ids,
    );

    let hostile_engagement_defensive =
        sum_hostile_engagement_defensive_bonus(&combat_begin_filtered);
    let attacker_mitigation_additive =
        sum_mitigation_additive(&combat_begin_filtered) + hostile_engagement_defensive;
    let attacker_accuracy_bonus = sum_accuracy_bonus(&combat_begin_filtered);
    let attacker_dodge_bonus =
        sum_dodge_bonus(&combat_begin_filtered) + hostile_engagement_defensive;
    let opponent_captain_maneuver_multiplier =
        opponent_captain_maneuver_multiplier_from_effects(&combat_begin_filtered);
    scale_crew_captain_maneuver_effects(&mut defender_crew, opponent_captain_maneuver_multiplier);

    // Precompute all effect vectors before moving crews into the struct
    let round_start_effects = active_effects_for_timing(&attacker_crew, TimingWindow::RoundStart);
    let attack_phase_effects = active_effects_for_timing(&attacker_crew, TimingWindow::AttackPhase);
    let defense_phase_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::DefensePhase);
    let round_end_effects = active_effects_for_timing(&attacker_crew, TimingWindow::RoundEnd);
    let after_subround_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::AfterSubround);
    let shield_break_effects = active_effects_for_timing(&attacker_crew, TimingWindow::ShieldBreak);
    let self_shield_break_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::SelfShieldBreak);
    let kill_effects = active_effects_for_timing(&attacker_crew, TimingWindow::Kill);
    let hull_breach_effects = active_effects_for_timing(&attacker_crew, TimingWindow::HullBreach);
    let receive_damage_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::ReceiveDamage);
    let combat_end_effects = active_effects_for_timing(&attacker_crew, TimingWindow::CombatEnd);

    let defender_combat_begin_effects =
        active_effects_for_timing(&defender_crew, TimingWindow::CombatBegin);
    let defender_round_start_effects =
        active_effects_for_timing(&defender_crew, TimingWindow::RoundStart);
    let defender_attack_phase_effects =
        active_effects_for_timing(&defender_crew, TimingWindow::AttackPhase);
    let defender_defense_phase_effects =
        active_effects_for_timing(&defender_crew, TimingWindow::DefensePhase);
    let defender_shield_break_effects =
        active_effects_for_timing(&defender_crew, TimingWindow::ShieldBreak);
    let defender_round_end_effects =
        active_effects_for_timing(&defender_crew, TimingWindow::RoundEnd);
    let defender_receive_damage_effects =
        active_effects_for_timing(&defender_crew, TimingWindow::ReceiveDamage);

    PreCombatSetup {
        attacker: attacker.clone(),
        defender: defender.clone(),
        config: config.clone(),
        attacker_crew,
        defender_crew,
        defender_faction,
        defender_ship_type,
        attacker_ship_type,
        defender_is_npc_hostile,
        defender_is_player_ship,
        attacker_tal_assigned_captain_or_bridge,
        combat_begin_effects,
        combat_begin_ctx,
        combat_begin_filtered,
        attacker_mitigation_additive,
        attacker_accuracy_bonus,
        attacker_dodge_bonus,
        conqueror_borg_beam_suppression,
        effective_conqueror_borg_beam_suppression,
        quantum_beam_instant_loss,
        evo_assim_instant_loss,
        round_start_effects,
        attack_phase_effects,
        defense_phase_effects,
        round_end_effects,
        after_subround_effects,
        shield_break_effects,
        self_shield_break_effects,
        kill_effects,
        hull_breach_effects,
        receive_damage_effects,
        combat_end_effects,
        defender_combat_begin_effects,
        defender_round_start_effects,
        defender_attack_phase_effects,
        defender_defense_phase_effects,
        defender_shield_break_effects,
        defender_round_end_effects,
        defender_receive_damage_effects,
        attacker_ship_id_arc,
        engagement_enemy_types_arc,
    }
}

/// Run a single combat trial from a precomputed [`PreCombatSetup`] with a specific seed.
/// All immutable setup is reused; only per-trial mutable state is freshly initialized.
pub fn simulate_combat_from_setup(setup: &PreCombatSetup, seed: u64) -> SimulationResult {
    let config = &setup.config;
    let attacker = &setup.attacker;
    let defender = &setup.defender;
    let attacker_crew = &setup.attacker_crew;
    let defender_crew = &setup.defender_crew;
    let defender_faction = setup.defender_faction;
    let defender_ship_type = setup.defender_ship_type;
    let attacker_ship_type = setup.attacker_ship_type;
    let defender_is_npc_hostile = setup.defender_is_npc_hostile;
    let defender_is_player_ship = setup.defender_is_player_ship;
    let attacker_tal_assigned_captain_or_bridge = setup.attacker_tal_assigned_captain_or_bridge;
    let attacker_ship_id_arc = &setup.attacker_ship_id_arc;
    let engagement_enemy_types_arc = &setup.engagement_enemy_types_arc;

    let mut rng = Rng::new(seed);
    let mut trace = TraceCollector::new(matches!(config.trace_mode, TraceMode::Events));
    let emit_snapshots = config.emit_state_snapshots && trace.is_enabled();
    let use_experimental_simd_damage_after_apex_base = avx2_supported() && !trace.is_enabled();
    let mut total_hull_damage = 0.0;
    let mut total_shield_damage = 0.0;
    let mut defender_shield_remaining = defender.shield_health.max(0.0);
    let mut attacker_shield_remaining = attacker.shield_health.max(0.0);
    let max_att_hull = attacker.hull_health.max(0.0);
    let mut total_attacker_hull_damage =
        config.initial_attacker_hull_damage.clamp(0.0, max_att_hull);
    let mut attacker_hull_gross_damage_this_round: f64 = 0.0;
    let mut attacker_hull_gross_damage_last_round: f64 = 0.0;
    let mut attacker_shield_gross_damage_this_round: f64 = 0.0;
    let mut attacker_shield_gross_damage_last_round: f64 = 0.0;
    let mut defender_hull_breach_rounds = 0_u32;
    // Breach-gated cumulative crit hull abilities (Hegh'ta "Open the Wound", Rotarran "Bird of
    // Prey"): while the opponent is hull breached, each weapon hit grows crit chance and each crit
    // grows crit damage, cumulative for the rest of the fight. `breach_hits` / `breach_crits` count
    // the qualifying events so far; the bonus applied to a shot reflects all *prior* such events.
    let breach_crit_chance_per_hit =
        sum_breach_cumulative_crit_chance_per_hit(&setup.attack_phase_effects);
    let breach_crit_damage_per_crit =
        sum_breach_cumulative_crit_damage_per_crit(&setup.attack_phase_effects);
    let mut breach_hits: u64 = 0;
    let mut breach_crits: u64 = 0;
    let mut defender_burning_rounds = 0_u32;
    let mut attacker_hull_breach_rounds = 0_u32;
    let mut attacker_burning_rounds = 0_u32;
    let mut assimilated_rounds_remaining = 0_u32;
    let mut defender_assimilated_rounds_remaining = 0_u32;
    let mut defender_morale_rounds_remaining = 0_u32;
    let mut shots_bonus_entries: Vec<(f64, u32)> = Vec::new();
    let mut defender_shots_bonus_entries: Vec<(f64, u32)> = Vec::new();
    let mut defender_weapon_fire_delayed_rounds = 0_u32;

    // Re-use precomputed effects from setup
    let combat_begin_effects = &setup.combat_begin_effects;
    let combat_begin_filtered = &setup.combat_begin_filtered;
    let attacker_mitigation_additive = setup.attacker_mitigation_additive;
    let attacker_accuracy_bonus = setup.attacker_accuracy_bonus;
    let attacker_dodge_bonus = setup.attacker_dodge_bonus;
    let shield_break_effects = &setup.shield_break_effects;
    let self_shield_break_effects = &setup.self_shield_break_effects;
    let kill_effects = &setup.kill_effects;
    let hull_breach_effects = &setup.hull_breach_effects;
    let receive_damage_effects = &setup.receive_damage_effects;
    let combat_end_effects = &setup.combat_end_effects;

    let round_start_effects = &setup.round_start_effects;
    let attack_phase_effects = &setup.attack_phase_effects;
    let defense_phase_effects = &setup.defense_phase_effects;
    let round_end_effects = &setup.round_end_effects;
    let after_subround_effects = &setup.after_subround_effects;

    let ctx_template = CombatCtxTemplate {
        defender_faction,
        attacker_owner_faction: config.attacker_owner_faction,
        defender_hull_faction_id: config.defender_hull_faction_id,
        defender_ship_type,
        attacker_ship_type,
        attacker_ship_id: std::sync::Arc::clone(attacker_ship_id_arc),
        defender_is_npc_hostile,
        defender_is_player_ship,
        attacker_tal_assigned_captain_or_bridge,
        defender_hostile_tag_mask: config.defender_hostile_tag_mask,
        engagement_enemy_types: std::sync::Arc::clone(engagement_enemy_types_arc),
        defender_level: config.defender_level,
    };

    let defender_combat_begin_effects = &setup.defender_combat_begin_effects;
    let defender_round_start_effects = &setup.defender_round_start_effects;
    let defender_attack_phase_effects = &setup.defender_attack_phase_effects;
    let defender_defense_phase_effects = &setup.defender_defense_phase_effects;
    let defender_shield_break_effects = &setup.defender_shield_break_effects;
    let defender_round_end_effects = &setup.defender_round_end_effects;
    let defender_receive_damage_effects = &setup.defender_receive_damage_effects;

    // Per-trial conqueror borg beam hyperthermic check (depends on seed)
    if setup.quantum_beam_instant_loss
        || hyperthermic_resonance_beam_instant_loss(
            defender_is_npc_hostile,
            config.defender_hostile_tag_mask,
            setup.effective_conqueror_borg_beam_suppression,
            &attacker.id,
            seed,
        )
        || setup.evo_assim_instant_loss
    {
        let total_attacker_hull_damage = max_att_hull;
        let attacker_hull_remaining = (attacker.hull_health - total_attacker_hull_damage).max(0.0);
        return SimulationResult {
            total_damage: 0.0,
            attacker_won: false,
            winner_by_round_limit: false,
            rounds_simulated: 0,
            attacker_hull_remaining: round_f64(attacker_hull_remaining),
            defender_hull_remaining: round_f64(defender.hull_health),
            defender_shield_remaining: round_f64(defender_shield_remaining),
            attacker_shield_remaining: round_f64(attacker_shield_remaining),
            events: trace.events(),
            conqueror_borg_beam_suppression: setup.conqueror_borg_beam_suppression,
        };
    }

    let conqueror_borg_beam_suppression = setup.conqueror_borg_beam_suppression;
    let combat_begin_assimilated = assimilated_rounds_remaining > 0;
    apply_combat_begin_phase(
        &mut trace,
        &mut rng,
        attacker,
        combat_begin_filtered,
        combat_begin_assimilated,
        &mut defender_burning_rounds,
        &mut defender_weapon_fire_delayed_rounds,
        &mut shots_bonus_entries,
    );

    let rounds_to_simulate = config.rounds.min(MAX_COMBAT_ROUNDS);
    shots_bonus_entries.reserve(rounds_to_simulate.min(32) as usize);
    defender_shots_bonus_entries.reserve(rounds_to_simulate.min(32) as usize);
    let mut rounds_completed = 0u32;

    for round_index in 1..=rounds_to_simulate {
        rounds_completed = round_index;

        let skip_defender_counter_attack = defender_weapon_fire_delayed_rounds > 0;
        if defender_weapon_fire_delayed_rounds > 0 {
            defender_weapon_fire_delayed_rounds =
                defender_weapon_fire_delayed_rounds.saturating_sub(1);
        }

        let DefenderRoundStartVitals {
            attacker_hull_pct_round,
            attacker_shield_pct_round,
            defender_hull_pct_round,
            defender_shield_pct_round,
        } = apply_defender_round_start(
            &mut trace,
            &mut rng,
            &ctx_template,
            round_index,
            attacker,
            defender,
            attacker_crew,
            defender_round_start_effects,
            defender_morale_rounds_remaining,
            defender_burning_rounds,
            defender_hull_breach_rounds,
            attacker_burning_rounds,
            attacker_hull_breach_rounds,
            &mut total_hull_damage,
            total_attacker_hull_damage,
            &mut defender_shield_remaining,
            attacker_shield_remaining,
            &mut defender_assimilated_rounds_remaining,
        );

        let mut combat_ctx = ctx_template.at(
            round_index,
            CtxVitals {
                defender_hull_pct: defender_hull_pct_round,
                defender_shield_pct: defender_shield_pct_round,
                attacker_hull_pct: attacker_hull_pct_round,
                attacker_shield_pct: attacker_shield_pct_round,
            },
            CtxStatusFlags {
                attacker_morale_active: false,
                defender_morale_active: defender_morale_rounds_remaining > 0,
                defender_burning_active: defender_burning_rounds > 0,
                defender_hull_breach_active: defender_hull_breach_rounds > 0,
                attacker_burning_active: attacker_burning_rounds > 0,
                attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
                defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
            },
        );

        // Symmetric attacker-outbound CDR: read `HostileCritDamageReduction` from the
        // defender's crew (in PvP, the opponent's `player_crit_damage_reduction` profile
        // bonus is wired in as a `HostileCritDamageReduction` seat in scenario.rs). The
        // floor clamp at the per-shot crit resolution site limits how low this can drive
        // the multiplier. The resolver folds in the per-round duration gate, so each
        // seat contributes only when `round_index` is within its `1..=duration_rounds`
        // window; overlapping sources stack additively (capped at 0.95).
        let effective_attacker_crit_reduction =
            hostile_crit_damage_reduction_active_at_round(defender_crew, &combat_ctx, round_index);

        let mut phase_effects = EffectAccumulator::default();
        let combat_begin_this_round =
            filter_effects_by_condition(combat_begin_effects, &combat_ctx);
        phase_effects.add_effects(
            TimingWindow::CombatBegin,
            &combat_begin_this_round,
            attacker.attack,
            assimilated_rounds_remaining > 0,
            round_index,
        );

        trace.record_if(|| CombatEvent {
            event_type: "round_start".to_string(),
            round_index,
            phase: "round".to_string(),
            source: EventSource {
                ship_ability_id: Some("baseline_round".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                ("attacker".to_string(), Value::String(attacker.id.clone())),
                ("defender".to_string(), Value::String(defender.id.clone())),
                (
                    "active_round_start_effects".to_string(),
                    Value::from(round_start_effects.len() as u64),
                ),
            ]),
        });

        let round_start_assimilated = assimilated_rounds_remaining > 0;
        // Round-start conditions that do not use [AbilityCondition::MoraleActive] (morale unknown yet).
        let bench = filter_effects_by_condition(round_start_effects, &combat_ctx);
        record_ability_activations(
            &mut trace,
            round_index,
            "round_start",
            attacker,
            &bench,
            round_start_assimilated,
        );
        phase_effects.add_effects(
            TimingWindow::RoundStart,
            &bench,
            attacker.attack,
            round_start_assimilated,
            round_index,
        );

        roll_attacker_round_start_procs(
            &mut trace,
            &mut rng,
            round_index,
            attacker,
            &bench,
            round_start_assimilated,
            hull_breach_effects,
            &combat_ctx,
            &mut phase_effects,
            &mut assimilated_rounds_remaining,
            &mut defender_burning_rounds,
            &mut defender_hull_breach_rounds,
            &mut defender_assimilated_rounds_remaining,
            &mut defender_morale_rounds_remaining,
            &mut shots_bonus_entries,
            &mut defender_weapon_fire_delayed_rounds,
        );

        roll_defender_round_start_shots_bonus(
            &mut trace,
            &mut rng,
            round_index,
            defender_round_start_effects,
            &combat_ctx,
            defender_assimilated_rounds_remaining,
            &mut defender_shots_bonus_entries,
        );

        // Morale proc after other round-start RNG consumers (assimilated, hull breach, burning, shots).
        // Sets [CombatContext::attacker_morale_active] for [AbilityCondition::MoraleActive] and pierce.
        let morale_triggered = roll_morale_activation(
            &mut trace,
            &mut rng,
            round_index,
            &bench,
            round_start_assimilated,
        );
        combat_ctx.attacker_morale_active = morale_triggered;
        combat_ctx.defender_morale_active = defender_morale_rounds_remaining > 0;
        // Round-start procs above may have applied breach/burning; refresh gates before attack-phase filtering.
        combat_ctx.defender_hull_breach_active = defender_hull_breach_rounds > 0;
        combat_ctx.attacker_hull_breach_active = attacker_hull_breach_rounds > 0;
        combat_ctx.defender_burning_active = defender_burning_rounds > 0;
        combat_ctx.attacker_burning_active = attacker_burning_rounds > 0;

        let full_round_start = filter_effects_by_condition(round_start_effects, &combat_ctx);
        let round_start_extra: Vec<_> = full_round_start
            .iter()
            .filter(|e| !bench.contains(e))
            .cloned()
            .collect();
        if !round_start_extra.is_empty() {
            record_ability_activations(
                &mut trace,
                round_index,
                "round_start",
                attacker,
                &round_start_extra,
                round_start_assimilated,
            );
            phase_effects.add_effects(
                TimingWindow::RoundStart,
                &round_start_extra,
                attacker.attack,
                round_start_assimilated,
                round_index,
            );
        }

        apply_attacker_round_start_regen(
            attacker,
            round_index,
            round_start_effects,
            &combat_ctx,
            round_start_assimilated,
            &mut phase_effects,
            &mut attacker_shield_remaining,
            &mut total_attacker_hull_damage,
            attacker_hull_gross_damage_last_round,
            attacker_shield_gross_damage_last_round,
        );

        // Prune expired shots bonuses and compute B_shots(r) for this round.
        shots_bonus_entries.retain(|(_, expires)| *expires >= round_index);
        let b_shots: f64 = shots_bonus_entries.iter().map(|(b, _)| b).sum();
        defender_shots_bonus_entries.retain(|(_, expires)| *expires >= round_index);
        let def_b_shots: f64 = defender_shots_bonus_entries.iter().map(|(b, _)| b).sum();

        if emit_snapshots {
            let snap = build_combat_state_snapshot(
                SnapshotAnchor::AfterRoundStart,
                round_index,
                None,
                None,
                attacker,
                defender,
                total_hull_damage,
                total_attacker_hull_damage,
                defender_shield_remaining,
                attacker_shield_remaining,
                &combat_ctx,
                assimilated_rounds_remaining,
                defender_assimilated_rounds_remaining,
                Some(&phase_effects),
            );
            trace.record(state_snapshot_as_combat_event(&snap));
        }

        let round_end_assimilated_early = assimilated_rounds_remaining > 0;
        let round_end_filtered = filter_effects_by_condition(round_end_effects, &combat_ctx);
        // RoundEnd stacking (apex, isolytic, shield mitigation, round-end damage multipliers, regen)
        // must not feed the same-round weapon sub-rounds. Apply RoundEnd only after all weapons
        // for this round (see merge into `phase_effects_round` below).
        let mut phase_effects_round = phase_effects.clone();
        let num_sub_rounds = attacker.weapon_count().max(defender.weapon_count());

        let attack_phase_assimilated = assimilated_rounds_remaining > 0;
        let attack_phase_filtered = filter_effects_by_condition(attack_phase_effects, &combat_ctx);
        let defense_phase_filtered =
            filter_effects_by_condition(defense_phase_effects, &combat_ctx);

        record_ability_activations(
            &mut trace,
            round_index,
            "attack",
            attacker,
            &attack_phase_filtered,
            attack_phase_assimilated,
        );
        let defense_phase_assimilated = assimilated_rounds_remaining > 0;
        record_ability_activations(
            &mut trace,
            round_index,
            "defense",
            attacker,
            &defense_phase_filtered,
            defense_phase_assimilated,
        );

        let defender_inbound_defense_filtered =
            filter_effects_by_condition(defender_defense_phase_effects, &combat_ctx);
        record_ability_activations(
            &mut trace,
            round_index,
            "defense_inbound",
            defender,
            &defender_inbound_defense_filtered,
            defender_assimilated_rounds_remaining > 0,
        );
        let use_simd_outbound_weapon_path = use_experimental_simd_damage_after_apex_base
            && defender_inbound_defense_filtered.is_empty()
            && defender_receive_damage_effects.is_empty();

        let weapon_round_base = phase_effects_round.clone();
        let mut phase_effects = EffectAccumulator::default();
        let mut after_subround_carry = EffectAccumulator::default();
        after_subround_carry.set_trace_contributions(trace.is_enabled());
        for weapon_index in 0..num_sub_rounds {
            let mut defender_shield_break_carry: Vec<ActiveAbilityEffect> = Vec::new();
            phase_effects.clear();
            phase_effects.set_trace_contributions(trace.is_enabled());
            phase_effects.merge_from(&weapon_round_base);
            phase_effects.merge_carry_additive(&after_subround_carry);
            let weapon_base = attacker
                .weapon_attack(weapon_index)
                .unwrap_or(attacker.attack);
            let mut effective_pierce = attacker.weapon_pierce(weapon_index)
                + phase_effects_round.pre_attack_pierce_bonus();
            if morale_triggered {
                effective_pierce *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
            }
            phase_effects.add_effects(
                TimingWindow::AttackPhase,
                &attack_phase_filtered,
                weapon_base,
                attack_phase_assimilated,
                round_index,
            );
            phase_effects.add_effects(
                TimingWindow::DefensePhase,
                &defense_phase_filtered,
                weapon_base,
                defense_phase_assimilated,
                round_index,
            );

            let weapon_index_u = weapon_index as u32;
            if emit_snapshots {
                let snap = build_combat_state_snapshot(
                    SnapshotAnchor::BeforeOutboundShot,
                    round_index,
                    Some(weapon_index_u),
                    None,
                    attacker,
                    defender,
                    total_hull_damage,
                    total_attacker_hull_damage,
                    defender_shield_remaining,
                    attacker_shield_remaining,
                    &combat_ctx,
                    assimilated_rounds_remaining,
                    defender_assimilated_rounds_remaining,
                    Some(&phase_effects),
                );
                trace.record(state_snapshot_as_combat_event(&snap));
            }

            let effective_apex_shred =
                (attacker.apex_shred + phase_effects.composed_apex_shred_bonus()).max(0.0);
            let effective_apex_barrier =
                (defender.apex_barrier + phase_effects.composed_apex_barrier_bonus()).max(0.0);
            let apex_damage_factor =
                compute_apex_damage_factor(effective_apex_shred, effective_apex_barrier);

            let base_shots = attacker.weapon_base_shots(weapon_index);
            let effective_shots = effective_shots_for_weapon(base_shots, b_shots);
            let shield_before_weapon = defender_shield_remaining;
            let mut simd_damage_batch: Vec<f64> =
                if use_simd_outbound_weapon_path && effective_shots > 0 {
                    Vec::with_capacity(4)
                } else {
                    Vec::new()
                };
            let mut simd_isolytic_batch: Vec<f64> =
                if use_simd_outbound_weapon_path && effective_shots > 0 {
                    Vec::with_capacity(4)
                } else {
                    Vec::new()
                };
            let mut simd_shield_mitigation_batch: Vec<f64> =
                if use_simd_outbound_weapon_path && effective_shots > 0 {
                    Vec::with_capacity(4)
                } else {
                    Vec::new()
                };
            let mut simd_damage_after_apex_batch: Vec<f64> =
                if use_simd_outbound_weapon_path && effective_shots > 0 {
                    Vec::with_capacity(4)
                } else {
                    Vec::new()
                };

            for hit_index in 0..effective_shots {
                if let Some(attacker_weapon_attack) = attacker.weapon_attack(weapon_index) {
                    let pre_mult = phase_effects.pre_attack_multiplier();
                    let g_galaxy = phase_effects.galaxy_additive_weapon_frac();
                    let p_prof = config.profile_weapon_damage_fraction;
                    let galaxy_dilution =
                        if g_galaxy > 0.0 && g_galaxy.is_finite() && (1.0 + p_prof) > 1e-12 {
                            1.0 + g_galaxy / (1.0 + p_prof)
                        } else {
                            1.0
                        };
                    let effective_attack = match config.weapon_damage_profile_additive_pool {
                        Some(p) if p > 0.0 && p.is_finite() => {
                            // Experimental: one additive pool for profile weapon_damage + dynamic pre-attack sum.
                            // Galaxy growth `g` shares the same additive pool as `p` (see findings doc).
                            attacker_weapon_attack * (p + pre_mult + g_galaxy) / (1.0 + p)
                        }
                        _ => attacker_weapon_attack * pre_mult * galaxy_dilution,
                    };

                    let roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                    trace.record_if(|| CombatEvent {
                        event_type: "attack_roll".to_string(),
                        round_index,
                        phase: "attack".to_string(),
                        source: EventSource {
                            officer_id: Some(attacker.id.clone()),
                            ..EventSource::default()
                        },
                        weapon_index: Some(weapon_index_u),
                        values: Map::from_iter([
                            ("roll".to_string(), Value::from(round_f64(roll))),
                            ("hit_index".to_string(), Value::from(hit_index)),
                            (
                                "base_attack".to_string(),
                                Value::from(attacker_weapon_attack),
                            ),
                            (
                                "effective_attack".to_string(),
                                Value::from(round_f64(effective_attack)),
                            ),
                        ]),
                    });

                    let mut mitigation_trace_breakdown = None;
                    let effective_mitigation =
                        if let Some(params) = &defender.hostile_mitigation_params {
                            let mut adjusted_attacker = params.base_attacker_stats;
                            adjusted_attacker.accuracy += attacker_accuracy_bonus;
                            let breakdown = mitigation_breakdown(
                                params.defender_stats,
                                adjusted_attacker,
                                params.ship_type,
                                params.mystery_mitigation_factor,
                            );
                            mitigation_trace_breakdown =
                                Some((breakdown, params.floor, params.ceiling));
                            breakdown.raw_mitigation.clamp(params.floor, params.ceiling)
                        } else {
                            defender.mitigation
                        };
                    let mitigation_multiplier = (1.0 - effective_mitigation).max(0.0);
                    trace.record_if(|| CombatEvent {
                        event_type: "mitigation_calc".to_string(),
                        round_index,
                        phase: "defense".to_string(),
                        source: EventSource {
                            hostile_ability_id: Some(format!("{}_mitigation", defender.id)),
                            ..EventSource::default()
                        },
                        weapon_index: Some(weapon_index_u),
                        values: {
                            let mut values = Map::from_iter([
                                (
                                    "mitigation".to_string(),
                                    Value::from(round_f64(effective_mitigation)),
                                ),
                                (
                                    "multiplier".to_string(),
                                    Value::from(round_f64(mitigation_multiplier)),
                                ),
                            ]);
                            if let Some((breakdown, floor, ceiling)) = mitigation_trace_breakdown {
                                values.insert(
                                    "mitigation_raw".to_string(),
                                    Value::from(round_f64(breakdown.raw_mitigation)),
                                );
                                values.insert(
                                    "mitigation_floor".to_string(),
                                    Value::from(round_f64(floor)),
                                );
                                values.insert(
                                    "mitigation_ceiling".to_string(),
                                    Value::from(round_f64(ceiling)),
                                );
                                values.insert(
                                    "c_armor".to_string(),
                                    Value::from(round_f64(breakdown.c_armor)),
                                );
                                values.insert(
                                    "c_shield".to_string(),
                                    Value::from(round_f64(breakdown.c_shield)),
                                );
                                values.insert(
                                    "c_dodge".to_string(),
                                    Value::from(round_f64(breakdown.c_dodge)),
                                );
                                values.insert(
                                    "armor_ratio".to_string(),
                                    Value::from(round_f64(breakdown.armor_ratio)),
                                );
                                values.insert(
                                    "shield_ratio".to_string(),
                                    Value::from(round_f64(breakdown.shield_ratio)),
                                );
                                values.insert(
                                    "dodge_ratio".to_string(),
                                    Value::from(round_f64(breakdown.dodge_ratio)),
                                );
                                values.insert(
                                    "f_armor".to_string(),
                                    Value::from(round_f64(breakdown.f_armor)),
                                );
                                values.insert(
                                    "f_shield".to_string(),
                                    Value::from(round_f64(breakdown.f_shield)),
                                );
                                values.insert(
                                    "f_dodge".to_string(),
                                    Value::from(round_f64(breakdown.f_dodge)),
                                );
                                values.insert(
                                    "weighted_armor".to_string(),
                                    Value::from(round_f64(breakdown.weighted_armor)),
                                );
                                values.insert(
                                    "weighted_shield".to_string(),
                                    Value::from(round_f64(breakdown.weighted_shield)),
                                );
                                values.insert(
                                    "weighted_dodge".to_string(),
                                    Value::from(round_f64(breakdown.weighted_dodge)),
                                );
                                values.insert(
                                    "mystery_mitigation_factor".to_string(),
                                    Value::from(round_f64(breakdown.mystery_mitigation_factor)),
                                );
                                values.insert(
                                    "one_minus_mystery".to_string(),
                                    Value::from(round_f64(breakdown.one_minus_mystery)),
                                );
                            }
                            values
                        },
                    });

                    let defender_inbound_assimilated = defender_assimilated_rounds_remaining > 0;
                    let mut inbound_defender_effects = EffectAccumulator::default();
                    inbound_defender_effects.set_trace_contributions(false);
                    inbound_defender_effects.add_effects(
                        TimingWindow::DefensePhase,
                        &defender_inbound_defense_filtered,
                        weapon_base,
                        defender_inbound_assimilated,
                        round_index,
                    );
                    let combined_defense_mitigation_bonus = phase_effects
                        .defense_mitigation_bonus()
                        + inbound_defender_effects.defense_mitigation_bonus();

                    // Damage-through factor: fraction of attack that gets through (can exceed 1.0 with pierce).
                    let damage_through_factor = compute_damage_through_factor(
                        mitigation_multiplier,
                        effective_pierce,
                        combined_defense_mitigation_bonus,
                    );
                    trace.record_if(|| CombatEvent {
                        event_type: "pierce_calc".to_string(),
                        round_index,
                        phase: "attack".to_string(),
                        source: EventSource {
                            officer_id: Some(attacker.id.clone()),
                            player_bonus_source: Some("attack_pierce_bonus".to_string()),
                            ..EventSource::default()
                        },
                        weapon_index: None,
                        values: Map::from_iter([
                            ("pierce".to_string(), Value::from(effective_pierce)),
                            (
                                "damage_through_factor".to_string(),
                                Value::from(round_f64(damage_through_factor)),
                            ),
                        ]),
                    });

                    let hull_breach_active = defender_hull_breach_rounds > 0;
                    // Breach-gated cumulative crit growth (Hegh'ta / Rotarran). The bonus reflects
                    // only events that occurred *before* this shot; the counters grow afterwards.
                    // Crit chance grows additively (clamps to [0,1] in the roll); crit damage grows
                    // as additive percentage points on the crit multiplier (per-crit stat bonus).
                    let breach_crit_chance_add = breach_crit_chance_per_hit * breach_hits as f64;
                    let breach_crit_damage_add = breach_crit_damage_per_crit * breach_crits as f64;
                    let crit = resolve_vehicle_weapon_crit(
                        attacker.weapon_crit_chance(weapon_index),
                        phase_effects.crit_chance_bonus() + breach_crit_chance_add,
                        attacker.weapon_crit_multiplier(weapon_index),
                        phase_effects.crit_damage_multiplier(),
                        breach_crit_damage_add,
                        effective_attacker_crit_reduction,
                        attacker.crit_damage_floor,
                        hull_breach_active,
                        &mut rng,
                    );
                    let crit_multiplier = crit.multiplier;
                    let is_crit = crit.is_crit;
                    // Grow the cumulative breach crit counters after this shot resolves, so the
                    // bonus benefits subsequent hits/crits (not the current one).
                    if hull_breach_active {
                        if breach_crit_chance_per_hit > 0.0 {
                            breach_hits += 1;
                        }
                        if is_crit && breach_crit_damage_per_crit > 0.0 {
                            breach_crits += 1;
                        }
                    }
                    trace.record_if(|| CombatEvent {
                        event_type: "crit_resolution".to_string(),
                        round_index,
                        phase: "attack".to_string(),
                        source: EventSource {
                            officer_id: Some(attacker.id.clone()),
                            ship_ability_id: Some("crit_matrix".to_string()),
                            ..EventSource::default()
                        },
                        weapon_index: None,
                        values: Map::from_iter([
                            ("roll".to_string(), Value::from(round_f64(crit.roll))),
                            ("is_crit".to_string(), Value::Bool(crit.is_crit)),
                            ("multiplier".to_string(), Value::from(crit_multiplier)),
                            (
                                "effective_crit_chance".to_string(),
                                Value::from(round_f64(crit.effective_crit_chance)),
                            ),
                            (
                                "attacker_crit_reduction".to_string(),
                                Value::from(round_f64(effective_attacker_crit_reduction)),
                            ),
                            (
                                "crit_damage_floor".to_string(),
                                Value::from(round_f64(attacker.crit_damage_floor)),
                            ),
                            (
                                "hull_breach_active".to_string(),
                                Value::Bool(hull_breach_active),
                            ),
                        ]),
                    });

                    for effect in &attack_phase_filtered {
                        let effective_effect =
                            scale_effect(effect.effect, attack_phase_assimilated);

                        if let AbilityEffect::Assimilated {
                            chance,
                            duration_rounds,
                        } = effective_effect
                        {
                            let assimilated_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                            let triggered = assimilated_roll < chance.clamp(0.0, 1.0);
                            if triggered {
                                assimilated_rounds_remaining =
                                    assimilated_rounds_remaining.max(duration_rounds.max(1));
                            }
                            trace.record_if(|| CombatEvent {
                                event_type: "assimilated_trigger".to_string(),
                                round_index,
                                phase: "attack".to_string(),
                                source: EventSource {
                                    officer_id: Some(attacker.id.clone()),
                                    ship_ability_id: Some(effect.ability_name.clone()),
                                    ..EventSource::default()
                                },
                                weapon_index: None,
                                values: Map::from_iter([
                                    ("roll".to_string(), Value::from(round_f64(assimilated_roll))),
                                    ("triggered".to_string(), Value::Bool(triggered)),
                                    ("chance".to_string(), Value::from(round_f64(chance))),
                                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                                ]),
                            });
                        }

                        if let AbilityEffect::HullBreach {
                            chance,
                            duration_rounds,
                            requires_critical,
                        } = effective_effect
                        {
                            if requires_critical && !is_crit {
                                continue;
                            }

                            let hull_breach_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                            let triggered = hull_breach_roll < chance.clamp(0.0, 1.0);
                            let breach_before = defender_hull_breach_rounds;
                            if triggered {
                                defender_hull_breach_rounds =
                                    defender_hull_breach_rounds.max(duration_rounds.max(1));
                            }
                            if breach_before == 0 && defender_hull_breach_rounds > 0 {
                                let mut ctx_hb = combat_ctx.clone();
                                ctx_hb.defender_hull_pct = 1.0
                                    - (total_hull_damage / defender.hull_health.max(0.0)).min(1.0);
                                ctx_hb.defender_shield_pct = if defender.shield_health > 0.0 {
                                    defender_shield_remaining / defender.shield_health
                                } else {
                                    1.0
                                };
                                apply_hull_breach_timing_window(
                                    &mut RoundPhaseCtx {
                                        trace: &mut trace,
                                        rng: &mut rng,
                                        round_index,
                                    },
                                    HullBreachSide::Defender,
                                    attacker,
                                    hull_breach_effects,
                                    ctx_hb,
                                    attack_phase_assimilated,
                                    weapon_base,
                                    &mut phase_effects_round,
                                    &mut defender_burning_rounds,
                                );
                            }
                            trace.record_if(|| CombatEvent {
                                event_type: "hull_breach_trigger".to_string(),
                                round_index,
                                phase: "attack".to_string(),
                                source: EventSource {
                                    officer_id: Some(attacker.id.clone()),
                                    ship_ability_id: Some(effect.ability_name.clone()),
                                    ..EventSource::default()
                                },
                                weapon_index: None,
                                values: Map::from_iter([
                                    ("roll".to_string(), Value::from(round_f64(hull_breach_roll))),
                                    ("triggered".to_string(), Value::Bool(triggered)),
                                    ("chance".to_string(), Value::from(round_f64(chance))),
                                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                                    (
                                        "requires_critical".to_string(),
                                        Value::Bool(requires_critical),
                                    ),
                                ]),
                            });
                        }

                        if let AbilityEffect::DefenderFireDelay {
                            chance,
                            delay_rounds,
                            requires_critical,
                        } = effective_effect
                        {
                            apply_defender_fire_delay(
                                &mut trace,
                                &mut rng,
                                round_index,
                                "attack",
                                &attacker.id,
                                &effect.ability_name,
                                chance,
                                delay_rounds,
                                requires_critical,
                                is_crit,
                                &mut defender_weapon_fire_delayed_rounds,
                            );
                        }

                        roll_burning_triggers(
                            &mut RoundPhaseCtx {
                                trace: &mut trace,
                                rng: &mut rng,
                                round_index,
                            },
                            std::slice::from_ref(effect),
                            attack_phase_assimilated,
                            "attack",
                            &attacker.id,
                            None,
                            &mut defender_burning_rounds,
                        );
                    }

                    for effect in &defense_phase_filtered {
                        roll_burning_triggers(
                            &mut RoundPhaseCtx {
                                trace: &mut trace,
                                rng: &mut rng,
                                round_index,
                            },
                            std::slice::from_ref(effect),
                            defense_phase_assimilated,
                            "defense",
                            &attacker.id,
                            None,
                            &mut defender_burning_rounds,
                        );
                    }

                    for effect in &defender_inbound_defense_filtered {
                        roll_burning_triggers(
                            &mut RoundPhaseCtx {
                                trace: &mut trace,
                                rng: &mut rng,
                                round_index,
                            },
                            std::slice::from_ref(effect),
                            defender_inbound_assimilated,
                            "defense_inbound",
                            &defender.id,
                            None,
                            &mut defender_burning_rounds,
                        );
                    }

                    let w_proc_chance = attacker.weapon_proc_chance(weapon_index);
                    let (did_proc, proc_roll) = roll_weapon_intrinsic_proc(w_proc_chance, &mut rng);
                    let proc_multiplier = if did_proc {
                        attacker.weapon_proc_multiplier(weapon_index)
                    } else {
                        1.0
                    };
                    trace.record_if(|| CombatEvent {
                        event_type: "proc_triggers".to_string(),
                        round_index,
                        phase: "proc".to_string(),
                        source: EventSource {
                            officer_id: Some(attacker.id.clone()),
                            ship_ability_id: Some("officer_proc".to_string()),
                            ..EventSource::default()
                        },
                        weapon_index: None,
                        values: Map::from_iter([
                            ("roll".to_string(), Value::from(round_f64(proc_roll))),
                            ("triggered".to_string(), Value::Bool(did_proc)),
                            ("multiplier".to_string(), Value::from(proc_multiplier)),
                        ]),
                    });

                    let pre_attack_damage = effective_attack
                        * damage_through_factor
                        * crit_multiplier
                        * proc_multiplier;
                    phase_effects.set_pre_attack_damage_base(pre_attack_damage);
                    let pre_attack_damage = phase_effects.composed_pre_attack_damage();
                    let damage = phase_effects.compose_attack_phase_damage(pre_attack_damage);

                    trace.record_if(|| {
                        let mut values = phase_effects.stack_resolution_values();
                        values.insert(
                            "pre_attack_damage_composed".to_string(),
                            Value::from(round_f64(pre_attack_damage)),
                        );
                        values.insert(
                            "damage_after_attack_phase_compose".to_string(),
                            Value::from(round_f64(damage)),
                        );
                        CombatEvent {
                            event_type: "stack_resolution".to_string(),
                            round_index,
                            phase: "attack".to_string(),
                            source: EventSource {
                                officer_id: Some(attacker.id.clone()),
                                player_bonus_source: Some("effect_stacks".to_string()),
                                ..EventSource::default()
                            },
                            weapon_index: Some(weapon_index_u),
                            values,
                        }
                    });

                    // Isolytic: from pre-apex standard damage; report formula: isolytic taken = Isolytic Damage / (1 + I_def).
                    let effective_isolytic_damage = (attacker.isolytic_damage
                        + phase_effects.composed_isolytic_damage_bonus())
                    .max(0.0);
                    let effective_isolytic_defense = (defender.isolytic_defense
                        + phase_effects.composed_isolytic_defense_bonus()
                        + inbound_defender_effects.composed_isolytic_defense_bonus())
                    .max(0.0);
                    let effective_isolytic_cascade = phase_effects
                        .composed_isolytic_cascade_damage_bonus()
                        .max(0.0);
                    let isolytic_taken = compute_isolytic_taken(
                        damage,
                        effective_isolytic_damage,
                        effective_isolytic_defense,
                        effective_isolytic_cascade,
                    );

                    // Apex barrier: apply once to combined pool (standard_net + isolytic_taken).
                    let damage_before_apex = damage + isolytic_taken;
                    let damage_after_apex = damage_before_apex * apex_damage_factor;

                    // Shield mitigation: S * damage to shield, (1-S) * damage to hull (STFC Toolbox game-mechanics).
                    // Additive bonuses (e.g. Quantum Slipstream cumulative debuff) compose first,
                    // then any multiplicative bypass (e.g. Harrison "Sabotage") scales the result
                    // by (1 - bypass). Bypass total is clamped to [0, 1] so it cannot exceed 100%.
                    let pre_bypass_shield_mitigation = (defender.shield_mitigation
                        + phase_effects.composed_shield_mitigation_bonus()
                        + inbound_defender_effects.composed_shield_mitigation_bonus())
                    .clamp(0.0, 1.0);
                    let total_bypass_fraction = (phase_effects.composed_shield_mitigation_bypass()
                        + inbound_defender_effects.composed_shield_mitigation_bypass())
                    .clamp(0.0, 1.0);
                    let effective_shield_mitigation = (pre_bypass_shield_mitigation
                        * (1.0 - total_bypass_fraction))
                        .clamp(0.0, 1.0);
                    let shield_mitigation = if defender_shield_remaining > 0.0 {
                        effective_shield_mitigation
                    } else {
                        0.0
                    };

                    if use_simd_outbound_weapon_path {
                        // The SIMD kernel consumes parallel slices: damage / isolytic_taken /
                        // shield_mitigation. The damage push was missing, so the kernel was
                        // called with `damage_after_attack_phase=&[]` (len 0) and
                        // `isolytic_taken=&[isolytic_taken]` (len 1), the length-mismatch
                        // error was silently dropped via `let _`, the output buffer stayed
                        // empty, and zero damage was applied on every avx2_supported() host.
                        simd_damage_batch.push(damage);
                        simd_isolytic_batch.push(isolytic_taken);
                        simd_shield_mitigation_batch.push(effective_shield_mitigation);

                        let flush_batch =
                            simd_damage_batch.len() == 4 || hit_index + 1 == effective_shots;
                        if flush_batch {
                            simd_damage_after_apex_batch.resize(simd_damage_batch.len(), 0.0);
                            let _ = compute_damage_after_apex_batch(
                                &simd_damage_batch,
                                &simd_isolytic_batch,
                                apex_damage_factor,
                                &mut simd_damage_after_apex_batch,
                            );
                            for lane in 0..simd_damage_after_apex_batch.len() {
                                let lane_shield_mitigation = if defender_shield_remaining > 0.0 {
                                    simd_shield_mitigation_batch[lane]
                                } else {
                                    0.0
                                };
                                let (actual_shield_damage, hull_damage_this_round) =
                                    apply_shield_hull_split(
                                        simd_damage_after_apex_batch[lane],
                                        lane_shield_mitigation,
                                        defender_shield_remaining,
                                    );
                                defender_shield_remaining =
                                    (defender_shield_remaining - actual_shield_damage).max(0.0);
                                total_hull_damage += hull_damage_this_round;
                                total_shield_damage += actual_shield_damage;
                            }
                            simd_damage_batch.clear();
                            simd_isolytic_batch.clear();
                            simd_shield_mitigation_batch.clear();
                        }
                        continue;
                    }

                    let (actual_shield_damage, hull_damage_this_round) = apply_shield_hull_split(
                        damage_after_apex,
                        shield_mitigation,
                        defender_shield_remaining,
                    );

                    defender_shield_remaining =
                        (defender_shield_remaining - actual_shield_damage).max(0.0);
                    total_hull_damage += hull_damage_this_round;
                    total_shield_damage += actual_shield_damage;

                    if hull_damage_this_round > 0.0 {
                        let def_rd_assim = defender_assimilated_rounds_remaining > 0;
                        let mut ctx_def_rd = combat_ctx.clone();
                        ctx_def_rd.defender_hull_pct =
                            1.0 - (total_hull_damage / defender.hull_health.max(0.0)).min(1.0);
                        ctx_def_rd.defender_shield_pct = if defender.shield_health > 0.0 {
                            defender_shield_remaining / defender.shield_health
                        } else {
                            1.0
                        };
                        let def_rd_filtered = filter_effects_by_condition(
                            defender_receive_damage_effects,
                            &ctx_def_rd,
                        );
                        roll_burning_triggers(
                            &mut RoundPhaseCtx {
                                trace: &mut trace,
                                rng: &mut rng,
                                round_index,
                            },
                            &def_rd_filtered,
                            def_rd_assim,
                            "receive_damage",
                            &defender.id,
                            None,
                            &mut defender_burning_rounds,
                        );
                    }

                    trace.record_if(|| CombatEvent {
                        event_type: "damage_application".to_string(),
                        round_index,
                        phase: "damage".to_string(),
                        source: EventSource {
                            officer_id: Some(attacker.id.clone()),
                            hostile_ability_id: Some(format!("{}_hull", defender.id)),
                            ..EventSource::default()
                        },
                        weapon_index: Some(weapon_index_u),
                        values: Map::from_iter([
                            (
                                "damage_after_apex".to_string(),
                                Value::from(round_f64(damage_after_apex)),
                            ),
                            (
                                "shield_mitigation".to_string(),
                                Value::from(round_f64(shield_mitigation)),
                            ),
                            (
                                "shield_damage".to_string(),
                                Value::from(round_f64(actual_shield_damage)),
                            ),
                            (
                                "hull_damage".to_string(),
                                Value::from(round_f64(hull_damage_this_round)),
                            ),
                            (
                                "running_hull_damage".to_string(),
                                Value::from(round_f64(total_hull_damage)),
                            ),
                            (
                                "defender_shield_remaining".to_string(),
                                Value::from(round_f64(defender_shield_remaining)),
                            ),
                            (
                                "shield_broke".to_string(),
                                Value::Bool(
                                    shield_before_weapon > 0.0 && defender_shield_remaining <= 0.0,
                                ),
                            ),
                            (
                                "assimilated_active".to_string(),
                                Value::Bool(assimilated_rounds_remaining > 0),
                            ),
                            ("hit_index".to_string(), Value::from(hit_index)),
                        ]),
                    });
                    if emit_snapshots {
                        let snap = build_combat_state_snapshot(
                            SnapshotAnchor::AfterOutboundDamage,
                            round_index,
                            Some(weapon_index_u),
                            Some(hit_index),
                            attacker,
                            defender,
                            total_hull_damage,
                            total_attacker_hull_damage,
                            defender_shield_remaining,
                            attacker_shield_remaining,
                            &combat_ctx,
                            assimilated_rounds_remaining,
                            defender_assimilated_rounds_remaining,
                            Some(&phase_effects),
                        );
                        trace.record(state_snapshot_as_combat_event(&snap));
                    }
                }
            }

            let shield_broke_this_round =
                shield_before_weapon > 0.0 && defender_shield_remaining <= 0.0;
            if shield_broke_this_round {
                process_defender_shield_break(
                    &mut trace,
                    &mut rng,
                    round_index,
                    attacker,
                    defender,
                    &combat_ctx,
                    shield_break_effects,
                    defender_shield_break_effects,
                    attack_phase_assimilated,
                    weapon_base,
                    &mut phase_effects_round,
                    &mut defender_burning_rounds,
                    &mut defender_weapon_fire_delayed_rounds,
                    &mut defender_shield_remaining,
                    &mut total_hull_damage,
                    &mut defender_shield_break_carry,
                );
            }

            if let Some(defender_weapon_attack) = defender.weapon_attack(weapon_index) {
                if skip_defender_counter_attack {
                    continue;
                }
                // Defender counter-attack: hostile weapon fire vs the player ship (attacker struct).
                // Uses the same damage-through, isolytic, apex, and shield/hull helpers as outbound shots
                // so the two paths stay in sync. Shot count mirrors outbound: `effective_shots_for_weapon`
                // on `defender.weapon_base_shots` with defender crew `ShotsBonus`.
                let def_base_shots = defender.weapon_base_shots(weapon_index);
                let def_effective_shots = effective_shots_for_weapon(def_base_shots, def_b_shots);

                // Inbound counter-fire mitigation: weight each component by the attacker
                // ship-type coefficients (c_armor, c_shield, c_dodge). `damage_reduction`
                // and `attacker_mitigation_additive` are flat post-mitigation reductions.
                // Fallback: when no profile-resolved components are set (e.g. legacy test
                // fixtures with only `attacker.mitigation` populated), use the aggregated
                // scalar directly so existing fixtures keep their calibrated behavior.
                let eff_player_mitigation = {
                    let (c_armor, c_shield, c_dodge) = attacker_ship_type.coefficients();
                    let component_sum = c_armor * attacker.armor
                        + c_shield * attacker.shield_deflection
                        + c_dodge * (attacker.dodge + attacker_dodge_bonus)
                        + attacker.damage_reduction;
                    let base = if component_sum > 0.0 {
                        component_sum
                    } else {
                        attacker.mitigation + attacker_dodge_bonus * c_dodge
                    };
                    (base + attacker_mitigation_additive).clamp(0.0, 1.0)
                };
                let counter_mitigation_mult = (1.0 - eff_player_mitigation).max(0.0);

                // Static hostile buffs for this weapon sub-round; cloned per counter shot so officer
                // `Proc*` rolls do not accumulate across hits.
                let mut defender_phase_template = EffectAccumulator::default();
                defender_phase_template.set_trace_contributions(false);
                let defender_ctx = CombatContext {
                    round_index,
                    defender_hull_pct: combat_ctx.defender_hull_pct,
                    defender_shield_pct: combat_ctx.defender_shield_pct,
                    attacker_hull_pct: combat_ctx.attacker_hull_pct,
                    attacker_shield_pct: combat_ctx.attacker_shield_pct,
                    attacker_morale_active: false,
                    defender_morale_active: defender_morale_rounds_remaining > 0,
                    defender_burning_active: false,
                    defender_hull_breach_active: false,
                    attacker_burning_active: combat_ctx.attacker_burning_active,
                    attacker_hull_breach_active: combat_ctx.attacker_hull_breach_active,
                    defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
                    defender_faction,
                    attacker_owner_faction: combat_ctx.attacker_owner_faction,
                    defender_hull_faction_id: config.defender_hull_faction_id,
                    defender_ship_type,
                    attacker_ship_type,
                    attacker_ship_id: std::sync::Arc::clone(attacker_ship_id_arc),
                    defender_is_npc_hostile,
                    defender_is_player_ship,
                    attacker_tal_assigned_captain_or_bridge: combat_ctx
                        .attacker_tal_assigned_captain_or_bridge,
                    defender_hostile_tag_mask: config.defender_hostile_tag_mask,
                    engagement_enemy_types: std::sync::Arc::clone(engagement_enemy_types_arc),
                    combat_battle_type_id: combat_ctx.combat_battle_type_id,
                    defender_level: combat_ctx.defender_level,
                };
                let defender_combat_begin_filtered =
                    filter_effects_by_condition(defender_combat_begin_effects, &defender_ctx);
                let defender_round_start_filtered =
                    filter_effects_by_condition(defender_round_start_effects, &defender_ctx);
                let defender_attack_filtered =
                    filter_effects_by_condition(defender_attack_phase_effects, &defender_ctx);
                let defender_defense_filtered =
                    filter_effects_by_condition(defender_defense_phase_effects, &defender_ctx);

                // Crew-derived CDR for this round (per-round sum of active seats, already
                // clamped to [0, 0.95] inside the resolver — see PR #188).
                let hostile_crit_reduction = hostile_crit_damage_reduction_active_at_round(
                    attacker_crew,
                    &defender_ctx,
                    round_index,
                );
                let (hostile_counter_debuff, hostile_counter_debuff_rounds) =
                    hostile_counter_stat_debuff_from_crew(attacker_crew, &defender_ctx);

                defender_phase_template.add_effects(
                    TimingWindow::CombatBegin,
                    &defender_combat_begin_filtered,
                    defender_weapon_attack,
                    false,
                    round_index,
                );
                defender_phase_template.add_effects(
                    TimingWindow::RoundStart,
                    &defender_round_start_filtered,
                    defender_weapon_attack,
                    false,
                    round_index,
                );
                defender_phase_template.add_effects(
                    TimingWindow::ShieldBreak,
                    &defender_shield_break_carry,
                    defender_weapon_attack,
                    false,
                    round_index,
                );
                defender_phase_template.add_effects(
                    TimingWindow::AttackPhase,
                    &defender_attack_filtered,
                    defender_weapon_attack,
                    false,
                    round_index,
                );
                defender_phase_template.add_effects(
                    TimingWindow::DefensePhase,
                    &defender_defense_filtered,
                    defender_weapon_attack,
                    false,
                    round_index,
                );

                let mut counter_hull_damage_this_subround = false;

                let counter_apex_factor = compute_apex_damage_factor(
                    defender.apex_shred.max(0.0),
                    attacker.apex_barrier.max(0.0),
                );

                let mut counter_simd_damage_batch: Vec<f64> =
                    if use_experimental_simd_damage_after_apex_base && def_effective_shots > 0 {
                        Vec::with_capacity(4)
                    } else {
                        Vec::new()
                    };
                let mut counter_simd_isolytic_batch: Vec<f64> =
                    if use_experimental_simd_damage_after_apex_base && def_effective_shots > 0 {
                        Vec::with_capacity(4)
                    } else {
                        Vec::new()
                    };
                let mut counter_simd_shield_mit_batch: Vec<f64> =
                    if use_experimental_simd_damage_after_apex_base && def_effective_shots > 0 {
                        Vec::with_capacity(4)
                    } else {
                        Vec::new()
                    };
                let mut counter_simd_crit_batch: Vec<bool> =
                    if use_experimental_simd_damage_after_apex_base && def_effective_shots > 0 {
                        Vec::with_capacity(4)
                    } else {
                        Vec::new()
                    };
                let mut counter_simd_after_apex_batch: Vec<f64> =
                    if use_experimental_simd_damage_after_apex_base && def_effective_shots > 0 {
                        Vec::with_capacity(4)
                    } else {
                        Vec::new()
                    };

                for hit_index in 0..def_effective_shots {
                    trace.record_if(|| {
                        let (c_armor, c_shield, c_dodge) = attacker_ship_type.coefficients();
                        let dodge_mitigation = attacker_dodge_bonus * c_dodge;
                        CombatEvent {
                            event_type: "mitigation_calc".to_string(),
                            round_index,
                            phase: "counter".to_string(),
                            source: EventSource {
                                hostile_ability_id: Some(format!(
                                    "{}_counter_mitigation",
                                    defender.id
                                )),
                                ..EventSource::default()
                            },
                            weapon_index: Some(weapon_index as u32),
                            values: Map::from_iter([
                                (
                                    "mitigation".to_string(),
                                    Value::from(round_f64(eff_player_mitigation)),
                                ),
                                (
                                    "multiplier".to_string(),
                                    Value::from(round_f64(counter_mitigation_mult)),
                                ),
                                (
                                    "base_mitigation".to_string(),
                                    Value::from(round_f64(attacker.mitigation)),
                                ),
                                (
                                    "armor_component".to_string(),
                                    Value::from(round_f64(c_armor * attacker.armor)),
                                ),
                                (
                                    "shield_deflection_component".to_string(),
                                    Value::from(round_f64(c_shield * attacker.shield_deflection)),
                                ),
                                (
                                    "dodge_component".to_string(),
                                    Value::from(round_f64(
                                        c_dodge * (attacker.dodge + attacker_dodge_bonus),
                                    )),
                                ),
                                (
                                    "damage_reduction".to_string(),
                                    Value::from(round_f64(attacker.damage_reduction)),
                                ),
                                (
                                    "mitigation_additive_bonus".to_string(),
                                    Value::from(round_f64(attacker_mitigation_additive)),
                                ),
                                (
                                    "dodge_bonus".to_string(),
                                    Value::from(round_f64(attacker_dodge_bonus)),
                                ),
                                (
                                    "dodge_coefficient".to_string(),
                                    Value::from(round_f64(c_dodge)),
                                ),
                                (
                                    "dodge_mitigation_bonus".to_string(),
                                    Value::from(round_f64(dodge_mitigation)),
                                ),
                            ]),
                        }
                    });
                    let defender_attack_phase_assimilated =
                        defender_assimilated_rounds_remaining > 0;
                    roll_assimilated_extensions_from_effects(
                        &mut RoundPhaseCtx {
                            trace: &mut trace,
                            rng: &mut rng,
                            round_index,
                        },
                        &defender_attack_filtered,
                        defender_attack_phase_assimilated,
                        "attack",
                        &defender.id,
                        &mut defender_assimilated_rounds_remaining,
                    );

                    let mut defender_phase_effects = defender_phase_template.clone();
                    defender_phase_effects.set_trace_contributions(trace.is_enabled());

                    // Per-shot `Proc*` (mirrors outbound per-shot weapon intrinsic proc: fresh rolls).
                    let (proc_pre_attack_multiplier, proc_pre_attack_pierce_bonus) =
                        accumulate_proc_attack_effects(
                            defender_combat_begin_filtered
                                .iter()
                                .chain(defender_round_start_filtered.iter())
                                .chain(defender_attack_filtered.iter())
                                .chain(defender_defense_filtered.iter()),
                            &mut rng,
                        );

                    defender_phase_effects.add_effect(
                        TimingWindow::CombatBegin,
                        AbilityEffect::AttackMultiplier(proc_pre_attack_multiplier - 1.0),
                        defender_weapon_attack,
                        round_index,
                        None,
                    );
                    defender_phase_effects.add_effect(
                        TimingWindow::CombatBegin,
                        AbilityEffect::PierceBonus(proc_pre_attack_pierce_bonus),
                        defender_weapon_attack,
                        round_index,
                        None,
                    );

                    let mut counter_pierce =
                        crate::combat::abilities::defender_morale_adjusted_pierce(
                            defender.weapon_pierce(weapon_index)
                                + defender_phase_effects.pre_attack_pierce_bonus(),
                            defender_ship_type,
                            defender_morale_rounds_remaining > 0,
                        );
                    if hostile_counter_debuff > 0.0
                        && round_in_inclusive_first_n(round_index, hostile_counter_debuff_rounds)
                    {
                        counter_pierce *= (1.0 - hostile_counter_debuff).max(0.0);
                    }
                    let counter_damage_through = compute_damage_through_factor(
                        counter_mitigation_mult,
                        counter_pierce,
                        defender_phase_effects.defense_mitigation_bonus(),
                    );
                    let attacker_hull_breach_active_for_crit = attacker_hull_breach_rounds > 0;
                    // Defender counter-fire: pass 0 for the new attacker-outbound CDR / floor
                    // params. The existing `hostile_crit_reduction` post-call adjustment below
                    // keeps the player-side defensive CDR semantics intact.
                    let def_crit = resolve_vehicle_weapon_crit(
                        defender.weapon_crit_chance(weapon_index),
                        defender_phase_effects.crit_chance_bonus(),
                        defender.weapon_crit_multiplier(weapon_index),
                        defender_phase_effects.crit_damage_multiplier(),
                        // No breach-cumulative crit-damage source on the counter-fire path today.
                        0.0,
                        0.0,
                        0.0,
                        attacker_hull_breach_active_for_crit,
                        &mut rng,
                    );
                    let def_is_crit = def_crit.is_crit;
                    let mut def_crit_mult = def_crit.multiplier;
                    // Hostile crit-damage reduction (U.S.S. Crozier "Gunboat Diplomacy", Borg
                    // Operating Table tech, profile `player_crit_damage_reduction`, …). The
                    // per-round duration gate already lives inside the resolver; here we just
                    // apply the resolved fraction.
                    if def_is_crit && hostile_crit_reduction > 0.0 {
                        def_crit_mult *= (1.0 - hostile_crit_reduction).max(0.05);
                    }
                    trace.record_if(|| CombatEvent {
                        event_type: "crit_resolution".to_string(),
                        round_index,
                        phase: "counter".to_string(),
                        source: EventSource {
                            hostile_ability_id: Some(format!("{}_counter_crit", defender.id)),
                            ..EventSource::default()
                        },
                        weapon_index: Some(weapon_index as u32),
                        values: Map::from_iter([
                            ("roll".to_string(), Value::from(round_f64(def_crit.roll))),
                            ("is_crit".to_string(), Value::Bool(def_is_crit)),
                            ("multiplier".to_string(), Value::from(def_crit_mult)),
                            (
                                "effective_crit_chance".to_string(),
                                Value::from(round_f64(def_crit.effective_crit_chance)),
                            ),
                            (
                                "hull_breach_active".to_string(),
                                Value::Bool(attacker_hull_breach_active_for_crit),
                            ),
                        ]),
                    });
                    let def_w_proc = defender.weapon_proc_chance(weapon_index);
                    let (def_did_proc, _def_proc_roll) =
                        roll_weapon_intrinsic_proc(def_w_proc, &mut rng);
                    let def_proc_mult = if def_did_proc {
                        defender.weapon_proc_multiplier(weapon_index)
                    } else {
                        1.0
                    };
                    let counter_effective_attack =
                        defender_weapon_attack * defender_phase_effects.pre_attack_multiplier();
                    let counter_base_damage = counter_effective_attack
                        * counter_damage_through
                        * def_crit_mult
                        * def_proc_mult;
                    defender_phase_effects.set_pre_attack_damage_base(counter_base_damage);
                    let counter_pre_attack_damage =
                        defender_phase_effects.composed_pre_attack_damage();
                    let counter_after_attack_phase = defender_phase_effects
                        .compose_attack_phase_damage(counter_pre_attack_damage);
                    // Attacker CombatBegin / RoundStart isolytic defense (e.g. Mara Dalen bridge)
                    // must reduce isolytic taken on counter-fire, not only on outbound shots.
                    let attacker_static_iso_def_bonus =
                        weapon_round_base.composed_isolytic_defense_bonus();
                    let counter_iso_taken = compute_isolytic_taken(
                        counter_after_attack_phase,
                        (defender.isolytic_damage
                            + defender_phase_effects.composed_isolytic_damage_bonus())
                        .max(0.0),
                        (attacker.isolytic_defense
                            + attacker_static_iso_def_bonus
                            + defender_phase_effects.composed_isolytic_defense_bonus())
                        .max(0.0),
                        defender_phase_effects
                            .composed_isolytic_cascade_damage_bonus()
                            .max(0.0),
                    );
                    if use_experimental_simd_damage_after_apex_base {
                        let att_mit_for_batch = effective_incoming_shield_mitigation(
                            attacker.shield_mitigation,
                            config,
                            round_index,
                            phase_effects.composed_attacker_shield_mitigation_bonus(),
                        );
                        counter_simd_damage_batch.push(counter_after_attack_phase);
                        counter_simd_isolytic_batch.push(counter_iso_taken);
                        counter_simd_shield_mit_batch.push(att_mit_for_batch);
                        counter_simd_crit_batch.push(def_is_crit);

                        let flush_batch = counter_simd_damage_batch.len() == 4
                            || hit_index + 1 == def_effective_shots;
                        if flush_batch {
                            counter_simd_after_apex_batch
                                .resize(counter_simd_damage_batch.len(), 0.0);
                            let _ = compute_damage_after_apex_batch(
                                &counter_simd_damage_batch,
                                &counter_simd_isolytic_batch,
                                counter_apex_factor,
                                &mut counter_simd_after_apex_batch,
                            );
                            for lane in 0..counter_simd_after_apex_batch.len() {
                                let att_shield_before_counter = attacker_shield_remaining;
                                let lane_mit = if attacker_shield_remaining > 0.0 {
                                    counter_simd_shield_mit_batch[lane]
                                } else {
                                    0.0
                                };
                                let (att_actual_shield_damage, att_hull_damage_this_round) =
                                    apply_shield_hull_split(
                                        counter_simd_after_apex_batch[lane],
                                        lane_mit,
                                        attacker_shield_remaining,
                                    );
                                attacker_shield_remaining =
                                    (attacker_shield_remaining - att_actual_shield_damage).max(0.0);
                                total_attacker_hull_damage += att_hull_damage_this_round;
                                attacker_hull_gross_damage_this_round += att_hull_damage_this_round;
                                attacker_shield_gross_damage_this_round += att_actual_shield_damage;

                                let def_is_crit_lane = counter_simd_crit_batch[lane];
                                if att_hull_damage_this_round > 0.0 {
                                    for effect in defender_attack_filtered
                                        .iter()
                                        .chain(defender_defense_filtered.iter())
                                    {
                                        let effective_effect = scale_effect(effect.effect, false);
                                        if let AbilityEffect::HullBreach {
                                            chance,
                                            duration_rounds,
                                            requires_critical,
                                        } = effective_effect
                                        {
                                            if requires_critical && !def_is_crit_lane {
                                                continue;
                                            }
                                            let hull_breach_roll =
                                                (rng.next_u64() as f64) / (u64::MAX as f64);
                                            let triggered =
                                                hull_breach_roll < chance.clamp(0.0, 1.0);
                                            let breach_before = attacker_hull_breach_rounds;
                                            if triggered {
                                                attacker_hull_breach_rounds =
                                                    attacker_hull_breach_rounds
                                                        .max(duration_rounds.max(1));
                                            }
                                            if breach_before == 0 && attacker_hull_breach_rounds > 0
                                            {
                                                let mut ctx_hb = combat_ctx.clone();
                                                ctx_hb.attacker_hull_pct = 1.0
                                                    - (total_attacker_hull_damage
                                                        / attacker.hull_health.max(0.0))
                                                    .min(1.0);
                                                ctx_hb.attacker_shield_pct =
                                                    if attacker.shield_health > 0.0 {
                                                        attacker_shield_remaining
                                                            / attacker.shield_health
                                                    } else {
                                                        0.0
                                                    };
                                                apply_hull_breach_timing_window(
                                                    &mut RoundPhaseCtx {
                                                        trace: &mut trace,
                                                        rng: &mut rng,
                                                        round_index,
                                                    },
                                                    HullBreachSide::Attacker,
                                                    attacker,
                                                    hull_breach_effects,
                                                    ctx_hb,
                                                    assimilated_rounds_remaining > 0,
                                                    defender_weapon_attack,
                                                    &mut phase_effects_round,
                                                    &mut defender_burning_rounds,
                                                );
                                            }
                                            trace.record_if(|| CombatEvent {
                                                event_type: "hull_breach_trigger".to_string(),
                                                round_index,
                                                phase: "counter".to_string(),
                                                source: EventSource {
                                                    hostile_ability_id: Some(format!(
                                                        "{}_counter_hull_breach",
                                                        defender.id
                                                    )),
                                                    ..EventSource::default()
                                                },
                                                weapon_index: Some(weapon_index as u32),
                                                values: Map::from_iter([
                                                    (
                                                        "roll".to_string(),
                                                        Value::from(round_f64(hull_breach_roll)),
                                                    ),
                                                    (
                                                        "triggered".to_string(),
                                                        Value::Bool(triggered),
                                                    ),
                                                    (
                                                        "chance".to_string(),
                                                        Value::from(round_f64(chance)),
                                                    ),
                                                    (
                                                        "duration_rounds".to_string(),
                                                        Value::from(duration_rounds),
                                                    ),
                                                    (
                                                        "requires_critical".to_string(),
                                                        Value::Bool(requires_critical),
                                                    ),
                                                    (
                                                        "target".to_string(),
                                                        Value::String("attacker".to_string()),
                                                    ),
                                                    (
                                                        "ability".to_string(),
                                                        Value::String(effect.ability_name.clone()),
                                                    ),
                                                ]),
                                            });
                                        }
                                        roll_burning_triggers(
                                            &mut RoundPhaseCtx {
                                                trace: &mut trace,
                                                rng: &mut rng,
                                                round_index,
                                            },
                                            std::slice::from_ref(effect),
                                            false,
                                            "counter",
                                            &defender.id,
                                            Some(weapon_index as u32),
                                            &mut attacker_burning_rounds,
                                        );
                                    }
                                }

                                let attacker_own_shields_broke = att_shield_before_counter > 0.0
                                    && attacker_shield_remaining <= 0.0;
                                if attacker_own_shields_broke {
                                    let mut ctx_self_sb = combat_ctx.clone();
                                    ctx_self_sb.attacker_shield_pct =
                                        if attacker.shield_health > 0.0 {
                                            attacker_shield_remaining / attacker.shield_health
                                        } else {
                                            0.0
                                        };
                                    ctx_self_sb.attacker_hull_pct = 1.0
                                        - (total_attacker_hull_damage
                                            / attacker.hull_health.max(0.0))
                                        .min(1.0);
                                    let self_sb_filtered = filter_effects_by_condition(
                                        self_shield_break_effects,
                                        &ctx_self_sb,
                                    );
                                    record_ability_activations(
                                        &mut trace,
                                        round_index,
                                        "self_shield_break",
                                        attacker,
                                        &self_sb_filtered,
                                        attack_phase_assimilated,
                                    );
                                    for e in &self_sb_filtered {
                                        match scale_effect(e.effect, attack_phase_assimilated) {
                                            AbilityEffect::ShieldRegen(v) => {
                                                attacker_shield_remaining =
                                                    (attacker_shield_remaining + v)
                                                        .min(attacker.shield_health.max(0.0));
                                            }
                                            AbilityEffect::ShieldRegenMaxFraction(f) => {
                                                let heal = f * attacker.shield_health.max(0.0);
                                                attacker_shield_remaining =
                                                    (attacker_shield_remaining + heal)
                                                        .min(attacker.shield_health.max(0.0));
                                            }
                                            AbilityEffect::HullRegen(v) => {
                                                total_attacker_hull_damage =
                                                    (total_attacker_hull_damage - v).max(0.0);
                                            }
                                            AbilityEffect::HullRegenMaxFraction(f) => {
                                                let heal = f * attacker.hull_health.max(0.0);
                                                total_attacker_hull_damage =
                                                    (total_attacker_hull_damage - heal).max(0.0);
                                            }
                                            _ => {}
                                        }
                                    }
                                    let self_sb_combat: Vec<ActiveAbilityEffect> =
                                        self_sb_filtered
                                            .iter()
                                            .filter(|e| {
                                                !matches!(
                                    scale_effect(e.effect, attack_phase_assimilated),
                                    AbilityEffect::ShieldRegen(_)
                                        | AbilityEffect::HullRegen(_)
                                        | AbilityEffect::ShieldRegenMaxFraction(_)
                                        | AbilityEffect::HullRegenMaxFraction(_)
                                        | AbilityEffect::HullRegenPrevRoundFraction(_)
                                        | AbilityEffect::ShieldRegenPrevRoundFraction(_)
                                )
                                            })
                                            .cloned()
                                            .collect();
                                    phase_effects_round.add_effects(
                                        TimingWindow::SelfShieldBreak,
                                        &self_sb_combat,
                                        weapon_base,
                                        attack_phase_assimilated,
                                        round_index,
                                    );
                                    roll_burning_triggers(
                                        &mut RoundPhaseCtx {
                                            trace: &mut trace,
                                            rng: &mut rng,
                                            round_index,
                                        },
                                        &self_sb_combat,
                                        attack_phase_assimilated,
                                        "self_shield_break",
                                        &attacker.id,
                                        None,
                                        &mut defender_burning_rounds,
                                    );
                                }
                                if att_hull_damage_this_round > 0.0 {
                                    counter_hull_damage_this_subround = true;
                                    let receive_damage_assimilated =
                                        assimilated_rounds_remaining > 0;
                                    let mut ctx_receive = combat_ctx.clone();
                                    ctx_receive.attacker_hull_pct = 1.0
                                        - (total_attacker_hull_damage
                                            / attacker.hull_health.max(0.0))
                                        .min(1.0);
                                    ctx_receive.attacker_shield_pct =
                                        if attacker.shield_health > 0.0 {
                                            attacker_shield_remaining / attacker.shield_health
                                        } else {
                                            0.0
                                        };
                                    let receive_damage_filtered = filter_effects_by_condition(
                                        receive_damage_effects,
                                        &ctx_receive,
                                    );
                                    roll_burning_triggers(
                                        &mut RoundPhaseCtx {
                                            trace: &mut trace,
                                            rng: &mut rng,
                                            round_index,
                                        },
                                        &receive_damage_filtered,
                                        receive_damage_assimilated,
                                        "receive_damage",
                                        &attacker.id,
                                        None,
                                        &mut attacker_burning_rounds,
                                    );
                                }
                            }
                            counter_simd_damage_batch.clear();
                            counter_simd_isolytic_batch.clear();
                            counter_simd_shield_mit_batch.clear();
                            counter_simd_crit_batch.clear();
                        }
                        continue;
                    }

                    let counter_after_apex =
                        (counter_after_attack_phase + counter_iso_taken) * counter_apex_factor;
                    let att_shield_mitigation = if attacker_shield_remaining > 0.0 {
                        effective_incoming_shield_mitigation(
                            attacker.shield_mitigation,
                            config,
                            round_index,
                            phase_effects.composed_attacker_shield_mitigation_bonus(),
                        )
                    } else {
                        0.0
                    };
                    let att_shield_before_counter = attacker_shield_remaining;
                    let (att_actual_shield_damage, att_hull_damage_this_round) =
                        apply_shield_hull_split(
                            counter_after_apex,
                            att_shield_mitigation,
                            attacker_shield_remaining,
                        );
                    attacker_shield_remaining =
                        (attacker_shield_remaining - att_actual_shield_damage).max(0.0);
                    total_attacker_hull_damage += att_hull_damage_this_round;
                    attacker_hull_gross_damage_this_round += att_hull_damage_this_round;
                    attacker_shield_gross_damage_this_round += att_actual_shield_damage;

                    if att_hull_damage_this_round > 0.0 {
                        for effect in defender_attack_filtered
                            .iter()
                            .chain(defender_defense_filtered.iter())
                        {
                            let effective_effect = scale_effect(effect.effect, false);
                            if let AbilityEffect::HullBreach {
                                chance,
                                duration_rounds,
                                requires_critical,
                            } = effective_effect
                            {
                                if requires_critical && !def_is_crit {
                                    continue;
                                }
                                let hull_breach_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                                let triggered = hull_breach_roll < chance.clamp(0.0, 1.0);
                                let breach_before = attacker_hull_breach_rounds;
                                if triggered {
                                    attacker_hull_breach_rounds =
                                        attacker_hull_breach_rounds.max(duration_rounds.max(1));
                                }
                                if breach_before == 0 && attacker_hull_breach_rounds > 0 {
                                    let mut ctx_hb = combat_ctx.clone();
                                    ctx_hb.attacker_hull_pct = 1.0
                                        - (total_attacker_hull_damage
                                            / attacker.hull_health.max(0.0))
                                        .min(1.0);
                                    ctx_hb.attacker_shield_pct = if attacker.shield_health > 0.0 {
                                        attacker_shield_remaining / attacker.shield_health
                                    } else {
                                        0.0
                                    };
                                    apply_hull_breach_timing_window(
                                        &mut RoundPhaseCtx {
                                            trace: &mut trace,
                                            rng: &mut rng,
                                            round_index,
                                        },
                                        HullBreachSide::Attacker,
                                        attacker,
                                        hull_breach_effects,
                                        ctx_hb,
                                        assimilated_rounds_remaining > 0,
                                        defender_weapon_attack,
                                        &mut phase_effects_round,
                                        &mut defender_burning_rounds,
                                    );
                                }
                                trace.record_if(|| CombatEvent {
                                    event_type: "hull_breach_trigger".to_string(),
                                    round_index,
                                    phase: "counter".to_string(),
                                    source: EventSource {
                                        hostile_ability_id: Some(format!(
                                            "{}_counter_hull_breach",
                                            defender.id
                                        )),
                                        ..EventSource::default()
                                    },
                                    weapon_index: Some(weapon_index as u32),
                                    values: Map::from_iter([
                                        (
                                            "roll".to_string(),
                                            Value::from(round_f64(hull_breach_roll)),
                                        ),
                                        ("triggered".to_string(), Value::Bool(triggered)),
                                        ("chance".to_string(), Value::from(round_f64(chance))),
                                        (
                                            "duration_rounds".to_string(),
                                            Value::from(duration_rounds),
                                        ),
                                        (
                                            "requires_critical".to_string(),
                                            Value::Bool(requires_critical),
                                        ),
                                        (
                                            "target".to_string(),
                                            Value::String("attacker".to_string()),
                                        ),
                                        (
                                            "ability".to_string(),
                                            Value::String(effect.ability_name.clone()),
                                        ),
                                    ]),
                                });
                            }
                            roll_burning_triggers(
                                &mut RoundPhaseCtx {
                                    trace: &mut trace,
                                    rng: &mut rng,
                                    round_index,
                                },
                                std::slice::from_ref(effect),
                                false,
                                "counter",
                                &defender.id,
                                Some(weapon_index as u32),
                                &mut attacker_burning_rounds,
                            );
                        }
                    }

                    let attacker_own_shields_broke =
                        att_shield_before_counter > 0.0 && attacker_shield_remaining <= 0.0;
                    if attacker_own_shields_broke {
                        let mut ctx_self_sb = combat_ctx.clone();
                        ctx_self_sb.attacker_shield_pct = if attacker.shield_health > 0.0 {
                            attacker_shield_remaining / attacker.shield_health
                        } else {
                            0.0
                        };
                        ctx_self_sb.attacker_hull_pct = 1.0
                            - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0);
                        let self_sb_filtered =
                            filter_effects_by_condition(self_shield_break_effects, &ctx_self_sb);
                        record_ability_activations(
                            &mut trace,
                            round_index,
                            "self_shield_break",
                            attacker,
                            &self_sb_filtered,
                            attack_phase_assimilated,
                        );
                        for e in &self_sb_filtered {
                            match scale_effect(e.effect, attack_phase_assimilated) {
                                AbilityEffect::ShieldRegen(v) => {
                                    attacker_shield_remaining = (attacker_shield_remaining + v)
                                        .min(attacker.shield_health.max(0.0));
                                }
                                AbilityEffect::ShieldRegenMaxFraction(f) => {
                                    let heal = f * attacker.shield_health.max(0.0);
                                    attacker_shield_remaining = (attacker_shield_remaining + heal)
                                        .min(attacker.shield_health.max(0.0));
                                }
                                AbilityEffect::HullRegen(v) => {
                                    total_attacker_hull_damage =
                                        (total_attacker_hull_damage - v).max(0.0);
                                }
                                AbilityEffect::HullRegenMaxFraction(f) => {
                                    let heal = f * attacker.hull_health.max(0.0);
                                    total_attacker_hull_damage =
                                        (total_attacker_hull_damage - heal).max(0.0);
                                }
                                _ => {}
                            }
                        }
                        let self_sb_combat: Vec<ActiveAbilityEffect> = self_sb_filtered
                            .iter()
                            .filter(|e| {
                                !matches!(
                                    scale_effect(e.effect, attack_phase_assimilated),
                                    AbilityEffect::ShieldRegen(_)
                                        | AbilityEffect::HullRegen(_)
                                        | AbilityEffect::ShieldRegenMaxFraction(_)
                                        | AbilityEffect::HullRegenMaxFraction(_)
                                        | AbilityEffect::HullRegenPrevRoundFraction(_)
                                        | AbilityEffect::ShieldRegenPrevRoundFraction(_)
                                )
                            })
                            .cloned()
                            .collect();
                        phase_effects_round.add_effects(
                            TimingWindow::SelfShieldBreak,
                            &self_sb_combat,
                            weapon_base,
                            attack_phase_assimilated,
                            round_index,
                        );
                        roll_burning_triggers(
                            &mut RoundPhaseCtx {
                                trace: &mut trace,
                                rng: &mut rng,
                                round_index,
                            },
                            &self_sb_combat,
                            attack_phase_assimilated,
                            "self_shield_break",
                            &attacker.id,
                            None,
                            &mut defender_burning_rounds,
                        );
                    }
                    if att_hull_damage_this_round > 0.0 {
                        counter_hull_damage_this_subround = true;
                        let receive_damage_assimilated = assimilated_rounds_remaining > 0;
                        let mut ctx_receive = combat_ctx.clone();
                        ctx_receive.attacker_hull_pct = 1.0
                            - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0);
                        ctx_receive.attacker_shield_pct = if attacker.shield_health > 0.0 {
                            attacker_shield_remaining / attacker.shield_health
                        } else {
                            0.0
                        };
                        let receive_damage_filtered =
                            filter_effects_by_condition(receive_damage_effects, &ctx_receive);
                        roll_burning_triggers(
                            &mut RoundPhaseCtx {
                                trace: &mut trace,
                                rng: &mut rng,
                                round_index,
                            },
                            &receive_damage_filtered,
                            receive_damage_assimilated,
                            "receive_damage",
                            &attacker.id,
                            None,
                            &mut attacker_burning_rounds,
                        );
                    }
                }

                if counter_hull_damage_this_subround {
                    let mut ctx_rd = combat_ctx.clone();
                    ctx_rd.attacker_hull_pct =
                        1.0 - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0);
                    ctx_rd.attacker_shield_pct = if attacker.shield_health > 0.0 {
                        attacker_shield_remaining / attacker.shield_health
                    } else {
                        0.0
                    };
                    let receive_damage_filtered =
                        filter_effects_by_condition(receive_damage_effects, &ctx_rd);
                    record_ability_activations(
                        &mut trace,
                        round_index,
                        "receive_damage",
                        attacker,
                        &receive_damage_filtered,
                        assimilated_rounds_remaining > 0,
                    );
                    phase_effects_round.add_effects(
                        TimingWindow::ReceiveDamage,
                        &receive_damage_filtered,
                        defender_weapon_attack,
                        assimilated_rounds_remaining > 0,
                        round_index,
                    );
                }
            }

            let ctx_after_subround = CombatContext {
                round_index,
                defender_hull_pct: 1.0
                    - (total_hull_damage / defender.hull_health.max(0.0)).min(1.0),
                defender_shield_pct: if defender.shield_health > 0.0 {
                    defender_shield_remaining / defender.shield_health
                } else {
                    1.0
                },
                attacker_hull_pct: 1.0
                    - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0),
                attacker_shield_pct: if attacker.shield_health > 0.0 {
                    attacker_shield_remaining / attacker.shield_health
                } else {
                    1.0
                },
                attacker_morale_active: combat_ctx.attacker_morale_active,
                defender_morale_active: defender_morale_rounds_remaining > 0,
                defender_burning_active: defender_burning_rounds > 0,
                defender_hull_breach_active: defender_hull_breach_rounds > 0,
                attacker_burning_active: attacker_burning_rounds > 0,
                attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
                defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
                defender_faction,
                attacker_owner_faction: combat_ctx.attacker_owner_faction,
                defender_hull_faction_id: config.defender_hull_faction_id,
                defender_ship_type,
                attacker_ship_type,
                attacker_ship_id: std::sync::Arc::clone(attacker_ship_id_arc),
                defender_is_npc_hostile,
                defender_is_player_ship,
                attacker_tal_assigned_captain_or_bridge: combat_ctx
                    .attacker_tal_assigned_captain_or_bridge,
                defender_hostile_tag_mask: config.defender_hostile_tag_mask,
                engagement_enemy_types: std::sync::Arc::clone(engagement_enemy_types_arc),
                combat_battle_type_id: combat_ctx.combat_battle_type_id,
                defender_level: combat_ctx.defender_level,
            };
            let after_subround_filtered =
                filter_effects_by_condition(after_subround_effects, &ctx_after_subround);
            record_ability_activations(
                &mut trace,
                round_index,
                "after_subround",
                attacker,
                &after_subround_filtered,
                attack_phase_assimilated,
            );
            roll_burning_triggers(
                &mut RoundPhaseCtx {
                    trace: &mut trace,
                    rng: &mut rng,
                    round_index,
                },
                &after_subround_filtered,
                attack_phase_assimilated,
                "after_subround",
                &attacker.id,
                None,
                &mut defender_burning_rounds,
            );
            after_subround_carry.add_effects(
                TimingWindow::AfterSubround,
                &after_subround_filtered,
                weapon_base,
                attack_phase_assimilated,
                round_index,
            );
        }

        let (bonus_damage, burning_damage, attacker_burning_damage) = apply_round_end_phase(
            &mut trace,
            &mut rng,
            &ctx_template,
            round_index,
            attacker,
            defender,
            &combat_ctx,
            round_end_effects,
            defender_round_end_effects,
            &round_end_filtered,
            round_end_assimilated_early,
            &mut phase_effects_round,
            &mut defender_burning_rounds,
            attacker_burning_rounds,
            defender_assimilated_rounds_remaining,
            &mut total_hull_damage,
            &mut total_attacker_hull_damage,
            &mut attacker_hull_gross_damage_this_round,
            &mut attacker_shield_remaining,
            &mut defender_shield_remaining,
        );

        defender_burning_rounds = defender_burning_rounds.saturating_sub(1);
        defender_hull_breach_rounds = defender_hull_breach_rounds.saturating_sub(1);
        attacker_burning_rounds = attacker_burning_rounds.saturating_sub(1);
        attacker_hull_breach_rounds = attacker_hull_breach_rounds.saturating_sub(1);
        assimilated_rounds_remaining = assimilated_rounds_remaining.saturating_sub(1);
        defender_assimilated_rounds_remaining =
            defender_assimilated_rounds_remaining.saturating_sub(1);
        defender_morale_rounds_remaining = defender_morale_rounds_remaining.saturating_sub(1);

        trace.record_if(|| CombatEvent {
            event_type: "end_of_round_effects".to_string(),
            round_index,
            phase: "end".to_string(),
            source: EventSource {
                player_bonus_source: Some("round_end_bonus".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                (
                    "bonus_damage".to_string(),
                    Value::from(round_f64(bonus_damage)),
                ),
                (
                    "burning_damage".to_string(),
                    Value::from(round_f64(burning_damage)),
                ),
                (
                    "attacker_burning_damage".to_string(),
                    Value::from(round_f64(attacker_burning_damage)),
                ),
                (
                    "running_hull_damage".to_string(),
                    Value::from(round_f64(total_hull_damage)),
                ),
            ]),
        });

        if emit_snapshots {
            let ctx_end = CombatContext {
                round_index,
                defender_hull_pct: 1.0
                    - (total_hull_damage / defender.hull_health.max(0.0)).min(1.0),
                defender_shield_pct: if defender.shield_health > 0.0 {
                    defender_shield_remaining / defender.shield_health
                } else {
                    1.0
                },
                attacker_hull_pct: 1.0
                    - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0),
                attacker_shield_pct: if attacker.shield_health > 0.0 {
                    attacker_shield_remaining / attacker.shield_health
                } else {
                    1.0
                },
                attacker_morale_active: combat_ctx.attacker_morale_active,
                defender_morale_active: defender_morale_rounds_remaining > 0,
                defender_burning_active: defender_burning_rounds > 0,
                defender_hull_breach_active: defender_hull_breach_rounds > 0,
                attacker_burning_active: attacker_burning_rounds > 0,
                attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
                defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
                defender_faction,
                attacker_owner_faction: config.attacker_owner_faction,
                defender_hull_faction_id: config.defender_hull_faction_id,
                defender_ship_type,
                attacker_ship_type,
                attacker_ship_id: std::sync::Arc::clone(attacker_ship_id_arc),
                defender_is_npc_hostile,
                defender_is_player_ship,
                attacker_tal_assigned_captain_or_bridge,
                defender_hostile_tag_mask: config.defender_hostile_tag_mask,
                engagement_enemy_types: std::sync::Arc::clone(engagement_enemy_types_arc),
                combat_battle_type_id: None,
                defender_level: config.defender_level,
            };
            let snap = build_combat_state_snapshot(
                SnapshotAnchor::EndOfRoundPostEffects,
                round_index,
                None,
                None,
                attacker,
                defender,
                total_hull_damage,
                total_attacker_hull_damage,
                defender_shield_remaining,
                attacker_shield_remaining,
                &ctx_end,
                assimilated_rounds_remaining,
                defender_assimilated_rounds_remaining,
                Some(&phase_effects_round),
            );
            trace.record(state_snapshot_as_combat_event(&snap));
        }

        // Fight ends when defender or attacker runs out of hull (HHP).
        let defender_hull_now = (defender.hull_health - total_hull_damage).max(0.0);
        let mut attacker_hull_now = (attacker.hull_health - total_attacker_hull_damage).max(0.0);
        if defender_hull_now <= 0.0 {
            let kill_ctx = CombatContext {
                round_index,
                defender_hull_pct: 0.0,
                defender_shield_pct: if defender.shield_health > 0.0 {
                    defender_shield_remaining / defender.shield_health
                } else {
                    0.0
                },
                attacker_hull_pct: 1.0
                    - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0),
                attacker_shield_pct: if attacker.shield_health > 0.0 {
                    attacker_shield_remaining / attacker.shield_health
                } else {
                    1.0
                },
                attacker_morale_active: combat_ctx.attacker_morale_active,
                defender_morale_active: combat_ctx.defender_morale_active,
                defender_burning_active: combat_ctx.defender_burning_active,
                defender_hull_breach_active: combat_ctx.defender_hull_breach_active,
                attacker_burning_active: combat_ctx.attacker_burning_active,
                attacker_hull_breach_active: combat_ctx.attacker_hull_breach_active,
                defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
                defender_faction,
                attacker_owner_faction: combat_ctx.attacker_owner_faction,
                defender_hull_faction_id: config.defender_hull_faction_id,
                defender_ship_type,
                attacker_ship_type,
                attacker_ship_id: std::sync::Arc::clone(attacker_ship_id_arc),
                defender_is_npc_hostile,
                defender_is_player_ship,
                attacker_tal_assigned_captain_or_bridge: combat_ctx
                    .attacker_tal_assigned_captain_or_bridge,
                defender_hostile_tag_mask: config.defender_hostile_tag_mask,
                engagement_enemy_types: std::sync::Arc::clone(engagement_enemy_types_arc),
                combat_battle_type_id: combat_ctx.combat_battle_type_id,
                defender_level: combat_ctx.defender_level,
            };
            let kill_filtered = filter_effects_by_condition(kill_effects, &kill_ctx);
            let kill_assimilated = assimilated_rounds_remaining > 0;
            record_ability_activations(
                &mut trace,
                round_index,
                "kill",
                attacker,
                &kill_filtered,
                kill_assimilated,
            );
            roll_burning_triggers(
                &mut RoundPhaseCtx {
                    trace: &mut trace,
                    rng: &mut rng,
                    round_index,
                },
                &kill_filtered,
                kill_assimilated,
                "kill",
                &attacker.id,
                None,
                &mut defender_burning_rounds,
            );
            let on_kill_regen = sum_on_kill_hull_regen(&kill_filtered, kill_assimilated);
            total_attacker_hull_damage = (total_attacker_hull_damage
                - on_kill_regen * attacker.hull_health.max(0.0))
            .max(0.0);
            attacker_hull_now = (attacker.hull_health - total_attacker_hull_damage).max(0.0);
        }
        attacker_hull_gross_damage_last_round = attacker_hull_gross_damage_this_round;
        attacker_hull_gross_damage_this_round = 0.0;
        attacker_shield_gross_damage_last_round = attacker_shield_gross_damage_this_round;
        attacker_shield_gross_damage_this_round = 0.0;

        if defender_hull_now <= 0.0 || attacker_hull_now <= 0.0 {
            break;
        }
    }

    let combat_end_ctx = CombatContext {
        round_index: rounds_completed,
        defender_hull_pct: 1.0 - (total_hull_damage / defender.hull_health.max(0.0)).min(1.0),
        defender_shield_pct: if defender.shield_health > 0.0 {
            defender_shield_remaining / defender.shield_health
        } else {
            1.0
        },
        attacker_hull_pct: 1.0
            - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0),
        attacker_shield_pct: if attacker.shield_health > 0.0 {
            attacker_shield_remaining / attacker.shield_health
        } else {
            1.0
        },
        attacker_morale_active: false,
        defender_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction,
        attacker_owner_faction: config.attacker_owner_faction,
        defender_hull_faction_id: config.defender_hull_faction_id,
        defender_ship_type,
        attacker_ship_type,
        attacker_ship_id: std::sync::Arc::clone(attacker_ship_id_arc),
        defender_is_npc_hostile,
        defender_is_player_ship,
        attacker_tal_assigned_captain_or_bridge,
        defender_hostile_tag_mask: config.defender_hostile_tag_mask,
        engagement_enemy_types: std::sync::Arc::clone(engagement_enemy_types_arc),
        combat_battle_type_id: None,
        defender_level: config.defender_level,
    };
    let combat_end_filtered = filter_effects_by_condition(combat_end_effects, &combat_end_ctx);
    record_ability_activations(
        &mut trace,
        rounds_completed,
        "combat_end",
        attacker,
        &combat_end_filtered,
        false,
    );

    // Apply CombatEnd timing window effects: hull/shield damage, healing, and regen from
    // attacker crew abilities that trigger at combat end (e.g. final-blow damage, deathrattle,
    // combat-summary bonuses). Mirrors the RoundEnd pattern: build an EffectAccumulator,
    // add CombatEnd-tagged effects, then apply regen and damage to the appropriate sides.
    {
        let mut combat_end_acc = EffectAccumulator::default();
        combat_end_acc.add_effects(
            TimingWindow::CombatEnd,
            &combat_end_filtered,
            attacker.attack,
            false,
            rounds_completed,
        );

        // Regen from attacker crew CombatEnd effects (applies to attacker's ship).
        let ce_shield_regen = combat_end_acc.composed_shield_regen();
        let ce_hull_regen = combat_end_acc.composed_hull_regen();
        let ce_shield_regen_frac = combat_end_acc.composed_shield_regen_max_fraction();
        let ce_hull_regen_frac = combat_end_acc.composed_hull_regen_max_fraction();
        let ce_shield_heal =
            ce_shield_regen + ce_shield_regen_frac * attacker.shield_health.max(0.0);
        let ce_hull_heal = ce_hull_regen + ce_hull_regen_frac * attacker.hull_health.max(0.0);
        attacker_shield_remaining =
            (attacker_shield_remaining + ce_shield_heal).min(attacker.shield_health.max(0.0));
        total_attacker_hull_damage = (total_attacker_hull_damage - ce_hull_heal).max(0.0);

        // Apply CombatEnd damage from attacker crew effects to the defender.
        let ce_defender_damage = combat_end_acc.compose_round_end_damage(0.0);
        if ce_defender_damage > 0.0 {
            let (dmg_to_shield, dmg_to_hull) = apply_shield_hull_split(
                ce_defender_damage,
                defender.shield_mitigation,
                defender_shield_remaining,
            );
            defender_shield_remaining = (defender_shield_remaining - dmg_to_shield).max(0.0);
            total_shield_damage += dmg_to_shield;
            total_hull_damage += dmg_to_hull;
        }

        trace.record_if(|| CombatEvent {
            event_type: "combat_end_effects".to_string(),
            round_index: rounds_completed,
            phase: "combat_end".to_string(),
            source: EventSource {
                player_bonus_source: Some("combat_end".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                (
                    "attacker_shield_heal".to_string(),
                    Value::from(round_f64(ce_shield_heal)),
                ),
                (
                    "attacker_hull_heal".to_string(),
                    Value::from(round_f64(ce_hull_heal)),
                ),
                (
                    "defender_damage".to_string(),
                    Value::from(round_f64(ce_defender_damage)),
                ),
            ]),
        });
    }

    let total_damage = total_hull_damage + total_shield_damage;
    let attacker_hull_remaining = (attacker.hull_health - total_attacker_hull_damage).max(0.0);
    let defender_hull_remaining = (defender.hull_health - total_hull_damage).max(0.0);
    let winner_by_round_limit = rounds_completed == MAX_COMBAT_ROUNDS
        && defender_hull_remaining > 0.0
        && attacker_hull_remaining > 0.0;
    let attacker_won = if attacker_hull_remaining <= 0.0 {
        false
    } else if defender_hull_remaining <= 0.0 {
        true
    } else if winner_by_round_limit {
        attacker_hull_remaining >= defender_hull_remaining
    } else {
        false
    };

    SimulationResult {
        total_damage: round_f64(total_damage),
        attacker_won,
        winner_by_round_limit,
        rounds_simulated: rounds_completed,
        attacker_hull_remaining: round_f64(attacker_hull_remaining),
        defender_hull_remaining: round_f64(defender_hull_remaining),
        defender_shield_remaining: round_f64(defender_shield_remaining),
        attacker_shield_remaining: round_f64(attacker_shield_remaining),
        events: trace.events(),
        conqueror_borg_beam_suppression,
    }
}

/// Batch-process M combat trials from a single precomputed setup with different seeds.
/// Returns M results in the order of the provided seeds.
pub fn simulate_combat_batch(setup: &PreCombatSetup, seeds: &[u64]) -> Vec<SimulationResult> {
    seeds
        .iter()
        .map(|&seed| simulate_combat_from_setup(setup, seed))
        .collect()
}

/// Borrowed per-round context shared by engine-internal helpers (burning, assimilation,
/// hull breach). Keeps the round-phase contract explicit: every per-round helper threads
/// the same `Rng`, `TraceCollector`, and `round_index` so seeds stay deterministic.
pub(crate) struct RoundPhaseCtx<'a> {
    pub trace: &'a mut TraceCollector,
    pub rng: &'a mut Rng,
    pub round_index: u32,
}

/// Round-end phase, everything before the status-counter decrements (which stay in the
/// orchestrator): attacker RoundEnd accumulator add + activations, the after-weapons context,
/// round-end burning rolls, apex-scaled bonus/burning hull damage, attacker round-end regen, and
/// defender round-end regen. Returns `(bonus_damage, burning_damage, attacker_burning_damage)`
/// for the end-of-round trace event.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); RNG draw order and
/// float evaluation order unchanged.
#[allow(clippy::too_many_arguments)]
#[inline]
fn apply_round_end_phase(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    ctx_template: &CombatCtxTemplate,
    round_index: u32,
    attacker: &Combatant,
    defender: &Combatant,
    combat_ctx: &CombatContext,
    round_end_effects: &[ActiveAbilityEffect],
    defender_round_end_effects: &[ActiveAbilityEffect],
    round_end_filtered: &[ActiveAbilityEffect],
    round_end_assimilated_early: bool,
    phase_effects_round: &mut EffectAccumulator,
    defender_burning_rounds: &mut u32,
    attacker_burning_rounds: u32,
    defender_assimilated_rounds_remaining: u32,
    total_hull_damage: &mut f64,
    total_attacker_hull_damage: &mut f64,
    attacker_hull_gross_damage_this_round: &mut f64,
    attacker_shield_remaining: &mut f64,
    defender_shield_remaining: &mut f64,
) -> (f64, f64, f64) {
    phase_effects_round.add_effects(
        TimingWindow::RoundEnd,
        round_end_filtered,
        attacker.attack,
        round_end_assimilated_early,
        round_index,
    );

    record_ability_activations(
        trace,
        round_index,
        "round_end",
        attacker,
        round_end_filtered,
        round_end_assimilated_early,
    );

    // RoundEnd burning: roll after outbound/counter damage so conditions use end-of-round hull/shield;
    // procs apply before the burn tick for this same round.
    let ctx_after_weapons = ctx_template.at(
        round_index,
        CtxVitals {
            defender_hull_pct: 1.0 - (*total_hull_damage / defender.hull_health.max(0.0)).min(1.0),
            defender_shield_pct: if defender.shield_health > 0.0 {
                *defender_shield_remaining / defender.shield_health
            } else {
                1.0
            },
            attacker_hull_pct: 1.0
                - (*total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0),
            attacker_shield_pct: if attacker.shield_health > 0.0 {
                *attacker_shield_remaining / attacker.shield_health
            } else {
                1.0
            },
        },
        CtxStatusFlags {
            attacker_morale_active: combat_ctx.attacker_morale_active,
            defender_morale_active: combat_ctx.defender_morale_active,
            defender_burning_active: combat_ctx.defender_burning_active,
            defender_hull_breach_active: combat_ctx.defender_hull_breach_active,
            attacker_burning_active: combat_ctx.attacker_burning_active,
            attacker_hull_breach_active: combat_ctx.attacker_hull_breach_active,
            defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
        },
    );
    let round_end_burn_filtered =
        filter_effects_by_condition(round_end_effects, &ctx_after_weapons);
    roll_burning_triggers(
        &mut RoundPhaseCtx {
            trace: &mut *trace,
            rng: &mut *rng,
            round_index,
        },
        &round_end_burn_filtered,
        round_end_assimilated_early,
        "round_end",
        &attacker.id,
        None,
        defender_burning_rounds,
    );

    let round_end_apex_shred =
        (attacker.apex_shred + phase_effects_round.composed_apex_shred_bonus()).max(0.0);
    let round_end_apex_barrier =
        (defender.apex_barrier + phase_effects_round.composed_apex_barrier_bonus()).max(0.0);
    let round_end_apex_factor =
        10000.0 / (10000.0 + round_end_apex_barrier / (1.0 + round_end_apex_shred).max(EPSILON));
    let bonus_damage = phase_effects_round.compose_round_end_damage(attacker.end_of_round_damage);
    // Burning: binary per-round tick — 1% of defender max hull while state active (Δ HHP_burn = 0.01 × HHP_max); no officer/research scaling of that rate.
    let burning_damage = if *defender_burning_rounds > 0 {
        defender.hull_health.max(0.0) * BURNING_HULL_DAMAGE_PER_ROUND
    } else {
        0.0
    };
    let attacker_burning_damage = if attacker_burning_rounds > 0 {
        attacker.hull_health.max(0.0) * BURNING_HULL_DAMAGE_PER_ROUND
    } else {
        0.0
    };
    // Round-end and burning apply to hull only (shields do not absorb these).
    *total_hull_damage += (bonus_damage + burning_damage) * round_end_apex_factor;
    *total_attacker_hull_damage += defender.end_of_round_damage;
    *attacker_hull_gross_damage_this_round += defender.end_of_round_damage;
    *total_attacker_hull_damage += attacker_burning_damage * round_end_apex_factor;
    *attacker_hull_gross_damage_this_round += attacker_burning_damage * round_end_apex_factor;

    // Regen: shield and hull restoration at round end from attacker's crew (officer/data regen effects apply to the ship with the crew).
    let shield_regen = phase_effects_round.composed_shield_regen();
    let hull_regen = phase_effects_round.composed_hull_regen();
    let shield_regen_frac = phase_effects_round.composed_shield_regen_max_fraction();
    let hull_regen_frac = phase_effects_round.composed_hull_regen_max_fraction();
    let shield_heal = shield_regen + shield_regen_frac * attacker.shield_health.max(0.0);
    let hull_heal = hull_regen + hull_regen_frac * attacker.hull_health.max(0.0);
    *attacker_shield_remaining =
        (*attacker_shield_remaining + shield_heal).min(attacker.shield_health.max(0.0));
    *total_attacker_hull_damage = (*total_attacker_hull_damage - hull_heal).max(0.0);

    let defender_round_end_filtered =
        filter_effects_by_condition(defender_round_end_effects, &ctx_after_weapons);
    let defender_re_assimilated = defender_assimilated_rounds_remaining > 0;
    record_ability_activations(
        trace,
        round_index,
        "round_end",
        defender,
        &defender_round_end_filtered,
        defender_re_assimilated,
    );
    let mut defender_round_end_acc = EffectAccumulator::default();
    defender_round_end_acc.add_effects(
        TimingWindow::RoundEnd,
        &defender_round_end_filtered,
        defender.attack,
        defender_re_assimilated,
        round_index,
    );
    let def_re_shield = defender_round_end_acc.composed_shield_regen();
    let def_re_hull = defender_round_end_acc.composed_hull_regen();
    let def_re_shield_frac = defender_round_end_acc.composed_shield_regen_max_fraction();
    let def_re_hull_frac = defender_round_end_acc.composed_hull_regen_max_fraction();
    let def_re_shield_heal = def_re_shield + def_re_shield_frac * defender.shield_health.max(0.0);
    let def_re_hull_heal = def_re_hull + def_re_hull_frac * defender.hull_health.max(0.0);
    *defender_shield_remaining =
        (*defender_shield_remaining + def_re_shield_heal).min(defender.shield_health.max(0.0));
    *total_hull_damage = (*total_hull_damage - def_re_hull_heal).max(0.0);

    (bonus_damage, burning_damage, attacker_burning_damage)
}

/// Defender-shield-break processing, run once when the defender's shields first hit zero within a
/// weapon sub-round: attacker `ShieldBreak` effects (accumulator add, burning rolls, fire-delay
/// rolls) followed by defender `ShieldBreak` regen, with non-regen defender effects pushed onto
/// the carry list consumed by counter-fire in later weapons this round.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); RNG draw order
/// unchanged.
#[allow(clippy::too_many_arguments)]
#[inline]
fn process_defender_shield_break(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    round_index: u32,
    attacker: &Combatant,
    defender: &Combatant,
    combat_ctx: &CombatContext,
    shield_break_effects: &[ActiveAbilityEffect],
    defender_shield_break_effects: &[ActiveAbilityEffect],
    attack_phase_assimilated: bool,
    weapon_base: f64,
    phase_effects_round: &mut EffectAccumulator,
    defender_burning_rounds: &mut u32,
    defender_weapon_fire_delayed_rounds: &mut u32,
    defender_shield_remaining: &mut f64,
    total_hull_damage: &mut f64,
    defender_shield_break_carry: &mut Vec<ActiveAbilityEffect>,
) {
    let shield_break_filtered = filter_effects_by_condition(shield_break_effects, combat_ctx);
    record_ability_activations(
        trace,
        round_index,
        "shield_break",
        attacker,
        &shield_break_filtered,
        attack_phase_assimilated,
    );
    phase_effects_round.add_effects(
        TimingWindow::ShieldBreak,
        &shield_break_filtered,
        weapon_base,
        attack_phase_assimilated,
        round_index,
    );
    roll_burning_triggers(
        &mut RoundPhaseCtx {
            trace: &mut *trace,
            rng: &mut *rng,
            round_index,
        },
        &shield_break_filtered,
        attack_phase_assimilated,
        "shield_break",
        &attacker.id,
        None,
        defender_burning_rounds,
    );

    for effect in &shield_break_filtered {
        if let AbilityEffect::DefenderFireDelay {
            chance,
            delay_rounds,
            requires_critical,
        } = scale_effect(effect.effect, attack_phase_assimilated)
        {
            apply_defender_fire_delay(
                trace,
                rng,
                round_index,
                "shield_break",
                &attacker.id,
                &effect.ability_name,
                chance,
                delay_rounds,
                requires_critical,
                false,
                defender_weapon_fire_delayed_rounds,
            );
        }
    }

    let def_sb_filtered = filter_effects_by_condition(defender_shield_break_effects, combat_ctx);
    record_ability_activations(
        trace,
        round_index,
        "shield_break",
        defender,
        &def_sb_filtered,
        false,
    );
    for e in &def_sb_filtered {
        match scale_effect(e.effect, false) {
            AbilityEffect::ShieldRegen(v) => {
                *defender_shield_remaining =
                    (*defender_shield_remaining + v).min(defender.shield_health.max(0.0));
            }
            AbilityEffect::ShieldRegenMaxFraction(f) => {
                let heal = f * defender.shield_health.max(0.0);
                *defender_shield_remaining =
                    (*defender_shield_remaining + heal).min(defender.shield_health.max(0.0));
            }
            AbilityEffect::HullRegen(v) => {
                *total_hull_damage = (*total_hull_damage - v).max(0.0);
            }
            AbilityEffect::HullRegenMaxFraction(f) => {
                let heal = f * defender.hull_health.max(0.0);
                *total_hull_damage = (*total_hull_damage - heal).max(0.0);
            }
            _ => defender_shield_break_carry.push(e.clone()),
        }
    }
}

/// Defender crew ShotsBonus for counter-fire: process the defender's RoundStart effects with the
/// same combat context the attacker's rolls used.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); RNG draw order
/// unchanged.
fn roll_defender_round_start_shots_bonus(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    round_index: u32,
    defender_round_start_effects: &[ActiveAbilityEffect],
    combat_ctx: &CombatContext,
    defender_assimilated_rounds_remaining: u32,
    defender_shots_bonus_entries: &mut Vec<(f64, u32)>,
) {
    let defender_rstart_filtered =
        filter_effects_by_condition(defender_round_start_effects, combat_ctx);
    let defender_rstart_assimilated = defender_assimilated_rounds_remaining > 0;
    for effect in &defender_rstart_filtered {
        let effective_effect = scale_effect(effect.effect, defender_rstart_assimilated);
        if let AbilityEffect::ShotsBonus {
            chance,
            bonus_pct,
            duration_rounds,
        } = effective_effect
        {
            let shots_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = shots_roll < chance.clamp(0.0, 1.0);
            if triggered {
                let duration = duration_rounds.max(1);
                defender_shots_bonus_entries.push((bonus_pct, round_index + duration));
            }
            trace.record_if(|| CombatEvent {
                event_type: "defender_shots_bonus_trigger".to_string(),
                round_index,
                phase: "round_start".to_string(),
                source: EventSource {
                    hostile_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(shots_roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("bonus_pct".to_string(), Value::from(round_f64(bonus_pct))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                ]),
            });
        }
    }
}

/// Morale activation roll: at most one `Morale` effect (first in `bench` order) is rolled, after
/// every other round-start RNG consumer. Returns whether morale is active this round.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); RNG draw order
/// unchanged.
fn roll_morale_activation(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    round_index: u32,
    bench: &[ActiveAbilityEffect],
    round_start_assimilated: bool,
) -> bool {
    let morale_source = bench.iter().find_map(|effect| {
        if let AbilityEffect::Morale(chance) = scale_effect(effect.effect, round_start_assimilated)
        {
            Some((effect.ability_name.clone(), chance.clamp(0.0, 1.0)))
        } else {
            None
        }
    });
    if let Some((morale_source, morale_chance)) = morale_source {
        let morale_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let triggered = morale_roll < morale_chance;
        trace.record_if(|| CombatEvent {
            event_type: "morale_activation".to_string(),
            round_index,
            phase: "round_start".to_string(),
            source: EventSource {
                ship_ability_id: Some(morale_source),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                ("triggered".to_string(), Value::Bool(triggered)),
                ("roll".to_string(), Value::from(round_f64(morale_roll))),
                ("chance".to_string(), Value::from(round_f64(morale_chance))),
                (
                    "applied_to".to_string(),
                    Value::String("primary_piercing".to_string()),
                ),
                (
                    "multiplier".to_string(),
                    Value::from(1.0 + MORALE_PRIMARY_PIERCING_BONUS),
                ),
            ]),
        });
        triggered
    } else {
        false
    }
}

/// Attacker round-start regen: CombatBegin + RoundStart shield/hull regen applied before weapon
/// sub-rounds (then cleared from the accumulator so round-end regen doesn't double-apply), plus
/// the previous-round hull/shield heal fractions (PIC Hugh style). No RNG draws.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); float evaluation
/// order unchanged.
#[allow(clippy::too_many_arguments)]
#[inline]
fn apply_attacker_round_start_regen(
    attacker: &Combatant,
    round_index: u32,
    round_start_effects: &[ActiveAbilityEffect],
    combat_ctx: &CombatContext,
    round_start_assimilated: bool,
    phase_effects: &mut EffectAccumulator,
    attacker_shield_remaining: &mut f64,
    total_attacker_hull_damage: &mut f64,
    attacker_hull_gross_damage_last_round: f64,
    attacker_shield_gross_damage_last_round: f64,
) {
    // Shield/hull regen from [`TimingWindow::CombatBegin`] + [`TimingWindow::RoundStart`]: apply
    // at the **start** of this round (before weapon sub-rounds), then remove from the accumulator
    // so it is not applied again at round end with ReceiveDamage/RoundEnd regen.
    let att_rs_shield = phase_effects.composed_shield_regen();
    let att_rs_hull = phase_effects.composed_hull_regen();
    let att_rs_shield_frac = phase_effects.composed_shield_regen_max_fraction();
    let att_rs_hull_frac = phase_effects.composed_hull_regen_max_fraction();
    if att_rs_shield != 0.0
        || att_rs_hull != 0.0
        || att_rs_shield_frac != 0.0
        || att_rs_hull_frac != 0.0
    {
        let shield_heal = att_rs_shield + att_rs_shield_frac * attacker.shield_health.max(0.0);
        let hull_heal = att_rs_hull + att_rs_hull_frac * attacker.hull_health.max(0.0);
        *attacker_shield_remaining =
            (*attacker_shield_remaining + shield_heal).min(attacker.shield_health.max(0.0));
        *total_attacker_hull_damage = (*total_attacker_hull_damage - hull_heal).max(0.0);
    }
    phase_effects.clear_shield_hull_regen_stacks();

    // PIC Hugh: heal a fraction of hull damage taken in the **previous** combat round (round 1: none).
    let round_start_prev_heal = filter_effects_by_condition(round_start_effects, combat_ctx);
    let prev_round_frac = EffectAccumulator::sum_hull_regen_prev_round_fraction(
        &round_start_prev_heal,
        round_start_assimilated,
    )
    .min(1.0);
    if round_index >= 2 && prev_round_frac > 0.0 && attacker_hull_gross_damage_last_round > 0.0 {
        let heal = prev_round_frac * attacker_hull_gross_damage_last_round;
        *total_attacker_hull_damage = (*total_attacker_hull_damage - heal).max(0.0);
    }

    let shield_prev_frac = EffectAccumulator::sum_shield_regen_prev_round_fraction(
        &round_start_prev_heal,
        round_start_assimilated,
    )
    .min(1.0);
    if round_index >= 2 && shield_prev_frac > 0.0 && attacker_shield_gross_damage_last_round > 0.0 {
        let heal_sh = shield_prev_frac * attacker_shield_gross_damage_last_round;
        *attacker_shield_remaining =
            (*attacker_shield_remaining + heal_sh).min(attacker.shield_health.max(0.0));
    }
}

/// Attacker round-start proc rolls over the pre-filtered `bench` effects: assimilation,
/// hull breach (with the first-transition timing window), burning, shots bonus, defender
/// fire delay, and random defender state — one roll block per effect in list order.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); the RNG draw
/// order is load-bearing for same-seed determinism and must not change.
#[allow(clippy::too_many_arguments)]
#[inline]
fn roll_attacker_round_start_procs(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    round_index: u32,
    attacker: &Combatant,
    bench: &[ActiveAbilityEffect],
    round_start_assimilated: bool,
    hull_breach_effects: &[ActiveAbilityEffect],
    combat_ctx: &CombatContext,
    phase_effects: &mut EffectAccumulator,
    assimilated_rounds_remaining: &mut u32,
    defender_burning_rounds: &mut u32,
    defender_hull_breach_rounds: &mut u32,
    defender_assimilated_rounds_remaining: &mut u32,
    defender_morale_rounds_remaining: &mut u32,
    shots_bonus_entries: &mut Vec<(f64, u32)>,
    defender_weapon_fire_delayed_rounds: &mut u32,
) {
    for effect in bench {
        let effective_effect = scale_effect(effect.effect, round_start_assimilated);

        if let AbilityEffect::Assimilated {
            chance,
            duration_rounds,
        } = effective_effect
        {
            let assimilated_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = assimilated_roll < chance.clamp(0.0, 1.0);
            if triggered {
                *assimilated_rounds_remaining =
                    (*assimilated_rounds_remaining).max(duration_rounds.max(1));
            }
            trace.record_if(|| CombatEvent {
                event_type: "assimilated_trigger".to_string(),
                round_index,
                phase: "round_start".to_string(),
                source: EventSource {
                    officer_id: Some(attacker.id.clone()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(assimilated_roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                ]),
            });
        }

        if let AbilityEffect::HullBreach {
            chance,
            duration_rounds,
            requires_critical,
        } = effective_effect
        {
            if requires_critical {
                continue;
            }

            let hull_breach_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = hull_breach_roll < chance.clamp(0.0, 1.0);
            let breach_before = *defender_hull_breach_rounds;
            if triggered {
                *defender_hull_breach_rounds =
                    (*defender_hull_breach_rounds).max(duration_rounds.max(1));
            }
            if breach_before == 0 && *defender_hull_breach_rounds > 0 {
                let weapon_base_rs = attacker.weapon_attack(0).unwrap_or(attacker.attack);
                apply_hull_breach_timing_window(
                    &mut RoundPhaseCtx {
                        trace: &mut *trace,
                        rng: &mut *rng,
                        round_index,
                    },
                    HullBreachSide::Defender,
                    attacker,
                    hull_breach_effects,
                    combat_ctx.clone(),
                    round_start_assimilated,
                    weapon_base_rs,
                    phase_effects,
                    defender_burning_rounds,
                );
            }
            trace.record_if(|| CombatEvent {
                event_type: "hull_breach_trigger".to_string(),
                round_index,
                phase: "round_start".to_string(),
                source: EventSource {
                    officer_id: Some(attacker.id.clone()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(hull_breach_roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                ]),
            });
        }

        roll_burning_triggers(
            &mut RoundPhaseCtx {
                trace: &mut *trace,
                rng: &mut *rng,
                round_index,
            },
            std::slice::from_ref(effect),
            round_start_assimilated,
            "round_start",
            &attacker.id,
            None,
            defender_burning_rounds,
        );

        if let AbilityEffect::ShotsBonus {
            chance,
            bonus_pct,
            duration_rounds,
        } = effective_effect
        {
            let shots_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = shots_roll < chance.clamp(0.0, 1.0);
            if triggered {
                let duration = duration_rounds.max(1);
                shots_bonus_entries.push((bonus_pct, round_index + duration));
            }
            trace.record_if(|| CombatEvent {
                event_type: "shots_bonus_trigger".to_string(),
                round_index,
                phase: "round_start".to_string(),
                source: EventSource {
                    officer_id: Some(attacker.id.clone()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(shots_roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("bonus_pct".to_string(), Value::from(round_f64(bonus_pct))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                ]),
            });
        }

        if let AbilityEffect::DefenderFireDelay {
            chance,
            delay_rounds,
            requires_critical,
        } = effective_effect
        {
            apply_defender_fire_delay(
                trace,
                rng,
                round_index,
                "round_start",
                &attacker.id,
                &effect.ability_name,
                chance,
                delay_rounds,
                requires_critical,
                false,
                defender_weapon_fire_delayed_rounds,
            );
        }

        if let AbilityEffect::RandomDefenderState {
            chance,
            duration_rounds,
            state_outcome_count,
            state_outcomes,
        } = effective_effect
        {
            let roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = roll < chance.clamp(0.0, 1.0);
            let mut state_applied = String::from("none");
            if triggered {
                let breach_before = *defender_hull_breach_rounds;
                let weights = crate::combat::abilities::random_defender_state_outcomes(
                    state_outcome_count,
                    &state_outcomes,
                );
                let state_id =
                    crate::combat::abilities::pick_weighted_state_id(weights, rng.next_u64());
                let label = crate::combat::abilities::apply_defender_random_state_id(
                    state_id,
                    duration_rounds,
                    defender_burning_rounds,
                    defender_hull_breach_rounds,
                    defender_assimilated_rounds_remaining,
                    defender_morale_rounds_remaining,
                );
                state_applied = label.to_string();
                if breach_before == 0 && *defender_hull_breach_rounds > 0 && label == "hull_breach"
                {
                    let weapon_base_rs = attacker.weapon_attack(0).unwrap_or(attacker.attack);
                    apply_hull_breach_timing_window(
                        &mut RoundPhaseCtx {
                            trace: &mut *trace,
                            rng: &mut *rng,
                            round_index,
                        },
                        HullBreachSide::Defender,
                        attacker,
                        hull_breach_effects,
                        combat_ctx.clone(),
                        round_start_assimilated,
                        weapon_base_rs,
                        phase_effects,
                        defender_burning_rounds,
                    );
                }
            }
            trace.record_if(|| CombatEvent {
                event_type: "random_defender_state_trigger".to_string(),
                round_index,
                phase: "round_start".to_string(),
                source: EventSource {
                    officer_id: Some(attacker.id.clone()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ("state".to_string(), Value::String(state_applied)),
                ]),
            });
        }
    }
}

/// The per-fight immutable [`CombatContext`] fields, captured once per trial so per-round
/// snapshots don't repeat 24-field literals. [`CombatCtxTemplate::at`] stamps a context; every
/// per-round field stays explicit at the call site so extractions remain verbatim-equivalent
/// (task 12). `combat_battle_type_id` is fixed `None`: nothing in the round loop ever sets it.
pub(crate) struct CombatCtxTemplate {
    defender_faction: OpponentFactionTag,
    attacker_owner_faction: OpponentFactionTag,
    defender_hull_faction_id: i64,
    defender_ship_type: ShipType,
    attacker_ship_type: ShipType,
    attacker_ship_id: std::sync::Arc<str>,
    defender_is_npc_hostile: bool,
    defender_is_player_ship: bool,
    attacker_tal_assigned_captain_or_bridge: bool,
    defender_hostile_tag_mask: u32,
    engagement_enemy_types: std::sync::Arc<EnemyTypes>,
    defender_level: Option<u32>,
}

/// The four hull/shield percentages of a [`CombatContext`] snapshot.
pub(crate) struct CtxVitals {
    pub defender_hull_pct: f64,
    pub defender_shield_pct: f64,
    pub attacker_hull_pct: f64,
    pub attacker_shield_pct: f64,
}

/// The seven status flags of a [`CombatContext`] snapshot.
pub(crate) struct CtxStatusFlags {
    pub attacker_morale_active: bool,
    pub defender_morale_active: bool,
    pub defender_burning_active: bool,
    pub defender_hull_breach_active: bool,
    pub attacker_burning_active: bool,
    pub attacker_hull_breach_active: bool,
    pub defender_assimilated_active: bool,
}

impl CombatCtxTemplate {
    fn at(&self, round_index: u32, vitals: CtxVitals, flags: CtxStatusFlags) -> CombatContext {
        CombatContext {
            round_index,
            defender_hull_pct: vitals.defender_hull_pct,
            defender_shield_pct: vitals.defender_shield_pct,
            attacker_hull_pct: vitals.attacker_hull_pct,
            attacker_shield_pct: vitals.attacker_shield_pct,
            attacker_morale_active: flags.attacker_morale_active,
            defender_morale_active: flags.defender_morale_active,
            defender_burning_active: flags.defender_burning_active,
            defender_hull_breach_active: flags.defender_hull_breach_active,
            attacker_burning_active: flags.attacker_burning_active,
            attacker_hull_breach_active: flags.attacker_hull_breach_active,
            defender_assimilated_active: flags.defender_assimilated_active,
            defender_faction: self.defender_faction,
            attacker_owner_faction: self.attacker_owner_faction,
            defender_hull_faction_id: self.defender_hull_faction_id,
            defender_ship_type: self.defender_ship_type,
            attacker_ship_type: self.attacker_ship_type,
            attacker_ship_id: std::sync::Arc::clone(&self.attacker_ship_id),
            defender_is_npc_hostile: self.defender_is_npc_hostile,
            defender_is_player_ship: self.defender_is_player_ship,
            attacker_tal_assigned_captain_or_bridge: self.attacker_tal_assigned_captain_or_bridge,
            defender_hostile_tag_mask: self.defender_hostile_tag_mask,
            engagement_enemy_types: std::sync::Arc::clone(&self.engagement_enemy_types),
            combat_battle_type_id: None,
            defender_level: self.defender_level,
        }
    }
}

/// The vitals percentages flowing out of [`apply_defender_round_start`] into the attacker
/// round-start context: attacker percentages are sampled *before* defender regen/drain, defender
/// percentages *after* — preserving the original in-loop sampling points exactly.
pub(crate) struct DefenderRoundStartVitals {
    pub attacker_hull_pct_round: f64,
    pub attacker_shield_pct_round: f64,
    pub defender_hull_pct_round: f64,
    pub defender_shield_pct_round: f64,
}

/// Defender round-start phase: assimilation-extension rolls, defender round-start regen, and
/// crew-driven per-round shield drain.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); RNG draw order
/// (assimilation rolls only) and float evaluation order are unchanged.
#[allow(clippy::too_many_arguments)]
#[inline]
fn apply_defender_round_start(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    ctx_template: &CombatCtxTemplate,
    round_index: u32,
    attacker: &Combatant,
    defender: &Combatant,
    attacker_crew: &CrewConfiguration,
    defender_round_start_effects: &[ActiveAbilityEffect],
    defender_morale_rounds_remaining: u32,
    defender_burning_rounds: u32,
    defender_hull_breach_rounds: u32,
    attacker_burning_rounds: u32,
    attacker_hull_breach_rounds: u32,
    total_hull_damage: &mut f64,
    total_attacker_hull_damage: f64,
    defender_shield_remaining: &mut f64,
    attacker_shield_remaining: f64,
    defender_assimilated_rounds_remaining: &mut u32,
) -> DefenderRoundStartVitals {
    let defender_hull_pct_for_def_round_start =
        1.0 - (*total_hull_damage / defender.hull_health.max(0.0)).min(1.0);
    let defender_shield_pct_for_def_round_start = if defender.shield_health > 0.0 {
        *defender_shield_remaining / defender.shield_health
    } else {
        1.0
    };
    let attacker_hull_pct_round =
        1.0 - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0);
    let attacker_shield_pct_round = if attacker.shield_health > 0.0 {
        attacker_shield_remaining / attacker.shield_health
    } else {
        1.0
    };

    // Defender RoundStart assimilate procs before attacker `combat_ctx` so `TargetHasAssimilated` gates see them.
    let ctx_def_round_start = ctx_template.at(
        round_index,
        CtxVitals {
            defender_hull_pct: defender_hull_pct_for_def_round_start,
            defender_shield_pct: defender_shield_pct_for_def_round_start,
            attacker_hull_pct: attacker_hull_pct_round,
            attacker_shield_pct: attacker_shield_pct_round,
        },
        CtxStatusFlags {
            attacker_morale_active: false,
            defender_morale_active: defender_morale_rounds_remaining > 0,
            defender_burning_active: defender_burning_rounds > 0,
            defender_hull_breach_active: defender_hull_breach_rounds > 0,
            attacker_burning_active: attacker_burning_rounds > 0,
            attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
            defender_assimilated_active: *defender_assimilated_rounds_remaining > 0,
        },
    );
    let defender_rs_for_assim =
        filter_effects_by_condition(defender_round_start_effects, &ctx_def_round_start);
    let def_rs_assim_active = *defender_assimilated_rounds_remaining > 0;
    roll_assimilated_extensions_from_effects(
        &mut RoundPhaseCtx {
            trace: &mut *trace,
            rng: &mut *rng,
            round_index,
        },
        &defender_rs_for_assim,
        def_rs_assim_active,
        "round_start",
        &defender.id,
        defender_assimilated_rounds_remaining,
    );

    let def_rs_shield = EffectAccumulator::sum_shield_regen_from_effects(
        &defender_rs_for_assim,
        def_rs_assim_active,
    );
    let def_rs_hull =
        EffectAccumulator::sum_hull_regen_from_effects(&defender_rs_for_assim, def_rs_assim_active);
    let def_rs_shield_frac = EffectAccumulator::sum_shield_regen_max_fraction_from_effects(
        &defender_rs_for_assim,
        def_rs_assim_active,
    );
    let def_rs_hull_frac = EffectAccumulator::sum_hull_regen_max_fraction_from_effects(
        &defender_rs_for_assim,
        def_rs_assim_active,
    );
    if def_rs_shield != 0.0
        || def_rs_hull != 0.0
        || def_rs_shield_frac != 0.0
        || def_rs_hull_frac != 0.0
    {
        let shield_heal = def_rs_shield + def_rs_shield_frac * defender.shield_health.max(0.0);
        let hull_heal = def_rs_hull + def_rs_hull_frac * defender.hull_health.max(0.0);
        *defender_shield_remaining =
            (*defender_shield_remaining + shield_heal).min(defender.shield_health.max(0.0));
        *total_hull_damage = (*total_hull_damage - hull_heal).max(0.0);
    }

    let shield_drain_ctx = ctx_template.at(
        round_index,
        CtxVitals {
            defender_hull_pct: 1.0 - (*total_hull_damage / defender.hull_health.max(0.0)).min(1.0),
            defender_shield_pct: if defender.shield_health > 0.0 {
                *defender_shield_remaining / defender.shield_health
            } else {
                1.0
            },
            attacker_hull_pct: attacker_hull_pct_round,
            attacker_shield_pct: if attacker.shield_health > 0.0 {
                attacker_shield_remaining / attacker.shield_health
            } else {
                1.0
            },
        },
        CtxStatusFlags {
            attacker_morale_active: false,
            defender_morale_active: defender_morale_rounds_remaining > 0,
            defender_burning_active: defender_burning_rounds > 0,
            defender_hull_breach_active: defender_hull_breach_rounds > 0,
            attacker_burning_active: attacker_burning_rounds > 0,
            attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
            defender_assimilated_active: *defender_assimilated_rounds_remaining > 0,
        },
    );
    let (shield_drain_frac, shield_drain_rounds) =
        defender_shield_drain_per_round_from_crew(attacker_crew, &shield_drain_ctx);
    if shield_drain_frac > 0.0
        && round_in_inclusive_first_n(round_index, shield_drain_rounds)
        && defender.shield_health > 0.0
    {
        let drain = shield_drain_frac * defender.shield_health;
        *defender_shield_remaining = (*defender_shield_remaining - drain).max(0.0);
    }

    let defender_hull_pct_round =
        1.0 - (*total_hull_damage / defender.hull_health.max(0.0)).min(1.0);
    let defender_shield_pct_round = if defender.shield_health > 0.0 {
        *defender_shield_remaining / defender.shield_health
    } else {
        1.0
    };

    DefenderRoundStartVitals {
        attacker_hull_pct_round,
        attacker_shield_pct_round,
        defender_hull_pct_round,
        defender_shield_pct_round,
    }
}

/// Combat-begin phase: record attacker ability activations, roll burning triggers, and roll
/// `DefenderFireDelay` / `ShotsBonus` effects from the pre-filtered combat-begin list.
///
/// Extracted verbatim from `simulate_combat_from_setup` (roadmap task 12); the RNG draw order
/// (burning rolls, then per-effect fire-delay/shots-bonus rolls in list order) is load-bearing
/// for same-seed determinism and must not change.
#[allow(clippy::too_many_arguments)]
#[inline]
fn apply_combat_begin_phase(
    trace: &mut TraceCollector,
    rng: &mut Rng,
    attacker: &Combatant,
    combat_begin_filtered: &[ActiveAbilityEffect],
    combat_begin_assimilated: bool,
    defender_burning_rounds: &mut u32,
    defender_weapon_fire_delayed_rounds: &mut u32,
    shots_bonus_entries: &mut Vec<(f64, u32)>,
) {
    record_ability_activations(
        trace,
        0,
        "combat_begin",
        attacker,
        combat_begin_filtered,
        combat_begin_assimilated,
    );
    roll_burning_triggers(
        &mut RoundPhaseCtx {
            trace: &mut *trace,
            rng: &mut *rng,
            round_index: 0,
        },
        combat_begin_filtered,
        combat_begin_assimilated,
        "combat_begin",
        &attacker.id,
        None,
        defender_burning_rounds,
    );

    for effect in combat_begin_filtered {
        let effective_effect = scale_effect(effect.effect, combat_begin_assimilated);
        if let AbilityEffect::DefenderFireDelay {
            chance,
            delay_rounds,
            requires_critical,
        } = effective_effect
        {
            apply_defender_fire_delay(
                trace,
                rng,
                0,
                "combat_begin",
                &attacker.id,
                &effect.ability_name,
                chance,
                delay_rounds,
                requires_critical,
                false,
                defender_weapon_fire_delayed_rounds,
            );
        }
        if let AbilityEffect::ShotsBonus {
            chance,
            bonus_pct,
            duration_rounds,
        } = effective_effect
        {
            let shots_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = shots_roll < chance.clamp(0.0, 1.0);
            if triggered {
                let duration = duration_rounds.max(1);
                shots_bonus_entries.push((bonus_pct, duration));
            }
            trace.record_if(|| CombatEvent {
                event_type: "shots_bonus_trigger".to_string(),
                round_index: 0,
                phase: "combat_begin".to_string(),
                source: EventSource {
                    officer_id: Some(attacker.id.clone()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(shots_roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("bonus_pct".to_string(), Value::from(round_f64(bonus_pct))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                ]),
            });
        }
    }
}

/// Which side just entered hull breach. Selects the [`CombatContext`] flag and trace phase
/// label for [`apply_hull_breach_timing_window`].
#[derive(Copy, Clone)]
pub(crate) enum HullBreachSide {
    Defender,
    Attacker,
}

impl HullBreachSide {
    fn phase(self) -> &'static str {
        match self {
            HullBreachSide::Defender => "hull_breach",
            HullBreachSide::Attacker => "attacker_hull_breach",
        }
    }
}

/// Rolls `Burning` procs from pre-filtered effects. Order of calls each round must stay stable for deterministic seeds:
/// combat_begin (once); round_start; per shot: attack_phase then defense_phase; shield_break; hull_breach state entry;
/// receive_damage (hull); round_end (before burn tick); kill when defender dies.
fn roll_burning_triggers(
    pc: &mut RoundPhaseCtx<'_>,
    effects: &[ActiveAbilityEffect],
    assimilated_active: bool,
    phase: &'static str,
    attacker_id: &str,
    weapon_index: Option<u32>,
    burning_rounds: &mut u32,
) {
    for effect in effects {
        let effective_effect = scale_effect(effect.effect, assimilated_active);
        if let AbilityEffect::Burning {
            chance,
            duration_rounds,
        } = effective_effect
        {
            let burning_roll = (pc.rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = burning_roll < chance.clamp(0.0, 1.0);
            if triggered {
                let d = duration_rounds.max(1);
                *burning_rounds = (*burning_rounds).max(d);
            }
            let round_index = pc.round_index;
            pc.trace.record_if(|| CombatEvent {
                event_type: "burning_trigger".to_string(),
                round_index,
                phase: phase.to_string(),
                source: EventSource {
                    officer_id: Some(attacker_id.to_string()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(burning_roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                ]),
            });
        }
    }
}

/// Extends attacker or defender Assimilate duration from pre-filtered effects (matches legacy inline proc rules).
fn roll_assimilated_extensions_from_effects(
    pc: &mut RoundPhaseCtx<'_>,
    effects: &[ActiveAbilityEffect],
    assimilated_active_for_scale: bool,
    phase: &'static str,
    ship_id_for_trace: &str,
    assimilated_rounds: &mut u32,
) {
    for effect in effects {
        let effective_effect = scale_effect(effect.effect, assimilated_active_for_scale);
        if let AbilityEffect::Assimilated {
            chance,
            duration_rounds,
        } = effective_effect
        {
            let assimilated_roll = (pc.rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = assimilated_roll < chance.clamp(0.0, 1.0);
            if triggered {
                *assimilated_rounds = (*assimilated_rounds).max(duration_rounds.max(1));
            }
            let round_index = pc.round_index;
            pc.trace.record_if(|| CombatEvent {
                event_type: "assimilated_trigger".to_string(),
                round_index,
                phase: phase.to_string(),
                source: EventSource {
                    officer_id: Some(ship_id_for_trace.to_string()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(assimilated_roll))),
                    ("triggered".to_string(), Value::Bool(triggered)),
                    ("chance".to_string(), Value::from(round_f64(chance))),
                    ("duration_rounds".to_string(), Value::from(duration_rounds)),
                ]),
            });
        }
    }
}

/// `on_hull_breach` / [`TimingWindow::HullBreach`] effects run when one side **enters** the hull-breached
/// state (first stack of [`AbilityEffect::HullBreach`] duration), not from a hull HP fraction threshold.
/// `side` selects which combatant just entered breach: [`HullBreachSide::Defender`] for the enemy,
/// [`HullBreachSide::Attacker`] for the player. Burning sub-procs from these effects always damage the
/// defender (enemy on fire) regardless of which side breached.
#[allow(clippy::too_many_arguments)] // engine-internal; merged defender/attacker variants
fn apply_hull_breach_timing_window(
    pc: &mut RoundPhaseCtx<'_>,
    side: HullBreachSide,
    attacker: &Combatant,
    hull_breach_effects: &[ActiveAbilityEffect],
    mut ctx: CombatContext,
    assimilated_active: bool,
    weapon_base: f64,
    accumulator: &mut EffectAccumulator,
    defender_burning_rounds: &mut u32,
) {
    match side {
        HullBreachSide::Defender => ctx.defender_hull_breach_active = true,
        HullBreachSide::Attacker => ctx.attacker_hull_breach_active = true,
    }
    let phase = side.phase();
    let hull_breach_filtered = filter_effects_by_condition(hull_breach_effects, &ctx);
    record_ability_activations(
        pc.trace,
        pc.round_index,
        phase,
        attacker,
        &hull_breach_filtered,
        assimilated_active,
    );
    accumulator.add_effects(
        TimingWindow::HullBreach,
        &hull_breach_filtered,
        weapon_base,
        assimilated_active,
        pc.round_index,
    );
    roll_burning_triggers(
        pc,
        &hull_breach_filtered,
        assimilated_active,
        phase,
        &attacker.id,
        None,
        defender_burning_rounds,
    );
}

/// Same as [`simulate_combat_with_defender_faction`] with [`OpponentFactionTag::Unknown`]
/// (faction-gated ship abilities never satisfy the faction condition).
pub fn simulate_combat(
    attacker: &Combatant,
    defender: &Combatant,
    config: &SimulationConfig,
    attacker_crew: &CrewConfiguration,
) -> SimulationResult {
    simulate_combat_with_defender_faction(
        attacker,
        defender,
        config,
        attacker_crew,
        OpponentFactionTag::Unknown,
    )
}

pub fn simulate_combat_with_defender_faction(
    attacker: &Combatant,
    defender: &Combatant,
    config: &SimulationConfig,
    attacker_crew: &CrewConfiguration,
    defender_faction: OpponentFactionTag,
) -> SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        config,
        attacker_crew,
        defender_faction,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: Vec::new() },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn simulate_combat_with_defender_faction_and_defender_crew(
    attacker: &Combatant,
    defender: &Combatant,
    config: &SimulationConfig,
    attacker_crew: &CrewConfiguration,
    defender_faction: OpponentFactionTag,
    defender_ship_type: ShipType,
    attacker_ship_type: ShipType,
    defender_is_npc_hostile: bool,
    defender_is_player_ship: bool,
    defender_crew: &CrewConfiguration,
) -> SimulationResult {
    let setup = build_combat_setup(
        attacker,
        defender,
        config,
        attacker_crew,
        defender_faction,
        defender_ship_type,
        attacker_ship_type,
        defender_is_npc_hostile,
        defender_is_player_ship,
        defender_crew,
    );
    simulate_combat_from_setup(&setup, config.seed)
}
