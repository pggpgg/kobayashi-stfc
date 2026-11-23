use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    AttackerStats, Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats,
    HostileMitigationParams, OpponentFactionTag, ShipType, SimulationConfig, TimingWindow,
    TraceMode, WeaponStats, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

fn raw_defense_ship(id: &str) -> Combatant {
    Combatant {
        id: id.into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: DefenderStats {
                armor: 1_000.0,
                shield_deflection: 1_000.0,
                dodge: 1_000.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 1_000.0,
                shield_piercing: 1_000.0,
                accuracy: 1_000.0,
            },
            profile_piercing_fractions: None,
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Battleship,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    }
}

fn counter_effective_armor_by_round(res: &kobayashi::combat::SimulationResult) -> Vec<f64> {
    res.events
        .iter()
        .filter(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
        .map(|e| e.values["effective_armor"].as_f64().unwrap())
        .collect()
}

fn outbound_mitigation_by_round(res: &kobayashi::combat::SimulationResult) -> Vec<f64> {
    res.events
        .iter()
        .filter(|e| e.event_type == "mitigation_calc" && e.phase == "defense")
        .map(|e| e.values["mitigation"].as_f64().unwrap())
        .collect()
}

fn crit_chances(res: &kobayashi::combat::SimulationResult, phase: &str) -> Vec<f64> {
    res.events
        .iter()
        .filter(|e| e.event_type == "crit_resolution" && e.phase == phase)
        .map(|e| e.values["effective_crit_chance"].as_f64().unwrap())
        .collect()
}

#[test]
fn self_all_defenses_officers_scale_raw_mitigation_stats_and_respect_gates() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let attacker = raw_defense_ship("player");
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let empty = CrewConfiguration::default();

    // Brad Boimler BD R5 (+1.30): effective armor = 1000 * (1 + 1.30) = 2300.
    let boimler = resolve_crew_to_buff_set(
        "",
        &[],
        &["brad-boimler-ee5262".into()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let cfg = SimulationConfig {
        rounds: 2,
        seed: 1,
        defender_level: Some(40),
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let res_boimler = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &boimler,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    assert_eq!(
        counter_effective_armor_by_round(&res_boimler),
        vec![2300.0, 2300.0]
    );

    // Sam Rutherford bridge R5 (+0.50) boosted by Pike T1 (+40% bridge -> +0.70 => 1700 armor).
    let pike_rutherford = resolve_crew_to_buff_set(
        "pike-1e7d0d",
        &["sam-rutherford-927c93".into()],
        &[],
        &officers,
        &ResolveOptions {
            officer_tiers: Some(
                [
                    ("pike-1e7d0d".to_string(), 1),
                    ("sam-rutherford-927c93".to_string(), 5),
                ]
                .into(),
            ),
            ..Default::default()
        },
    )
    .crew;
    let res_rutherford = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &pike_rutherford,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    assert_eq!(
        counter_effective_armor_by_round(&res_rutherford),
        vec![1700.0, 1700.0]
    );

    // Chen captain (+0.20 vs Hostile <= L51): active at L51 (1200), inactive at L52 (1000).
    let chen = resolve_crew_to_buff_set(
        "chen-cdb1ca",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let res_chen_51 = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &SimulationConfig {
            defender_level: Some(51),
            ..cfg.clone()
        },
        &chen,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    let res_chen_52 = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &SimulationConfig {
            defender_level: Some(52),
            ..cfg.clone()
        },
        &chen,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    assert_eq!(
        counter_effective_armor_by_round(&res_chen_51),
        vec![1200.0, 1200.0]
    );
    assert_eq!(
        counter_effective_armor_by_round(&res_chen_52),
        vec![1000.0, 1000.0]
    );

    // Kumak bridge R5 (+1.00 at RoundStart while enemy Burning, num_rounds=1): re-arms on both
    // rounds while Burning holds (2000, 2000 — not clipped to round 1!).
    let mut kumak = resolve_crew_to_buff_set(
        "",
        &["kumak-c5b0db".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    kumak.seats.push(CrewSeatContext {
        seat: CrewSeat::Captain,
        ability: Ability {
            weapon_scope: Default::default(),
            name: "Guaranteed Burn".into(),
            class: AbilityClass::CaptainManeuver,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::Burning {
                chance: 1.0,
                duration_rounds: 3,
            },
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });
    let res_kumak = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &kumak,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    assert_eq!(
        counter_effective_armor_by_round(&res_kumak),
        vec![2000.0, 2000.0]
    );

    // Hendorff captain (+0.20 at RoundStart when hull < 50% of fight-start hull):
    // Heavy hostile hits drop player below 50% in R1 -> inactive in R1 (1000), active in R2 (1200).
    let hendorff = resolve_crew_to_buff_set(
        "hendorff-549c65",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let heavy_defender = Combatant {
        id: "heavy".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 60_000.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let res_hendorff = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &heavy_defender,
        &cfg,
        &hendorff,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    assert_eq!(
        counter_effective_armor_by_round(&res_hendorff),
        vec![1000.0, 1200.0]
    );

    // Gossa BD R5 (+3.00 cumulative per round vs Assimilated Player): R1=4000, R2=7000, R3=10000.
    let gossa = resolve_crew_to_buff_set(
        "",
        &[],
        &["gossa-dafefb".into()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let assim_def_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Assimilate".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::Assimilated {
                    chance: 1.0,
                    duration_rounds: 5,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let res_gossa = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &SimulationConfig { rounds: 3, ..cfg },
        &gossa,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &assim_def_crew,
    );
    assert_eq!(
        counter_effective_armor_by_round(&res_gossa),
        vec![4000.0, 7000.0, 10000.0]
    );
}

#[test]
fn badgey_all_defenses_reduction_and_self_all_piercing_scale_outbound_mitigation() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let attacker = Combatant {
        id: "player".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 1_000.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    // Defender with hostile_mitigation_params so outbound mitigation is computed dynamically.
    let defender = Combatant {
        id: "hostile".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: DefenderStats {
                armor: 2_000.0,
                shield_deflection: 2_000.0,
                dodge: 2_000.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 1_000.0,
                shield_piercing: 1_000.0,
                accuracy: 1_000.0,
            },
            profile_piercing_fractions: None,
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Battleship,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let empty = CrewConfiguration::default();
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 1,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let base_res = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &empty,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    let base_mit = outbound_mitigation_by_round(&base_res)[0];

    // Badgey bridge R5 (-20% enemy raw defenses) lowers hostile mitigation vs non-Armada.
    let badgey_bridge = resolve_crew_to_buff_set(
        "",
        &["badgey-13df33".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let res_badgey = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &badgey_bridge,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    let badgey_mit = outbound_mitigation_by_round(&res_badgey)[0];
    assert!(
        badgey_mit < base_mit - 0.01,
        "Badgey bridge should lower defender mitigation: badgey={badgey_mit}, base={base_mit}"
    );
    // Inactive vs Armada (TargetNotArmada).
    let res_badgey_armada = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &badgey_bridge,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    assert!((outbound_mitigation_by_round(&res_badgey_armada)[0] - base_mit).abs() < 1e-6);

    // Brad Boimler bridge R5 (+50% AllPiercing) and Badgey BD R5 (+65% AllPiercing) lower mitigation.
    let boimler_bridge = resolve_crew_to_buff_set(
        "",
        &["brad-boimler-ee5262".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let badgey_bd = resolve_crew_to_buff_set(
        "",
        &[],
        &["badgey-13df33".into()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let boimler_mit =
        outbound_mitigation_by_round(&simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &cfg,
            &boimler_bridge,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &empty,
        ))[0];
    let badgey_bd_mit =
        outbound_mitigation_by_round(&simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &cfg,
            &badgey_bd,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &empty,
        ))[0];
    assert!(boimler_mit < base_mit - 0.01);
    assert!(badgey_bd_mit < boimler_mit);

    // Native MultVal pool dilution check for Badgey BD R5 (+65% AllPiercing):
    // With profile_piercing_fractions = 1.0 (base_attacker_stats already 2x raw), adding +0.65
    // scales by (2+0.65)/2 = 1.325x, giving a HIGHER mitigation than undiluted 1.65x!
    let mut high_def = defender.clone();
    high_def
        .hostile_mitigation_params
        .as_mut()
        .unwrap()
        .defender_stats = DefenderStats {
        armor: 2_200.0,
        shield_deflection: 2_200.0,
        dodge: 2_200.0,
    };
    let undiluted_badgey_mit =
        outbound_mitigation_by_round(&simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &high_def,
            &cfg,
            &badgey_bd,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &empty,
        ))[0];
    let mut high_def_diluted = high_def.clone();
    high_def_diluted
        .hostile_mitigation_params
        .as_mut()
        .unwrap()
        .profile_piercing_fractions = Some(AttackerStats {
        armor_piercing: 1.0,
        shield_piercing: 1.0,
        accuracy: 1.0,
    });
    let diluted_badgey_mit =
        outbound_mitigation_by_round(&simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &high_def_diluted,
            &cfg,
            &badgey_bd,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &empty,
        ))[0];
    assert!(
        diluted_badgey_mit > undiluted_badgey_mit + 0.01,
        "Profile piercing pool should dilute Badgey BD's +65% bonus: diluted={diluted_badgey_mit}, undiluted={undiluted_badgey_mit}"
    );

    // Data bridge R5 (+300% AllPiercing) and Worf bridge R5 (+100% AllDefenses) compile with
    // their exact CombatBattleTypeAny([2, 3, 5, 6, 7, 8, 9]) gate intact.
    let data_bridge = resolve_crew_to_buff_set(
        "",
        &["data-d20ef8".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    assert!(data_bridge.seats.iter().any(|s| {
        matches!(s.ability.effect, AbilityEffect::BasePiercingStatsBonus(v) if (v - 3.0).abs() < 1e-12)
            && s.ability.condition
                == Some(kobayashi::combat::AbilityCondition::CombatBattleTypeAny(
                    vec![2, 3, 5, 6, 7, 8, 9],
                ))
    }));
    let worf_bridge = resolve_crew_to_buff_set(
        "",
        &["worf-c55d58".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    assert!(worf_bridge.seats.iter().any(|s| {
        matches!(s.ability.effect, AbilityEffect::BaseMitigationStatsBonus(v) if (v - 1.0).abs() < 1e-12)
            && s.ability.condition
                == Some(kobayashi::combat::AbilityCondition::CombatBattleTypeAny(
                    vec![2, 3, 5, 6, 7, 8, 9],
                ))
    }));

    // Borg Queen BD R5 (+500% cumulative AllPiercing per round vs Assimilated Player):
    let borg_queen_bd = resolve_crew_to_buff_set(
        "",
        &[],
        &["borg-queen-c8e67d".into()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let assim_def_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Assimilate".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::Assimilated {
                    chance: 1.0,
                    duration_rounds: 5,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let mut very_high_def = defender.clone();
    very_high_def
        .hostile_mitigation_params
        .as_mut()
        .unwrap()
        .defender_stats = DefenderStats {
        armor: 20_000.0,
        shield_deflection: 20_000.0,
        dodge: 20_000.0,
    };
    let bq_mits =
        outbound_mitigation_by_round(&simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &very_high_def,
            &SimulationConfig {
                rounds: 3,
                ..cfg.clone()
            },
            &borg_queen_bd,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            false,
            true,
            &assim_def_crew,
        ));
    assert_eq!(bq_mits.len(), 3);
    assert!(
        bq_mits[1] < bq_mits[0] && bq_mits[2] < bq_mits[1],
        "Borg Queen BD should cumulatively lower defender mitigation each round: {bq_mits:?}"
    );

    // Quarantined AllDefenses rows (Joachim captain, Khan captain, Qa'Ug captain, Tiza bridge)
    // must remain UnsupportedOfficerEffect.
    for (cap, br) in [
        ("joachim-03587e", vec![]),
        ("khan-3f1d1e", vec![]),
        ("qa-ug-a165bf", vec![]),
        ("", vec!["tiza-9d38f9".into()]),
    ] {
        let crew =
            resolve_crew_to_buff_set(cap, &br, &[], &officers, &ResolveOptions::default()).crew;
        assert!(
            crew.seats.iter().any(|s| matches!(
                s.ability.effect,
                AbilityEffect::UnsupportedOfficerEffect { .. }
            )),
            "expected quarantined UnsupportedOfficerEffect for cap={cap:?} br={br:?}"
        );
    }
}

#[test]
fn extended_enemy_crit_chance_reductions_respect_duration_cumulative_and_on_hit() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let player = Combatant {
        id: "player".into(),
        hull_health: 50_000.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(1),
            crit_chance: Some(1.0),
            crit_multiplier: Some(2.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let opp = Combatant {
        id: "opp".into(),
        ..player.clone()
    };
    let empty = CrewConfiguration::default();

    // VIL Gul Dukat BD R5 (-0.50 CombatStart EnemyAllShips num_rounds=4 vs Player):
    // Rounds 1..=4 => 0.50; Round 5 => 1.00.
    let dukat = resolve_crew_to_buff_set(
        "",
        &[],
        &["vil-gul-dukat-c46cb8".into()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let res_dukat = simulate_combat_with_defender_faction_and_defender_crew(
        &player,
        &opp,
        &SimulationConfig {
            rounds: 5,
            seed: 1,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &dukat,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &empty,
    );
    let dukat_ch = crit_chances(&res_dukat, "counter");
    assert_eq!(dukat_ch, vec![0.5, 0.5, 0.5, 0.5, 1.0]);

    // PvP symmetry: VIL Gul Dukat on defender_crew reduces attacker outbound crit chance in R1..=4.
    let res_dukat_def = simulate_combat_with_defender_faction_and_defender_crew(
        &player,
        &opp,
        &SimulationConfig {
            rounds: 5,
            seed: 1,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &empty,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &dukat,
    );
    assert_eq!(
        crit_chances(&res_dukat_def, "attack"),
        vec![0.5, 0.5, 0.5, 0.5, 1.0]
    );

    // Nog bridge R5 (-0.70 RoundStart TargetStateAny num_rounds=2 vs Player):
    // Burn applied on R1 only (duration 1 round) -> Nog fires at R1 start (expires R2),
    // so R1=0.30, R2=0.30 (still active without Burn!), R3=1.00 (expired!).
    let mut nog = resolve_crew_to_buff_set(
        "",
        &["nog-0c9672".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    nog.seats.push(CrewSeatContext {
        seat: CrewSeat::Captain,
        ability: Ability {
            weapon_scope: Default::default(),
            name: "R1 Burn".into(),
            class: AbilityClass::CaptainManeuver,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::Burning {
                chance: 1.0,
                duration_rounds: 1,
            },
            condition: Some(kobayashi::combat::AbilityCondition::RoundRange { min: 1, max: 1 }),
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });
    let res_nog = simulate_combat_with_defender_faction_and_defender_crew(
        &player,
        &opp,
        &SimulationConfig {
            rounds: 3,
            seed: 1,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &nog,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &empty,
    );
    let nog_ch = crit_chances(&res_nog, "counter");
    for (a, e) in nog_ch.iter().zip([0.3, 0.3, 1.0]) {
        assert!((a - e).abs() < 1e-6, "Nog crit chances: {nog_ch:?}");
    }

    // Changeling Kira captain (-0.02 cumulative per round vs Cardassian Armada faction 3536012672):
    let kira = resolve_crew_to_buff_set(
        "changeling-kira-666ebe",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    let res_kira = simulate_combat_with_defender_faction_and_defender_crew(
        &player,
        &opp,
        &SimulationConfig {
            rounds: 3,
            seed: 1,
            defender_hull_faction_id: 3_536_012_672,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &kira,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    let kira_ch = crit_chances(&res_kira, "counter");
    for (a, e) in kira_ch.iter().zip([0.98, 0.96, 0.94]) {
        assert!(
            (a - e).abs() < 1e-6,
            "Changeling Kira crit chances: {kira_ch:?}"
        );
    }

    // Sela bridge R5 (-0.06 cumulative per EnemyTakesHit vs Player):
    // 2-weapon ships: R1 w0 counter=1.0, attacker w0 hits -> R1 w1 counter=0.94, attacker w1 hits
    // -> R2 w0 counter=0.88, attacker w0 hits -> R2 w1 counter=0.82.
    let sela = resolve_crew_to_buff_set(
        "",
        &["sela-bd6e1b".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let two_weapon = Combatant {
        id: "two_weapon".into(),
        hull_health: 50_000.0,
        weapons: vec![
            WeaponStats {
                attack: 10.0,
                shots: Some(1),
                crit_chance: Some(1.0),
                crit_multiplier: Some(2.0),
                ..Default::default()
            };
            2
        ],
        ..Default::default()
    };
    let res_sela = simulate_combat_with_defender_faction_and_defender_crew(
        &two_weapon,
        &two_weapon,
        &SimulationConfig {
            rounds: 2,
            seed: 1,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &sela,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &empty,
    );
    let sela_ch = crit_chances(&res_sela, "counter");
    for (a, e) in sela_ch.iter().zip([1.0, 0.94, 0.88, 0.82]) {
        assert!((a - e).abs() < 1e-6, "Sela crit chances: {sela_ch:?}");
    }
}
