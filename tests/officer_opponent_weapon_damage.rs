use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TimingWindow, WeaponStats, WeaponType, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

#[test]
fn opponent_damage_captains_reduce_only_opponent_weapon_damage_symmetrically() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let player = Combatant {
        id: "player".into(),
        hull_health: 10_000.0,
        attack: 100.0,
        ..Default::default()
    };
    let opponent = Combatant {
        id: "opponent".into(),
        ..player.clone()
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 42,
        ..Default::default()
    };
    let empty = CrewConfiguration::default();
    for id in ["cadet-uhura-3ef15c", "next-gen-la-forge-ee6d76"] {
        let crew =
            resolve_crew_to_buff_set(id, &[], &[], &officers, &ResolveOptions::default()).crew;
        // On attacker crew: reduces opponent counter-fire by 10% (2 rounds * 90 = 180) without
        // buffing own outbound damage (still 2 * 100 = 200).
        let attacking = simulate_combat_with_defender_faction_and_defender_crew(
            &player,
            &opponent,
            &config,
            &crew,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            false,
            true,
            &empty,
        );
        assert_eq!(attacking.total_damage, 200.0, "{id} outbound");
        assert_eq!(attacking.attacker_hull_remaining, 9_820.0, "{id} inbound");

        // On defender crew in PvP: reduces attacker outbound damage by 10% (180) without buffing
        // defender counter-fire (still 200 -> 9_800 hull remaining).
        let defending = simulate_combat_with_defender_faction_and_defender_crew(
            &player,
            &opponent,
            &config,
            &empty,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            false,
            true,
            &crew,
        );
        assert_eq!(defending.total_damage, 180.0, "{id} defending outbound");
        assert_eq!(
            defending.attacker_hull_remaining, 9_800.0,
            "{id} defending inbound"
        );
    }
}

#[test]
fn npc_weapon_reductions_respect_weapon_type_level_cap_armada_gate_and_pike_scaling() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let player = Combatant {
        id: "player".into(),
        hull_health: 10_000.0,
        weapons: [WeaponType::Kinetic, WeaponType::Energy]
            .into_iter()
            .map(|weapon_type| WeaponStats {
                attack: 100.0,
                shots: Some(1),
                weapon_type,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let opponent = Combatant {
        id: "hostile".into(),
        ..player.clone()
    };
    let empty = CrewConfiguration::default();

    // Rank 4 values: Cath = 21% Kinetic, Chen = 25% Energy, T'Laan = 15% Kinetic.
    // Hostile has 1 Kinetic (100) + 1 Energy (100) = 200 raw counter-fire.
    for (id, rank_four) in [
        ("cath-f0e149", 0.21),
        ("chen-cdb1ca", 0.25),
        ("t-laan-c4627b", 0.15),
    ] {
        let crew = resolve_crew_to_buff_set(
            "",
            &[id.into()],
            &[],
            &officers,
            &ResolveOptions {
                tier: Some(4),
                ..Default::default()
            },
        )
        .crew;
        for (level, class, expected_hull) in [
            // Level <= 51 non-armada hostile: reduces matching weapon type only (100 + 100*(1-r)).
            (
                51,
                ShipType::Battleship,
                10_000.0 - (200.0 - 100.0 * rank_four),
            ),
            // Level 52 exceeds TargetMaxLevel(51): inactive.
            (52, ShipType::Battleship, 9_800.0),
            // Armada excluded by TargetNotArmada: inactive.
            (51, ShipType::Armada, 9_800.0),
        ] {
            let config = SimulationConfig {
                rounds: 1,
                seed: 42,
                defender_level: Some(level),
                ..Default::default()
            };
            let result = simulate_combat_with_defender_faction_and_defender_crew(
                &player,
                &opponent,
                &config,
                &crew,
                OpponentFactionTag::Unknown,
                class,
                ShipType::Battleship,
                true,
                false,
                &empty,
            );
            assert_eq!(result.total_damage, 200.0, "{id}, level {level}, {class:?}");
            assert!(
                (result.attacker_hull_remaining - expected_hull).abs() < 1e-6,
                "{id}, level {level}, {class:?}: got {}, expected {}",
                result.attacker_hull_remaining,
                expected_hull
            );
        }
    }

    // Pike T1 (+40% bridge effectiveness) boosts Chen T5 (30% Energy -> 42% Energy).
    let pike_chen = resolve_crew_to_buff_set(
        "pike-1e7d0d",
        &["chen-cdb1ca".into()],
        &[],
        &officers,
        &ResolveOptions {
            officer_tiers: Some(
                [
                    ("pike-1e7d0d".to_string(), 1),
                    ("chen-cdb1ca".to_string(), 5),
                ]
                .into(),
            ),
            ..Default::default()
        },
    )
    .crew;
    let config = SimulationConfig {
        rounds: 1,
        seed: 42,
        defender_level: Some(40),
        ..Default::default()
    };
    let boosted = simulate_combat_with_defender_faction_and_defender_crew(
        &player,
        &opponent,
        &config,
        &pike_chen,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &empty,
    );
    // Kinetic = 100, Energy = 100 * (1 - 0.30 * 1.40) = 58 -> 158 damage -> 9_842 hull.
    assert!((boosted.attacker_hull_remaining - 9_842.0).abs() < 1e-6);
}

#[test]
fn round_start_opponent_weapon_reduction_distinguishes_cumulative_garak_from_single_turn_lorca() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let ship = Combatant {
        id: "ship".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            weapon_type: WeaponType::Kinetic,
            ..Default::default()
        }],
        ..Default::default()
    };
    let with_breach = |mut crew: CrewConfiguration| {
        crew.seats.push(CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "Guaranteed Hull Breach".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HullBreach {
                    chance: 1.0,
                    duration_rounds: 3,
                    requires_critical: false,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        });
        crew
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 7,
        ..Default::default()
    };
    let empty = CrewConfiguration::default();

    // Garak captain (Interceptor vs Player w/ Hull Breach, Kinetic): cumulative 5% per round.
    // Round 1: 95, Round 2: 90, Round 3: 85 -> total 270 damage -> 9_730 hull.
    let garak = with_breach(
        resolve_crew_to_buff_set(
            "garak-771862",
            &[],
            &[],
            &officers,
            &ResolveOptions::default(),
        )
        .crew,
    );
    let garak_res = simulate_combat_with_defender_faction_and_defender_crew(
        &ship,
        &ship,
        &config,
        &garak,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Interceptor,
        false,
        true,
        &empty,
    );
    assert!((garak_res.attacker_hull_remaining - 9_730.0).abs() < 1e-6);

    // Lorca captain (vs Player w/ Hull Breach, num_rounds=1): non-cumulative 100% reduction
    // re-evaluated every round while Hull Breach holds (not clipped to round 1 only!).
    // Round 1: 0, Round 2: 0, Round 3: 0 -> total 0 damage -> 10_000 hull.
    let lorca = with_breach(
        resolve_crew_to_buff_set(
            "lorca-d32ec8",
            &[],
            &[],
            &officers,
            &ResolveOptions::default(),
        )
        .crew,
    );
    let lorca_res = simulate_combat_with_defender_faction_and_defender_crew(
        &ship,
        &ship,
        &config,
        &lorca,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &empty,
    );
    assert!(
        (lorca_res.attacker_hull_remaining - 10_000.0).abs() < 1e-6,
        "Lorca should reduce opponent weapon damage on every round Hull Breach holds, got {}",
        lorca_res.attacker_hull_remaining
    );

    // Carol captain has SelfDefending (unmodeled on attack path) -> must remain quarantined!
    let carol = resolve_crew_to_buff_set(
        "carol-755a05",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    )
    .crew;
    assert!(carol.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::UnsupportedOfficerEffect { .. }
    )));
}
