//! The retained Enterprise-D fight constrains its real per-round firing pattern.
use kobayashi::combat::{
    simulate_combat, simulate_combat_with_defender_faction_and_defender_crew, AbilityEffect,
    Combatant, CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig, TraceMode,
    WeaponFiringSchedule, WeaponStats, WeaponType,
};
use kobayashi::data::profile_index::profile_data_dir;
use kobayashi::data::{
    data_registry::DataRegistry, loader::resolve_ship_with_tier_level,
    ship::apply_component_overrides_to_ship_record,
    ship_ability_resolve::ship_abilities_to_crew_seat_contexts,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};
use kobayashi::optimizer::crew_generator::{resolve_below_decks_slots_for_ship, CrewCandidate};
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry, unresolved_officers_for_candidate, DefenderOpponent,
};
use std::collections::{BTreeMap, HashMap};

const FIGHT_EXPORT: &str = include_str!("../fight samples/uss enterprise d vs vger hurak 59.csv");

fn recorded_player_shots() -> BTreeMap<u32, usize> {
    let mut lines = FIGHT_EXPORT.lines();
    let header = lines
        .find(|line| line.split('\t').next() == Some("Round"))
        .expect("fight event header");
    let columns: Vec<_> = header.split('\t').collect();
    let col = |name| columns.iter().position(|value| *value == name).unwrap();
    let round_col = col("Round");
    let type_col = col("Type");
    let ship_col = col("Attacker Ship");
    let mut shots = BTreeMap::new();
    for line in lines {
        let row: Vec<_> = line.split('\t').collect();
        let Some(round) = row
            .get(round_col)
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        if row.get(type_col) == Some(&"Attack")
            && row
                .get(ship_col)
                .is_some_and(|ship| ship.contains("ENTERPRISE-D"))
        {
            *shots.entry(round).or_insert(0) += 1;
        }
    }
    shots
}

#[test]
fn enterprise_d_weapon_firing_counts_match_the_retained_fight() {
    let ship = resolve_ship_with_tier_level("uss_enterprise_d", Some(5), Some(18)).unwrap();
    let attacker = Combatant {
        id: "enterprise-d".into(),
        attack: ship.attack,
        hull_health: 1e30,
        weapons: ship.to_weapons(),
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e30,
        ..Default::default()
    };
    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 23,
            seed: 7,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &CrewConfiguration::default(),
    );
    let mut simulated = BTreeMap::new();
    for event in &result.events {
        if event.event_type == "attack_roll" && event.phase == "attack" {
            *simulated.entry(event.round_index).or_insert(0) += 1;
        }
    }
    let observed = recorded_player_shots();
    assert_eq!(observed.len(), 23);
    assert_eq!(observed.get(&1), Some(&6));
    assert_eq!(observed.get(&2), Some(&8));
    assert_eq!(simulated, observed, "player weapon shots by round");
}

#[test]
fn enterprise_d_fires_its_active_phaser_before_the_torpedo() {
    let mut lines = FIGHT_EXPORT.lines();
    let header = lines
        .find(|line| line.split('\t').next() == Some("Round"))
        .unwrap();
    let columns: Vec<_> = header.split('\t').collect();
    let col = |name| columns.iter().position(|value| *value == name).unwrap();
    let (round, kind, ship, critical, damage) = (
        col("Round"),
        col("Type"),
        col("Attacker Ship"),
        col("Critical Hit?"),
        col("Total Damage"),
    );
    let shots: Vec<_> = lines
        .filter_map(|line| {
            let row: Vec<_> = line.split('\t').collect();
            if row.get(round) != Some(&"2")
                || row.get(kind) != Some(&"Attack")
                || !row
                    .get(ship)
                    .is_some_and(|value| value.contains("ENTERPRISE-D"))
            {
                return None;
            }
            Some((
                row.get(critical)?.to_owned(),
                row.get(damage)?.parse::<f64>().ok()?,
            ))
        })
        .collect();
    assert_eq!(shots.len(), 8);
    let energy_noncritical: Vec<_> = shots[..6]
        .iter()
        .filter(|(crit, _)| *crit == "NO")
        .map(|(_, damage)| *damage)
        .collect();
    assert!(!energy_noncritical.is_empty());
    assert!(shots[6..]
        .iter()
        .all(|(crit, damage)| *crit == "NO"
            && energy_noncritical.iter().all(|energy| damage > energy)));

    let ship = resolve_ship_with_tier_level("uss_enterprise_d", Some(5), Some(18)).unwrap();
    let weapons = ship.to_weapons();
    let attacker = Combatant {
        hull_health: 1e30,
        weapons: weapons.clone(),
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e30,
        ..Default::default()
    };
    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 2,
            seed: 7,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &CrewConfiguration::default(),
    );
    let fired_types: Vec<_> = result
        .events
        .iter()
        .filter(|event| {
            event.round_index == 2 && event.event_type == "attack_roll" && event.phase == "attack"
        })
        .map(|event| weapons[event.weapon_index.unwrap() as usize].weapon_type)
        .collect();
    assert_eq!(
        fired_types,
        [vec![WeaponType::Energy; 6], vec![WeaponType::Kinetic; 2]].concat()
    );
}

#[test]
fn synced_component_selection_preserves_reviewed_weapon_schedules() {
    let raw: serde_json::Value = serde_json::from_slice(
        &std::fs::read("data/upstream/data-stfc-space/ships/448699234.json").unwrap(),
    )
    .unwrap();
    let mut ship = resolve_ship_with_tier_level("uss_enterprise_d", Some(5), Some(18)).unwrap();
    let expected: Vec<_> = ship
        .to_weapons()
        .iter()
        .map(|weapon| weapon.firing_schedule)
        .collect();
    assert_eq!(
        expected,
        vec![
            Some(WeaponFiringSchedule {
                first_round: 1,
                interval_rounds: 2
            }),
            Some(WeaponFiringSchedule {
                first_round: 2,
                interval_rounds: 2
            }),
            Some(WeaponFiringSchedule {
                first_round: 2,
                interval_rounds: 3
            }),
        ]
    );
    let tier_six = raw["tiers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tier| tier["tier"] == 6)
        .unwrap();
    let component_ids: Vec<_> = tier_six["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|component| component["data"]["tag"] == "Weapon")
        .map(|component| component["id"].as_i64().unwrap())
        .collect();
    assert_eq!(component_ids.len(), 3);
    apply_component_overrides_to_ship_record(&mut ship, &raw, 5, &component_ids)
        .expect("component overrides resolve");
    assert_eq!(
        ship.to_weapons()
            .iter()
            .map(|weapon| weapon.firing_schedule)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn every_enterprise_d_tier_matches_its_native_weapon_components() {
    let raw: serde_json::Value = serde_json::from_slice(
        &std::fs::read("data/upstream/data-stfc-space/ships/448699234.json").unwrap(),
    )
    .unwrap();
    let tiers = raw["tiers"].as_array().unwrap();
    assert_eq!(tiers.len(), 12);
    for tier in tiers {
        let tier_number = tier["tier"].as_u64().unwrap() as u32;
        let ship = resolve_ship_with_tier_level("uss_enterprise_d", Some(tier_number), Some(1))
            .expect("Enterprise-D tier");
        let expected: Vec<_> = tier["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|component| component["data"]["tag"] == "Weapon")
            .map(|component| {
                let data = &component["data"];
                (
                    Some(data["shots"].as_u64().unwrap() as u32),
                    WeaponFiringSchedule {
                        first_round: data["warm_up"].as_u64().unwrap() as u32,
                        interval_rounds: data["cool_down"].as_u64().unwrap() as u32,
                    },
                    match data["weapon_type"].as_u64().unwrap() {
                        1 => WeaponType::Energy,
                        2 => WeaponType::Kinetic,
                        value => panic!("unexpected weapon type {value}"),
                    },
                )
            })
            .collect();
        let actual: Vec<_> = ship
            .to_weapons()
            .iter()
            .map(|weapon| {
                (
                    weapon.shots,
                    weapon.firing_schedule.expect("reviewed schedule"),
                    weapon.weapon_type,
                )
            })
            .collect();
        assert_eq!(actual, expected, "tier {tier_number}");
    }
}

#[test]
fn scheduled_attacker_and_defender_fire_independently() {
    let attacker = Combatant {
        hull_health: 1e9,
        weapons: vec![WeaponStats {
            attack: 10.0,
            firing_schedule: Some(WeaponFiringSchedule {
                first_round: 2,
                interval_rounds: 2,
            }),
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e9,
        weapons: vec![WeaponStats {
            attack: 10.0,
            firing_schedule: Some(WeaponFiringSchedule {
                first_round: 1,
                interval_rounds: 2,
            }),
            ..Default::default()
        }],
        ..Default::default()
    };
    let run = |trace_mode| {
        simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &SimulationConfig {
                rounds: 4,
                seed: 7,
                trace_mode,
                ..Default::default()
            },
            &CrewConfiguration::default(),
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Explorer,
            true,
            false,
            &CrewConfiguration::default(),
        )
    };
    let mut result = run(TraceMode::Events);
    let rounds = |kind: &str, phase: &str| {
        result
            .events
            .iter()
            .filter(|event| event.event_type == kind && event.phase == phase)
            .map(|event| event.round_index)
            .collect::<Vec<_>>()
    };
    assert_eq!(rounds("attack_roll", "attack"), [2, 4]);
    assert_eq!(rounds("crit_resolution", "counter"), [1, 3]);
    result.events.clear();
    assert_eq!(result, run(TraceMode::Off));
}

#[test]
fn legacy_weapons_fire_each_round_and_invalid_schedules_fail_closed() {
    let legacy = Combatant {
        weapons: vec![WeaponStats {
            attack: 10.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    let invalid = Combatant {
        weapons: vec![WeaponStats {
            attack: 10.0,
            firing_schedule: Some(WeaponFiringSchedule {
                first_round: 1,
                interval_rounds: 0,
            }),
            ..Default::default()
        }],
        ..Default::default()
    };
    for round in 1..=4 {
        assert!(legacy.weapon_fires_in_round(0, round));
        assert!(!invalid.weapon_fires_in_round(0, round));
        assert!(!legacy.weapon_fires_in_round(1, round));
    }
}

#[test]
fn enterprise_d_cascade_crew_grants_picard_bonus_only_after_fired_volleys() {
    let ship = resolve_ship_with_tier_level("uss_enterprise_d", Some(5), Some(18)).unwrap();
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let names = [
        "ent-e-picard-556227",
        "ent-e-data-871245",
        "five-of-eleven-d9aa11",
        "harry-kim-a79fdf",
        "b-elanna-torres-75cf02",
        "neelix-c8a380",
        "snw-nurse-chapel-d80ed9",
    ];
    let tiers: HashMap<_, _> = names.iter().map(|name| ((*name).to_owned(), 5)).collect();
    let mut crew = resolve_crew_to_buff_set(
        names[0],
        &[names[1].into(), names[2].into()],
        &names[3..]
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
        &officers,
        &ResolveOptions {
            tier: None,
            officer_tiers: Some(tiers),
            officer_levels: None,
        },
    )
    .crew;
    assert!(crew.seats.iter().any(|seat| matches!(seat.ability.effect,
        AbilityEffect::OnHitTimedBonus { cascade_damage, .. } if cascade_damage > 0.0)));
    crew.seats.extend(ship_abilities_to_crew_seat_contexts(
        ship.abilities.as_deref().unwrap(),
    ));
    let attacker = Combatant {
        id: "enterprise-d".into(),
        attack: ship.attack,
        hull_health: 1e30,
        weapons: ship.to_weapons(),
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e30,
        ..Default::default()
    };
    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 8,
            seed: 7,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &crew,
    );
    for round in 1..=8 {
        let activations = result
            .events
            .iter()
            .filter(|event| {
                event.round_index == round
                    && event.event_type == "ability_activation"
                    && event.phase == "after_hit"
            })
            .count();
        let expected = if round >= 2 && (round - 2) % 3 == 0 {
            2
        } else {
            1
        };
        assert_eq!(activations, expected, "Picard grants in round {round}");
        assert!(result.events.iter().any(|event| event.round_index == round
            && event.event_type == "morale_activation"
            && event.values["triggered"] == true));
    }
    let cascade = |weapon_index| {
        result
            .events
            .iter()
            .find(|event| {
                event.round_index == 2
                    && event.event_type == "stack_resolution"
                    && event.phase == "attack"
                    && event.weapon_index == Some(weapon_index)
            })
            .unwrap()
            .values["stacks"]["isolytic_cascade_damage_bonus"]["composed"]
            .as_f64()
            .unwrap()
    };
    assert!(
        (cascade(2) - cascade(1) - 0.15).abs() < 1e-9,
        "Picard's rank-5 grant from the phaser must apply to the following torpedo"
    );
}

#[test]
fn replay_with_enterprise_d_crew_uses_the_reviewed_weapon_pattern() {
    let registry = DataRegistry::load().expect("public demo data");
    assert!(resolve_below_decks_slots_for_ship("uss_enterprise_d", Some(6), Some(30), None) >= 4);
    // Synthetic officer ranks exercise the requested seats without private player data.
    let candidate = CrewCandidate {
        captain: "ent-e-picard-556227 (T5)".into(),
        bridge: vec![
            "ent-e-data-871245 (T5)".into(),
            "five-of-eleven-d9aa11 (T5)".into(),
        ],
        below_decks: vec![
            "harry-kim-a79fdf (T5)".into(),
            "b-elanna-torres-75cf02 (T5)".into(),
            "neelix-c8a380 (T5)".into(),
            "snw-nurse-chapel-d80ed9 (T5)".into(),
        ],
    };
    assert!(
        registry.lcars_officers().is_some(),
        "officer data is loaded"
    );
    assert_eq!(
        unresolved_officers_for_candidate(&registry, &candidate),
        Vec::<String>::new()
    );
    // The generic damage sponge is classified as an armada target and would
    // correctly suppress this crew's non-armada-only cascade abilities.
    let hostile = "1099730812";
    assert_ne!(
        registry
            .resolve_hostile(hostile)
            .unwrap()
            .ship_type_for_combat(),
        ShipType::Armada
    );
    let synthetic_profile = format!("enterprise-d-schedule-test-{}", uuid::Uuid::new_v4());
    assert!(!profile_data_dir(&synthetic_profile).exists());
    let replay = replay_optimize_iteration_with_registry(
        &registry,
        "uss_enterprise_d",
        hostile,
        Some(6),
        Some(30),
        &candidate,
        7,
        0,
        Some(&synthetic_profile),
        2_000_000,
        None,
        DefenderOpponent::Hostile,
    );
    assert!(!replay.using_placeholder_combatants);
    assert!(!replay.trace_truncated);
    assert!(
        replay.rounds_simulated >= 8,
        "rounds={}, won={}, attacker_hull={}, defender_hull={}",
        replay.rounds_simulated,
        replay.attacker_won,
        replay.attacker_hull_remaining,
        replay.defender_hull_remaining
    );
    let actual: Vec<_> = (1..=8)
        .map(|round| {
            replay
                .trace_events
                .iter()
                .filter(|event| {
                    event.round_index == round
                        && event.event_type == "attack_roll"
                        && event.phase == "attack"
                })
                .count()
        })
        .collect();
    assert_eq!(actual, [6, 8, 6, 6, 8, 6, 6, 8]);
    for round in 1..=8 {
        let grants = replay
            .trace_events
            .iter()
            .filter(|event| {
                event.round_index == round
                    && event.event_type == "ability_activation"
                    && event.phase == "after_hit"
            })
            .count();
        let expected = if round >= 2 && (round - 2) % 3 == 0 {
            2
        } else {
            1
        };
        assert_eq!(
            grants,
            expected,
            "Picard grants in round {round}; events: {:?}",
            replay
                .trace_events
                .iter()
                .filter(
                    |event| event.round_index == round && event.event_type == "ability_activation"
                )
                .collect::<Vec<_>>()
        );
        assert!(
            replay.trace_events.iter().any(|event| {
                event.round_index == round
                    && event.event_type == "morale_activation"
                    && event.values["triggered"] == true
            }),
            "Neelix applies Morale in round {round}"
        );
    }
}
