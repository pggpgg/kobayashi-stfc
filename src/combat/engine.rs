//! Combat loop orchestration. Types, mitigation, effects, events, and damage helpers live in sibling modules.

pub use crate::combat::events::serialize_events_json;
pub use crate::combat::mitigation::{
    apply_morale_primary_piercing, component_mitigation, isolytic_damage, mitigation,
    mitigation_for_hostile, mitigation_with_morale, mitigation_with_mystery,
    pierce_damage_through_bonus, MITIGATION_CEILING, MITIGATION_FLOOR, PIERCE_CAP,
};
pub use crate::combat::types::{
    round_half_even, AttackerStats, CombatEvent, Combatant, DefenderStats, EventSource, FightResult,
    ShipType, SimulationConfig, SimulationResult, TraceCollector, TraceMode, WeaponStats,
    BATTLESHIP_COEFFICIENTS, EPSILON, EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS,
    MAX_COMBAT_ROUNDS, MORALE_PRIMARY_PIERCING_BONUS, SURVEY_COEFFICIENTS,
};

use serde_json::{Map, Value};

use crate::combat::abilities::{
    active_effects_for_timing, filter_effects_by_condition, AbilityEffect,
    CombatContext, CrewConfiguration, TimingWindow,
};
use crate::combat::damage::{
    apply_shield_hull_split, compute_apex_damage_factor, compute_crit_multiplier,
    compute_damage_through_factor, compute_isolytic_taken,
};
use crate::combat::effect_accumulator::{
    record_ability_activations, scale_effect, sum_on_kill_hull_regen, EffectAccumulator,
};
use crate::combat::events::round_f64;
use crate::combat::rng::Rng;
use crate::combat::types::BURNING_HULL_DAMAGE_PER_ROUND;

pub fn simulate_combat(
    attacker: &Combatant,
    defender: &Combatant,
    config: SimulationConfig,
    attacker_crew: &CrewConfiguration,
) -> SimulationResult {
    let mut rng = Rng::new(config.seed);
    let mut trace = TraceCollector::new(matches!(config.trace_mode, TraceMode::Events));
    let mut total_hull_damage = 0.0;
    let mut total_shield_damage = 0.0;
    let mut defender_shield_remaining = defender.shield_health.max(0.0);
    let mut attacker_shield_remaining = attacker.shield_health.max(0.0);
    let mut total_attacker_hull_damage = 0.0;
    let mut hull_breach_rounds_remaining = 0_u32;
    let mut burning_rounds_remaining = 0_u32;
    let mut assimilated_rounds_remaining = 0_u32;
    // Active shots bonuses: (bonus_pct, expires_round). B_shots(r) = sum of bonus where expires_round >= r.
    let mut shots_bonus_entries: Vec<(f64, u32)> = Vec::new();
    let combat_begin_effects = active_effects_for_timing(attacker_crew, TimingWindow::CombatBegin);
    let combat_begin_ctx = CombatContext {
        round_index: 0,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: 1.0,
        attacker_shield_pct: 1.0,
    };
    let combat_begin_filtered =
        filter_effects_by_condition(&combat_begin_effects, &combat_begin_ctx);
    let shield_break_effects = active_effects_for_timing(attacker_crew, TimingWindow::ShieldBreak);
    let kill_effects = active_effects_for_timing(attacker_crew, TimingWindow::Kill);
    let hull_breach_effects = active_effects_for_timing(attacker_crew, TimingWindow::HullBreach);
    let receive_damage_effects = active_effects_for_timing(attacker_crew, TimingWindow::ReceiveDamage);
    let combat_end_effects = active_effects_for_timing(attacker_crew, TimingWindow::CombatEnd);

    // Pre-compute effects by timing once per combat; round loop only filters by condition.
    let round_start_effects = active_effects_for_timing(attacker_crew, TimingWindow::RoundStart);
    let attack_phase_effects = active_effects_for_timing(attacker_crew, TimingWindow::AttackPhase);
    let defense_phase_effects = active_effects_for_timing(attacker_crew, TimingWindow::DefensePhase);
    let round_end_effects = active_effects_for_timing(attacker_crew, TimingWindow::RoundEnd);

    let combat_begin_assimilated = assimilated_rounds_remaining > 0;
    record_ability_activations(
        &mut trace,
        0,
        "combat_begin",
        attacker,
        &combat_begin_filtered,
        combat_begin_assimilated,
    );

    let rounds_to_simulate = config.rounds.min(MAX_COMBAT_ROUNDS);
    let mut rounds_completed = 0u32;

    for round_index in 1..=rounds_to_simulate {
        rounds_completed = round_index;

        let combat_ctx = CombatContext {
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
        let round_start_filtered = filter_effects_by_condition(&round_start_effects, &combat_ctx);
        record_ability_activations(
            &mut trace,
            round_index,
            "round_start",
            attacker,
            &round_start_filtered,
            round_start_assimilated,
        );
        phase_effects.add_effects(
            TimingWindow::RoundStart,
            &round_start_filtered,
            attacker.attack,
            round_start_assimilated,
            round_index,
        );

        for effect in &round_start_filtered {
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
                if triggered {
                    hull_breach_rounds_remaining =
                        hull_breach_rounds_remaining.max(duration_rounds.max(1));
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

            if let AbilityEffect::Burning {
                chance,
                duration_rounds,
            } = effective_effect
            {
                let burning_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = burning_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    burning_rounds_remaining = burning_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record_if(|| CombatEvent {
                    event_type: "burning_trigger".to_string(),
                    round_index,
                    phase: "round_start".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(burning_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
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

        // Prune expired shots bonuses and compute B_shots(r) for this round.
        shots_bonus_entries.retain(|(_, expires)| *expires >= round_index);
        let b_shots: f64 = shots_bonus_entries.iter().map(|(b, _)| b).sum();

        let round_end_assimilated_early = assimilated_rounds_remaining > 0;
        let round_end_filtered = filter_effects_by_condition(&round_end_effects, &combat_ctx);
        phase_effects.add_effects(
            TimingWindow::RoundEnd,
            &round_end_filtered,
            attacker.attack,
            round_end_assimilated_early,
            round_index,
        );
        let mut phase_effects_round = phase_effects.clone();
        let num_sub_rounds = attacker.weapon_count().max(defender.weapon_count());
        let mut hull_breach_threshold_fired = false;

        let mut effective_pierce = attacker.pierce + phase_effects_round.pre_attack_pierce_bonus();
        let morale_source = round_start_filtered.iter().find_map(|effect| {
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
            let morale_triggered = morale_roll < morale_chance;
            if morale_triggered {
                effective_pierce *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
            }
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
                    ("triggered".to_string(), Value::Bool(morale_triggered)),
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
        }

        let attack_phase_assimilated = assimilated_rounds_remaining > 0;
        let attack_phase_filtered =
            filter_effects_by_condition(&attack_phase_effects, &combat_ctx);
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
        for weapon_index in 0..num_sub_rounds {
            phase_effects.clear();
            phase_effects.merge_from(&weapon_round_base);
            let weapon_base = attacker.weapon_attack(weapon_index).unwrap_or(attacker.attack);
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

            let effective_apex_shred = (attacker.apex_shred + phase_effects.composed_apex_shred_bonus())
                .max(0.0);
            let effective_apex_barrier = (defender.apex_barrier + phase_effects.composed_apex_barrier_bonus())
                .max(0.0);
            let apex_damage_factor =
                compute_apex_damage_factor(effective_apex_shred, effective_apex_barrier);

            let base_shots = attacker.weapon_base_shots(weapon_index);
            let effective_shots = round_half_even(base_shots as f64 * (1.0 + b_shots));
            let shield_before_weapon = defender_shield_remaining;

            let weapon_index_u = weapon_index as u32;
            for _ in 0..effective_shots {
            if let Some(attacker_weapon_attack) = attacker.weapon_attack(weapon_index) {
            let effective_attack = attacker_weapon_attack * phase_effects.pre_attack_multiplier();

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
                    ("base_attack".to_string(), Value::from(attacker_weapon_attack)),
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

        let hull_breach_active = hull_breach_rounds_remaining > 0;
        let crit_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let is_crit = crit_roll < attacker.crit_chance;
        let crit_multiplier = compute_crit_multiplier(
            is_crit,
            attacker.crit_multiplier,
            hull_breach_active,
        );
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
                ("roll".to_string(), Value::from(round_f64(crit_roll))),
                ("is_crit".to_string(), Value::Bool(is_crit)),
                ("multiplier".to_string(), Value::from(crit_multiplier)),
                (
                    "hull_breach_active".to_string(),
                    Value::Bool(hull_breach_active),
                ),
            ]),
        });

        for effect in &attack_phase_filtered {
            let effective_effect = scale_effect(effect.effect, attack_phase_assimilated);

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
                if triggered {
                    hull_breach_rounds_remaining =
                        hull_breach_rounds_remaining.max(duration_rounds.max(1));
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

            if let AbilityEffect::Burning {
                chance,
                duration_rounds,
            } = effective_effect
            {
                let burning_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = burning_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    burning_rounds_remaining = burning_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record_if(|| CombatEvent {
                    event_type: "burning_trigger".to_string(),
                    round_index,
                    phase: "attack".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(burning_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
            }
        }

        let proc_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let did_proc = proc_roll < attacker.proc_chance;
        let proc_multiplier = if did_proc {
            attacker.proc_multiplier
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

        let pre_attack_damage =
            effective_attack * damage_through_factor * crit_multiplier * proc_multiplier;
        phase_effects.set_pre_attack_damage_base(pre_attack_damage);
        let pre_attack_damage = phase_effects.composed_pre_attack_damage();
        let damage = phase_effects.compose_attack_phase_damage(pre_attack_damage);

        // Isolytic: from pre-apex standard damage; report formula: isolytic taken = Isolytic Damage / (1 + I_def).
        let effective_isolytic_damage = (attacker.isolytic_damage + phase_effects.composed_isolytic_damage_bonus()).max(0.0);
        let effective_isolytic_defense = (defender.isolytic_defense + phase_effects.composed_isolytic_defense_bonus()).max(0.0);
        let effective_isolytic_cascade = phase_effects.composed_isolytic_cascade_damage_bonus().max(0.0);
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
        let effective_shield_mitigation = (defender.shield_mitigation + phase_effects.composed_shield_mitigation_bonus()).clamp(0.0, 1.0);
        let shield_mitigation = if defender_shield_remaining > 0.0 {
            effective_shield_mitigation
        } else {
            0.0
        };
        let (actual_shield_damage, hull_damage_this_round) =
            apply_shield_hull_split(damage_after_apex, shield_mitigation, defender_shield_remaining);

        defender_shield_remaining = (defender_shield_remaining - actual_shield_damage).max(0.0);
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
                ("damage_after_apex".to_string(), Value::from(round_f64(damage_after_apex))),
                ("shield_mitigation".to_string(), Value::from(round_f64(shield_mitigation))),
                ("shield_damage".to_string(), Value::from(round_f64(actual_shield_damage))),
                ("hull_damage".to_string(), Value::from(round_f64(hull_damage_this_round))),
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
                    Value::Bool(shield_before_weapon > 0.0 && defender_shield_remaining <= 0.0),
                ),
                (
                    "assimilated_active".to_string(),
                    Value::Bool(assimilated_rounds_remaining > 0),
                ),
            ]),
        });
            }
            }

            let shield_broke_this_round = shield_before_weapon > 0.0 && defender_shield_remaining <= 0.0;
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
        }

        let defender_hull_pct = 1.0
            - (total_hull_damage / defender.hull_health.max(0.0)).min(1.0);
        if !hull_breach_threshold_fired && defender_hull_pct < 0.5 {
            hull_breach_threshold_fired = true;
            let hull_breach_filtered =
                filter_effects_by_condition(&hull_breach_effects, &combat_ctx);
            record_ability_activations(
                &mut trace,
                round_index,
                "hull_breach",
                attacker,
                &hull_breach_filtered,
                attack_phase_assimilated,
            );
            phase_effects_round.add_effects(
                TimingWindow::HullBreach,
                &hull_breach_filtered,
                weapon_base,
                attack_phase_assimilated,
                round_index,
            );
        }

            if let Some(defender_weapon_attack) = defender.weapon_attack(weapon_index) {
        // Defender counter-attack: defender deals damage to attacker (ship runs out of HHP ends fight).
        let def_mitigation_mult = (1.0 - attacker.mitigation).max(0.0);
        let def_damage_through = (def_mitigation_mult + defender.pierce).max(0.0);
        let def_crit_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let def_crit_mult = if def_crit_roll < defender.crit_chance {
            defender.crit_multiplier
        } else {
            1.0
        };
        let def_proc_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let def_proc_mult = if def_proc_roll < defender.proc_chance {
            defender.proc_multiplier
        } else {
            1.0
        };
        let def_to_attacker_damage =
            defender_weapon_attack * def_damage_through * def_crit_mult * def_proc_mult;
        let att_shield_mitigation = if attacker_shield_remaining > 0.0 {
            attacker.shield_mitigation.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let att_shield_portion = def_to_attacker_damage * att_shield_mitigation;
        let att_hull_portion = def_to_attacker_damage * (1.0 - att_shield_mitigation);
        let att_actual_shield_damage = att_shield_portion.min(attacker_shield_remaining);
        let att_shield_overflow = att_shield_portion - att_actual_shield_damage;
        let att_hull_damage_this_round = att_hull_portion + att_shield_overflow;
        attacker_shield_remaining = (attacker_shield_remaining - att_actual_shield_damage).max(0.0);
        total_attacker_hull_damage += att_hull_damage_this_round;
        if att_hull_damage_this_round > 0.0 {
            let receive_damage_filtered =
                filter_effects_by_condition(&receive_damage_effects, &combat_ctx);
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
        }

        record_ability_activations(
            &mut trace,
            round_index,
            "round_end",
            attacker,
            &round_end_filtered,
            round_end_assimilated_early,
        );

        let round_end_apex_shred = (attacker.apex_shred + phase_effects_round.composed_apex_shred_bonus()).max(0.0);
        let round_end_apex_barrier = (defender.apex_barrier + phase_effects_round.composed_apex_barrier_bonus()).max(0.0);
        let round_end_apex_factor = 10000.0 / (10000.0 + round_end_apex_barrier / (1.0 + round_end_apex_shred).max(EPSILON));
        let bonus_damage = phase_effects_round.compose_round_end_damage(attacker.end_of_round_damage);
        // Burning: 1% of max hull per round (official: Δ HHP_burn = 0.01 × HHP_max), no scaling.
        let burning_damage = if burning_rounds_remaining > 0 {
            defender.hull_health.max(0.0) * BURNING_HULL_DAMAGE_PER_ROUND
        } else {
            0.0
        };
        // Round-end and burning apply to hull only (shields do not absorb these).
        total_hull_damage += (bonus_damage + burning_damage) * round_end_apex_factor;
        total_attacker_hull_damage += defender.end_of_round_damage;

        // Regen: shield and hull restoration at round end from attacker's crew (officer/data regen effects apply to the ship with the crew).
        let shield_regen = phase_effects_round.composed_shield_regen();
        let hull_regen = phase_effects_round.composed_hull_regen();
        attacker_shield_remaining = (attacker_shield_remaining + shield_regen)
            .min(attacker.shield_health.max(0.0));
        total_attacker_hull_damage = (total_attacker_hull_damage - hull_regen).max(0.0);

        if burning_rounds_remaining > 0 {
            burning_rounds_remaining -= 1;
        }
        if hull_breach_rounds_remaining > 0 {
            hull_breach_rounds_remaining -= 1;
        }
        if assimilated_rounds_remaining > 0 {
            assimilated_rounds_remaining -= 1;
        }

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
                attacker_hull_pct: 1.0 - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0),
                attacker_shield_pct: if attacker.shield_health > 0.0 {
                    attacker_shield_remaining / attacker.shield_health
                } else {
                    1.0
                },
            };
            let kill_filtered = filter_effects_by_condition(&kill_effects, &kill_ctx);
            record_ability_activations(
                &mut trace,
                round_index,
                "kill",
                attacker,
                &kill_filtered,
                assimilated_rounds_remaining > 0,
            );
            let on_kill_regen = sum_on_kill_hull_regen(&kill_filtered, assimilated_rounds_remaining > 0);
            total_attacker_hull_damage =
                (total_attacker_hull_damage - on_kill_regen * attacker.hull_health.max(0.0)).max(0.0);
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
        attacker_hull_pct: 1.0 - (total_attacker_hull_damage / attacker.hull_health.max(0.0)).min(1.0),
        attacker_shield_pct: if attacker.shield_health > 0.0 {
            attacker_shield_remaining / attacker.shield_health
        } else {
            1.0
        },
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
        events: trace.events(),
    }
}


pub fn simulate_once() -> FightResult {
    FightResult { won: true }
}
