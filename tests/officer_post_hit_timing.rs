//! Client BuffSpecs and the fresh Realta report establish post-weapon activation.
//! These are controlled mechanic regressions, not a claim of full-log accuracy.
use kobayashi::combat::{
    build_combat_setup, can_activate_in_seat, simulate_combat_from_setup, Ability, AbilityClass,
    AbilityEffect, Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag,
    ShipType, SimulationConfig, SimulationResult, TimingWindow, TraceMode, WeaponStats, WeaponType,
    WeaponTypeScope,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

#[test]
fn finite_pre_attack_bonuses_are_not_reinterpreted_as_post_hit_grants() {
    use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec_full;
    use kobayashi::data::combat_effect_spec::CombatEffectSpec;
    for (modifier, expected) in [
        ("crit_chance", AbilityEffect::CritChanceBonus(0.04)),
        (
            "isolytic_cascade_damage",
            AbilityEffect::IsolyticCascadeDamageBonus(0.04),
        ),
    ] {
        let spec: CombatEffectSpec = serde_json::from_value(serde_json::json!({
            "id": "finite_pre_attack", "source": "manual", "trigger": "attack_phase",
            "target": "attacker_self", "modifier": modifier, "operation": "add",
            "value": {"scalar": 0.04}, "duration": {"type":"rounds", "rounds":4}
        }))
        .unwrap();
        let compiled = compile_officer_combat_spec_full(&spec).unwrap();
        assert_eq!(compiled.effect, expected);
        assert!(
            compiled.condition.is_some(),
            "ordinary finite buff retains round gate"
        );
    }
}

fn seat(effect: AbilityEffect) -> CrewSeatContext {
    CrewSeatContext::legacy(
        CrewSeat::Captain,
        Ability {
            name: "post_hit".into(),
            class: AbilityClass::BridgeAbility,
            timing: TimingWindow::AttackPhase,
            boostable: true,
            effect,
            condition: None,
            weapon_scope: Default::default(),
        },
        false,
    )
}

fn run(crew: &CrewConfiguration, rounds: u32, shots: u32) -> SimulationResult {
    run_with_weapons(
        crew,
        rounds,
        vec![
            WeaponStats {
                attack: 100.0,
                shots: Some(shots),
                ..Default::default()
            };
            2
        ],
        1e9,
    )
}

fn run_with_weapons(
    crew: &CrewConfiguration,
    rounds: u32,
    weapons: Vec<WeaponStats>,
    defender_hull: f64,
) -> SimulationResult {
    let attacker = Combatant {
        id: "attacker".into(),
        attack: 100.0,
        hull_health: 1e9,
        crit_chance: 0.1,
        crit_multiplier: 1.0,
        proc_multiplier: 1.0,
        weapons,
        ..Default::default()
    };
    let defender = Combatant {
        id: "defender".into(),
        hull_health: defender_hull,
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    simulate_combat_from_setup(&setup, 7)
}

#[test]
fn timed_grants_obey_weapon_scope_for_trigger_and_later_application() {
    let mut source = seat(AbilityEffect::OnHitTimedBonus {
        crit_chance: 0.04,
        cascade_damage: 0.1,
        duration_rounds: 4,
    });
    source.ability.weapon_scope = WeaponTypeScope::EnergyOnly;
    let result = run_with_weapons(
        &CrewConfiguration {
            seats: vec![source],
        },
        2,
        [WeaponType::Energy, WeaponType::Kinetic]
            .into_iter()
            .map(|weapon_type| WeaponStats {
                attack: 100.0,
                weapon_type,
                ..Default::default()
            })
            .collect(),
        1e9,
    );
    let chances: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "crit_resolution" && e.phase == "attack")
        .map(|e| e.values["effective_crit_chance"].as_f64().unwrap())
        .collect();
    assert_eq!(chances, [0.1, 0.1, 0.14, 0.1]);
    let activations = result
        .events
        .iter()
        .filter(|e| {
            e.event_type == "ability_activation"
                && e.source.ship_ability_id.as_deref() == Some("post_hit")
        })
        .count();
    assert_eq!(activations, 2);
}

#[test]
fn lethal_volley_ends_combat_before_post_hit_grant() {
    let result = run_with_weapons(
        &CrewConfiguration {
            seats: vec![seat(AbilityEffect::OnHitTimedBonus {
                crit_chance: 0.04,
                cascade_damage: 0.1,
                duration_rounds: 4,
            })],
        },
        1,
        vec![WeaponStats {
            attack: 100.0,
            ..Default::default()
        }],
        50.0,
    );
    assert!(result
        .events
        .iter()
        .any(|e| e.event_type == "damage_application"));
    assert!(!result
        .events
        .iter()
        .any(|e| e.event_type == "ability_activation"
            && e.source.ship_ability_id.as_deref() == Some("post_hit")));
}

#[test]
fn officer_ability_seat_matrix_includes_captain_without_enabling_other_seats() {
    for class in [
        AbilityClass::CaptainManeuver,
        AbilityClass::BridgeAbility,
        AbilityClass::BelowDeck,
        AbilityClass::ShipAbility,
    ] {
        for position in [
            CrewSeat::Captain,
            CrewSeat::Bridge,
            CrewSeat::BelowDeck,
            CrewSeat::Ship,
        ] {
            let mut row = seat(AbilityEffect::CritChanceBonus(0.1));
            row.ability.class = class;
            row.seat = position;
            let expected = matches!(
                (class, position),
                (AbilityClass::CaptainManeuver, CrewSeat::Captain)
                    | (
                        AbilityClass::BridgeAbility,
                        CrewSeat::Captain | CrewSeat::Bridge
                    )
                    | (AbilityClass::BelowDeck, CrewSeat::BelowDeck)
                    | (AbilityClass::ShipAbility, CrewSeat::Ship)
            );
            assert_eq!(
                can_activate_in_seat(&row),
                expected,
                "{class:?}/{position:?}"
            );
        }
    }
}

#[test]
fn timed_grants_start_after_volley_stack_additively_and_expire_independently() {
    let crew = CrewConfiguration {
        seats: vec![seat(AbilityEffect::OnHitTimedBonus {
            crit_chance: 0.04,
            cascade_damage: 0.1,
            duration_rounds: 2,
        })],
    };
    let result = run(&crew, 4, 2);
    let chances: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "crit_resolution" && e.phase == "attack")
        .map(|e| e.values["effective_crit_chance"].as_f64().unwrap())
        .collect();
    // Two shots in each weapon share its pre-volley state. Previous-round grants
    // survive; round-one grants expire before round three begins.
    let grants = [0, 0, 1, 1, 2, 2, 3, 3, 2, 2, 3, 3, 2, 2, 3, 3];
    assert_eq!(chances.len(), grants.len());
    for (actual, count) in chances.iter().zip(grants) {
        assert!((actual - (0.1 + f64::from(count) * 0.04)).abs() < 1e-8);
    }
    let isolation: Vec<_> = result
        .events
        .iter()
        .filter(|e| {
            e.event_type == "damage_application"
                && e.source.officer_id.as_deref() == Some("attacker")
        })
        .map(|e| e.values["isolytic_component"].as_f64().unwrap())
        .collect();
    for (actual, count) in isolation.iter().zip(grants) {
        assert!(
            (actual - f64::from(count) * 10.0).abs() < 1e-8,
            "cascade={actual}, grants={count}"
        );
    }
    let first_damage = result
        .events
        .iter()
        .position(|e| {
            e.event_type == "damage_application"
                && e.source.officer_id.as_deref() == Some("attacker")
        })
        .unwrap();
    let activations: Vec<_> = result
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.event_type == "ability_activation"
                && e.source.ship_ability_id.as_deref() == Some("post_hit")
        })
        .collect();
    assert_eq!(
        activations.len(),
        8,
        "once per volley, not once per projectile"
    );
    assert!(activations
        .iter()
        .all(|(index, e)| *index > first_damage && e.phase == "after_hit"));
}

#[test]
fn realta_captain_picard_and_riker_compile_client_durations_and_post_hit_bonuses() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let tiers = std::collections::HashMap::from([
        ("ent-e-picard-556227".to_string(), 4),
        ("ent-e-riker-516b8d".to_string(), 2),
    ]);
    let crew = resolve_crew_to_buff_set(
        "ent-e-picard-556227",
        &["ent-e-riker-516b8d".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: None,
            officer_tiers: Some(tiers),
            officer_levels: None,
        },
    )
    .crew;
    let effects: Vec<_> = crew
        .seats
        .iter()
        .filter(|s| can_activate_in_seat(s))
        .map(|s| s.ability.effect)
        .collect();
    assert!(effects.contains(&AbilityEffect::OnHitTimedBonus {
        crit_chance: 0.04,
        cascade_damage: 0.0,
        duration_rounds: 4
    }));
    assert!(effects.contains(&AbilityEffect::OnHitTimedBonus {
        crit_chance: 0.0,
        cascade_damage: 0.1,
        duration_rounds: 3
    }));
    let result = run(&crew, 1, 1);
    let chances: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "crit_resolution" && e.phase == "attack")
        .map(|e| e.values["effective_crit_chance"].as_f64().unwrap())
        .collect();
    assert_eq!(chances, [0.1, 0.14]);
    let iso: Vec<_> = result
        .events
        .iter()
        .filter(|e| {
            e.event_type == "damage_application"
                && e.source.officer_id.as_deref() == Some("attacker")
        })
        .map(|e| e.values["isolytic_component"].as_f64().unwrap())
        .collect();
    assert_eq!(iso, [0.0, 10.0]);
}

#[test]
fn captain_maneuver_and_officer_ability_survive_duplicate_filter_as_one_slot() {
    let mut officers =
        index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    // Give Picard a combat captain row too: his actual loot maneuver creates no
    // combat seat and would otherwise hide this independent duplicate-batch bug.
    let picard = officers.get_mut("ent-e-picard-556227").unwrap();
    picard.captain_ability = picard.bridge_ability.clone();
    picard.captain_ability.as_mut().unwrap().name = "controlled_captain".into();
    let crew = resolve_crew_to_buff_set(
        "ent-e-picard-556227",
        &[],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(4),
            officer_tiers: None,
            officer_levels: None,
        },
    )
    .crew;
    assert_eq!(crew.seats.len(), 2);
    assert_eq!(
        crew.seats[0].contribution_batch,
        crew.seats[1].contribution_batch
    );
    let filtered = kobayashi::combat::apply_duplicate_officer_policy(&crew);
    assert_eq!(filtered.seats.len(), 2);
    let result = run(&crew, 1, 1);
    let iso: Vec<_> = result
        .events
        .iter()
        .filter(|e| {
            e.event_type == "damage_application"
                && e.source.officer_id.as_deref() == Some("attacker")
        })
        .map(|e| e.values["isolytic_component"].as_f64().unwrap())
        .collect();
    assert_eq!(iso, [0.0, 20.0]);
}
