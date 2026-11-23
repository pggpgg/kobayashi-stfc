//! Issue #419 regression suite: honor ship and hostile weapon warm-up and cooldown
//! schedules across catalog resolution, synthetic NX-01/Aquatic fights, trace parity,
//! optimizer evaluation parity, and synced component overrides.

use kobayashi::combat::{
    build_combat_setup, simulate_combat, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponFiringSchedule, WeaponStats,
    WeaponType,
};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::data::ship::apply_component_overrides_to_ship_record;
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry, run_monte_carlo_with_registry, DefenderOpponent,
};

fn active_shots_in_round(combatant: &Combatant, round: u32) -> u32 {
    (0..combatant.weapon_count())
        .filter(|&idx| combatant.weapon_fires_in_round(idx, round))
        .map(|idx| combatant.weapon_base_shots(idx))
        .sum()
}

#[test]
fn player_ships_honor_upstream_weapon_warmup_and_cooldown_schedules() {
    // U.S.S. Saladin: primary kinetic big gun (w=1, c=3, 1 shot) + secondary energy (w=1, c=1, 1 shot)
    let saladin = resolve_ship_with_tier_level("uss_saladin", Some(6), Some(30)).unwrap();
    let saladin_c = Combatant {
        weapons: saladin.to_weapons(),
        ..Default::default()
    };
    assert_eq!(
        [1, 2, 3, 4, 5, 6].map(|r| active_shots_in_round(&saladin_c, r)),
        [2, 1, 1, 2, 1, 1],
        "Saladin primary weapon must fire on rounds 1, 4, ..."
    );

    // SS Revenant: two c=1 weapons (4+2=6 base shots) + two c=2 odd-round weapons (2+4=6 base shots)
    // Matches the exact 2:1 odd/even shot ratio observed in the 26-round Borg Type 03.1 recorded fight.
    let revenant = resolve_ship_with_tier_level("ss_revenant", Some(6), Some(30)).unwrap();
    let revenant_c = Combatant {
        weapons: revenant.to_weapons(),
        ..Default::default()
    };
    assert_eq!(
        [1, 2, 3, 4, 5, 6].map(|r| active_shots_in_round(&revenant_c, r)),
        [12, 6, 12, 6, 12, 6],
        "SS Revenant odd/even base shot cadence must be 12/6"
    );

    // D4 Class: two alternating 2-shot kinetic weapons (w=1,c=2 and w=2,c=2)
    let d4 = resolve_ship_with_tier_level("d4_class", Some(6), Some(30)).unwrap();
    let d4_c = Combatant {
        weapons: d4.to_weapons(),
        ..Default::default()
    };
    assert_eq!(
        [1, 2, 3, 4].map(|r| active_shots_in_round(&d4_c, r)),
        [2, 2, 2, 2],
        "D4 must alternate weapons rather than firing both every round"
    );

    // Augur: two c=1 2-shot energy weapons + heavy kinetic (w=2, c=3, 1 shot)
    let augur = resolve_ship_with_tier_level("augur", Some(6), Some(30)).unwrap();
    let augur_c = Combatant {
        weapons: augur.to_weapons(),
        ..Default::default()
    };
    assert_eq!(
        [1, 2, 3, 4, 5, 6].map(|r| active_shots_in_round(&augur_c, r)),
        [4, 5, 4, 4, 5, 4],
        "Augur heavy weapon must warm up on R2 and fire every 3 rounds (R2, R5, ...)"
    );

    // Monaveen: two c=1 2-shot energy weapons + kinetic (w=1, c=2, 1 shot)
    let monaveen = resolve_ship_with_tier_level("monaveen", Some(6), Some(30)).unwrap();
    let monaveen_c = Combatant {
        weapons: monaveen.to_weapons(),
        ..Default::default()
    };
    assert_eq!(
        [1, 2, 3, 4].map(|r| active_shots_in_round(&monaveen_c, r)),
        [5, 4, 5, 4],
        "Monaveen kinetic weapon must fire on odd rounds only"
    );

    // Gorn Eviscerator: two c=1 1-shot weapons + even-round 2-shot kinetic (w=2, c=2)
    let gorn = resolve_ship_with_tier_level("gorn_eviscerator", Some(6), Some(30)).unwrap();
    let gorn_c = Combatant {
        weapons: gorn.to_weapons(),
        ..Default::default()
    };
    assert_eq!(
        [1, 2, 3, 4].map(|r| active_shots_in_round(&gorn_c, r)),
        [2, 4, 2, 4],
        "Gorn Eviscerator third weapon must fire on even rounds only"
    );
}

#[test]
fn non_xindi_hostiles_honor_upstream_weapon_warmup_and_cooldown_schedules() {
    // Borg Drone L44 (1031834643): has a w=10, c=10 mega weapon that must stay silent on R1..R9.
    let borg = resolve_hostile("1031834643").unwrap();
    let borg_weapons = borg.weapons_for_counter_attack(ShipType::Interceptor, Default::default());
    let borg_mega_idx = borg_weapons
        .iter()
        .position(|w| {
            w.firing_schedule
                == Some(WeaponFiringSchedule {
                    first_round: 10,
                    interval_rounds: 10,
                })
        })
        .expect("Borg Drone must carry w=10, c=10 mega weapon schedule");
    let borg_c = Combatant {
        weapons: borg_weapons,
        ..Default::default()
    };
    for r in 1..=9 {
        assert!(
            !borg_c.weapon_fires_in_round(borg_mega_idx, r),
            "Borg mega weapon must not fire in round {r}"
        );
    }
    assert!(borg_c.weapon_fires_in_round(borg_mega_idx, 10));
    assert!(!borg_c.weapon_fires_in_round(borg_mega_idx, 11));
    assert!(borg_c.weapon_fires_in_round(borg_mega_idx, 20));

    // Actian Apex L48 (1099730812): 4-round rotating cadence (w=1..4, c=4)
    let actian = resolve_hostile("1099730812").unwrap();
    let actian_weapons =
        actian.weapons_for_counter_attack(ShipType::Interceptor, Default::default());
    assert!(
        actian_weapons.iter().any(|w| {
            w.firing_schedule
                == Some(WeaponFiringSchedule {
                    first_round: 4,
                    interval_rounds: 4,
                })
        }),
        "Actian Apex must retain its w=4, c=4 fourth-round heavy weapon schedule"
    );
    let actian_c = Combatant {
        weapons: actian_weapons,
        ..Default::default()
    };
    let r1_shots = active_shots_in_round(&actian_c, 1);
    let r4_shots = active_shots_in_round(&actian_c, 4);
    assert!(
        r4_shots > r1_shots,
        "Actian Apex heavy R4 weapon (9 shots) must only fire on R4, not R1 (r1={r1_shots}, r4={r4_shots})"
    );

    // Swarm Cluster L41 (1003172485): w=12, c=12 kill weapon
    let swarm = resolve_hostile("1003172485").unwrap();
    let swarm_weapons = swarm.weapons_for_counter_attack(ShipType::Interceptor, Default::default());
    let swarm_heavy = swarm_weapons
        .iter()
        .position(|w| {
            w.firing_schedule
                == Some(WeaponFiringSchedule {
                    first_round: 12,
                    interval_rounds: 12,
                })
        })
        .expect("Swarm Cluster must retain w=12, c=12 schedule");
    let swarm_c = Combatant {
        weapons: swarm_weapons,
        ..Default::default()
    };
    assert!(!swarm_c.weapon_fires_in_round(swarm_heavy, 1));
    assert!(swarm_c.weapon_fires_in_round(swarm_heavy, 12));

    // Species 8472 Infiltrator L58 (1265230501): w=8, c=8 bio-beam
    let bio = resolve_hostile("1265230501").unwrap();
    let bio_weapons = bio.weapons_for_counter_attack(ShipType::Explorer, Default::default());
    let bio_beam = bio_weapons
        .iter()
        .position(|w| {
            w.firing_schedule
                == Some(WeaponFiringSchedule {
                    first_round: 8,
                    interval_rounds: 8,
                })
        })
        .expect("Species 8472 must retain w=8, c=8 bio-beam schedule");
    let bio_c = Combatant {
        weapons: bio_weapons,
        ..Default::default()
    };
    assert!(!bio_c.weapon_fires_in_round(bio_beam, 1));
    assert!(!bio_c.weapon_fires_in_round(bio_beam, 7));
    assert!(bio_c.weapon_fires_in_round(bio_beam, 8));
}

#[test]
fn synthetic_nx01_vs_aquatic_charging_kinetics_and_single_fire_cruiser_weapon_with_trace_parity() {
    // Mirrors the issue #419 reproduction:
    // - NX-01: two 1-shot energy weapons (w=1, c=1) + two 4-shot charging kinetics (w=2, c=2).
    // - Aquatic Cruiser: four 10-shot regular weapons (w=1, c=1) + fifth weapon with 11x crit
    //   multiplier and cool_down=100 (fires in round 1 only during a 4-round fight).
    let nx01 = Combatant {
        id: "synthetic_nx01".into(),
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        mitigation: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.5,
        weapons: vec![
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                weapon_type: WeaponType::Energy,
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 1,
                }),
                ..Default::default()
            },
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                weapon_type: WeaponType::Energy,
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 1,
                }),
                ..Default::default()
            },
            WeaponStats {
                attack: 250.0,
                shots: Some(4),
                weapon_type: WeaponType::Kinetic,
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 2,
                    interval_rounds: 2,
                }),
                ..Default::default()
            },
            WeaponStats {
                attack: 250.0,
                shots: Some(4),
                weapon_type: WeaponType::Kinetic,
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 2,
                    interval_rounds: 2,
                }),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let aquatic = Combatant {
        id: "synthetic_aquatic".into(),
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        mitigation: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.5,
        weapons: vec![
            WeaponStats {
                attack: 10.0,
                shots: Some(10),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 1,
                }),
                ..Default::default()
            },
            WeaponStats {
                attack: 10.0,
                shots: Some(10),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 1,
                }),
                ..Default::default()
            },
            WeaponStats {
                attack: 10.0,
                shots: Some(10),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 1,
                }),
                ..Default::default()
            },
            WeaponStats {
                attack: 10.0,
                shots: Some(10),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 1,
                }),
                ..Default::default()
            },
            WeaponStats {
                attack: 100.0,
                shots: Some(10),
                crit_chance: Some(1.0),
                crit_multiplier: Some(11.0),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 100,
                }),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let cfg_events = SimulationConfig {
        rounds: 4,
        seed: 419,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let cfg_off = SimulationConfig {
        trace_mode: TraceMode::Off,
        ..cfg_events.clone()
    };

    let traced = simulate_combat(&nx01, &aquatic, &cfg_events, &CrewConfiguration::default());
    let fast = simulate_combat(&nx01, &aquatic, &cfg_off, &CrewConfiguration::default());
    let fast_repeat = simulate_combat(&nx01, &aquatic, &cfg_off, &CrewConfiguration::default());

    // Exact trace-on vs trace-off and deterministic seed parity
    assert_eq!(traced.total_damage, fast.total_damage);
    assert_eq!(traced.attacker_hull_remaining, fast.attacker_hull_remaining);
    assert_eq!(traced.defender_hull_remaining, fast.defender_hull_remaining);
    assert_eq!(traced.rounds_simulated, fast.rounds_simulated);
    assert_eq!(traced.attacker_won, fast.attacker_won);
    assert_eq!(fast.total_damage, fast_repeat.total_damage);
    assert_eq!(
        fast.attacker_hull_remaining,
        fast_repeat.attacker_hull_remaining
    );

    // Outbound NX-01 shots per round: R1=2 (200 dmg), R2=10 (2200 dmg), R3=2 (200 dmg), R4=10 (2200 dmg)
    let outbound_by_round = |r: u32| {
        traced
            .events
            .iter()
            .filter(|e| e.event_type == "attack_roll" && e.phase == "attack" && e.round_index == r)
            .count()
    };
    assert_eq!([1, 2, 3, 4].map(outbound_by_round), [2, 10, 2, 10]);
    assert!(
        (traced.total_damage - 4_800.0).abs() < 1e-6,
        "expected 200+2200+200+2200 = 4800 total outbound damage, got {}",
        traced.total_damage
    );

    // Inbound Aquatic Cruiser: 4 regular weapons = 400/round over 4 rounds (1600),
    // plus 5th weapon (10 shots * 100 * 11x crit = 11,000) in round 1 ONLY -> 12,600 total damage.
    let counter_slot4_rounds: Vec<u32> = traced
        .events
        .iter()
        .filter(|e| {
            e.event_type == "crit_resolution" && e.phase == "counter" && e.weapon_index == Some(4)
        })
        .map(|e| e.round_index)
        .collect();
    assert_eq!(
        counter_slot4_rounds,
        vec![1; 10],
        "5th Aquatic weapon (cool_down=100) must fire 10 shots in R1 and zero in R2..R4"
    );
    let expected_attacker_hull = 1_000_000.0 - 12_600.0;
    assert!(
        (traced.attacker_hull_remaining - expected_attacker_hull).abs() < 1e-6,
        "expected attacker hull {expected_attacker_hull}, got {}",
        traced.attacker_hull_remaining
    );

    // Also verify build_combat_setup + simulate_combat_from_setup matches identically
    let setup = build_combat_setup(
        &nx01,
        &aquatic,
        &cfg_off,
        &CrewConfiguration::default(),
        OpponentFactionTag::Xindi,
        ShipType::Interceptor,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let from_setup = simulate_combat_from_setup(&setup, 419);
    assert_eq!(from_setup.total_damage, fast.total_damage);
    assert_eq!(
        from_setup.attacker_hull_remaining,
        fast.attacker_hull_remaining
    );
}

#[test]
fn optimizer_monte_carlo_and_replay_preserve_ship_and_hostile_schedules() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let candidate = CrewCandidate {
        captain: "ent-e-picard-556227".to_string(),
        bridge: vec![
            "ent-e-data-871245".to_string(),
            "five-of-eleven-d9aa11".to_string(),
        ],
        below_decks: vec![],
    };
    let seed = 1337_u64;

    // 1. Player ship schedule through optimizer MC + replay: Saladin vs damage sponge.
    let (saladin_mc, saladin_ph) = run_monte_carlo_with_registry(
        &registry,
        "uss_saladin",
        "kobayashi_theoretical_damage_sponge",
        Some(6),
        Some(30),
        std::slice::from_ref(&candidate),
        1,
        seed,
        Some(DEMO_PROFILE_ID),
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert!(!saladin_ph);
    assert_eq!(saladin_mc.len(), 1);

    let saladin_replay = replay_optimize_iteration_with_registry(
        &registry,
        "uss_saladin",
        "kobayashi_theoretical_damage_sponge",
        Some(6),
        Some(30),
        &candidate,
        seed,
        0,
        Some(DEMO_PROFILE_ID),
        20_000,
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        saladin_replay.rounds_simulated >= 4,
        "Saladin vs damage sponge must reach round 4, got {}",
        saladin_replay.rounds_simulated
    );
    let saladin_slot0_in_round = |r: u32| {
        saladin_replay
            .trace_events
            .iter()
            .filter(|e| {
                e.event_type == "attack_roll"
                    && e.phase == "attack"
                    && e.round_index == r
                    && e.weapon_index == Some(0)
            })
            .count()
    };
    assert!(
        saladin_slot0_in_round(1) > 0,
        "Saladin slot 0 (w=1, c=3) must fire in round 1"
    );
    assert_eq!(
        saladin_slot0_in_round(2),
        0,
        "Saladin slot 0 (w=1, c=3) must be silent in round 2"
    );
    assert_eq!(
        saladin_slot0_in_round(3),
        0,
        "Saladin slot 0 (w=1, c=3) must be silent in round 3"
    );
    assert!(
        saladin_slot0_in_round(4) > 0,
        "Saladin slot 0 (w=1, c=3) must fire again in round 4"
    );

    // 2. Hostile schedule through optimizer MC + replay: Enterprise-D vs Charged Krenim Warships L50
    //    (1101241646): hull 6.1e9, r1dmg 1.68e7, w=[1/1, 1/1, 1/2, 1/2] -> guaranteed multi-round fight.
    let (hostile_mc, hostile_ph) = run_monte_carlo_with_registry(
        &registry,
        "uss_enterprise_d",
        "1101241646",
        Some(5),
        Some(18),
        std::slice::from_ref(&candidate),
        1,
        seed,
        Some(DEMO_PROFILE_ID),
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert!(!hostile_ph);
    assert_eq!(hostile_mc.len(), 1);

    let hostile_replay = replay_optimize_iteration_with_registry(
        &registry,
        "uss_enterprise_d",
        "1101241646",
        Some(5),
        Some(18),
        &candidate,
        seed,
        0,
        Some(DEMO_PROFILE_ID),
        20_000,
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        hostile_replay.rounds_simulated >= 3,
        "Enterprise-D vs Charged Krenim Warships L50 must reach round 3, got {}",
        hostile_replay.rounds_simulated
    );
    let hostile_c2_in_round = |r: u32| {
        hostile_replay
            .trace_events
            .iter()
            .filter(|e| {
                e.event_type == "crit_resolution"
                    && e.phase == "counter"
                    && e.round_index == r
                    && matches!(e.weapon_index, Some(2) | Some(3))
            })
            .count()
    };
    assert!(
        hostile_c2_in_round(1) > 0,
        "Charged Krenim Warships c=2 weapons (slots 2 and 3) must fire in round 1"
    );
    assert_eq!(
        hostile_c2_in_round(2),
        0,
        "Charged Krenim Warships c=2 weapons (slots 2 and 3) must be silent in round 2"
    );
    assert!(
        hostile_c2_in_round(3) > 0,
        "Charged Krenim Warships c=2 weapons (slots 2 and 3) must fire again in round 3"
    );

    let expected_win = if hostile_replay.attacker_won && !hostile_replay.winner_by_round_limit {
        1.0
    } else {
        0.0
    };
    assert_eq!(hostile_mc[0].win_rate, expected_win);
}

#[test]
fn component_overrides_preserve_firing_schedules_on_all_ships() {
    let raw_text = std::fs::read_to_string("data/upstream/data-stfc-space/ships/3056258007.json")
        .expect("Saladin upstream JSON");
    let raw: serde_json::Value = serde_json::from_str(&raw_text).expect("valid JSON");
    let tier6_ids: Vec<i64> = raw["tiers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tier"].as_u64() == Some(6))
        .unwrap()["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["id"].as_i64())
        .collect();

    let mut ship = resolve_ship_with_tier_level("uss_saladin", Some(6), Some(30)).unwrap();
    let summary = apply_component_overrides_to_ship_record(&mut ship, &raw, 6, &tier6_ids)
        .expect("component override summary");
    assert!(!summary.applied());
    let weapons = ship.to_weapons();
    assert_eq!(
        weapons[0].firing_schedule,
        Some(WeaponFiringSchedule {
            first_round: 1,
            interval_rounds: 3,
        }),
        "Saladin primary weapon schedule must survive component override"
    );
    assert_eq!(
        weapons[1].firing_schedule,
        Some(WeaponFiringSchedule {
            first_round: 1,
            interval_rounds: 1,
        }),
        "Saladin secondary weapon schedule must survive component override"
    );
}
