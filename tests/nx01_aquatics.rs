use kobayashi::combat::hostile_tags::HOSTILE_TAG_MASK_XINDI_AQUATIC;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration, ShipType,
    SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

#[test]
fn reviewed_aquatic_weapon_schedules_survive_resolution() {
    let ship = resolve_ship_with_tier_level("enterprise_nx_01", Some(12), Some(55)).unwrap();
    let nx = Combatant {
        weapons: ship.to_weapons(),
        ..Default::default()
    };
    let shots = |ship: &Combatant, round| {
        (0..ship.weapon_count())
            .filter(|&index| ship.weapon_fires_in_round(index, round))
            .map(|index| ship.weapon_base_shots(index))
            .sum::<u32>()
    };
    assert_eq!([1, 2, 3, 4].map(|round| shots(&nx, round)), [2, 10, 2, 10]);

    for (id, long_cooldown_index) in [(YOTON_CRUISER, 4), (YOTON_ENHANCED_CRUISER, 3)] {
        let hostile = resolve_hostile(id).unwrap();
        let opponent = Combatant {
            weapons: hostile.weapons_for_counter_attack(ShipType::Explorer, Default::default()),
            ..Default::default()
        };
        assert!(opponent.weapon_fires_in_round(long_cooldown_index, 1));
        assert!(!opponent.weapon_fires_in_round(long_cooldown_index, 2));
        assert!(!opponent.weapon_fires_in_round(long_cooldown_index, 100));
        assert!(opponent.weapon_fires_in_round(long_cooldown_index, 101));
        assert_eq!([1, 2].map(|round| shots(&opponent, round)), [59, 49]);
    }
}

#[test]
fn archer_and_trip_stack_after_each_damaging_volley_and_expire() {
    use kobayashi::combat::{AbilityEffect, WeaponFiringSchedule};
    use kobayashi::lcars::build_officer_model_default;
    use kobayashi::lcars::resolver::{resolve_crew_to_buff_set, ResolveOptions};

    let officers = build_officer_model_default()
        .unwrap()
        .into_iter()
        .map(|officer| (officer.id.clone(), officer))
        .collect();
    let mut crew = resolve_crew_to_buff_set(
        "jonathan-archer-4e9cd0",
        &["trip-tucker-75d4f9".to_owned()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(1),
            ..Default::default()
        },
    )
    .crew;
    assert_eq!(
        crew.seats
            .iter()
            .filter(|seat| matches!(
                seat.ability.effect,
                AbilityEffect::DamageTakenCritStack { .. }
            ))
            .count(),
        2,
    );
    let ship = resolve_ship_with_tier_level("enterprise_nx_01", Some(12), Some(55)).unwrap();
    let polarized = ship
        .abilities
        .unwrap()
        .into_iter()
        .find(|ability| ability.id == "polarized_hull")
        .unwrap();
    crew.seats
        .push(ship_ability_to_crew_seat_context(&polarized).unwrap());

    let attacker = Combatant {
        hull_health: 1e9,
        crit_chance: 1.0,
        crit_multiplier: 2.0,
        weapons: vec![
            WeaponStats {
                attack: 10.0,
                ..Default::default()
            };
            3
        ],
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e9,
        crit_chance: 1.0,
        crit_multiplier: 4.0,
        weapons: [12, 9, 10]
            .map(|shots| WeaponStats {
                attack: 10.0,
                shots: Some(shots),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 100,
                }),
                ..Default::default()
            })
            .to_vec(),
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 3,
        trace_mode: TraceMode::Events,
        defender_hostile_tag_mask: HOSTILE_TAG_MASK_XINDI_AQUATIC,
        ..Default::default()
    };
    let setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &crew,
        resolve_hostile(YOTON_CRUISER)
            .unwrap()
            .opponent_faction_tag(),
        ShipType::Interceptor,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let trace = simulate_combat_from_setup(&setup, 17);
    for name in ["Faith of the Heart", "Things Blow Up"] {
        let events: Vec<_> = trace
            .events
            .iter()
            .filter(|event| event.source.ship_ability_id.as_deref() == Some(name))
            .collect();
        assert_eq!(events.len(), 2, "{name}");
        assert!(events
            .iter()
            .all(|event| event.phase == "after_damage_taken" && event.round_index == 1));
    }
    let crits = |phase, round| {
        trace
            .events
            .iter()
            .filter(|event| {
                event.event_type == "crit_resolution"
                    && event.phase == phase
                    && event.round_index == round
            })
            .map(|event| event.values["multiplier"].as_f64().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(crits("counter", 1), [4.0, 4.0, 4.0, 3.5]);
    for (round, expected) in [(1, [2.0, 2.05, 2.05]), (2, [2.1; 3]), (3, [2.0; 3])] {
        let actual = crits("attack", round);
        assert_eq!(actual.len(), expected.len());
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-9),
            "round {round}: {actual:?}"
        );
    }

    let assault_config = SimulationConfig {
        engagement_enemy_types: kobayashi::combat::EnemyTypes::new(vec![
            kobayashi::combat::EnemyType::Assaults,
        ]),
        ..config.clone()
    };
    let assault_setup = build_combat_setup(
        &attacker,
        &defender,
        &assault_config,
        &crew,
        resolve_hostile(YOTON_CRUISER)
            .unwrap()
            .opponent_faction_tag(),
        ShipType::Interceptor,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(simulate_combat_from_setup(&assault_setup, 17)
        .events
        .iter()
        .all(|event| event.phase != "after_damage_taken"));

    let mut untraced = config;
    untraced.trace_mode = TraceMode::Off;
    let untraced_setup = build_combat_setup(
        &attacker,
        &defender,
        &untraced,
        &crew,
        resolve_hostile(YOTON_CRUISER)
            .unwrap()
            .opponent_faction_tag(),
        ShipType::Interceptor,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let mut traced_without_events = trace;
    traced_without_events.events.clear();
    assert_eq!(
        traced_without_events,
        simulate_combat_from_setup(&untraced_setup, 17)
    );
}

#[test]
fn temporal_conduit_resolves_equipped_rank_and_tier_unlock() {
    use kobayashi::combat::{AbilityEffect, TimingWindow};
    use kobayashi::data::forbidden_chaos::{load_forbidden_chaos, DEFAULT_FORBIDDEN_CHAOS_PATH};
    use kobayashi::data::import::ForbiddenTechEntry;
    use kobayashi::data::profile::{
        merge_tech_fids_into_profile_with_level_tier, resolve_effective_tech_fids,
        temporal_conduit_chaos_tech_seats, PlayerProfile,
    };

    let catalog = load_forbidden_chaos(DEFAULT_FORBIDDEN_CHAOS_PATH).unwrap();
    let fid = 258_464_522;
    let mut imported = [ForbiddenTechEntry {
        fid,
        tier: 2,
        level: 10,
        shard_count: 0,
    }];
    let mut profile = PlayerProfile::default();
    let effective = resolve_effective_tech_fids(&profile, &imported, &catalog);
    assert!(temporal_conduit_chaos_tech_seats(&imported, &effective, &catalog).is_empty());
    profile.equipped_chaos_fid = Some(fid);
    let effective = resolve_effective_tech_fids(&profile, &imported, &catalog);
    merge_tech_fids_into_profile_with_level_tier(
        &mut profile,
        &effective,
        &imported,
        &catalog,
        false,
    );
    assert!(!profile.bonuses.contains_key("apex_barrier"));

    let seats = temporal_conduit_chaos_tech_seats(&imported, &effective, &catalog);
    assert_eq!(seats.len(), 2);
    assert_eq!(seats[0].ability.timing, TimingWindow::CombatBegin);
    assert_eq!(
        seats[0].ability.effect,
        AbilityEffect::ApexBarrierBonus(4500.0)
    );
    assert_eq!(seats[1].ability.timing, TimingWindow::RoundStart);
    assert_eq!(
        seats[1].ability.effect,
        AbilityEffect::HullBreach {
            chance: 0.15,
            duration_rounds: 2,
            requires_critical: false,
        }
    );
    imported[0].tier = 1;
    imported[0].level = 1;
    let seats = temporal_conduit_chaos_tech_seats(&imported, &effective, &catalog);
    assert_eq!(seats.len(), 1);
    assert!(matches!(
        seats[0].ability.effect,
        AbilityEffect::HullBreach { chance: 0.1, .. }
    ));
    imported[0].level = 61;
    assert!(temporal_conduit_chaos_tech_seats(&imported, &effective, &catalog).is_empty());
}

const YOTON_CRUISER: &str = "3919830203";
const YOTON_ENHANCED_CRUISER: &str = "4198740904";
const REPTILIAN_WARSHIP: &str = "331553574";
const OTHER_HOSTILE: &str = "2918121098";

#[test]
fn nx01_uses_native_level_curve_and_fixed_polarized_hull() {
    for (level, expected_damage) in [(1, 160_000.0), (55, 3_400_000.0), (60, 3_700_000.0)] {
        let ship = resolve_ship_with_tier_level("enterprise_nx_01", Some(12), Some(level)).unwrap();
        let abilities = ship.abilities.unwrap();
        assert_eq!(abilities.len(), 2);
        assert_eq!(abilities[0].effect_type, "component_base_weapon_damage");
        assert_eq!(abilities[0].value, expected_damage);
        assert_eq!(abilities[1].id, "polarized_hull");
        assert_eq!(abilities[1].value, 9.0);
        assert!(abilities.iter().all(|ability| {
            ability.condition_defender_is_npc_hostile
                && ability
                    .condition_opponent_hostile_tags
                    .as_deref()
                    .is_some_and(|tags| tags.len() == 1 && tags[0] == "xindi_aquatic")
        }));
    }
}

#[test]
fn nx01_adds_base_damage_once_and_deflects_each_aquatic_volley() {
    let ship = resolve_ship_with_tier_level("enterprise_nx_01", Some(12), Some(55)).unwrap();
    let crew = CrewConfiguration {
        seats: ship
            .abilities
            .as_ref()
            .unwrap()
            .iter()
            .map(|ability| ship_ability_to_crew_seat_context(ability).unwrap())
            .collect(),
    };
    let attacker = Combatant {
        id: ship.id.clone(),
        hull_health: 1e12,
        weapons: vec![WeaponStats {
            attack: 300.0,
            component_base_attack: Some(100.0),
            firing_schedule: None,
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e14,
        weapons: [10, 9]
            .into_iter()
            .map(|shots| WeaponStats {
                attack: 10.0,
                shots: Some(shots),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let no_abilities = CrewConfiguration::default();

    for (id, npc, has_abilities, active) in [
        (YOTON_CRUISER, true, true, true),
        (YOTON_ENHANCED_CRUISER, true, true, true),
        (REPTILIAN_WARSHIP, true, true, false),
        (OTHER_HOSTILE, true, true, false),
        (YOTON_CRUISER, false, true, false),
        (YOTON_CRUISER, true, false, false),
    ] {
        let hostile = resolve_hostile(id).unwrap();
        let mask = hostile.hostile_tag_mask();
        assert_eq!(
            mask & HOSTILE_TAG_MASK_XINDI_AQUATIC != 0,
            matches!(id, YOTON_CRUISER | YOTON_ENHANCED_CRUISER)
        );
        let run = |trace_mode| {
            let config = SimulationConfig {
                trace_mode,
                defender_hostile_tag_mask: mask,
                rounds: 2,
                ..Default::default()
            };
            let setup = build_combat_setup(
                &attacker,
                &defender,
                &config,
                if has_abilities { &crew } else { &no_abilities },
                hostile.opponent_faction_tag(),
                hostile.ship_type_for_combat(),
                ShipType::Explorer,
                npc,
                !npc,
                &no_abilities,
            );
            simulate_combat_from_setup(&setup, 17)
        };
        let mut traced = run(TraceMode::Events);
        let attack = traced
            .events
            .iter()
            .find(|event| event.event_type == "attack_roll")
            .unwrap();
        let expected = if active { 340_000_300.0 } else { 300.0 };
        assert_eq!(attack.values["effective_attack"], expected, "{id}");
        for (index, expected_shots) in [
            (0, if active { 1 } else { 10 }),
            (1, if active { 0 } else { 9 }),
        ] {
            for round in [1, 2] {
                let count = traced
                    .events
                    .iter()
                    .filter(|event| {
                        event.event_type == "crit_resolution"
                            && event.phase == "counter"
                            && event.weapon_index == Some(index)
                            && event.round_index == round
                    })
                    .count();
                assert_eq!(count, expected_shots, "{id} weapon {index} round {round}");
            }
        }
        assert_eq!(traced, run(TraceMode::Events), "{id}");
        traced.events.clear();
        assert_eq!(traced, run(TraceMode::Off), "{id}");
    }
}
