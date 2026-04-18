//! Combat loop orchestration. Types, mitigation, effects, events, and damage helpers live in sibling modules.

pub use crate::combat::events::serialize_events_json;
pub use crate::combat::mitigation::{
    apply_morale_primary_piercing, component_mitigation, isolytic_damage, mitigation,
    mitigation_for_hostile, mitigation_with_morale, mitigation_with_mystery,
    pierce_damage_through_bonus, MITIGATION_CEILING, MITIGATION_FLOOR, PIERCE_CAP,
};
pub use crate::combat::types::{
    effective_shots_for_weapon, round_half_even, AttackerStats, CombatEvent, Combatant,
    DefenderStats, EventSource, FightResult, OpponentFactionTag, ShipType, SimulationConfig,
    SimulationResult, TraceCollector, TraceMode, WeaponStats, BATTLESHIP_COEFFICIENTS, EPSILON,
    EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS, MAX_COMBAT_ROUNDS,
    MORALE_PRIMARY_PIERCING_BONUS, SURVEY_COEFFICIENTS,
};

use serde_json::{Map, Value};

use crate::combat::abilities::{
    active_effects_for_timing, apply_duplicate_officer_policy,
    attacker_crew_tal_assigned_captain_or_bridge, filter_effects_by_condition,
    hostile_crit_damage_reduction_from_crew, sum_mitigation_additive, AbilityEffect,
    ActiveAbilityEffect, CombatContext, CrewConfiguration, TimingWindow,
};
use crate::combat::condition::round_in_inclusive_first_n;
use crate::combat::crit::resolve_vehicle_weapon_crit;
use crate::combat::damage::{
    apply_shield_hull_split, compute_apex_damage_factor, compute_damage_through_factor,
    compute_isolytic_taken,
};
use crate::combat::effect_accumulator::{
    record_ability_activations, scale_effect, sum_on_kill_hull_regen, EffectAccumulator,
};
use crate::combat::events::round_f64;
use crate::combat::proc::{accumulate_proc_attack_effects, roll_weapon_intrinsic_proc};
use crate::combat::rng::Rng;
use crate::combat::types::BURNING_HULL_DAMAGE_PER_ROUND;

/// Rolls `Burning` procs from pre-filtered effects. Order of calls each round must stay stable for deterministic seeds:
/// combat_begin (once); round_start; per shot: attack_phase then defense_phase; shield_break; hull_breach state entry;
/// receive_damage (hull); round_end (before burn tick); kill when defender dies.
#[allow(clippy::too_many_arguments)] // engine-internal; splitting would obscure round-phase contract
fn roll_burning_triggers(
    effects: &[ActiveAbilityEffect],
    assimilated_active: bool,
    rng: &mut Rng,
    trace: &mut TraceCollector,
    round_index: u32,
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
            let burning_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = burning_roll < chance.clamp(0.0, 1.0);
            if triggered {
                let d = duration_rounds.max(1);
                *burning_rounds = (*burning_rounds).max(d);
            }
            trace.record_if(|| CombatEvent {
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
    effects: &[ActiveAbilityEffect],
    assimilated_active_for_scale: bool,
    rng: &mut Rng,
    trace: &mut TraceCollector,
    round_index: u32,
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
            let assimilated_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let triggered = assimilated_roll < chance.clamp(0.0, 1.0);
            if triggered {
                *assimilated_rounds = (*assimilated_rounds).max(duration_rounds.max(1));
            }
            trace.record_if(|| CombatEvent {
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

/// `on_hull_breach` / [`TimingWindow::HullBreach`] effects run when the defender **enters** the hull-breached
/// state (first stack of [`AbilityEffect::HullBreach`] duration), not from a hull HP fraction threshold.
#[allow(clippy::too_many_arguments)]
fn apply_hull_breach_timing_window(
    trace: &mut TraceCollector,
    round_index: u32,
    attacker: &Combatant,
    hull_breach_effects: &[ActiveAbilityEffect],
    mut ctx: CombatContext,
    assimilated_active: bool,
    weapon_base: f64,
    accumulator: &mut EffectAccumulator,
    rng: &mut Rng,
    defender_burning_rounds: &mut u32,
) {
    ctx.defender_hull_breach_active = true;
    let hull_breach_filtered = filter_effects_by_condition(hull_breach_effects, &ctx);
    record_ability_activations(
        trace,
        round_index,
        "hull_breach",
        attacker,
        &hull_breach_filtered,
        assimilated_active,
    );
    accumulator.add_effects(
        TimingWindow::HullBreach,
        &hull_breach_filtered,
        weapon_base,
        assimilated_active,
        round_index,
    );
    roll_burning_triggers(
        &hull_breach_filtered,
        assimilated_active,
        rng,
        trace,
        round_index,
        "hull_breach",
        &attacker.id,
        None,
        defender_burning_rounds,
    );
}

/// When the **attacker** (player) enters hull breach, run player [`TimingWindow::HullBreach`] follow-ups.
/// Burning sub-procs from those effects still apply to the **defender** (enemy on fire), same as
/// [`apply_hull_breach_timing_window`].
#[allow(clippy::too_many_arguments)]
fn apply_attacker_hull_breach_timing_window(
    trace: &mut TraceCollector,
    round_index: u32,
    attacker: &Combatant,
    hull_breach_effects: &[ActiveAbilityEffect],
    mut ctx: CombatContext,
    assimilated_active: bool,
    weapon_base: f64,
    accumulator: &mut EffectAccumulator,
    rng: &mut Rng,
    defender_burning_rounds: &mut u32,
) {
    ctx.attacker_hull_breach_active = true;
    let hull_breach_filtered = filter_effects_by_condition(hull_breach_effects, &ctx);
    record_ability_activations(
        trace,
        round_index,
        "attacker_hull_breach",
        attacker,
        &hull_breach_filtered,
        assimilated_active,
    );
    accumulator.add_effects(
        TimingWindow::HullBreach,
        &hull_breach_filtered,
        weapon_base,
        assimilated_active,
        round_index,
    );
    roll_burning_triggers(
        &hull_breach_filtered,
        assimilated_active,
        rng,
        trace,
        round_index,
        "attacker_hull_breach",
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
    config: SimulationConfig,
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
    config: SimulationConfig,
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
    config: SimulationConfig,
    attacker_crew: &CrewConfiguration,
    defender_faction: OpponentFactionTag,
    defender_ship_type: ShipType,
    attacker_ship_type: ShipType,
    defender_is_npc_hostile: bool,
    defender_is_player_ship: bool,
    defender_crew: &CrewConfiguration,
) -> SimulationResult {
    let attacker_crew = apply_duplicate_officer_policy(attacker_crew);
    let defender_crew = apply_duplicate_officer_policy(defender_crew);
    let attacker_tal_assigned_captain_or_bridge =
        attacker_crew_tal_assigned_captain_or_bridge(&attacker_crew);

    let (hostile_crit_reduction, hostile_crit_reduction_rounds) =
        hostile_crit_damage_reduction_from_crew(&attacker_crew);
    let mut rng = Rng::new(config.seed);
    let mut trace = TraceCollector::new(matches!(config.trace_mode, TraceMode::Events));
    let mut total_hull_damage = 0.0;
    let mut total_shield_damage = 0.0;
    let mut defender_shield_remaining = defender.shield_health.max(0.0);
    let mut attacker_shield_remaining = attacker.shield_health.max(0.0);
    let max_att_hull = attacker.hull_health.max(0.0);
    let mut total_attacker_hull_damage =
        config.initial_attacker_hull_damage.clamp(0.0, max_att_hull);
    let mut defender_hull_breach_rounds = 0_u32;
    let mut defender_burning_rounds = 0_u32;
    let mut attacker_hull_breach_rounds = 0_u32;
    let mut attacker_burning_rounds = 0_u32;
    let mut assimilated_rounds_remaining = 0_u32;
    let mut defender_assimilated_rounds_remaining = 0_u32;
    // Active shots bonuses: (bonus_pct, expires_round). B_shots(r) = sum of bonus where expires_round >= r.
    let mut shots_bonus_entries: Vec<(f64, u32)> = Vec::new();
    let combat_begin_effects = active_effects_for_timing(&attacker_crew, TimingWindow::CombatBegin);
    let combat_begin_ctx = CombatContext {
        round_index: 0,
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
        defender_faction,
        defender_hull_faction_id: config.defender_hull_faction_id,
        defender_ship_type,
        attacker_ship_type,
        attacker_ship_id: attacker.id.clone(),
        defender_is_npc_hostile,
        defender_is_player_ship,
        attacker_tal_assigned_captain_or_bridge,
    };
    let combat_begin_filtered =
        filter_effects_by_condition(&combat_begin_effects, &combat_begin_ctx);
    let attacker_mitigation_additive = sum_mitigation_additive(&combat_begin_filtered);
    let shield_break_effects = active_effects_for_timing(&attacker_crew, TimingWindow::ShieldBreak);
    let self_shield_break_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::SelfShieldBreak);
    let kill_effects = active_effects_for_timing(&attacker_crew, TimingWindow::Kill);
    let hull_breach_effects = active_effects_for_timing(&attacker_crew, TimingWindow::HullBreach);
    let receive_damage_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::ReceiveDamage);
    let combat_end_effects = active_effects_for_timing(&attacker_crew, TimingWindow::CombatEnd);

    // Pre-compute effects by timing once per combat; round loop only filters by condition.
    let round_start_effects = active_effects_for_timing(&attacker_crew, TimingWindow::RoundStart);
    let attack_phase_effects = active_effects_for_timing(&attacker_crew, TimingWindow::AttackPhase);
    let defense_phase_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::DefensePhase);
    let round_end_effects = active_effects_for_timing(&attacker_crew, TimingWindow::RoundEnd);
    let after_subround_effects =
        active_effects_for_timing(&attacker_crew, TimingWindow::AfterSubround);

    // Defender-side effects for return fire.
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

    let combat_begin_assimilated = assimilated_rounds_remaining > 0;
    record_ability_activations(
        &mut trace,
        0,
        "combat_begin",
        attacker,
        &combat_begin_filtered,
        combat_begin_assimilated,
    );
    roll_burning_triggers(
        &combat_begin_filtered,
        combat_begin_assimilated,
        &mut rng,
        &mut trace,
        0,
        "combat_begin",
        &attacker.id,
        None,
        &mut defender_burning_rounds,
    );

    let rounds_to_simulate = config.rounds.min(MAX_COMBAT_ROUNDS);
    shots_bonus_entries.reserve(rounds_to_simulate.min(32) as usize);
    let mut rounds_completed = 0u32;

    for round_index in 1..=rounds_to_simulate {
        rounds_completed = round_index;

        let defender_hull_pct_round =
            1.0 - (total_hull_damage / defender.hull_health.max(0.0)).min(1.0);
        let defender_shield_pct_round = if defender.shield_health > 0.0 {
            defender_shield_remaining / defender.shield_health
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
        let ctx_def_round_start = CombatContext {
            round_index,
            defender_hull_pct: defender_hull_pct_round,
            defender_shield_pct: defender_shield_pct_round,
            attacker_hull_pct: attacker_hull_pct_round,
            attacker_shield_pct: attacker_shield_pct_round,
            attacker_morale_active: false,
            defender_burning_active: defender_burning_rounds > 0,
            defender_hull_breach_active: defender_hull_breach_rounds > 0,
            attacker_burning_active: attacker_burning_rounds > 0,
            attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
            defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
            defender_faction,
            defender_hull_faction_id: config.defender_hull_faction_id,
            defender_ship_type,
            attacker_ship_type,
            attacker_ship_id: attacker.id.clone(),
            defender_is_npc_hostile,
            defender_is_player_ship,
            attacker_tal_assigned_captain_or_bridge,
        };
        let defender_rs_for_assim =
            filter_effects_by_condition(&defender_round_start_effects, &ctx_def_round_start);
        let def_rs_assim_active = defender_assimilated_rounds_remaining > 0;
        roll_assimilated_extensions_from_effects(
            &defender_rs_for_assim,
            def_rs_assim_active,
            &mut rng,
            &mut trace,
            round_index,
            "round_start",
            &defender.id,
            &mut defender_assimilated_rounds_remaining,
        );

        let mut combat_ctx = CombatContext {
            round_index,
            defender_hull_pct: defender_hull_pct_round,
            defender_shield_pct: defender_shield_pct_round,
            attacker_hull_pct: attacker_hull_pct_round,
            attacker_shield_pct: attacker_shield_pct_round,
            attacker_morale_active: false,
            defender_burning_active: defender_burning_rounds > 0,
            defender_hull_breach_active: defender_hull_breach_rounds > 0,
            attacker_burning_active: attacker_burning_rounds > 0,
            attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
            defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
            defender_faction,
            defender_hull_faction_id: config.defender_hull_faction_id,
            defender_ship_type,
            attacker_ship_type,
            attacker_ship_id: attacker.id.clone(),
            defender_is_npc_hostile,
            defender_is_player_ship,
            attacker_tal_assigned_captain_or_bridge,
        };

        let mut phase_effects = EffectAccumulator::default();
        phase_effects.add_effects(
            TimingWindow::CombatBegin,
            &combat_begin_filtered,
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
        let bench = filter_effects_by_condition(&round_start_effects, &combat_ctx);
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

        for effect in &bench {
            let effective_effect = scale_effect(effect.effect, round_start_assimilated);

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
                let breach_before = defender_hull_breach_rounds;
                if triggered {
                    defender_hull_breach_rounds =
                        defender_hull_breach_rounds.max(duration_rounds.max(1));
                }
                if breach_before == 0 && defender_hull_breach_rounds > 0 {
                    let weapon_base_rs = attacker.weapon_attack(0).unwrap_or(attacker.attack);
                    apply_hull_breach_timing_window(
                        &mut trace,
                        round_index,
                        attacker,
                        &hull_breach_effects,
                        combat_ctx.clone(),
                        round_start_assimilated,
                        weapon_base_rs,
                        &mut phase_effects,
                        &mut rng,
                        &mut defender_burning_rounds,
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
                std::slice::from_ref(effect),
                round_start_assimilated,
                &mut rng,
                &mut trace,
                round_index,
                "round_start",
                &attacker.id,
                None,
                &mut defender_burning_rounds,
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
        }

        // Morale proc after other round-start RNG consumers (assimilated, hull breach, burning, shots).
        // Sets [CombatContext::attacker_morale_active] for [AbilityCondition::MoraleActive] and pierce.
        let morale_triggered = {
            let morale_source = bench.iter().find_map(|effect| {
                if let AbilityEffect::Morale(chance) =
                    scale_effect(effect.effect, round_start_assimilated)
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
        };
        combat_ctx.attacker_morale_active = morale_triggered;

        let full_round_start = filter_effects_by_condition(&round_start_effects, &combat_ctx);
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

        // Prune expired shots bonuses and compute B_shots(r) for this round.
        shots_bonus_entries.retain(|(_, expires)| *expires >= round_index);
        let b_shots: f64 = shots_bonus_entries.iter().map(|(b, _)| b).sum();

        let round_end_assimilated_early = assimilated_rounds_remaining > 0;
        let round_end_filtered = filter_effects_by_condition(&round_end_effects, &combat_ctx);
        // RoundEnd stacking (apex, isolytic, shield mitigation, round-end damage multipliers, regen)
        // must not feed the same-round weapon sub-rounds. Apply RoundEnd only after all weapons
        // for this round (see merge into `phase_effects_round` below).
        let mut phase_effects_round = phase_effects.clone();
        let num_sub_rounds = attacker.weapon_count().max(defender.weapon_count());

        let attack_phase_assimilated = assimilated_rounds_remaining > 0;
        let attack_phase_filtered = filter_effects_by_condition(&attack_phase_effects, &combat_ctx);
        let defense_phase_filtered =
            filter_effects_by_condition(&defense_phase_effects, &combat_ctx);

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

            let effective_apex_shred =
                (attacker.apex_shred + phase_effects.composed_apex_shred_bonus()).max(0.0);
            let effective_apex_barrier =
                (defender.apex_barrier + phase_effects.composed_apex_barrier_bonus()).max(0.0);
            let apex_damage_factor =
                compute_apex_damage_factor(effective_apex_shred, effective_apex_barrier);

            let base_shots = attacker.weapon_base_shots(weapon_index);
            let effective_shots = effective_shots_for_weapon(base_shots, b_shots);
            let shield_before_weapon = defender_shield_remaining;

            let weapon_index_u = weapon_index as u32;
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

                    let mitigation_multiplier = (1.0 - defender.mitigation).max(0.0);
                    trace.record_if(|| CombatEvent {
                        event_type: "mitigation_calc".to_string(),
                        round_index,
                        phase: "defense".to_string(),
                        source: EventSource {
                            hostile_ability_id: Some(format!("{}_mitigation", defender.id)),
                            ..EventSource::default()
                        },
                        weapon_index: Some(weapon_index_u),
                        values: Map::from_iter([
                            ("mitigation".to_string(), Value::from(defender.mitigation)),
                            (
                                "multiplier".to_string(),
                                Value::from(round_f64(mitigation_multiplier)),
                            ),
                        ]),
                    });

                    // Damage-through factor: fraction of attack that gets through (can exceed 1.0 with pierce).
                    let damage_through_factor = compute_damage_through_factor(
                        mitigation_multiplier,
                        effective_pierce,
                        phase_effects.defense_mitigation_bonus(),
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
                    let crit = resolve_vehicle_weapon_crit(
                        attacker.weapon_crit_chance(weapon_index),
                        phase_effects.crit_chance_bonus(),
                        attacker.weapon_crit_multiplier(weapon_index),
                        phase_effects.crit_damage_multiplier(),
                        hull_breach_active,
                        &mut rng,
                    );
                    let crit_multiplier = crit.multiplier;
                    let is_crit = crit.is_crit;
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
                                    &mut trace,
                                    round_index,
                                    attacker,
                                    &hull_breach_effects,
                                    ctx_hb,
                                    attack_phase_assimilated,
                                    weapon_base,
                                    &mut phase_effects_round,
                                    &mut rng,
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

                        roll_burning_triggers(
                            std::slice::from_ref(effect),
                            attack_phase_assimilated,
                            &mut rng,
                            &mut trace,
                            round_index,
                            "attack",
                            &attacker.id,
                            None,
                            &mut defender_burning_rounds,
                        );
                    }

                    for effect in &defense_phase_filtered {
                        roll_burning_triggers(
                            std::slice::from_ref(effect),
                            defense_phase_assimilated,
                            &mut rng,
                            &mut trace,
                            round_index,
                            "defense",
                            &attacker.id,
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
                        + phase_effects.composed_isolytic_defense_bonus())
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
                    let effective_shield_mitigation = (defender.shield_mitigation
                        + phase_effects.composed_shield_mitigation_bonus())
                    .clamp(0.0, 1.0);
                    let shield_mitigation = if defender_shield_remaining > 0.0 {
                        effective_shield_mitigation
                    } else {
                        0.0
                    };
                    let (actual_shield_damage, hull_damage_this_round) = apply_shield_hull_split(
                        damage_after_apex,
                        shield_mitigation,
                        defender_shield_remaining,
                    );

                    defender_shield_remaining =
                        (defender_shield_remaining - actual_shield_damage).max(0.0);
                    total_hull_damage += hull_damage_this_round;
                    total_shield_damage += actual_shield_damage;

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
                }
            }

            let shield_broke_this_round =
                shield_before_weapon > 0.0 && defender_shield_remaining <= 0.0;
            if shield_broke_this_round {
                let shield_break_filtered =
                    filter_effects_by_condition(&shield_break_effects, &combat_ctx);
                record_ability_activations(
                    &mut trace,
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
                    &shield_break_filtered,
                    attack_phase_assimilated,
                    &mut rng,
                    &mut trace,
                    round_index,
                    "shield_break",
                    &attacker.id,
                    None,
                    &mut defender_burning_rounds,
                );

                let def_sb_filtered =
                    filter_effects_by_condition(&defender_shield_break_effects, &combat_ctx);
                record_ability_activations(
                    &mut trace,
                    round_index,
                    "shield_break",
                    defender,
                    &def_sb_filtered,
                    false,
                );
                for e in &def_sb_filtered {
                    match scale_effect(e.effect, false) {
                        AbilityEffect::ShieldRegen(v) => {
                            defender_shield_remaining = (defender_shield_remaining + v)
                                .min(defender.shield_health.max(0.0));
                        }
                        AbilityEffect::HullRegen(v) => {
                            total_hull_damage = (total_hull_damage - v).max(0.0);
                        }
                        _ => defender_shield_break_carry.push(e.clone()),
                    }
                }
            }

            if let Some(defender_weapon_attack) = defender.weapon_attack(weapon_index) {
                // Defender counter-attack: hostile weapon fire vs the player ship (attacker struct).
                // Uses the same damage-through, isolytic, apex, and shield/hull helpers as outbound shots
                // so the two paths stay in sync. Shot count matches outbound: `effective_shots_for_weapon`
                // on `defender.weapon_base_shots` (defender crew `ShotsBonus` is not wired here yet).
                let def_base_shots = defender.weapon_base_shots(weapon_index);
                let def_effective_shots = effective_shots_for_weapon(def_base_shots, 0.0);

                let eff_player_mitigation =
                    (attacker.mitigation + attacker_mitigation_additive).clamp(0.0, 1.0);
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
                    defender_burning_active: false,
                    defender_hull_breach_active: false,
                    attacker_burning_active: combat_ctx.attacker_burning_active,
                    attacker_hull_breach_active: combat_ctx.attacker_hull_breach_active,
                    defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
                    defender_faction,
                    defender_hull_faction_id: config.defender_hull_faction_id,
                    defender_ship_type,
                    attacker_ship_type,
                    attacker_ship_id: attacker.id.clone(),
                    defender_is_npc_hostile,
                    defender_is_player_ship,
                    attacker_tal_assigned_captain_or_bridge: combat_ctx
                        .attacker_tal_assigned_captain_or_bridge,
                };
                let defender_combat_begin_filtered =
                    filter_effects_by_condition(&defender_combat_begin_effects, &defender_ctx);
                let defender_round_start_filtered =
                    filter_effects_by_condition(&defender_round_start_effects, &defender_ctx);
                let defender_attack_filtered =
                    filter_effects_by_condition(&defender_attack_phase_effects, &defender_ctx);
                let defender_defense_filtered =
                    filter_effects_by_condition(&defender_defense_phase_effects, &defender_ctx);

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

                for _hit_index in 0..def_effective_shots {
                    let defender_attack_phase_assimilated =
                        defender_assimilated_rounds_remaining > 0;
                    roll_assimilated_extensions_from_effects(
                        &defender_attack_filtered,
                        defender_attack_phase_assimilated,
                        &mut rng,
                        &mut trace,
                        round_index,
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

                    let counter_damage_through = compute_damage_through_factor(
                        counter_mitigation_mult,
                        defender.weapon_pierce(weapon_index)
                            + defender_phase_effects.pre_attack_pierce_bonus(),
                        defender_phase_effects.defense_mitigation_bonus(),
                    );
                    let attacker_hull_breach_active_for_crit = attacker_hull_breach_rounds > 0;
                    let def_crit = resolve_vehicle_weapon_crit(
                        defender.weapon_crit_chance(weapon_index),
                        defender_phase_effects.crit_chance_bonus(),
                        defender.weapon_crit_multiplier(weapon_index),
                        defender_phase_effects.crit_damage_multiplier(),
                        attacker_hull_breach_active_for_crit,
                        &mut rng,
                    );
                    let def_is_crit = def_crit.is_crit;
                    let mut def_crit_mult = def_crit.multiplier;
                    // U.S.S. Crozier "Gunboat Diplomacy": reduce hostile crit damage for the first N rounds.
                    if def_is_crit
                        && hostile_crit_reduction > 0.0
                        && round_in_inclusive_first_n(round_index, hostile_crit_reduction_rounds)
                    {
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
                    let counter_iso_taken = compute_isolytic_taken(
                        counter_after_attack_phase,
                        (defender.isolytic_damage
                            + defender_phase_effects.composed_isolytic_damage_bonus())
                        .max(0.0),
                        (attacker.isolytic_defense
                            + defender_phase_effects.composed_isolytic_defense_bonus())
                        .max(0.0),
                        defender_phase_effects
                            .composed_isolytic_cascade_damage_bonus()
                            .max(0.0),
                    );
                    let counter_before_apex = counter_after_attack_phase + counter_iso_taken;
                    let counter_apex_factor = compute_apex_damage_factor(
                        defender.apex_shred.max(0.0),
                        attacker.apex_barrier.max(0.0),
                    );
                    let counter_after_apex = counter_before_apex * counter_apex_factor;
                    let att_shield_mitigation = if attacker_shield_remaining > 0.0 {
                        attacker.shield_mitigation.clamp(0.0, 1.0)
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
                                    apply_attacker_hull_breach_timing_window(
                                        &mut trace,
                                        round_index,
                                        attacker,
                                        &hull_breach_effects,
                                        ctx_hb,
                                        assimilated_rounds_remaining > 0,
                                        defender_weapon_attack,
                                        &mut phase_effects_round,
                                        &mut rng,
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
                                std::slice::from_ref(effect),
                                false,
                                &mut rng,
                                &mut trace,
                                round_index,
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
                            filter_effects_by_condition(&self_shield_break_effects, &ctx_self_sb);
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
                                AbilityEffect::HullRegen(v) => {
                                    total_attacker_hull_damage =
                                        (total_attacker_hull_damage - v).max(0.0);
                                }
                                _ => {}
                            }
                        }
                        let self_sb_combat: Vec<ActiveAbilityEffect> = self_sb_filtered
                            .iter()
                            .filter(|e| {
                                !matches!(
                                    scale_effect(e.effect, attack_phase_assimilated),
                                    AbilityEffect::ShieldRegen(_) | AbilityEffect::HullRegen(_)
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
                            &self_sb_combat,
                            attack_phase_assimilated,
                            &mut rng,
                            &mut trace,
                            round_index,
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
                            filter_effects_by_condition(&receive_damage_effects, &ctx_receive);
                        roll_burning_triggers(
                            &receive_damage_filtered,
                            receive_damage_assimilated,
                            &mut rng,
                            &mut trace,
                            round_index,
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
                        filter_effects_by_condition(&receive_damage_effects, &ctx_rd);
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
                defender_burning_active: defender_burning_rounds > 0,
                defender_hull_breach_active: defender_hull_breach_rounds > 0,
                attacker_burning_active: attacker_burning_rounds > 0,
                attacker_hull_breach_active: attacker_hull_breach_rounds > 0,
                defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
                defender_faction,
                defender_hull_faction_id: config.defender_hull_faction_id,
                defender_ship_type,
                attacker_ship_type,
                attacker_ship_id: attacker.id.clone(),
                defender_is_npc_hostile,
                defender_is_player_ship,
                attacker_tal_assigned_captain_or_bridge: combat_ctx
                    .attacker_tal_assigned_captain_or_bridge,
            };
            let after_subround_filtered =
                filter_effects_by_condition(&after_subround_effects, &ctx_after_subround);
            record_ability_activations(
                &mut trace,
                round_index,
                "after_subround",
                attacker,
                &after_subround_filtered,
                attack_phase_assimilated,
            );
            roll_burning_triggers(
                &after_subround_filtered,
                attack_phase_assimilated,
                &mut rng,
                &mut trace,
                round_index,
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

        phase_effects_round.add_effects(
            TimingWindow::RoundEnd,
            &round_end_filtered,
            attacker.attack,
            round_end_assimilated_early,
            round_index,
        );

        record_ability_activations(
            &mut trace,
            round_index,
            "round_end",
            attacker,
            &round_end_filtered,
            round_end_assimilated_early,
        );

        // RoundEnd burning: roll after outbound/counter damage so conditions use end-of-round hull/shield;
        // procs apply before the burn tick for this same round.
        let ctx_after_weapons = CombatContext {
            round_index,
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
            attacker_morale_active: combat_ctx.attacker_morale_active,
            defender_burning_active: combat_ctx.defender_burning_active,
            defender_hull_breach_active: combat_ctx.defender_hull_breach_active,
            attacker_burning_active: combat_ctx.attacker_burning_active,
            attacker_hull_breach_active: combat_ctx.attacker_hull_breach_active,
            defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
            defender_faction,
            defender_hull_faction_id: config.defender_hull_faction_id,
            defender_ship_type,
            attacker_ship_type,
            attacker_ship_id: attacker.id.clone(),
            defender_is_npc_hostile,
            defender_is_player_ship,
            attacker_tal_assigned_captain_or_bridge: combat_ctx
                .attacker_tal_assigned_captain_or_bridge,
        };
        let round_end_burn_filtered =
            filter_effects_by_condition(&round_end_effects, &ctx_after_weapons);
        roll_burning_triggers(
            &round_end_burn_filtered,
            round_end_assimilated_early,
            &mut rng,
            &mut trace,
            round_index,
            "round_end",
            &attacker.id,
            None,
            &mut defender_burning_rounds,
        );

        let round_end_apex_shred =
            (attacker.apex_shred + phase_effects_round.composed_apex_shred_bonus()).max(0.0);
        let round_end_apex_barrier =
            (defender.apex_barrier + phase_effects_round.composed_apex_barrier_bonus()).max(0.0);
        let round_end_apex_factor = 10000.0
            / (10000.0 + round_end_apex_barrier / (1.0 + round_end_apex_shred).max(EPSILON));
        let bonus_damage =
            phase_effects_round.compose_round_end_damage(attacker.end_of_round_damage);
        // Burning: binary per-round tick — 1% of defender max hull while state active (Δ HHP_burn = 0.01 × HHP_max); no officer/research scaling of that rate.
        let burning_damage = if defender_burning_rounds > 0 {
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
        total_hull_damage += (bonus_damage + burning_damage) * round_end_apex_factor;
        total_attacker_hull_damage += defender.end_of_round_damage;
        total_attacker_hull_damage += attacker_burning_damage * round_end_apex_factor;

        // Regen: shield and hull restoration at round end from attacker's crew (officer/data regen effects apply to the ship with the crew).
        let shield_regen = phase_effects_round.composed_shield_regen();
        let hull_regen = phase_effects_round.composed_hull_regen();
        attacker_shield_remaining =
            (attacker_shield_remaining + shield_regen).min(attacker.shield_health.max(0.0));
        total_attacker_hull_damage = (total_attacker_hull_damage - hull_regen).max(0.0);

        defender_burning_rounds = defender_burning_rounds.saturating_sub(1);
        defender_hull_breach_rounds = defender_hull_breach_rounds.saturating_sub(1);
        attacker_burning_rounds = attacker_burning_rounds.saturating_sub(1);
        attacker_hull_breach_rounds = attacker_hull_breach_rounds.saturating_sub(1);
        assimilated_rounds_remaining = assimilated_rounds_remaining.saturating_sub(1);
        defender_assimilated_rounds_remaining =
            defender_assimilated_rounds_remaining.saturating_sub(1);

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
                defender_burning_active: combat_ctx.defender_burning_active,
                defender_hull_breach_active: combat_ctx.defender_hull_breach_active,
                attacker_burning_active: combat_ctx.attacker_burning_active,
                attacker_hull_breach_active: combat_ctx.attacker_hull_breach_active,
                defender_assimilated_active: defender_assimilated_rounds_remaining > 0,
                defender_faction,
                defender_hull_faction_id: config.defender_hull_faction_id,
                defender_ship_type,
                attacker_ship_type,
                attacker_ship_id: attacker.id.clone(),
                defender_is_npc_hostile,
                defender_is_player_ship,
                attacker_tal_assigned_captain_or_bridge: combat_ctx
                    .attacker_tal_assigned_captain_or_bridge,
            };
            let kill_filtered = filter_effects_by_condition(&kill_effects, &kill_ctx);
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
                &kill_filtered,
                kill_assimilated,
                &mut rng,
                &mut trace,
                round_index,
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
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction,
        defender_hull_faction_id: config.defender_hull_faction_id,
        defender_ship_type,
        attacker_ship_type,
        attacker_ship_id: attacker.id.clone(),
        defender_is_npc_hostile,
        defender_is_player_ship,
        attacker_tal_assigned_captain_or_bridge,
    };
    let combat_end_filtered = filter_effects_by_condition(&combat_end_effects, &combat_end_ctx);
    record_ability_activations(
        &mut trace,
        rounds_completed,
        "combat_end",
        attacker,
        &combat_end_filtered,
        false,
    );

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
    }
}

pub fn simulate_once() -> FightResult {
    FightResult { won: true }
}
