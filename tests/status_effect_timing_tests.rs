//! Explicit timing contracts for morale, burning, and hull breach relative to round phases.
//!
//! These tests encode engine ordering documented in `src/combat/engine.rs` (round_start bench vs
//! post-morale filter, `roll_burning_triggers` vs burn tick, end-of-round decrements).

use kobayashi::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, CombatEvent, Combatant,
    CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, SimulationConfig,
    TimingWindow, TraceMode, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use serde_json::Value;

fn passive_attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 0.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1.0e6,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

fn huge_defender() -> Combatant {
    Combatant {
        id: "def".into(),
        attack: 0.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

/// `Burning` at combat begin sets duration N; engine applies one tick per round-end while the
/// counter is positive, then decrements (so N rounds of non-zero burn damage).
#[test]
fn burning_combat_begin_ticks_exactly_duration_rounds_then_stops() {
    let duration = 4u32;
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "burn-contract".into(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::Burning {
                    chance: 1.0,
                    duration_rounds: duration,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let defender = huge_defender();
    // Must match `BURNING_HULL_DAMAGE_PER_ROUND` in `src/combat/types.rs` (1% max hull / round).
    let tick = defender.hull_health * 0.01;
    let r = simulate_combat(
        &passive_attacker(),
        &defender,
        &SimulationConfig {
            rounds: duration + 3,
            seed: 41,
            trace_mode: TraceMode::Events,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            engagement_enemy_types: Default::default(),
            defender_level: None,
            attacker_roster_officer_ids: Default::default(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_hyperthermic_decay_fraction: 0.0,
            emit_state_snapshots: false,
        },
        &crew,
    );

    let mut positive_ticks = 0u32;
    let mut zero_ticks = 0u32;
    for ev in &r.events {
        if ev.event_type != "end_of_round_effects" {
            continue;
        }
        let bd = ev
            .values
            .get("burning_damage")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if bd > 0.0 {
            assert!(
                (bd - tick).abs() < 1e-6,
                "burn tick should match 1% max hull, got {bd}"
            );
            positive_ticks += 1;
        } else {
            zero_ticks += 1;
        }
    }
    assert_eq!(
        positive_ticks,
        duration,
        "expected {duration} burning ticks, events: {:?}",
        r.events
            .iter()
            .filter(|e| e.event_type == "end_of_round_effects")
            .collect::<Vec<_>>()
    );
    assert!(zero_ticks >= 3, "expected trailing rounds with no burn");
}

/// Round-end burn rolls are recorded before `end_of_round_effects` (proc may extend duration before
/// the same-round tick).
#[test]
fn burning_round_end_trigger_precedes_end_of_round_effects_in_event_order() {
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "burn-re".into(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundEnd,
                boostable: false,
                effect: AbilityEffect::Burning {
                    chance: 1.0,
                    duration_rounds: 2,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let r = simulate_combat(
        &passive_attacker(),
        &huge_defender(),
        &SimulationConfig {
            rounds: 2,
            seed: 43,
            trace_mode: TraceMode::Events,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            engagement_enemy_types: Default::default(),
            defender_level: None,
            attacker_roster_officer_ids: Default::default(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_hyperthermic_decay_fraction: 0.0,
            emit_state_snapshots: false,
        },
        &crew,
    );

    for round in 1u32..=2 {
        let idx_burn = r
            .events
            .iter()
            .position(|e| {
                e.event_type == "burning_trigger"
                    && e.phase == "round_end"
                    && e.round_index == round
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing round_end burning_trigger for round {round}: {:?}",
                    r.events
                )
            });
        let idx_eor = r
            .events
            .iter()
            .position(|e| e.event_type == "end_of_round_effects" && e.round_index == round)
            .unwrap_or_else(|| panic!("missing end_of_round_effects for round {round}"));
        assert!(
            idx_burn < idx_eor,
            "round {round}: burning_trigger should precede end_of_round_effects (burn {idx_burn}, eor {idx_eor})"
        );
    }
}

/// Morale RNG runs after other round-start procs; first outbound `attack_roll` is always later than
/// `morale_activation` for the same round.
#[test]
fn morale_activation_precedes_first_attack_roll_each_round() {
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "morale-contract".into(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::Morale(1.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let attacker = Combatant {
        id: "att".into(),
        attack: 40.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1.0e6,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let r = simulate_combat(
        &attacker,
        &huge_defender(),
        &SimulationConfig {
            rounds: 3,
            seed: 47,
            trace_mode: TraceMode::Events,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            engagement_enemy_types: Default::default(),
            defender_level: None,
            attacker_roster_officer_ids: Default::default(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_hyperthermic_decay_fraction: 0.0,
            emit_state_snapshots: false,
        },
        &crew,
    );

    for round in 1u32..=3 {
        let idx_morale = r
            .events
            .iter()
            .position(|e| e.event_type == "morale_activation" && e.round_index == round)
            .unwrap_or_else(|| panic!("missing morale_activation round {round}"));
        let idx_attack = r
            .events
            .iter()
            .position(|e| {
                e.event_type == "attack_roll" && e.round_index == round && e.phase == "attack"
            })
            .unwrap_or_else(|| panic!("missing attack_roll round {round}"));
        assert!(
            idx_morale < idx_attack,
            "round {round}: morale_activation ({idx_morale}) must precede first attack_roll ({idx_attack})"
        );
    }
}

fn round_start_hull_breach_triggered(events: &[CombatEvent], round: u32) -> Option<bool> {
    events
        .iter()
        .find(|e| {
            e.event_type == "hull_breach_trigger"
                && e.phase == "round_start"
                && e.round_index == round
        })
        .and_then(|e| e.values.get("triggered").and_then(|v| v.as_bool()))
}

/// With `chance == 1`, each successful round-start proc refreshes `hull_breach_rounds_remaining` to
/// at least `duration`, so the breach flag does **not** decay across rounds (documented here so
/// future “duration semantics” tests are not confused with refresh).
#[test]
fn hull_breach_round_start_chance_one_refreshes_duration_each_round() {
    let duration = 3u32;
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "breach-refresh".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullBreach {
                    chance: 1.0,
                    duration_rounds: duration,
                    requires_critical: false,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let attacker = Combatant {
        id: "att".into(),
        attack: 25.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1.0e6,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let r = simulate_combat(
        &attacker,
        &huge_defender(),
        &SimulationConfig {
            rounds: 5,
            seed: 53,
            trace_mode: TraceMode::Events,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            engagement_enemy_types: Default::default(),
            defender_level: None,
            attacker_roster_officer_ids: Default::default(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_hyperthermic_decay_fraction: 0.0,
            emit_state_snapshots: false,
        },
        &crew,
    );
    for round in 1u32..=5 {
        assert_eq!(
            round_start_hull_breach_triggered(&r.events, round),
            Some(true),
            "round {round} should re-proc hull breach (refresh semantics)"
        );
        assert!(
            r.events.iter().any(|e| {
                e.event_type == "crit_resolution"
                    && e.round_index == round
                    && e.phase == "attack"
                    && e.values.get("hull_breach_active") == Some(&Value::Bool(true))
            }),
            "round {round}: crit_resolution should still see hull_breach_active"
        );
    }
}

/// When the round-start breach roll **fails** on later rounds, `duration_rounds` ticks down with
/// end-of-round `saturating_sub` and `hull_breach_active` clears after the stack expires.
#[test]
fn hull_breach_decays_when_round_start_proc_does_not_refresh() {
    let duration = 3u32;
    let breach_chance = 0.5_f64;
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "breach-decay".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullBreach {
                    chance: breach_chance,
                    duration_rounds: duration,
                    requires_critical: false,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let attacker = Combatant {
        id: "att".into(),
        attack: 25.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1.0e6,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let cfg = |seed: u64| SimulationConfig {
        rounds: duration + 2,
        seed,
        trace_mode: TraceMode::Events,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_hyperthermic_decay_fraction: 0.0,
        emit_state_snapshots: false,
    };

    let mut chosen = None;
    for seed in 0u64..2000 {
        let r = simulate_combat(&attacker, &huge_defender(), &cfg(seed), &crew);
        let t1 = round_start_hull_breach_triggered(&r.events, 1);
        let t2 = round_start_hull_breach_triggered(&r.events, 2);
        if t1 == Some(true) && t2 == Some(false) {
            chosen = Some((seed, r));
            break;
        }
    }
    let (seed, r) = chosen.expect(
        "expected a seed where round 1 hull breach procs and round 2 fails (expand search if RNG stream changes)",
    );

    fn round_has_breach_flag(events: &[CombatEvent], round: u32) -> bool {
        events.iter().any(|e| {
            e.event_type == "crit_resolution"
                && e.round_index == round
                && e.phase == "attack"
                && e.values.get("hull_breach_active") == Some(&Value::Bool(true))
        })
    }

    for round in 1..=duration {
        assert!(
            round_has_breach_flag(&r.events, round),
            "seed {seed}: expected hull_breach_active in attack round {round}"
        );
    }
    assert!(
        !round_has_breach_flag(&r.events, duration + 1),
        "seed {seed}: breach stack should clear by round {} attack",
        duration + 1
    );
}

/// First `hull_breach_trigger` for a round-start proc is logged before the first outbound
/// `crit_resolution` of that round.
#[test]
fn hull_breach_round_start_trigger_precedes_crit_resolution_same_round() {
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "breach-order".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullBreach {
                    chance: 1.0,
                    duration_rounds: 2,
                    requires_critical: false,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let attacker = Combatant {
        id: "att".into(),
        attack: 30.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1.0e6,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let r = simulate_combat(
        &attacker,
        &huge_defender(),
        &SimulationConfig {
            rounds: 1,
            seed: 59,
            trace_mode: TraceMode::Events,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            engagement_enemy_types: Default::default(),
            defender_level: None,
            attacker_roster_officer_ids: Default::default(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_hyperthermic_decay_fraction: 0.0,
            emit_state_snapshots: false,
        },
        &crew,
    );
    let idx_trig = r
        .events
        .iter()
        .position(|e| {
            e.event_type == "hull_breach_trigger" && e.phase == "round_start" && e.round_index == 1
        })
        .expect("hull_breach_trigger");
    let idx_crit = r
        .events
        .iter()
        .position(|e| {
            e.event_type == "crit_resolution" && e.phase == "attack" && e.round_index == 1
        })
        .expect("crit_resolution");
    assert!(
        idx_trig < idx_crit,
        "hull breach trigger should precede first crit_resolution"
    );
}
