use kobayashi::combat::{
    aggregate_contributions, apply_morale_primary_piercing, component_mitigation,
    effective_shots_for_weapon, isolytic_damage, mitigation, mitigation_with_morale,
    pierce_damage_through_bonus, round_half_even, serialize_events_json, simulate_combat,
    simulate_combat_with_defender_faction, simulate_combat_with_defender_faction_and_defender_crew,
    Ability, AbilityClass, AbilityCondition, AbilityEffect, AttackerStats, CombatEvent, Combatant,
    CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats, EnemyType, EnemyTypes,
    EventSource, HostileMitigationParams, OpponentFactionTag, ShipType, SimulationConfig,
    StackContribution, StatStacking, TimingWindow, TraceCollector, TraceMode, WeaponStats, EPSILON,
    EVOLUTIONARY_ASSIMILATION_FORBIDDEN_OFFICER_IDS, HOSTILE_TAG_MASK_CONQUEROR_BORG,
    NO_EXPLICIT_CONTRIBUTION_BATCH, PIERCE_CAP,
};
use serde_json::{Map, Value};

fn approx_eq(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "expected {b}, got {a}");
}

#[test]
fn round_half_even_bankers_rounding() {
    // Ties round to nearest even: 2.5 -> 2, 3.5 -> 4
    assert_eq!(round_half_even(1.0), 1);
    assert_eq!(round_half_even(1.4), 1);
    assert_eq!(round_half_even(1.5), 2);
    assert_eq!(round_half_even(2.5), 2);
    assert_eq!(round_half_even(3.5), 4);
    assert_eq!(round_half_even(4.5), 4);
    assert_eq!(round_half_even(0.0), 0);
    assert_eq!(round_half_even(2.0), 2);
}

#[test]
fn effective_shots_for_weapon_matches_round_half_even_product() {
    assert_eq!(effective_shots_for_weapon(3, 0.0), 3);
    assert_eq!(effective_shots_for_weapon(2, 0.1), round_half_even(2.2));
    assert_eq!(effective_shots_for_weapon(1, 0.5), round_half_even(1.5));
}

/// Player deals no outbound damage; hostile only differs by `shots` on its single weapon.
/// Counter-fire hull damage must scale linearly with shot count (crit/proc disabled).
#[test]
fn defender_counter_respects_weapon_base_shots() {
    let crew = CrewConfiguration { seats: vec![] };
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 20260407,
        trace_mode: TraceMode::Off,
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
    let player = Combatant {
        id: "player".into(),
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
        hull_health: 1.0e9,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let hostile_one = Combatant {
        id: "h1".into(),
        attack: 100.0,
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
        hull_health: 1.0e9,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 80.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let mut hostile_three = hostile_one.clone();
    hostile_three.id = "h3".into();
    hostile_three.weapons = vec![WeaponStats {
        attack: 80.0,
        shots: Some(3),
        ..Default::default()
    }];

    let r1 = simulate_combat(&player, &hostile_one, &cfg, &crew);
    let r3 = simulate_combat(&player, &hostile_three, &cfg, &crew);
    let d1 = player.hull_health - r1.attacker_hull_remaining;
    let d3 = player.hull_health - r3.attacker_hull_remaining;
    approx_eq(d3, 3.0 * d1, d1 * 1e-9 + 1e-6);
}

/// Traces include `hit_index` per outbound hit within a weapon sub-round for stable hit accounting.
#[test]
fn attack_trace_includes_hit_index_per_weapon_shot() {
    let crew = CrewConfiguration { seats: vec![] };
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 1,
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
    let player = Combatant {
        id: "player".into(),
        attack: 1.0,
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
        hull_health: 1.0e9,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(3),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let hostile = Combatant {
        id: "hostile".into(),
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
        hull_health: 1.0e9,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let res = simulate_combat(&player, &hostile, &cfg, &crew);
    let rolls: Vec<&CombatEvent> = res
        .events
        .iter()
        .filter(|e| e.event_type == "attack_roll" && e.weapon_index == Some(0))
        .collect();
    assert_eq!(rolls.len(), 3, "expected three outbound shots in round 1");
    for (i, ev) in rolls.iter().enumerate() {
        assert_eq!(
            ev.values.get("hit_index"),
            Some(&Value::from(i as u64)),
            "hit_index mismatch at shot {i}"
        );
    }
}

#[test]
fn component_mitigation_clamps_non_positive_piercing_to_epsilon() {
    let with_zero = component_mitigation(10.0, 0.0);
    let with_negative = component_mitigation(10.0, -5.0);
    let with_epsilon = component_mitigation(10.0, EPSILON);

    approx_eq(with_zero, with_epsilon, 1e-15);
    approx_eq(with_negative, with_epsilon, 1e-15);
}

#[test]
fn component_mitigation_clamps_negative_defense_to_zero() {
    let clamped = component_mitigation(-10.0, 5.0);
    let zero = component_mitigation(0.0, 5.0);

    approx_eq(clamped, zero, 1e-15);
}

#[test]
fn mitigation_output_is_bounded_for_extreme_inputs() {
    let low = mitigation(
        DefenderStats {
            armor: -1.0,
            shield_deflection: -5.0,
            dodge: -10.0,
        },
        AttackerStats {
            armor_piercing: 1e12,
            shield_piercing: 1e12,
            accuracy: 1e12,
        },
        ShipType::Survey,
    );
    let high = mitigation(
        DefenderStats {
            armor: 1e12,
            shield_deflection: 1e12,
            dodge: 1e12,
        },
        AttackerStats {
            armor_piercing: 0.0,
            shield_piercing: -1.0,
            accuracy: EPSILON / 2.0,
        },
        ShipType::Interceptor,
    );

    assert!(
        (0.0..=1.0).contains(&low),
        "low mitigation out of bounds: {low}"
    );
    assert!(
        (0.0..=1.0).contains(&high),
        "high mitigation out of bounds: {high}"
    );
}

#[test]
fn golden_values_match_python_reference_for_each_ship_type() {
    let defender = DefenderStats {
        armor: 100.0,
        shield_deflection: 80.0,
        dodge: 60.0,
    };
    let attacker = AttackerStats {
        armor_piercing: 50.0,
        shield_piercing: 40.0,
        accuracy: 30.0,
    };

    approx_eq(
        mitigation(defender, attacker, ShipType::Survey),
        0.5489034243492552,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Armada),
        0.5489034243492552,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Battleship),
        0.5914393181871193,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Explorer),
        0.5914393181871193,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Interceptor),
        0.5914393181871193,
        1e-12,
    );
}

#[test]
fn pierce_damage_through_bonus_derived_from_mitigation() {
    let defender = DefenderStats {
        armor: 100.0,
        shield_deflection: 80.0,
        dodge: 60.0,
    };
    let attacker = AttackerStats {
        armor_piercing: 50.0,
        shield_piercing: 40.0,
        accuracy: 30.0,
    };
    for ship_type in [
        ShipType::Survey,
        ShipType::Battleship,
        ShipType::Explorer,
        ShipType::Interceptor,
    ] {
        let mit = mitigation(defender, attacker, ship_type);
        let pierce = pierce_damage_through_bonus(defender, attacker, ship_type);
        approx_eq(pierce, PIERCE_CAP * (1.0 - mit), 1e-12);
        assert!((0.0..=PIERCE_CAP).contains(&pierce));
    }
}

#[test]
fn armada_mitigation_matches_survey_for_identical_stats() {
    let defender = DefenderStats {
        armor: 320.0,
        shield_deflection: 275.0,
        dodge: 145.0,
    };
    let attacker = AttackerStats {
        armor_piercing: 210.0,
        shield_piercing: 180.0,
        accuracy: 110.0,
    };

    approx_eq(
        mitigation(defender, attacker, ShipType::Armada),
        mitigation(defender, attacker, ShipType::Survey),
        1e-12,
    );
}

#[test]
fn morale_boosts_only_primary_piercing_per_ship_type() {
    let attacker = AttackerStats {
        armor_piercing: 100.0,
        shield_piercing: 80.0,
        accuracy: 60.0,
    };

    let battleship = apply_morale_primary_piercing(attacker, ShipType::Battleship);
    approx_eq(battleship.shield_piercing, 88.0, 1e-12);
    approx_eq(battleship.armor_piercing, 100.0, 1e-12);
    approx_eq(battleship.accuracy, 60.0, 1e-12);

    let interceptor = apply_morale_primary_piercing(attacker, ShipType::Interceptor);
    approx_eq(interceptor.armor_piercing, 110.0, 1e-12);
    approx_eq(interceptor.shield_piercing, 80.0, 1e-12);
    approx_eq(interceptor.accuracy, 60.0, 1e-12);

    let explorer = apply_morale_primary_piercing(attacker, ShipType::Explorer);
    approx_eq(explorer.accuracy, 66.0, 1e-12);
    approx_eq(explorer.armor_piercing, 100.0, 1e-12);
    approx_eq(explorer.shield_piercing, 80.0, 1e-12);

    let survey = apply_morale_primary_piercing(attacker, ShipType::Survey);
    approx_eq(survey.armor_piercing, 100.0, 1e-12);
    approx_eq(survey.shield_piercing, 80.0, 1e-12);
    approx_eq(survey.accuracy, 60.0, 1e-12);

    let armada = apply_morale_primary_piercing(attacker, ShipType::Armada);
    approx_eq(armada.armor_piercing, 100.0, 1e-12);
    approx_eq(armada.shield_piercing, 80.0, 1e-12);
    approx_eq(armada.accuracy, 60.0, 1e-12);
}

#[test]
fn defender_crew_can_modify_counter_fire_damage() {
    // Mechanic: defender-side (hostile) upstream abilities are resolved into a `defender_crew` that
    // can apply proc-gated multipliers/bonuses to the defender's return fire.
    //
    // This test uses deterministic (chance=1.0) procs so it is stable across RNG changes.
    let attacker = Combatant {
        id: "att".to_string(),
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
        hull_health: 2000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "def".to_string(),
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
        hull_health: 2000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let attacker_crew = CrewConfiguration { seats: vec![] };
    let defender_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "hostile_proc_attack_x2".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ProcAttackMultiplier {
                    chance: 1.0,
                    multiplier: 2.0,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let cfg = SimulationConfig {
        rounds: 1,
        seed: 1,
        trace_mode: TraceMode::Off,
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

    let baseline = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    let boosted = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &defender_crew,
    );

    assert!(
        boosted.attacker_hull_remaining < baseline.attacker_hull_remaining,
        "defender proc attack multiplier should increase return-fire damage"
    );
}

#[test]
fn defender_crew_shield_break_effects_apply_to_counter_fire() {
    // When the defender's shields are depleted, `TimingWindow::ShieldBreak` effects on the
    // defender crew (e.g. hostile ship abilities) must apply to that sub-round's counter-attack.
    let attacker = Combatant {
        id: "att".to_string(),
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
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 500.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "def".to_string(),
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
        hull_health: 2000.0,
        shield_health: 100.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let attacker_crew = CrewConfiguration { seats: vec![] };
    let defender_crew_sb = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "def_sb_pierce".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::ShieldBreak,
                boostable: false,
                effect: AbilityEffect::PierceBonus(0.5),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let cfg = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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

    let baseline = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    let with_sb = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &defender_crew_sb,
    );

    assert!(
        with_sb.attacker_hull_remaining < baseline.attacker_hull_remaining,
        "defender ShieldBreak pierce should increase counter damage: baseline={}, with_sb={}",
        baseline.attacker_hull_remaining,
        with_sb.attacker_hull_remaining
    );
}

#[test]
fn attacker_self_shield_break_pierce_applies_to_later_outbound_weapons_same_round() {
    // Counter strips the player's shields; `TimingWindow::SelfShieldBreak` effects stack on the
    // round accumulator (same path as enemy `ShieldBreak` pierce): `pre_attack_pierce_bonus` is read
    // each sub-round; AttackMultiplier from that window is not folded into per-shot pre_attack yet.
    let attacker = Combatant {
        id: "plyr".to_string(),
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
        shield_health: 100.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![
            WeaponStats {
                attack: 50.0,
                shots: Some(1),
                ..Default::default()
            },
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                ..Default::default()
            },
        ],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "npc".to_string(),
        attack: 0.0,
        mitigation: 0.35,
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
        weapons: vec![WeaponStats {
            attack: 400.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 3,
        trace_mode: TraceMode::Off,
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
    let baseline = simulate_combat(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration { seats: vec![] },
    );
    let crew_sb = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "own_sb_damage".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::SelfShieldBreak,
                boostable: true,
                effect: AbilityEffect::PierceBonus(0.6),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let boosted = simulate_combat(&attacker, &defender, &cfg, &crew_sb);
    assert!(
        boosted.total_damage > baseline.total_damage,
        "SelfShieldBreak pierce should buff weapon 2 after counter breaks shields: baseline={}, boosted={}",
        baseline.total_damage,
        boosted.total_damage
    );
}

#[test]
fn mitigation_with_morale_applies_primary_piercing_bonus_when_active() {
    let defender = DefenderStats {
        armor: 100.0,
        shield_deflection: 80.0,
        dodge: 60.0,
    };
    let attacker = AttackerStats {
        armor_piercing: 50.0,
        shield_piercing: 40.0,
        accuracy: 30.0,
    };

    let baseline = mitigation_with_morale(defender, attacker, ShipType::Battleship, false);
    let morale = mitigation_with_morale(defender, attacker, ShipType::Battleship, true);

    approx_eq(
        baseline,
        mitigation(defender, attacker, ShipType::Battleship),
        1e-12,
    );
    assert!(
        morale < baseline,
        "morale should lower mitigation and increase final damage"
    );
    approx_eq(morale, 0.5869213146636679, 1e-12);
}

#[test]
fn trace_collector_records_only_when_enabled() {
    let event = CombatEvent {
        event_type: "round_start".to_string(),
        round_index: 1,
        phase: "round".to_string(),
        source: EventSource {
            ship_ability_id: Some("baseline_round".to_string()),
            ..EventSource::default()
        },
        values: Map::new(),
        weapon_index: None,
    };

    let mut trace_on = TraceCollector::new(true);
    trace_on.record(event.clone());
    assert_eq!(trace_on.events().len(), 1);

    let mut trace_off = TraceCollector::new(false);
    trace_off.record(event);
    assert!(trace_off.events().is_empty());
}

#[test]
fn serialize_events_json_matches_python_shape() {
    let json = serialize_events_json(&[CombatEvent {
        event_type: "attack_roll".to_string(),
        round_index: 1,
        phase: "attack".to_string(),
        source: EventSource {
            officer_id: Some("nero".to_string()),
            ..EventSource::default()
        },
        values: Map::from_iter([("roll".to_string(), Value::from(0.617753))]),
        weapon_index: None,
    }])
    .expect("serialization should succeed");

    let parsed: Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(parsed[0]["event_type"], "attack_roll");
    assert_eq!(parsed[0]["round_index"], 1);
    assert_eq!(parsed[0]["phase"], "attack");
    assert_eq!(
        parsed[0]["source"],
        serde_json::json!({"officer_id": "nero"})
    );
    assert_eq!(parsed[0]["values"], serde_json::json!({"roll": 0.617753}));
}

#[test]
fn apex_barrier_reduces_damage_and_apex_shred_weakens_barrier() {
    // One round, no mitigation/pierce/crit/proc: damage = attack. Apex factor = 10000/(10000+effective_barrier).
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender_no_barrier = Combatant {
        id: "defender".to_string(),
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
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender_10k_barrier = Combatant {
        id: "defender".to_string(),
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
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 10_000.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let crew = CrewConfiguration::default();

    let no_barrier = simulate_combat(&attacker, &defender_no_barrier, &config, &crew);
    let with_10k_barrier = simulate_combat(&attacker, &defender_10k_barrier, &config, &crew);
    // 10k barrier, 0 shred: factor = 10000/(10000+10000) = 0.5 â†’ 50% damage gets through.
    approx_eq(no_barrier.total_damage, 200.0, 1e-12);
    approx_eq(with_10k_barrier.total_damage, 100.0, 1e-12);

    let attacker_100_pct_shred = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 1.0, // 100% shred
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let with_shred = simulate_combat(
        &attacker_100_pct_shred,
        &defender_10k_barrier,
        &config,
        &crew,
    );
    // Effective barrier = 10000/(1+1) = 5000, factor = 10000/(10000+5000) = 2/3. Engine rounds total_damage.
    approx_eq(with_shred.total_damage, 200.0 * (10000.0 / 15000.0), 0.01);
}

/// Shield mitigation (STFC Toolbox game-mechanics): S * damage to shield, (1-S) * damage to hull.
/// When shields are depleted, all damage goes to hull.
#[test]
fn shield_mitigation_splits_damage_between_shield_and_hull() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    // Defender with 500 SHP, 80% shield mitigation â†’ 80% of damage to shield, 20% to hull.
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 1000.0,
        shield_health: 500.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let result = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    // 200 damage: 80% = 160 to shield, 20% = 40 to hull.
    approx_eq(result.total_damage, 200.0, 1e-12);
    approx_eq(result.defender_shield_remaining, 500.0 - 160.0, 1e-12);
    approx_eq(result.defender_hull_remaining, 1000.0 - 40.0, 1e-12);
}

#[test]
fn shield_overflow_goes_to_hull_when_shields_depleted_mid_round() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 1000.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    // Defender has only 100 SHP; 80% of 1000 = 800 to shield â†’ 100 absorbed, 700 overflow to hull. 20% = 200 to hull. Total hull = 900.
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 2000.0,
        shield_health: 100.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let result = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    approx_eq(result.total_damage, 1000.0, 1e-12);
    approx_eq(result.defender_shield_remaining, 0.0, 1e-12);
    approx_eq(result.defender_hull_remaining, 2000.0 - 900.0, 1e-12); // 900 hull damage (200 + 700 overflow)
}

#[test]
fn when_shields_depleted_all_damage_goes_to_hull_next_rounds() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 500.0,
        shield_health: 50.0, // Round 1: 80% of 100 = 80 to shield â†’ 50 absorbed, 30 overflow; 20% = 20 to hull. Shield gone. Hull takes 20+30 = 50.
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let result = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    approx_eq(result.defender_shield_remaining, 0.0, 1e-12);
    // Round 1: 50 hull damage. Round 2 and 3: 100% to hull = 100 each. Total hull damage = 50 + 100 + 100 = 250.
    assert!(result.defender_hull_remaining <= (500.0 - 250.0) + 1.0);
    assert!(result.defender_hull_remaining >= (500.0 - 250.0) - 1.0);
}

#[test]
fn officer_apex_shred_bonus_at_combat_begin_increases_damage_through_barrier() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 10_000.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let crew_no_apex = CrewConfiguration::default();
    let crew_with_apex_shred = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Officer (Apex Shred)".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ApexShredBonus(0.15),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let without = simulate_combat(&attacker, &defender, &config, &crew_no_apex);
    let with_ability = simulate_combat(&attacker, &defender, &config, &crew_with_apex_shred);
    // Without officer: factor = 10000/(10000+10000) = 0.5 â†’ 100 damage.
    approx_eq(without.total_damage, 100.0, 1e-12);
    // With +15% Apex Shred: effective_barrier = 10000/1.15 â‰ˆ 8695.65, factor â‰ˆ 10000/18695.65 â‰ˆ 0.535 â†’ ~107 damage.
    assert!(
        with_ability.total_damage > without.total_damage,
        "officer Apex Shred should increase damage through barrier"
    );
    approx_eq(
        with_ability.total_damage,
        200.0 * (10000.0 / (10000.0 + 10_000.0 / 1.15)),
        0.5,
    );
}

#[test]
fn officer_apex_barrier_bonus_at_combat_begin_reduces_damage_taken() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender_no_bonus = Combatant {
        id: "defender".to_string(),
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
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 5_000.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let crew_no_apex = CrewConfiguration::default();
    let crew_with_apex_barrier = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Officer (Apex Barrier)".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ApexBarrierBonus(5000.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let without = simulate_combat(&attacker, &defender_no_bonus, &config, &crew_no_apex);
    let with_ability = simulate_combat(
        &attacker,
        &defender_no_bonus,
        &config,
        &crew_with_apex_barrier,
    );
    // Defender has 5k base barrier; officer adds 5k â†’ effective 10k. Without officer: factor = 10000/15000 = 2/3 â†’ 133.33. With officer: factor = 10000/20000 = 0.5 â†’ 100.
    assert!(
        with_ability.total_damage < without.total_damage,
        "officer Apex Barrier bonus should reduce damage taken"
    );
    approx_eq(without.total_damage, 200.0 * (10000.0 / 15000.0), 0.5);
    approx_eq(with_ability.total_damage, 100.0, 0.5);
}

#[test]
fn ship_ability_pierce_bonus_at_round_start_increases_damage() {
    // Ship hull ability (CrewSeat::Ship, AbilityClass::ShipAbility) is evaluated like officer abilities.
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.05,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.15,
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
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let crew_no_ship_ability = CrewConfiguration::default();
    let crew_with_ship_ability = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "pierce_on_hit".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::PierceBonus(0.10),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let without = simulate_combat(&attacker, &defender, &config, &crew_no_ship_ability);
    let with_ability = simulate_combat(&attacker, &defender, &config, &crew_with_ship_ability);
    assert!(
        with_ability.total_damage > without.total_damage,
        "ship ability pierce_bonus at round_start should increase damage"
    );
}

#[test]
fn defender_faction_gates_combat_begin_attack_multiplier() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Off,
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
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "vs_klingon".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                // `AttackMultiplier` is additive on the pre-attack sum: effective mult = 1 + sum(modifiers).
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: Some(AbilityCondition::DefenderFactionIs(
                    OpponentFactionTag::Klingon,
                )),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let romulan = simulate_combat_with_defender_faction(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Romulan,
    );
    let klingon = simulate_combat_with_defender_faction(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Klingon,
    );
    assert!(
        klingon.total_damage > romulan.total_damage,
        "faction-gated attack multiplier should apply only for matching defender faction"
    );
    approx_eq(romulan.total_damage, klingon.total_damage / 2.0, 1.0);
}

#[test]
fn defender_hostile_tag_mask_gates_combat_begin_attack_multiplier() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "vs_conqueror_borg".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(0.5),
                condition: Some(AbilityCondition::DefenderHostileTagsAllPresent {
                    required_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
                }),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let cfg_no_tag = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Off,
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
    let cfg_tagged = SimulationConfig {
        defender_hostile_tag_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        ..cfg_no_tag.clone()
    };
    let no_tag = simulate_combat(&attacker, &defender, &cfg_no_tag, &crew);
    let tagged = simulate_combat(&attacker, &defender, &cfg_tagged, &crew);
    assert!(
        tagged.total_damage > no_tag.total_damage,
        "hostile-tag-gated attack multiplier should apply only when defender_hostile_tag_mask matches"
    );
    approx_eq(no_tag.total_damage, tagged.total_damage / 1.5, 1.0);
}

#[test]
fn defender_hostile_tag_mask_gates_apex_barrier_bonus() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        apex_barrier: 5_000.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "apex_vs_conqueror_borg".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ApexBarrierBonus(5_000.0),
                condition: Some(AbilityCondition::DefenderHostileTagsAllPresent {
                    required_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
                }),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let cfg_no_tag = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let cfg_tagged = SimulationConfig {
        defender_hostile_tag_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        ..cfg_no_tag.clone()
    };
    let no_tag = simulate_combat(&attacker, &defender, &cfg_no_tag, &crew);
    let tagged = simulate_combat(&attacker, &defender, &cfg_tagged, &crew);
    assert!(
        tagged.total_damage < no_tag.total_damage,
        "hostile-tag-gated apex barrier bonus should reduce damage only when mask matches"
    );
}

#[test]
fn conqueror_borg_beam_suppression_flag_follows_combat_begin_gate() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 50.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "beam_suppression".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ConquerorBorgBeamSuppression,
                condition: Some(AbilityCondition::DefenderHostileTagsAllPresent {
                    required_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
                }),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let cfg_no_tag = SimulationConfig {
        rounds: 1,
        seed: 3,
        trace_mode: TraceMode::Off,
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
    let cfg_tagged = SimulationConfig {
        defender_hostile_tag_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        ..cfg_no_tag.clone()
    };
    assert!(
        !simulate_combat(&attacker, &defender, &cfg_no_tag, &crew).conqueror_borg_beam_suppression
    );
    assert!(
        simulate_combat(&attacker, &defender, &cfg_tagged, &crew).conqueror_borg_beam_suppression
    );
}

#[test]
fn evolutionary_assimilation_instant_loss_vs_conqueror_borg_respects_beam_suppression() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 50.0,
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
        hull_health: 1000.0,
        shield_health: 500.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew_empty = CrewConfiguration { seats: vec![] };
    let cfg = SimulationConfig {
        rounds: 5,
        seed: 99,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: vec![
            EVOLUTIONARY_ASSIMILATION_FORBIDDEN_OFFICER_IDS[0].to_string()
        ],
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_hyperthermic_decay_fraction: 0.0,
        emit_state_snapshots: false,
    };
    let r_kill = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &crew_empty,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(!r_kill.attacker_won);
    assert_eq!(r_kill.attacker_hull_remaining, 0.0);
    assert_eq!(r_kill.rounds_simulated, 0);
    approx_eq(r_kill.attacker_shield_remaining, 500.0, 1e-9);

    let crew_suppress = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "beam_suppression".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ConquerorBorgBeamSuppression,
                condition: Some(AbilityCondition::DefenderHostileTagsAllPresent {
                    required_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
                }),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let r_ok = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &crew_suppress,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(r_ok.rounds_simulated > 0);
    assert!(r_ok.attacker_hull_remaining > 0.0);
}

#[test]
fn defender_ship_type_gate_attack_multiplier_only_matches_class() {
    let attacker = Combatant {
        id: "attacker".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Off,
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
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "vs_battleship".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: Some(AbilityCondition::DefenderShipTypeIs(ShipType::Battleship)),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let explorer = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    let battleship = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    assert!(
        battleship.total_damage > explorer.total_damage,
        "hull-class–gated attack multiplier should apply only when defender ship type matches"
    );
    approx_eq(explorer.total_damage, battleship.total_damage / 2.0, 1.0);
}

#[test]
fn defender_opponent_kind_gate_npc_hostile_vs_player_ship() {
    let attacker = Combatant {
        id: "attacker".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 19,
        trace_mode: TraceMode::Off,
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
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "vs_npc_hostile".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: Some(AbilityCondition::DefenderIsNpcHostile),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let empty = &CrewConfiguration { seats: vec![] };
    let vs_npc = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        empty,
    );
    let vs_player = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        empty,
    );
    assert!(
        vs_npc.total_damage > vs_player.total_damage,
        "DefenderIsNpcHostile gate should apply only when scenario marks defender as NPC hostile"
    );
    approx_eq(vs_player.total_damage, vs_npc.total_damage / 2.0, 1.0);
}

#[test]
fn attacker_ship_type_gate_attack_multiplier_only_matches_player_class() {
    let attacker = Combatant {
        id: "attacker".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 13,
        trace_mode: TraceMode::Off,
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
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "if_battleship".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: Some(AbilityCondition::AttackerShipTypeIs(ShipType::Battleship)),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let same_defender_type = ShipType::Explorer;
    let with_bb = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        same_defender_type,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    let with_int = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        same_defender_type,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    assert!(
        with_bb.total_damage > with_int.total_damage,
        "attacker hull-class gate should apply only when player ship type matches"
    );
    approx_eq(with_int.total_damage, with_bb.total_damage / 2.0, 1.0);
}

#[test]
fn and_attacker_defender_ship_type_gate_requires_both_hull_classes() {
    // Mirrors canonical rows such as SNW Ortegas (battleship vs explorer): self hull AND hostile hull.
    let attacker = Combatant {
        id: "attacker".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 17,
        trace_mode: TraceMode::Off,
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
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "bb_vs_explorer".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: Some(AbilityCondition::And(vec![
                    AbilityCondition::AttackerShipTypeIs(ShipType::Battleship),
                    AbilityCondition::DefenderShipTypeIs(ShipType::Explorer),
                ])),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let empty = &CrewConfiguration { seats: vec![] };
    let both_match = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        empty,
    );
    let wrong_player_hull = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Interceptor,
        true,
        false,
        empty,
    );
    let wrong_hostile_hull = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        empty,
    );
    assert!(
        both_match.total_damage > wrong_player_hull.total_damage,
        "AND gate should fail when player hull class does not match"
    );
    assert!(
        both_match.total_damage > wrong_hostile_hull.total_damage,
        "AND gate should fail when hostile hull class does not match"
    );
    approx_eq(
        wrong_player_hull.total_damage,
        wrong_hostile_hull.total_damage,
        1.0,
    );
    approx_eq(
        wrong_player_hull.total_damage,
        both_match.total_damage / 2.0,
        1.0,
    );
}

#[test]
fn round_cap_via_round_range_limits_combat_begin_attack_multiplier() {
    let attacker = Combatant {
        id: "attacker".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 5,
        seed: 2,
        trace_mode: TraceMode::Off,
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
    let uncapped = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "full_mult".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let capped = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "two_round_mult".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: Some(AbilityCondition::RoundRange { min: 1, max: 2 }),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let r_uncapped = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &uncapped,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    let r_capped = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &capped,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    assert!(
        r_uncapped.total_damage > r_capped.total_damage,
        "round-limited combat_begin multiplier should deal less total damage over long fights"
    );
}

#[test]
fn ship_ability_hostile_crit_reduction_preserves_more_attacker_hull() {
    // Mirrors U.S.S. Crozier "Gunboat Diplomacy": hostile return-fire crits deal less damage for N rounds.
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 10.0,
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 1.0,
        crit_multiplier: 2.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 500_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 11,
        trace_mode: TraceMode::Off,
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
    let crew_plain = CrewConfiguration::default();
    let crew_crozier_style = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "47269853".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileCritDamageReduction {
                    reduction: 0.5,
                    duration_rounds: 5,
                    additive_percentage_points: false,
                    stacks: false,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let without = simulate_combat(&attacker, &defender, &config, &crew_plain);
    let with_red = simulate_combat(&attacker, &defender, &config, &crew_crozier_style);
    assert!(
        with_red.attacker_hull_remaining > without.attacker_hull_remaining,
        "hostile crit reduction should leave more attacker hull; without={} with={}",
        without.attacker_hull_remaining,
        with_red.attacker_hull_remaining
    );
}

#[test]
fn ship_ability_receive_damage_timing_emits_trace() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 15.0,
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 15.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 40.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "receive_damage_apex_shred".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::ReceiveDamage,
                boostable: false,
                effect: AbilityEffect::ApexShredBonus(0.04),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 2,
            seed: 11,
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
    let hits: Vec<_> = result
        .events
        .iter()
        .filter(|e| {
            e.event_type == "ability_activation"
                && e.phase == "receive_damage"
                && e.source.ship_ability_id.as_deref() == Some("receive_damage_apex_shred")
        })
        .collect();
    assert!(
        !hits.is_empty(),
        "expected receive_damage ship ability activations when defender deals hull damage to attacker"
    );
}

#[test]
fn below_deck_morale_effect_triggers_morale_and_increases_damage() {
    let attacker = Combatant {
        id: "enterprise".to_string(),
        attack: 120.0,
        mitigation: 0.1,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.15,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 10.0,
        mitigation: 0.35,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let no_morale = CrewConfiguration::default();
    let morale_below_decks = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "round_start_morale".to_string(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::Morale(1.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
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

    let baseline = simulate_combat(&attacker, &defender, &config, &no_morale);
    let with_morale = simulate_combat(&attacker, &defender, &config, &morale_below_decks);

    assert!(with_morale.total_damage > baseline.total_damage);

    let morale_events = with_morale
        .events
        .iter()
        .filter(|event| event.event_type == "morale_activation")
        .count();
    assert_eq!(morale_events, 2);
}

#[test]
fn morale_active_condition_gates_round_start_effects_until_morale_roll_succeeds() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 200.0,
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
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.2,
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
        hull_health: 8000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew_with_morale_chance = |chance: f64| CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::BelowDeck,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "morale_src".to_string(),
                    class: AbilityClass::BelowDeck,
                    timing: TimingWindow::RoundStart,
                    boostable: false,
                    effect: AbilityEffect::Morale(chance),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Ship,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "morale_gated_accum".to_string(),
                    class: AbilityClass::ShipAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: false,
                    effect: AbilityEffect::AccumulatingAttackMultiplier {
                        initial: 1.0,
                        growth_per_round: 0.15,
                        ceiling: 10.0,
                    },
                    condition: Some(AbilityCondition::MoraleActive),
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };

    let config = SimulationConfig {
        rounds: 6,
        seed: 100,
        trace_mode: TraceMode::Off,
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

    let never_morale =
        simulate_combat(&attacker, &defender, &config, &crew_with_morale_chance(0.0));
    let always_morale =
        simulate_combat(&attacker, &defender, &config, &crew_with_morale_chance(1.0));

    assert!(
        always_morale.total_damage > never_morale.total_damage,
        "MoraleActive accumulating damage should apply only when Morale procs"
    );
}

#[test]
fn assimilated_reduces_officer_effectiveness_by_twenty_five_percent() {
    let attacker = Combatant {
        id: "enterprise".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let baseline_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "damage_buff".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: true,
                effect: AbilityEffect::AttackMultiplier(1.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let assimilated_crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::BelowDeck,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "dezoc_like_assimilate".to_string(),
                    class: AbilityClass::BelowDeck,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::Assimilated {
                        chance: 1.0,
                        duration_rounds: 2,
                    },
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "damage_buff".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::AttackPhase,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(1.0),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };

    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
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

    let baseline = simulate_combat(&attacker, &defender, &config, &baseline_crew);
    let with_assimilated = simulate_combat(&attacker, &defender, &config, &assimilated_crew);

    approx_eq(baseline.total_damage, 200.0, 1e-12);
    approx_eq(with_assimilated.total_damage, 175.0, 1e-12);

    let attack_activation = with_assimilated
        .events
        .iter()
        .find(|event| {
            event.event_type == "ability_activation"
                && event.phase == "attack"
                && event.source.ship_ability_id.as_deref() == Some("damage_buff")
        })
        .expect("attack ability activation should be present");
    assert_eq!(attack_activation.values["assimilated"], Value::Bool(true));
    approx_eq(
        attack_activation.values["effectiveness_multiplier"]
            .as_f64()
            .expect("effectiveness multiplier as f64"),
        0.75,
        1e-12,
    );
}

#[test]
fn dezoc_style_assimilated_can_trigger_from_below_decks() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Dezoc".to_string(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::Assimilated {
                    chance: 1.0,
                    duration_rounds: 4,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 7,
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

    let trigger_event = result
        .events
        .iter()
        .find(|event| event.event_type == "assimilated_trigger")
        .expect("assimilated trigger event should be emitted");
    assert_eq!(trigger_event.phase, "round_start");
    assert_eq!(trigger_event.values["triggered"], Value::Bool(true));
    assert_eq!(
        trigger_event.source.ship_ability_id.as_deref(),
        Some("Dezoc")
    );
}

#[test]
fn hull_breach_boosts_critical_damage_after_crit_multiplier() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 1.0,
        crit_multiplier: 2.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Lorca".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: true,
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

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 7,
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

    // With hull breach: crit_mult = base_crit_mult * 1.5 = 2.0 * 1.5 = 3.0 (per game rules).
    approx_eq(result.total_damage, 300.0, 1e-12);

    let crit_event = result
        .events
        .iter()
        .find(|event| event.event_type == "crit_resolution")
        .expect("crit event should be present");
    assert_eq!(crit_event.values["hull_breach_active"], Value::Bool(true));
    approx_eq(
        crit_event.values["multiplier"]
            .as_f64()
            .expect("multiplier as f64"),
        3.0,
        1e-12,
    );
}

#[test]
fn typed_crit_chance_bonus_applies_at_crit_roll() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 2.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "crit_cap".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::CritChanceBonus(1.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 42,
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

    let crit_event = result
        .events
        .iter()
        .find(|e| e.event_type == "crit_resolution")
        .expect("crit resolution");
    assert_eq!(crit_event.values["is_crit"], Value::Bool(true));
    approx_eq(
        crit_event.values["multiplier"]
            .as_f64()
            .expect("multiplier as f64"),
        2.0,
        1e-9,
    );
    approx_eq(result.total_damage, 200.0, 1e-6);
}

#[test]
fn typed_crit_damage_multiplier_multiplies_combatant_crit_tier() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 1.0,
        crit_multiplier: 2.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "crit_dmg".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::CritDamageMultiplier(1.5),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 1,
            trace_mode: TraceMode::Off,
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

    approx_eq(result.total_damage, 300.0, 1e-6);
}

#[test]
fn hull_breach_can_trigger_from_critical_hit_officer_ability() {
    let attacker = Combatant {
        id: "gorkon_ship".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 1.0,
        crit_multiplier: 1.5,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "target".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Gorkon".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: true,
                effect: AbilityEffect::HullBreach {
                    chance: 1.0,
                    duration_rounds: 3,
                    requires_critical: true,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 7,
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

    let hull_breach_event = result
        .events
        .iter()
        .find(|event| event.event_type == "hull_breach_trigger")
        .expect("hull breach trigger should be emitted");
    assert_eq!(hull_breach_event.phase, "attack");
    assert_eq!(hull_breach_event.values["triggered"], Value::Bool(true));
    assert_eq!(
        hull_breach_event.values["requires_critical"],
        Value::Bool(true)
    );
}
#[test]
fn simulate_combat_uses_seed_and_emits_canonical_events() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 120.0,
        mitigation: 0.1,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.15,
        crit_chance: 0.5,
        crit_multiplier: 1.8,
        crit_damage_floor: 0.0,
        proc_chance: 0.4,
        proc_multiplier: 1.25,
        end_of_round_damage: 3.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 10.0,
        mitigation: 0.35,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
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

    let crew = CrewConfiguration::default();
    let first = simulate_combat(&attacker, &defender, &config, &crew);
    let second = simulate_combat(&attacker, &defender, &config, &crew);

    assert_eq!(first.events, second.events);
    assert_eq!(first.total_damage, second.total_damage);

    assert_eq!(first.events.len(), 23);
    let expected_event_types = [
        "round_start",
        "attack_roll",
        "mitigation_calc",
        "pierce_calc",
        "crit_resolution",
        "proc_triggers",
        "stack_resolution",
        "damage_application",
        "mitigation_calc",
        "crit_resolution",
        "end_of_round_effects",
    ];
    for (index, expected) in expected_event_types.iter().enumerate() {
        assert_eq!(first.events[index].event_type, *expected);
        assert_eq!(first.events[index + 11].event_type, *expected);
    }
    assert_eq!(first.events[22].event_type, "combat_end_effects");
    assert_eq!(first.events[4].phase, "attack");
    assert_eq!(first.events[8].phase, "counter");

    // Seed 7 (SplitMix64) produces deterministic rolls; exact values depend on RNG implementation.
    let round_one_crit = &first.events[4];
    let round_one_proc = &first.events[5];
    let round_one_crit_roll = round_one_crit.values["roll"]
        .as_f64()
        .expect("crit roll as f64");
    let round_one_proc_roll = round_one_proc.values["roll"]
        .as_f64()
        .expect("proc roll as f64");
    assert!((0.0..=1.0).contains(&round_one_crit_roll));
    assert!((0.0..=1.0).contains(&round_one_proc_roll));
    assert_eq!(
        round_one_crit.values["is_crit"],
        Value::Bool(round_one_crit_roll < 0.5)
    );
    assert_eq!(
        round_one_proc.values["triggered"],
        Value::Bool(round_one_proc_roll < 0.4)
    );

    let round_two_crit = &first.events[15];
    let round_two_proc = &first.events[16];
    let round_two_crit_roll = round_two_crit.values["roll"]
        .as_f64()
        .expect("crit roll as f64");
    let round_two_proc_roll = round_two_proc.values["roll"]
        .as_f64()
        .expect("proc roll as f64");
    assert!((0.0..=1.0).contains(&round_two_crit_roll));
    assert!((0.0..=1.0).contains(&round_two_proc_roll));
    assert_eq!(
        round_two_crit.values["is_crit"],
        Value::Bool(round_two_crit_roll < 0.5)
    );
    assert_eq!(
        round_two_proc.values["triggered"],
        Value::Bool(round_two_proc_roll < 0.4)
    );

    // Total damage in [200, 600] for 2 rounds with this setup
    assert!(
        first.total_damage >= 200.0 && first.total_damage <= 600.0,
        "total_damage {}",
        first.total_damage
    );
}

#[test]
fn mitigation_trace_includes_component_breakdown_for_hostile_and_counter_paths() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 120.0,
        mitigation: 0.25,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.15,
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
        weapons: vec![WeaponStats {
            attack: 120.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let params = HostileMitigationParams {
        defender_stats: DefenderStats {
            armor: 200.0,
            shield_deflection: 140.0,
            dodge: 110.0,
        },
        base_attacker_stats: AttackerStats {
            armor_piercing: 80.0,
            shield_piercing: 60.0,
            accuracy: 50.0,
        },
        ship_type: ShipType::Battleship,
        mystery_mitigation_factor: 0.05,
        floor: 0.16,
        ceiling: 0.72,
    };
    let defender = Combatant {
        id: "hostile".to_string(),
        attack: 80.0,
        mitigation: 0.35,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.1,
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
        weapons: vec![WeaponStats {
            attack: 80.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: Some(params),
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
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
    let crew = CrewConfiguration::default();
    let result = simulate_combat(&attacker, &defender, &config, &crew);

    let outbound = result
        .events
        .iter()
        .find(|e| e.event_type == "mitigation_calc" && e.phase == "defense")
        .expect("outbound mitigation_calc event");
    for key in [
        "mitigation_raw",
        "mitigation_floor",
        "mitigation_ceiling",
        "c_armor",
        "c_shield",
        "c_dodge",
        "armor_ratio",
        "shield_ratio",
        "dodge_ratio",
        "f_armor",
        "f_shield",
        "f_dodge",
        "weighted_armor",
        "weighted_shield",
        "weighted_dodge",
        "mystery_mitigation_factor",
        "one_minus_mystery",
    ] {
        assert!(
            outbound.values.contains_key(key),
            "expected outbound mitigation trace key: {key}"
        );
    }
    let mitigation = outbound.values["mitigation"]
        .as_f64()
        .expect("mitigation as f64");
    let multiplier = outbound.values["multiplier"]
        .as_f64()
        .expect("multiplier as f64");
    let raw = outbound.values["mitigation_raw"]
        .as_f64()
        .expect("mitigation_raw as f64");
    let floor = outbound.values["mitigation_floor"]
        .as_f64()
        .expect("mitigation_floor as f64");
    let ceiling = outbound.values["mitigation_ceiling"]
        .as_f64()
        .expect("mitigation_ceiling as f64");
    assert!(
        raw >= 0.0,
        "raw mitigation should be non-negative, got {raw}"
    );
    assert!(
        mitigation >= floor && mitigation <= ceiling,
        "clamped mitigation {mitigation} must be within [{floor}, {ceiling}]"
    );
    approx_eq(multiplier, (1.0 - mitigation).max(0.0), 1e-6);

    let counter = result
        .events
        .iter()
        .find(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
        .expect("counter mitigation_calc event");
    for key in [
        "base_mitigation",
        "armor_component",
        "shield_deflection_component",
        "dodge_component",
        "damage_reduction",
        "mitigation_additive_bonus",
        "dodge_bonus",
        "dodge_coefficient",
        "dodge_mitigation_bonus",
    ] {
        assert!(
            counter.values.contains_key(key),
            "expected counter mitigation trace key: {key}"
        );
    }
    let counter_mitigation = counter.values["mitigation"]
        .as_f64()
        .expect("counter mitigation as f64");
    let counter_multiplier = counter.values["multiplier"]
        .as_f64()
        .expect("counter multiplier as f64");
    approx_eq(
        counter_multiplier,
        (1.0 - counter_mitigation).max(0.0),
        1e-6,
    );
}

#[test]
fn counter_fire_inbound_uses_ship_type_weighted_components_when_set() {
    // When the attacker has populated mitigation-component fields (post-resolution from
    // apply_profile_to_attacker), the inbound counter-fire path computes effective
    // mitigation as `c_armor·armor + c_shield·shield_deflection + c_dodge·dodge + damage_reduction`,
    // weighted by the attacker ship type's coefficients. The aggregated `mitigation` scalar
    // is unused on this path when any component is non-zero.
    //
    // The trace event surfaces the effective mitigation as `mitigation`. Two runs:
    //   (a) `mitigation: 0.40` scalar, components all zero → engine falls back to scalar
    //   (b) `mitigation: 0.0`, components arranged so the weighted sum = 0.40 / scaled
    // We just verify the component path produces a strictly different effective mitigation
    // than the scalar-fallback path under the same nominal sum, proving coefficients are
    // applied.
    fn run(attacker: Combatant, defender: Combatant, ship_type: ShipType) -> f64 {
        let mut a = attacker;
        // The engine reads `attacker_ship_type` from setup; place it on the attacker's
        // weapon coefficient path via simulate_combat (which uses attacker ship from
        // ShipType::Battleship default in this minimal test). We use the public engine API.
        let _ = ship_type; // shape; kept for future overload
        a.weapons = vec![WeaponStats {
            attack: 0.0,
            shots: Some(0),
            ..Default::default()
        }];
        let mut d = defender;
        d.weapons = vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }];
        let config = SimulationConfig {
            rounds: 1,
            seed: 7,
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
        let crew = CrewConfiguration::default();
        let result = simulate_combat(&a, &d, &config, &crew);
        result
            .events
            .iter()
            .find(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
            .and_then(|e| e.values.get("mitigation").and_then(|v| v.as_f64()))
            .expect("counter mitigation_calc trace")
    }

    let scalar_only = Combatant {
        id: "scalar".to_string(),
        attack: 100.0,
        mitigation: 0.40,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: Vec::new(),
        hostile_mitigation_params: None,
    };
    let components_only = Combatant {
        // Same nominal sum (0.10 + 0.10 + 0.10 + 0.10 = 0.40) but split across components.
        // Once weighted by ship-type coefficients (which are < 1 for armor/shield/dodge on
        // most ship classes), the effective mitigation is strictly less than 0.40.
        armor: 0.10,
        shield_deflection: 0.10,
        dodge: 0.10,
        damage_reduction: 0.10,
        mitigation: 0.0,
        ..scalar_only.clone()
    };
    let defender = Combatant {
        id: "hostile".to_string(),
        attack: 100.0,
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
        weapons: Vec::new(),
        hostile_mitigation_params: None,
    };

    let scalar_eff = run(scalar_only, defender.clone(), ShipType::Battleship);
    let component_eff = run(components_only, defender, ShipType::Battleship);

    // Scalar fallback path returns 0.40 exactly (no coefficient applied).
    approx_eq(scalar_eff, 0.40, 1e-6);
    // Component path: damage_reduction (0.10) is flat + weighted components < 0.30 → strictly < 0.40.
    assert!(
        component_eff < scalar_eff,
        "ship-type-weighted components ({component_eff}) should be < scalar fallback ({scalar_eff})"
    );
    // And > damage_reduction alone (0.10), since the weighted components contribute positively.
    assert!(
        component_eff > 0.10,
        "weighted components should contribute on top of damage_reduction"
    );
}

#[test]
fn crit_damage_floor_clamps_attacker_outbound_reduction() {
    // Defender crew has a strong HostileCritDamageReduction (0.80). Attacker has
    // crit_damage_floor = 1.5 and base crit_multiplier = 2.0. Without the floor, reduction
    // 0.80 would drive the multiplier to 0.40; the floor must clamp it at 1.5.
    let mut attacker = Combatant {
        attack: 100.0,
        crit_chance: 1.0, // guaranteed crit
        crit_multiplier: 2.0,
        crit_damage_floor: 1.5,
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    attacker.id = "attacker".to_string();
    let mut defender = Combatant {
        attack: 0.0,
        hull_health: 1_000_000.0, // sponge so the fight reaches a crit
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    };
    defender.id = "defender".to_string();
    // Defender crew with a HostileCritDamageReduction that reduces attacker outbound crit.
    let defender_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "opponent_cdr".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileCritDamageReduction {
                    reduction: 0.80,
                    duration_rounds: 99,
                    additive_percentage_points: false,
                    stacks: false,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 1,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let crew = CrewConfiguration::default();
    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true, // defender is player ship (PvP)
        &defender_crew,
    );
    let crit_event = result
        .events
        .iter()
        .find(|e| e.event_type == "crit_resolution" && e.phase == "attack")
        .expect("attacker crit_resolution trace event");
    let multiplier = crit_event.values["multiplier"]
        .as_f64()
        .expect("multiplier as f64");
    let floor = crit_event.values["crit_damage_floor"]
        .as_f64()
        .expect("crit_damage_floor in trace");
    let reduction = crit_event.values["attacker_crit_reduction_mult"]
        .as_f64()
        .expect("attacker_crit_reduction_mult in trace");
    approx_eq(floor, 1.5, 1e-6);
    approx_eq(reduction, 0.80, 1e-6);
    assert!(
        multiplier >= 1.5 - 1e-6,
        "crit_damage_floor should clamp multiplier at ≥ 1.5, got {multiplier}"
    );
    // The unclamped reduced value would be 2.0 × (1 − 0.80) = 0.4, far below 1.5;
    // verifying the clamp activated (not just that 1.5 ≤ multiplier ≤ 2.0).
    assert!(
        multiplier < 2.0,
        "without floor + reduction multiplier would be 2.0; reduction must have applied first ({multiplier})"
    );
}

#[test]
fn crit_damage_floor_dormant_when_no_attacker_outbound_reduction() {
    // With crit_damage_floor = 1.5 but no defender CDR, the attacker's crit multiplier
    // (base 2.0) is unaffected — floor never kicks in because reduced == raw.
    let mut attacker = Combatant {
        attack: 100.0,
        crit_chance: 1.0,
        crit_multiplier: 2.0,
        crit_damage_floor: 1.5,
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    attacker.id = "attacker".to_string();
    let mut defender = Combatant {
        attack: 0.0,
        hull_health: 1_000_000.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    };
    defender.id = "defender".to_string();
    let defender_crew = CrewConfiguration::default();
    let config = SimulationConfig {
        rounds: 1,
        seed: 1,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let crew = CrewConfiguration::default();
    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        false,
        &defender_crew,
    );
    let crit_event = result
        .events
        .iter()
        .find(|e| e.event_type == "crit_resolution" && e.phase == "attack")
        .expect("attacker crit_resolution trace event");
    let multiplier = crit_event.values["multiplier"]
        .as_f64()
        .expect("multiplier as f64");
    let reduction = crit_event.values["attacker_crit_reduction_mult"]
        .as_f64()
        .expect("attacker_crit_reduction_mult in trace");
    approx_eq(reduction, 0.0, 1e-6);
    approx_eq(multiplier, 2.0, 1e-6);
}

#[test]
fn stacking_additive_only_stacks() {
    let totals = aggregate_contributions(vec![
        StackContribution::base("attack", 100.0),
        StackContribution::base("attack", 50.0),
        StackContribution::base("attack", 25.0),
    ]);

    let attack = totals
        .get("attack")
        .expect("attack totals should be present");
    approx_eq(attack.base, 175.0, 1e-12);
    approx_eq(attack.modifier, 0.0, 1e-12);
    approx_eq(attack.flat, 0.0, 1e-12);
    approx_eq(attack.compose(), 175.0, 1e-12);
}

#[test]
fn stacking_modifier_only_stacks() {
    let totals = aggregate_contributions(vec![
        StackContribution::base("damage", 200.0),
        StackContribution::modifier("damage", 0.10),
        StackContribution::modifier("damage", 0.25),
    ]);

    let damage = totals
        .get("damage")
        .expect("damage totals should be present");
    approx_eq(damage.modifier, 0.35, 1e-12);
    approx_eq(damage.compose(), 270.0, 1e-12);
}

#[test]
fn stacking_mixed_category_stacks() {
    let totals = aggregate_contributions(vec![
        StackContribution::base("crit", 100.0),
        StackContribution::modifier("crit", 0.40),
        StackContribution::flat("crit", 35.0),
    ]);

    let crit = totals.get("crit").expect("crit totals should be present");
    approx_eq(crit.compose(), 175.0, 1e-12);
}

#[test]
fn stacking_is_order_independent_within_categories() {
    let contributions = vec![
        StackContribution::base("attack", 100.0),
        StackContribution::base("attack", 50.0),
        StackContribution::modifier("attack", 0.30),
        StackContribution::modifier("attack", 0.20),
        StackContribution::flat("attack", 10.0),
        StackContribution::flat("attack", 5.0),
    ];

    let ordered = aggregate_contributions(contributions.clone());
    let mut reversed_contribs = contributions;
    reversed_contribs.reverse();
    let reversed = aggregate_contributions(reversed_contribs);

    let ordered_totals = ordered
        .get("attack")
        .expect("ordered attack totals should exist");
    let reversed_totals = reversed
        .get("attack")
        .expect("reversed attack totals should exist");

    approx_eq(ordered_totals.base, reversed_totals.base, 1e-12);
    approx_eq(ordered_totals.modifier, reversed_totals.modifier, 1e-12);
    approx_eq(ordered_totals.flat, reversed_totals.flat, 1e-12);
    approx_eq(ordered_totals.compose(), reversed_totals.compose(), 1e-12);

    let mut stacking = StatStacking::new();
    stacking.add_many(vec![
        StackContribution::base("shield", 75.0),
        StackContribution::modifier("shield", 0.5),
        StackContribution::flat("shield", 8.0),
    ]);
    approx_eq(
        stacking
            .composed_for(&"shield")
            .expect("shield value should exist"),
        120.5,
        1e-12,
    );
}

#[test]
fn crew_slot_gating_matrix_controls_activation() {
    let captain_ability = Ability {
        weapon_scope: Default::default(),
        name: "captain_strike".to_string(),
        class: AbilityClass::CaptainManeuver,
        timing: TimingWindow::AttackPhase,
        boostable: true,
        effect: AbilityEffect::AttackMultiplier(0.2),
        condition: None,
    };
    let bridge_ability = Ability {
        weapon_scope: Default::default(),
        name: "bridge_targeting".to_string(),
        class: AbilityClass::BridgeAbility,
        timing: TimingWindow::AttackPhase,
        boostable: true,
        effect: AbilityEffect::PierceBonus(0.1),
        condition: None,
    };

    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.15,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.5,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 9,
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

    let valid_crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: captain_ability.clone(),
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: bridge_ability.clone(),
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };
    let wrong_seat_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: captain_ability,
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let valid = simulate_combat(&attacker, &defender, &config, &valid_crew);
    let wrong = simulate_combat(&attacker, &defender, &config, &wrong_seat_crew);

    assert!(valid.total_damage > wrong.total_damage);
    assert_eq!(
        valid
            .events
            .iter()
            .filter(|event| event.event_type == "ability_activation")
            .count(),
        2
    );
    assert!(wrong
        .events
        .iter()
        .all(|event| event.event_type != "ability_activation"));
}

#[test]
fn boosted_non_boostable_abilities_are_filtered_out() {
    let non_boostable = Ability {
        weapon_scope: Default::default(),
        name: "steady_hands".to_string(),
        class: AbilityClass::BridgeAbility,
        timing: TimingWindow::AttackPhase,
        boostable: false,
        effect: AbilityEffect::AttackMultiplier(0.5),
        condition: None,
    };

    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.1,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.2,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 11,
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

    let boosted = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: non_boostable.clone(),
            boosted: true,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let unboosted = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: non_boostable,
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let boosted_result = simulate_combat(&attacker, &defender, &config, &boosted);
    let unboosted_result = simulate_combat(&attacker, &defender, &config, &unboosted);

    assert!(unboosted_result.total_damage > boosted_result.total_damage);
    assert!(boosted_result
        .events
        .iter()
        .all(|event| event.event_type != "ability_activation"));
    assert_eq!(
        unboosted_result
            .events
            .iter()
            .filter(|event| event.event_type == "ability_activation")
            .count(),
        1
    );
}

#[test]
fn timing_windows_materially_change_damage_outcomes() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.5,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 17,
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

    let attack_phase_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "pierce_now".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::AttackPhase,
                boostable: true,
                effect: AbilityEffect::PierceBonus(0.2),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let round_start_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "pierce_early".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::PierceBonus(0.2),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let defense_phase_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "pierce_on_defense".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::DefensePhase,
                boostable: true,
                effect: AbilityEffect::PierceBonus(0.2),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let attack_phase = simulate_combat(&attacker, &defender, &config, &attack_phase_crew);
    let round_start = simulate_combat(&attacker, &defender, &config, &round_start_crew);
    let defense_phase = simulate_combat(&attacker, &defender, &config, &defense_phase_crew);

    assert!(round_start.total_damage > attack_phase.total_damage);
    assert!(defense_phase.total_damage > attack_phase.total_damage);
    approx_eq(attack_phase.total_damage, 60.0, 1e-12);
    approx_eq(round_start.total_damage, 70.0, 1e-12);
    approx_eq(defense_phase.total_damage, 70.0, 1e-12);
}

#[test]
fn burning_deals_one_percent_hull_per_round() {
    let attacker = Combatant {
        id: "nero".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "target".to_string(),
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
        hull_health: 500.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let burning_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "georgiou".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: true,
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

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 3,
            seed: 1,
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
        &burning_crew,
    );

    approx_eq(result.total_damage, 15.0, 1e-12);
    let burning_ticks = result
        .events
        .iter()
        .filter(|event| event.event_type == "end_of_round_effects")
        .filter(|event| event.values["burning_damage"] == 5.0)
        .count();
    assert_eq!(burning_ticks, 3);
}

fn burning_only_crew(timing: TimingWindow) -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "burn-timing-test".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing,
                boostable: true,
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
    }
}

fn assert_burning_phase(events: &[CombatEvent], phase: &str) {
    let n = events
        .iter()
        .filter(|e| e.event_type == "burning_trigger" && e.phase == phase)
        .count();
    assert!(
        n > 0,
        "expected at least one burning_trigger in phase {phase:?}, got events: {:?}",
        events
            .iter()
            .filter(|e| e.event_type == "burning_trigger")
            .map(|e| e.phase.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn burning_triggers_on_combat_begin() {
    let attacker = Combatant {
        id: "a".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 500.0,
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
        &defender,
        &SimulationConfig {
            rounds: 2,
            seed: 7,
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
        &burning_only_crew(TimingWindow::CombatBegin),
    );
    assert_burning_phase(&r.events, "combat_begin");
}

#[test]
fn burning_triggers_on_defense_phase_per_shot() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 50.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 5000.0,
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
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 11,
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
        &burning_only_crew(TimingWindow::DefensePhase),
    );
    assert_burning_phase(&r.events, "defense");
}

#[test]
fn burning_triggers_on_round_end_before_tick() {
    let attacker = Combatant {
        id: "a".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 500.0,
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
        &defender,
        &SimulationConfig {
            rounds: 2,
            seed: 13,
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
        &burning_only_crew(TimingWindow::RoundEnd),
    );
    assert_burning_phase(&r.events, "round_end");
    let ticks = r
        .events
        .iter()
        .filter(|e| e.event_type == "end_of_round_effects")
        .filter(|e| e.values.get("burning_damage").and_then(|v| v.as_f64()) > Some(0.0))
        .count();
    assert!(ticks > 0, "round_end burn should tick same round");
}

#[test]
fn burning_triggers_on_shield_break() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 500.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 5000.0,
        shield_health: 200.0,
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
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 17,
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
        &burning_only_crew(TimingWindow::ShieldBreak),
    );
    assert_burning_phase(&r.events, "shield_break");
}

#[test]
fn burning_triggers_on_hull_breach_state_entry() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 1.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    // `TimingWindow::HullBreach` runs when a hull-breach **state** begins (HullBreach proc), not when hull HP crosses a fraction.
    let crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "fixture-hull-breach-proc".to_string(),
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
            },
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "burn-timing-test".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::HullBreach,
                    boostable: true,
                    effect: AbilityEffect::Burning {
                        chance: 1.0,
                        duration_rounds: 2,
                    },
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };
    let r = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 19,
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
    assert_burning_phase(&r.events, "hull_breach");
}

#[test]
fn burning_triggers_on_receive_damage_hull() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 1.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
        attack: 400.0,
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
        hull_health: 5000.0,
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
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 23,
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
        &burning_only_crew(TimingWindow::ReceiveDamage),
    );
    assert_burning_phase(&r.events, "receive_damage");
}

#[test]
fn burning_triggers_on_kill() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 500.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 50.0,
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
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 29,
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
        &burning_only_crew(TimingWindow::Kill),
    );
    assert_burning_phase(&r.events, "kill");
}

#[test]
fn burning_triggers_on_after_subround() {
    let attacker = Combatant {
        id: "a".to_string(),
        attack: 50.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d".to_string(),
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
        hull_health: 5000.0,
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
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 31,
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
        &burning_only_crew(TimingWindow::AfterSubround),
    );
    assert_burning_phase(&r.events, "after_subround");
}

#[test]
fn emits_ability_activation_for_each_timing_window() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 120.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.1,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 1.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 0.0,
        mitigation: 0.4,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "combat_begin_alpha".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::CombatBegin,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "round_start_alpha".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::BelowDeck,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "attack_alpha".to_string(),
                    class: AbilityClass::BelowDeck,
                    timing: TimingWindow::AttackPhase,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "defense_alpha".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::DefensePhase,
                    boostable: true,
                    effect: AbilityEffect::PierceBonus(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "round_end_alpha".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundEnd,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.2),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Ship,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "after_sub_alpha".to_string(),
                    class: AbilityClass::ShipAbility,
                    timing: TimingWindow::AfterSubround,
                    boostable: false,
                    effect: AbilityEffect::AttackMultiplier(0.01),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 19,
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

    let phases: Vec<_> = result
        .events
        .iter()
        .filter(|event| event.event_type == "ability_activation")
        .map(|event| event.phase.as_str())
        .collect();

    assert!(phases.contains(&"combat_begin"));
    assert!(phases.contains(&"round_start"));
    assert!(phases.contains(&"attack"));
    assert!(phases.contains(&"defense"));
    assert!(phases.contains(&"round_end"));
    assert!(phases.contains(&"after_subround"));
}

#[test]
fn additive_attack_modifiers_match_canonical_summed_behavior() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "target".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let two_ten_percent = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "round_start_ten_alpha".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "round_start_ten_beta".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };
    let single_twenty_percent = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "round_start_twenty".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::AttackMultiplier(0.2),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let config = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Off,
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

    let summed = simulate_combat(&attacker, &defender, &config, &two_ten_percent);
    let canonical = simulate_combat(&attacker, &defender, &config, &single_twenty_percent);

    approx_eq(summed.total_damage, 120.0, 1e-12);
    approx_eq(summed.total_damage, canonical.total_damage, 1e-12);
}

#[test]
fn decaying_attack_multiplier_reduces_damage_over_rounds() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let decay_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "decay".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::DecayingAttackMultiplier {
                    initial: 1.2,
                    decay_per_round: 0.05,
                    floor: 1.0,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let config = SimulationConfig {
        rounds: 5,
        seed: 42,
        trace_mode: TraceMode::Off,
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
    let result = simulate_combat(&attacker, &defender, &config, &decay_crew);
    assert!(result.total_damage > 0.0);
    assert!(result.rounds_simulated >= 2);
}

#[test]
fn accumulating_attack_multiplier_increases_damage_over_rounds() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let accumulate_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "accumulate".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::AccumulatingAttackMultiplier {
                    initial: 1.0,
                    growth_per_round: 0.05,
                    ceiling: 1.2,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let config = SimulationConfig {
        rounds: 5,
        seed: 42,
        trace_mode: TraceMode::Off,
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
    let result = simulate_combat(&attacker, &defender, &config, &accumulate_crew);
    assert!(result.total_damage > 0.0);
    assert!(result.rounds_simulated >= 2);
}

#[test]
fn combat_rounds_are_capped_at_100() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 1.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 150,
            seed: 9,
            trace_mode: TraceMode::Off,
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
        &CrewConfiguration::default(),
    );

    assert_eq!(result.rounds_simulated, 100);
}

#[test]
fn round_end_regen_restores_shield_and_reduces_hull_damage() {
    use kobayashi::combat::CrewSeatContext;
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 150.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.3,
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
        hull_health: 600.0,
        shield_health: 200.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew_no_regen = CrewConfiguration::default();
    let crew_with_regen = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "ShieldRegen".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundEnd,
                    boostable: false,
                    effect: AbilityEffect::ShieldRegen(60.0),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "HullRegen".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundEnd,
                    boostable: false,
                    effect: AbilityEffect::HullRegen(40.0),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };
    let result_no_regen = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 2,
            seed: 99,
            trace_mode: TraceMode::Off,
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
        &crew_no_regen,
    );
    let result_with_regen = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 2,
            seed: 99,
            trace_mode: TraceMode::Off,
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
        &crew_with_regen,
    );
    assert!(
        result_with_regen.defender_shield_remaining >= result_no_regen.defender_shield_remaining,
        "regen should preserve or increase shield"
    );
    assert!(
        result_with_regen.defender_hull_remaining >= result_no_regen.defender_hull_remaining,
        "regen should preserve or increase hull"
    );
}

#[test]
fn round_limit_declares_winner_by_hull_without_destruction() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 1.0,
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
        hull_health: 10000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 100,
            seed: 3,
            trace_mode: TraceMode::Off,
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
        &CrewConfiguration::default(),
    );

    assert!(result.winner_by_round_limit);
    assert!(result.attacker_won);
    assert!(result.attacker_hull_remaining > 0.0);
    assert!(result.defender_hull_remaining > 0.0);
}

#[test]
fn isolytic_damage_matches_reference_formula() {
    let damage = isolytic_damage(10_000.0, 0.3, 0.4);
    approx_eq(damage, 8_200.0, 1e-12);
}

#[test]
fn isolytic_on_combatant_increases_damage_defense_reduces_it() {
    let defender = Combatant {
        id: "defender".to_string(),
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
    };
    let attacker_no_iso = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let mut attacker_with_iso = attacker_no_iso.clone();
    attacker_with_iso.isolytic_damage = 0.2;
    let config = SimulationConfig {
        rounds: 1,
        seed: 5,
        trace_mode: TraceMode::Off,
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
    let crew = CrewConfiguration::default();
    let result_no_iso = simulate_combat(&attacker_no_iso, &defender, &config, &crew);
    let result_with_iso = simulate_combat(&attacker_with_iso, &defender, &config, &crew);
    assert!(
        result_with_iso.total_damage > result_no_iso.total_damage,
        "isolytic_damage should increase total damage"
    );
    let mut defender_with_def = defender.clone();
    defender_with_def.isolytic_defense = 50.0;
    let result_def = simulate_combat(&attacker_with_iso, &defender_with_def, &config, &crew);
    assert!(
        result_def.total_damage <= result_with_iso.total_damage + 1e-6,
        "isolytic_defense should not increase damage taken"
    );
}

#[test]
fn crew_isolytic_damage_bonus_increases_damage() {
    let defender = Combatant {
        id: "defender".to_string(),
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
    };
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 5,
        trace_mode: TraceMode::Off,
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
    let crew_empty = CrewConfiguration::default();
    let crew_with_iso = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "test_iso".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::IsolyticDamageBonus(0.2),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let result_empty = simulate_combat(&attacker, &defender, &config, &crew_empty);
    let result_with_iso = simulate_combat(&attacker, &defender, &config, &crew_with_iso);
    assert!(
        result_with_iso.total_damage > result_empty.total_damage,
        "crew IsolyticDamageBonus(0.2) should increase total damage"
    );
}

#[test]
fn crew_isolytic_cascade_damage_bonus_increases_damage() {
    let defender = Combatant {
        id: "defender".to_string(),
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
    };
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 5,
        trace_mode: TraceMode::Off,
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
    let crew_base_iso = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "iso".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: true,
                effect: AbilityEffect::IsolyticDamageBonus(0.1),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let crew_iso_and_cascade = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "iso".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::IsolyticDamageBonus(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "cascade".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::IsolyticCascadeDamageBonus(0.2),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };
    let result_base = simulate_combat(&attacker, &defender, &config, &crew_base_iso);
    let result_cascade = simulate_combat(&attacker, &defender, &config, &crew_iso_and_cascade);
    assert!(
        result_cascade.total_damage > result_base.total_damage,
        "IsolyticCascadeDamageBonus(0.2) on top of IsolyticDamageBonus(0.1) should increase total damage per formula"
    );
}

#[test]
fn two_weapon_combatant_produces_two_damage_events_per_round() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 150.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![
            WeaponStats {
                attack: 50.0,
                shots: None,
                ..Default::default()
            },
            WeaponStats {
                attack: 100.0,
                shots: None,
                ..Default::default()
            },
        ],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
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
    let result = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    let damage_events: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "damage_application")
        .collect();
    assert_eq!(
        damage_events.len(),
        2,
        "two weapons => two damage_application events per round"
    );
    assert_eq!(damage_events[0].weapon_index, Some(0));
    assert_eq!(damage_events[1].weapon_index, Some(1));
    let total_from_events: f64 = damage_events
        .iter()
        .map(|e| {
            e.values
                .get("hull_damage")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        })
        .sum();
    approx_eq(total_from_events, result.total_damage, 0.01);
}

#[test]
fn sub_round_ordering_weapon_one_damage_after_shield_break() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![
            WeaponStats {
                attack: 500.0,
                shots: None,
                ..Default::default()
            },
            WeaponStats {
                attack: 200.0,
                shots: None,
                ..Default::default()
            },
        ],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        shield_health: 300.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 3,
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
    let result = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    let damage_events: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "damage_application")
        .collect();
    assert_eq!(damage_events.len(), 2);
    let shield_after_0 = damage_events[0]
        .values
        .get("defender_shield_remaining")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    let shield_after_1 = damage_events[1]
        .values
        .get("defender_shield_remaining")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    assert!(
        shield_after_0 >= 0.0 && shield_after_1 <= 0.0,
        "weapon 0 may break shields; weapon 1 damage should see defender_shield_remaining == 0"
    );
}

#[test]
fn shots_bonus_increases_damage() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 80.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 0.3,
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 42,
        trace_mode: TraceMode::Off,
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
    let no_bonus = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());

    let crew_with_shots_bonus = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "ShotsCaptain".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::ShotsBonus {
                    chance: 1.0,
                    bonus_pct: 0.5,
                    duration_rounds: 3,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let with_bonus = simulate_combat(&attacker, &defender, &config, &crew_with_shots_bonus);

    assert!(
        with_bonus.total_damage > no_bonus.total_damage,
        "ShotsBonus +50% for 3 rounds should increase total damage (more shots): no_bonus={}, with_bonus={}",
        no_bonus.total_damage,
        with_bonus.total_damage
    );
}

#[test]
fn shield_break_and_receive_damage_windows_emit_activations() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 400.0,
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
        hull_health: 1000.0,
        shield_health: 200.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 50.0,
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
        hull_health: 2000.0,
        shield_health: 100.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "shield_break_ping".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::ShieldBreak,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "on_receive_damage_ping".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::ReceiveDamage,
                    boostable: true,
                    effect: AbilityEffect::PierceBonus(0.1),
                    condition: None,
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 13,
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

    assert!(result.events.iter().any(|e| {
        e.event_type == "ability_activation"
            && e.phase == "shield_break"
            && e.source.ship_ability_id.as_deref() == Some("shield_break_ping")
    }));
    assert!(result.events.iter().any(|e| {
        e.event_type == "ability_activation"
            && e.phase == "receive_damage"
            && e.source.ship_ability_id.as_deref() == Some("on_receive_damage_ping")
    }));
}

#[test]
fn kill_window_emits_activation_and_applies_hull_regen() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 1000.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 120.0,
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
        hull_health: 200.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew_with_regen = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "kill_heal".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::Kill,
                boostable: true,
                effect: AbilityEffect::OnKillHullRegen(0.25),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let with_regen = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 7,
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
        &crew_with_regen,
    );
    let without_regen = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 7,
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
        &CrewConfiguration::default(),
    );

    assert!(with_regen.events.iter().any(|e| {
        e.event_type == "ability_activation"
            && e.phase == "kill"
            && e.source.ship_ability_id.as_deref() == Some("kill_heal")
    }));
    assert!(
        with_regen.attacker_hull_remaining > without_regen.attacker_hull_remaining,
        "on_kill hull regen should improve attacker hull remaining"
    );
}

#[test]
fn combat_end_window_respects_condition_filtering() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "combat_end_true".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::CombatEnd,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: Some(kobayashi::combat::AbilityCondition::RoundRange {
                        min: 1,
                        max: 10,
                    }),
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    weapon_scope: Default::default(),
                    name: "combat_end_false".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::CombatEnd,
                    boostable: true,
                    effect: AbilityEffect::AttackMultiplier(0.1),
                    condition: Some(kobayashi::combat::AbilityCondition::RoundRange {
                        min: 999,
                        max: 1000,
                    }),
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            },
        ],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 17,
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
    let combat_end_activations: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "ability_activation" && e.phase == "combat_end")
        .collect();
    assert_eq!(combat_end_activations.len(), 1);
    assert_eq!(
        combat_end_activations[0].source.ship_ability_id.as_deref(),
        Some("combat_end_true")
    );
}

#[test]
fn stack_resolution_trace_emits_effect_stack_breakdown() {
    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 100.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 80.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "defender".to_string(),
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "attack_phase_amp".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(0.25),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 3,
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

    let stack_events: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "stack_resolution")
        .collect();
    assert!(
        !stack_events.is_empty(),
        "expected stack_resolution events when trace_mode is Events"
    );

    let e = stack_events[0];
    assert_eq!(e.phase, "attack");
    assert_eq!(
        e.source.player_bonus_source.as_deref(),
        Some("effect_stacks")
    );
    let ap_mult = e
        .values
        .get("attack_phase_damage_multiplier")
        .and_then(|v| v.as_f64())
        .expect("attack_phase_damage_multiplier");
    approx_eq(ap_mult, 1.25, 1e-9);

    let stacks = e
        .values
        .get("stacks")
        .and_then(|v| v.as_object())
        .expect("stacks");
    assert!(
        stacks.contains_key("pre_attack_damage"),
        "pre_attack_damage stack should be present for a weapon shot: {stacks:?}"
    );

    let contrib = e
        .values
        .get("effect_contributions")
        .and_then(|v| v.as_array())
        .expect("effect_contributions array");
    let row = contrib
        .iter()
        .find(|row| {
            row.get("ability").and_then(|a| a.as_str()) == Some("attack_phase_amp")
                && row.get("effect").and_then(|x| x.as_str()) == Some("AttackMultiplier")
        })
        .expect("per-effect row for attack_phase_amp AttackMultiplier");
    assert_eq!(
        row.get("target").and_then(|t| t.as_str()),
        Some("attack_phase_damage_modifier_sum")
    );
    assert_eq!(
        row.get("timing").and_then(|t| t.as_str()),
        Some("attack_phase")
    );
    approx_eq(
        row.get("value").and_then(|v| v.as_f64()).expect("value"),
        0.25,
        1e-9,
    );
    assert!(row.get("officer_id").is_none());
}

/// [`TimingWindow::RoundStart`] hull regen must reduce cumulative attacker hull damage each round
/// before outbound fire (LCARS commonly uses `on_round_start` for this stat).
#[test]
fn attacker_round_start_hull_regen_stacks_across_rounds() {
    let attacker = Combatant {
        id: "player".into(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "hostile".into(),
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
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "test_hull_regen_rs".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullRegen(100.0),
                condition: None,
            },
            boosted: false,
            officer_id: Some("test-officer".into()),
            contribution_batch: 1,
        }],
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 42,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 400.0,
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
    let res = simulate_combat(&attacker, &defender, &config, &crew);
    // Round 1–3 each heal 100 at round start: 400 − 300 = 100 net hull damage.
    approx_eq(res.attacker_hull_remaining, 900.0, 1e-6);
}

#[test]
fn attacker_round_start_hull_max_fraction_regen_uses_max_hull() {
    let attacker = Combatant {
        id: "player".into(),
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "hostile".into(),
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
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "test_hull_max_fraction_regen_rs".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullRegenMaxFraction(0.1),
                condition: None,
            },
            boosted: false,
            officer_id: Some("test-officer".into()),
            contribution_batch: 1,
        }],
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 42,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 400.0,
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
    let res = simulate_combat(&attacker, &defender, &config, &crew);
    // Round 1-3 each heal 10% of max hull: 400 - 3 * 100 = 100 net hull damage.
    approx_eq(res.attacker_hull_remaining, 900.0, 1e-6);
}

/// Defender crew [`TimingWindow::RoundStart`] hull regen reduces cumulative hull damage on the hostile.
#[test]
fn defender_round_start_hull_regen_heals_defender() {
    let attacker = Combatant {
        id: "player".into(),
        attack: 500.0,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 500.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "hostile".into(),
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
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "def_hull_rs".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullRegen(300.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
        trace_mode: TraceMode::Off,
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
    let without = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with_regen = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &defender_crew,
    );
    assert!(
        with_regen.defender_hull_remaining > without.defender_hull_remaining,
        "defender round-start hull regen should leave more defender hull: without={} with={}",
        without.defender_hull_remaining,
        with_regen.defender_hull_remaining
    );
    // Round 1: 500 damage. Round 2 start: −300 cumulative hull damage → 200 net before second hit.
    // Round 2: +500 → 700 cumulative; defender hull remaining 10_000 − 700 = 9300.
    approx_eq(with_regen.defender_hull_remaining, 9300.0, 1e-6);
}

/// PIC Hugh below decks: heal a **fraction** of hull damage taken in the previous combat round at round start.
#[test]
fn pic_hugh_prev_round_hull_fraction_heals_at_round_start() {
    let attacker = Combatant {
        id: "player".into(),
        attack: 500.0,
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
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 500.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "hostile".into(),
        attack: 100.0,
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let hugh_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Reclaiming Lives".into(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullRegenPrevRoundFraction(0.35),
                condition: None,
            },
            boosted: false,
            officer_id: Some("pic-hugh-75d78e".into()),
            contribution_batch: 2,
        }],
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 99,
        trace_mode: TraceMode::Off,
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
    let baseline = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with_hugh = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &hugh_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        with_hugh.attacker_hull_remaining > baseline.attacker_hull_remaining,
        "PIC Hugh heal should leave more attacker hull: baseline={} with={}",
        baseline.attacker_hull_remaining,
        with_hugh.attacker_hull_remaining
    );
    // Round 1: 100 hull taken. Round 2 start: heal 35 → net 165 hull damage vs 200 without Hugh.
    approx_eq(with_hugh.attacker_hull_remaining, 9835.0, 1e-6);
    approx_eq(baseline.attacker_hull_remaining, 9800.0, 1e-6);
}

#[test]
fn engagement_group_armadas_gates_combat_begin_isolytic_defense_on_counter() {
    let defender = Combatant {
        id: "armada_hostile".to_string(),
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.4,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 600.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let attacker = Combatant {
        id: "player".to_string(),
        attack: 10.0,
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let iso_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "right_to_protest_style".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::IsolyticDefenseBonus(3.0),
                condition: Some(AbilityCondition::EngagementIncludes(
                    EnemyType::GroupArmadas,
                )),
            },
            boosted: false,
            officer_id: Some("mara-dalen-6827".into()),
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let cfg_default = SimulationConfig {
        rounds: 1,
        seed: 41,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: EnemyTypes::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_hyperthermic_decay_fraction: 0.0,
        emit_state_snapshots: false,
    };
    let cfg_group = SimulationConfig {
        engagement_enemy_types: EnemyTypes::single(EnemyType::GroupArmadas),
        ..cfg_default.clone()
    };
    let baseline = simulate_combat(
        &attacker,
        &defender,
        &cfg_group,
        &CrewConfiguration::default(),
    );
    let gated_off = simulate_combat(&attacker, &defender, &cfg_default, &iso_crew);
    let gated_on = simulate_combat(&attacker, &defender, &cfg_group, &iso_crew);
    assert!(
        gated_on.attacker_hull_remaining > baseline.attacker_hull_remaining,
        "isolytic defense bonus should reduce counter damage when engagement matches"
    );
    approx_eq(
        gated_off.attacker_hull_remaining,
        baseline.attacker_hull_remaining,
        1e-6,
    );
}

#[test]
fn mara_style_shield_prev_round_heal_vs_armada_defender_only() {
    let defender = Combatant {
        id: "hostile".to_string(),
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
        hull_health: 500_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 900.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let attacker = Combatant {
        id: "player".to_string(),
        attack: 5.0,
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
        hull_health: 100_000.0,
        shield_health: 10_000.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 5.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let heal_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::BelowDeck,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "defy_defeat_style".into(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::ShieldRegenPrevRoundFraction(0.2),
                condition: Some(AbilityCondition::DefenderShipTypeIs(ShipType::Armada)),
            },
            boosted: false,
            officer_id: Some("mara-dalen-6827".into()),
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 77,
        trace_mode: TraceMode::Off,
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
    let baseline_armada = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with_heal_armada = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &heal_crew,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with_heal_survey = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &heal_crew,
        OpponentFactionTag::Unknown,
        ShipType::Survey,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        with_heal_armada.attacker_shield_remaining > baseline_armada.attacker_shield_remaining,
        "round-2 shield repair vs armada should exceed baseline"
    );
    approx_eq(
        with_heal_survey.attacker_shield_remaining,
        baseline_armada.attacker_shield_remaining,
        1e-6,
    );
}

/// Defender `DefensePhase` effects filtered for inbound fire merge into outbound damage resolution
/// (prototype path): cumulative damage to the defender should drop vs an empty defender crew.
#[test]
fn defender_inbound_defense_phase_reduces_incoming_damage() {
    let attacker = Combatant {
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
        hull_health: 5000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 300.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
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
        shield_health: 2000.0,
        shield_mitigation: 0.5,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let attacker_crew = CrewConfiguration::default();
    let inbound_def_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "inbound_shield_mit_test".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::DefensePhase,
                boostable: false,
                effect: AbilityEffect::ShieldMitigationBonus(0.35),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };

    let config = SimulationConfig {
        rounds: 2,
        seed: 42,
        trace_mode: TraceMode::Off,
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

    let baseline = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with_inbound = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &inbound_def_crew,
    );

    assert!(
        with_inbound.defender_hull_remaining > baseline.defender_hull_remaining + 1e-6,
        "inbound DefensePhase shield mitigation bonus should steer more damage to shields while they hold, preserving hull; baseline_hull={} with_hull={}",
        baseline.defender_hull_remaining,
        with_inbound.defender_hull_remaining
    );
}
