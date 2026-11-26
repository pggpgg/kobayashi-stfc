use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TimingWindow, TraceMode, WeaponFiringSchedule, WeaponStats, WeaponType,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

fn crit_chances(result: &kobayashi::combat::SimulationResult, phase: &str) -> Vec<f64> {
    result
        .events
        .iter()
        .filter(|e| e.event_type == "crit_resolution" && e.phase == phase)
        .map(|e| e.values["effective_crit_chance"].as_f64().unwrap())
        .collect()
}

fn crit_mults(result: &kobayashi::combat::SimulationResult, phase: &str) -> Vec<f64> {
    result
        .events
        .iter()
        .filter(|e| e.event_type == "crit_resolution" && e.phase == phase)
        .map(|e| e.values["multiplier"].as_f64().unwrap())
        .collect()
}

#[test]
fn khan_stacks_crit_chance_once_per_multishot_volley_on_hull_damage() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let crew = resolve_crew_to_buff_set(
        "",
        &["khan-3f1d1e".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;

    let attacker = Combatant {
        id: "player".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            crit_chance: Some(0.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(3),
            ..Default::default()
        }],
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 11,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let res = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );

    let actual = crit_chances(&res, "attack");
    let expected = [0.0, 0.05, 0.10];
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a - e).abs() < 1e-6,
            "got {actual:?}, expected {expected:?}"
        );
    }
    let after_dmg_events = res
        .events
        .iter()
        .filter(|e| e.phase == "after_damage_taken")
        .count();
    assert_eq!(
        after_dmg_events, 3,
        "multi-shot weapon should trigger Khan once per volley (3 rounds = 3 events)"
    );
}

#[test]
fn hugh_and_pic_beverly_respect_duration_chance_and_armada_exclusion() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let attacker = Combatant {
        id: "player".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            crit_chance: Some(0.0),
            crit_multiplier: Some(2.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    // Defender fires only on round 1 so we can verify Hugh's 2-round expiry in round 3.
    let defender_r1_only = Combatant {
        id: "hostile".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            firing_schedule: Some(WeaponFiringSchedule {
                first_round: 1,
                interval_rounds: 100,
            }),
            ..Default::default()
        }],
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 4,
        seed: 7,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };

    // Hugh R5: 100% chance, +0.25 crit chance for 2 rounds vs non-Armada hostile.
    // On a 1-weapon ship, R1's hit occurs on the final sub-round, so the 2-round buff takes effect
    // at R2 sub-round 1 (matching live STFC Fight 4 sub-turn boundaries), lasts R2 + R3, and expires
    // at R4.
    let hugh_r5 = resolve_crew_to_buff_set(
        "",
        &[],
        &["hugh-9fc348".into()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let res_hostile = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender_r1_only,
        &config,
        &hugh_r5,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let ch = crit_chances(&res_hostile, "attack");
    assert_eq!(ch.len(), 4);
    assert!((ch[0] - 0.0).abs() < 1e-6);
    assert!(
        (ch[1] - 0.25).abs() < 1e-6,
        "round 2 should have +0.25: {ch:?}"
    );
    assert!(
        (ch[2] - 0.25).abs() < 1e-6,
        "round 3 should still have +0.25 (2 full rounds active): {ch:?}"
    );
    assert!((ch[3] - 0.0).abs() < 1e-6, "round 4 should expire: {ch:?}");

    // Hugh must NOT activate against an Armada.
    let res_armada = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender_r1_only,
        &config,
        &hugh_r5,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert_eq!(
        crit_chances(&res_armada, "attack"),
        vec![0.0, 0.0, 0.0, 0.0]
    );

    // Hugh R1 (45% chance): should proc roughly ~45% of seeds across 200 trials.
    let hugh_r1 = resolve_crew_to_buff_set(
        "",
        &[],
        &["hugh-9fc348".into()],
        &officers,
        &ResolveOptions {
            tier: Some(1),
            ..Default::default()
        },
    )
    .crew;
    let mut procs = 0u32;
    for seed in 1..=200 {
        let cfg = SimulationConfig {
            rounds: 2,
            seed,
            trace_mode: TraceMode::Events,
            ..Default::default()
        };
        let r = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender_r1_only,
            &cfg,
            &hugh_r1,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &CrewConfiguration::default(),
        );
        if crit_chances(&r, "attack")[1] > 0.2 {
            procs += 1;
        }
    }
    assert!(
        (60..=120).contains(&procs),
        "Hugh R1 45% proc rate across 200 seeds out of range: {procs}"
    );

    // PIC Beverly Crusher R5: +0.08 cumulative BaseCritDamageBonus vs non-Armada hostile.
    let pic_bev = resolve_crew_to_buff_set(
        "",
        &["pic-beverly-crusher-26f56a".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let mut crit_attacker = attacker.clone();
    crit_attacker.weapons[0].crit_chance = Some(1.0);
    let defender_every_round = Combatant {
        id: "hostile".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let res_bev = simulate_combat_with_defender_faction_and_defender_crew(
        &crit_attacker,
        &defender_every_round,
        &config,
        &pic_bev,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let mults = crit_mults(&res_bev, "attack");
    for (a, e) in mults.iter().zip([2.0, 2.08, 2.16, 2.24]) {
        assert!((a - e).abs() < 1e-6, "PIC Beverly mults: {mults:?}");
    }
    let res_bev_armada = simulate_combat_with_defender_faction_and_defender_crew(
        &crit_attacker,
        &defender_every_round,
        &config,
        &pic_bev,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert_eq!(
        crit_mults(&res_bev_armada, "attack"),
        vec![2.0, 2.0, 2.0, 2.0]
    );
}

#[test]
fn decius_hull_damage_vs_eurydice_shield_damage_diverge_after_shields_drop() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let attacker = Combatant {
        id: "player".into(),
        hull_health: 10_000.0,
        shield_health: 50.0,
        shield_mitigation: 0.8,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 5,
        defender_level: Some(40),
        ..Default::default()
    };

    // Decius R5 (+0.10 per HullDamageTaken): R1=100, R2=110, R3=120 -> total 330.
    let decius = resolve_crew_to_buff_set(
        "",
        &["decius-8fce68".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let res_decius = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &decius,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_decius.total_damage - 330.0).abs() < 1e-6,
        "Decius total_damage={}",
        res_decius.total_damage
    );

    // Eurydice captain (+0.10 per ShieldDamageTaken): shields break in R1, so only R1 grants a
    // stack -> R1=100, R2=110, R3=110 -> total 320.
    let eurydice = resolve_crew_to_buff_set(
        "eurydice-18b643",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let res_eurydice = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &eurydice,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_eurydice.total_damage - 320.0).abs() < 1e-6,
        "Eurydice total_damage={}",
        res_eurydice.total_damage
    );

    // Native MultVal pool dilution with profile_weapon_damage_fraction = 1.0:
    // Decius +0.10 -> +0.05 effective per stack: R1=100, R2=105, R3=110 -> total 315.
    let diluted_cfg = SimulationConfig {
        profile_weapon_damage_fraction: 1.0,
        ..config
    };
    let res_diluted = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &diluted_cfg,
        &decius,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_diluted.total_damage - 315.0).abs() < 1e-6,
        "Diluted Decius total_damage={}",
        res_diluted.total_damage
    );
}

#[test]
fn janeway_wok_carol_and_mudd_apply_one_round_defensive_stacks_to_later_weapons() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let attacker = Combatant {
        id: "player".into(),
        hull_health: 10_000.0,
        shield_health: 10_000.0,
        shield_mitigation: 0.8,
        weapons: vec![
            WeaponStats {
                attack: 10.0,
                shots: Some(1),
                ..Default::default()
            };
            2
        ],
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 10_000.0,
        weapons: vec![
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                ..Default::default()
            };
            2
        ],
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 3,
        defender_level: Some(50),
        ..Default::default()
    };

    // WOK Carol R5 (+0.25 shield mitigation for 1 round -> 0.8 + 0.25 clamped to 1.0):
    // Per round: weapon 0 hits at 0.8 (20 hull dmg), grants stack; weapon 1 hits at 1.0 (0 hull dmg).
    // Round 2 resets (num_rounds=1 expired), repeating 20 + 0 = 20 -> total 40 hull dmg -> 9_960 hull.
    let wok_carol = resolve_crew_to_buff_set(
        "",
        &[],
        &["wok-carol-52a350".into()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let res_carol = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &wok_carol,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_carol.attacker_hull_remaining - 9_960.0).abs() < 1e-6,
        "WOK Carol R5 hull remaining={}",
        res_carol.attacker_hull_remaining
    );

    // Kathryn Janeway captain (+0.06 shield mitigation for 1 round):
    // R1: weapon 0 hits at 0.80 (20 hull dmg -> grants stack expiring end of R1); weapon 1 hits
    // at 0.86 (14 hull dmg -> last sub-round grant carries into R2 per STFC sub-turn boundary).
    // R2: R1 W0 stack expired, R1 W1 stack active -> weapon 0 hits at 0.86 (14 hull dmg, +stack),
    // weapon 1 hits at 0.92 (8 hull dmg) -> total 20 + 14 + 14 + 8 = 56 hull dmg -> 9_944 hull.
    let janeway = resolve_crew_to_buff_set(
        "kathryn-janeway-bd4a19",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let res_janeway = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &janeway,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_janeway.attacker_hull_remaining - 9_944.0).abs() < 1e-6,
        "Janeway captain hull remaining={}",
        res_janeway.attacker_hull_remaining
    );

    // Mudd captain (-10% Armada weapon damage for 1 round on HitTaken, max_level=70):
    // Unshielded attacker: R1 weapon 0 deals 100 (+stack), weapon 1 deals 90 (+last-subround stack
    // carrying into R2); R2 weapon 0 deals 90 (+stack), weapon 1 deals 80 -> 360 total -> 9_640.
    let mut unshielded = attacker.clone();
    unshielded.shield_health = 0.0;
    let mudd = resolve_crew_to_buff_set(
        "mudd-32546a",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let res_mudd_armada = simulate_combat_with_defender_faction_and_defender_crew(
        &unshielded,
        &defender,
        &config,
        &mudd,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_mudd_armada.attacker_hull_remaining - 9_640.0).abs() < 1e-6,
        "Mudd vs L50 Armada hull remaining={}",
        res_mudd_armada.attacker_hull_remaining
    );
    let res_mudd_hostile = simulate_combat_with_defender_faction_and_defender_crew(
        &unshielded,
        &defender,
        &config,
        &mudd,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_mudd_hostile.attacker_hull_remaining - 9_600.0).abs() < 1e-6,
        "Mudd must stay inactive vs non-Armada"
    );
}

#[test]
fn critical_shot_taken_officers_trigger_only_on_incoming_crits() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let attacker = Combatant {
        id: "player".into(),
        hull_health: 50_000.0,
        weapons: vec![
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                ..Default::default()
            };
            2
        ],
        ..Default::default()
    };
    let make_defender = |crit_chance: f64| Combatant {
        id: "opp".into(),
        hull_health: 50_000.0,
        weapons: vec![
            WeaponStats {
                attack: 10.0,
                shots: Some(1),
                crit_chance: Some(crit_chance),
                crit_multiplier: Some(1.5),
                ..Default::default()
            };
            2
        ],
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 9,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };

    // Mitchell R5 (+0.70 weapon damage for 1 round on CriticalShotTaken):
    // crit_chance = 0 -> 100 + 100 = 200; crit_chance = 1 -> 100 + 170 = 270.
    let mitchell = resolve_crew_to_buff_set(
        "",
        &["mitchell-0217f7".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let no_crit = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &make_defender(0.0),
        &config,
        &mitchell,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!((no_crit.total_damage - 200.0).abs() < 1e-6);
    let with_crit = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &make_defender(1.0),
        &config,
        &mitchell,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!(
        (with_crit.total_damage - 270.0).abs() < 1e-6,
        "Mitchell R5 with incoming crit total_damage={}",
        with_crit.total_damage
    );

    // Saru captain (-0.10 opponent crit chance cumulative on CriticalShotTaken vs Player):
    // 1-weapon defender with base crit_chance 1.0 -> R1: 1.0, R2: 0.9, R3: 0.8.
    let saru = resolve_crew_to_buff_set(
        "saru-68cf4f",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let one_weapon_def = Combatant {
        id: "opp".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            crit_chance: Some(1.0),
            crit_multiplier: Some(1.5),
            ..Default::default()
        }],
        ..Default::default()
    };
    let saru_res = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &one_weapon_def,
        &SimulationConfig {
            rounds: 3,
            seed: 42,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &saru,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    let def_ch = crit_chances(&saru_res, "counter");
    assert_eq!(def_ch.len(), 3);
    assert!((def_ch[0] - 1.0).abs() < 1e-6, "{def_ch:?}");
    assert!((def_ch[1] - 0.9).abs() < 1e-6, "{def_ch:?}");
    assert!(def_ch[2] <= 0.9 + 1e-6, "{def_ch:?}");

    // Sulu captain (40% chance Morale on CriticalShotTaken, num_rounds=2):
    // Never triggers when crit_chance = 0.0; triggers in some seeds when crit_chance = 1.0.
    let sulu = resolve_crew_to_buff_set(
        "sulu-fe562d",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let mut sulu_no_crit_events = 0;
    let mut sulu_crit_events = 0;
    for seed in 1..=50 {
        let cfg = SimulationConfig {
            rounds: 2,
            seed,
            trace_mode: TraceMode::Events,
            ..Default::default()
        };
        let r0 = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &make_defender(0.0),
            &cfg,
            &sulu,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            false,
            true,
            &CrewConfiguration::default(),
        );
        if r0.events.iter().any(|e| e.phase == "after_damage_taken") {
            sulu_no_crit_events += 1;
        }
        let r1 = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &one_weapon_def,
            &cfg,
            &sulu,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            false,
            true,
            &CrewConfiguration::default(),
        );
        if r1.events.iter().any(|e| e.phase == "after_damage_taken") {
            sulu_crit_events += 1;
        }
    }
    assert_eq!(sulu_no_crit_events, 0);
    assert!(
        sulu_crit_events > 10,
        "Sulu should proc Morale on crits: {sulu_crit_events}"
    );
}

#[test]
fn krell_mirek_mirror_uhura_and_quarantined_officers_behave_correctly() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);

    // Krell captain (ShieldDamageTaken -> Hull Breach) must NOT proc with 0 shields.
    let krell = resolve_crew_to_buff_set(
        "krell-ef559b",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut unshielded_attacker = Combatant {
        id: "player".into(),
        hull_health: 10_000.0,
        shield_health: 0.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut krell_unshielded_procs = 0;
    let mut krell_shielded_procs = 0;
    for seed in 1..=60 {
        let cfg = SimulationConfig {
            rounds: 1,
            seed,
            trace_mode: TraceMode::Events,
            ..Default::default()
        };
        let r0 = simulate_combat_with_defender_faction_and_defender_crew(
            &unshielded_attacker,
            &defender,
            &cfg,
            &krell,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &CrewConfiguration::default(),
        );
        if r0.events.iter().any(|e| e.phase == "after_damage_taken") {
            krell_unshielded_procs += 1;
        }
        unshielded_attacker.shield_health = 1000.0;
        unshielded_attacker.shield_mitigation = 0.8;
        let r1 = simulate_combat_with_defender_faction_and_defender_crew(
            &unshielded_attacker,
            &defender,
            &cfg,
            &krell,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &CrewConfiguration::default(),
        );
        unshielded_attacker.shield_health = 0.0;
        if r1.events.iter().any(|e| e.phase == "after_damage_taken") {
            krell_shielded_procs += 1;
        }
    }
    assert_eq!(krell_unshielded_procs, 0);
    assert!(
        krell_shielded_procs > 5,
        "Krell should proc with shields: {krell_shielded_procs}"
    );

    // Mirek captain (HullDamageTaken -> Burning 1R) deals burning damage at end of round.
    let mirek = resolve_crew_to_buff_set(
        "mirek-953093",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let mut mirek_burned = 0;
    for seed in 1..=60 {
        let cfg = SimulationConfig {
            rounds: 1,
            seed,
            trace_mode: TraceMode::Events,
            ..Default::default()
        };
        let r = simulate_combat_with_defender_faction_and_defender_crew(
            &unshielded_attacker,
            &defender,
            &cfg,
            &mirek,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &CrewConfiguration::default(),
        );
        if r.total_damage > 10.0 + 1e-6 {
            mirek_burned += 1;
        }
    }
    assert!(
        mirek_burned > 3,
        "Mirek should burn enemy on hull damage: {mirek_burned}"
    );

    // Mirror Uhura R5 (+1.0x existing Apex Barrier on HullDamageTaken vs Assimilated Player, no Tal):
    let mirror_uhura = resolve_crew_to_buff_set(
        "",
        &["mirror-uhura-0a7168".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let mirror_uhura_with_tal = resolve_crew_to_buff_set(
        "",
        &[
            "mirror-uhura-0a7168".into(),
            kobayashi::combat::TAL_OFFICER_LCARS_ID.into(),
        ],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let assimilated_defender_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Assimilate Self".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::Assimilated {
                    chance: 1.0,
                    duration_rounds: 3,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let apex_attacker = Combatant {
        id: "player".into(),
        hull_health: 50_000.0,
        apex_barrier: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            weapon_type: WeaponType::Energy,
            ..Default::default()
        }],
        ..Default::default()
    };
    let pvp_def = Combatant {
        id: "opp".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 1_000.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let pvp_cfg = SimulationConfig {
        rounds: 2,
        seed: 1,
        ..Default::default()
    };
    let base_res = simulate_combat_with_defender_faction_and_defender_crew(
        &apex_attacker,
        &pvp_def,
        &pvp_cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &assimilated_defender_crew,
    );
    let uhura_res = simulate_combat_with_defender_faction_and_defender_crew(
        &apex_attacker,
        &pvp_def,
        &pvp_cfg,
        &mirror_uhura,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &assimilated_defender_crew,
    );
    let uhura_tal_res = simulate_combat_with_defender_faction_and_defender_crew(
        &apex_attacker,
        &pvp_def,
        &pvp_cfg,
        &mirror_uhura_with_tal,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &assimilated_defender_crew,
    );
    // Base apex_barrier 10_000 => factor = 1 / (1 + 10000/10000) = 0.5 -> 500/rd -> 49_000.
    // Mirror Uhura R5 doubles barrier in R2 to 20_000 => factor = 1/3 -> R1=500, R2=333.33 -> ~49_166.67.
    assert!(
        uhura_res.attacker_hull_remaining > base_res.attacker_hull_remaining + 100.0,
        "Mirror Uhura R5 should double apex barrier in R2: uhura={}, base={}",
        uhura_res.attacker_hull_remaining,
        base_res.attacker_hull_remaining
    );
    assert!(
        (uhura_tal_res.attacker_hull_remaining - base_res.attacker_hull_remaining).abs() < 1e-6,
        "Tal on bridge must suppress Mirror Uhura's apex barrier bonus: tal={}, base={}",
        uhura_tal_res.attacker_hull_remaining,
        base_res.attacker_hull_remaining
    );

    // Verified enemy-targeting HullDamageTaken MultiplyBaseSub ShieldHPRepair officer
    // (Michael Burnham, Batch B Fight C unified op 6 family) compiles to reactive
    // CrewStatScaledDeferred::DefenderShieldDrainFlat.
    let burnham_crew = resolve_crew_to_buff_set(
        "",
        &["michael-burnham-6c711c".into()],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    assert!(
        burnham_crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::CrewStatScaledDeferred {
                target: kobayashi::combat::CrewStatScaledTarget::DefenderShieldDrainFlat,
                axis: kobayashi::combat::CrewStatAxis::Attack,
                reactive_trigger: Some(kobayashi::combat::DamageTakenTrigger::HullDamage),
                ..
            }
        )),
        "michael-burnham-6c711c bridge ability must compile to reactive op 6 DefenderShieldDrainFlat, got {:?}",
        burnham_crew
            .seats
            .iter()
            .map(|s| s.ability.effect)
            .collect::<Vec<_>>()
    );

    // The Doctor (HullDamageTaken SelfShip MultiplyBaseAdd AllDefenses, 2R, Health) is now modeled
    // via unified op 5 reactive PRE_ALL CrewStatScaledDeferred.
    let doctor_crew = resolve_crew_to_buff_set(
        "",
        &["the-doctor-327fc3".into()],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    assert!(
        doctor_crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::CrewStatScaledDeferred {
                target: kobayashi::combat::CrewStatScaledTarget::AllDefensesPreAll,
                axis: kobayashi::combat::CrewStatAxis::Health,
                duration_rounds: 2,
                reactive_trigger: Some(kobayashi::combat::DamageTakenTrigger::HullDamage),
                ..
            }
        )),
        "the-doctor-327fc3 bridge ability must compile to reactive 2R PRE_ALL CrewStatScaledDeferred, got {:?}",
        doctor_crew
            .seats
            .iter()
            .map(|s| s.ability.effect)
            .collect::<Vec<_>>()
    );
}
