//! Validate source-bound officer effects through the combat loop, not only conversion.
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup,
    simulate_combat_with_defender_faction_and_defender_crew, AbilityCondition, AbilityEffect,
    Combatant, CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig, TraceMode,
    WeaponStats,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

fn run(crew: &CrewConfiguration, rounds: u32) -> kobayashi::combat::SimulationResult {
    let attacker = Combatant {
        id: "attacker".into(),
        hull_health: 1e9,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            crit_chance: Some(1.0),
            crit_multiplier: Some(2.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        id: "defender".into(),
        hull_health: 1e9,
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
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    simulate_combat_from_setup(&setup, 7)
}

#[test]
fn nero_source_burning_can_trigger_after_round_two_and_lasts_two_rounds() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let buff = resolve_crew_to_buff_set(
        "nero-1aeca9",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    );
    let mut seat = buff
        .crew
        .seats
        .into_iter()
        .find(|s| matches!(s.ability.effect, AbilityEffect::Burning { .. }))
        .unwrap();
    seat.ability.effect = AbilityEffect::Burning {
        chance: 1.0,
        duration_rounds: 2,
    };
    let continually_triggered = run(
        &CrewConfiguration {
            seats: vec![seat.clone()],
        },
        4,
    );
    let rounds: Vec<_> = continually_triggered
        .events
        .iter()
        .filter(|e| {
            e.event_type == "burning_trigger"
                && e.phase == "attack"
                && e.values["triggered"] == true
        })
        .map(|e| e.round_index)
        .collect();
    assert!(
        rounds.contains(&3) && rounds.contains(&4),
        "later round procs missing: {rounds:?}"
    );

    seat.ability.condition = Some(AbilityCondition::RoundRange { min: 1, max: 1 });
    let single_proc = run(&CrewConfiguration { seats: vec![seat] }, 4);
    let positive_burn_rounds: Vec<_> = single_proc
        .events
        .iter()
        .filter(|e| {
            e.event_type == "end_of_round_effects"
                && e.values["burning_damage"].as_f64().unwrap_or(0.0) > 0.0
        })
        .map(|e| e.round_index)
        .collect();
    assert_eq!(positive_burn_rounds, [1, 2]);
}

#[test]
fn suder_source_crit_bonus_expires_after_five_rounds_in_combat() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let buff = resolve_crew_to_buff_set(
        "phlox-72c07b",
        &[],
        &["suder-d348a9".into()],
        &officers,
        &ResolveOptions::default(),
    );
    let suder = buff
        .crew
        .seats
        .into_iter()
        .find(|s| s.officer_id.as_deref() == Some("suder-d348a9"))
        .unwrap();
    let result = run(&CrewConfiguration { seats: vec![suder] }, 6);
    let crits: Vec<_> = result
        .events
        .iter()
        .filter(|e| e.event_type == "crit_resolution" && e.phase == "attack")
        .map(|e| (e.round_index, e.values["multiplier"].as_f64().unwrap()))
        .collect();
    assert_eq!(crits.len(), 6);
    let buffed = crits[0].1;
    let unbuffed = crits[5].1;
    assert!(buffed > unbuffed, "Suder did not increase critical damage");
    for (round, multiplier) in crits {
        let expected = if round <= 5 { buffed } else { unbuffed };
        assert!(
            (multiplier - expected).abs() < 1e-9,
            "round {round}, crit multiplier {multiplier}"
        );
    }
}

#[test]
fn cadet_sulu_and_next_gen_riker_boost_all_first_round_shots_only() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let attacker = Combatant {
        id: "attacker".into(),
        hull_health: 10_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(2),
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        id: "defender".into(),
        ..attacker.clone()
    };
    let empty = CrewConfiguration::default();
    let mut config = SimulationConfig {
        rounds: 3,
        seed: 42,
        ..Default::default()
    };
    for (captain, synergy_officer) in [
        ("cadet-sulu-784421", "cadet-scotty-b342ae"),
        ("next-gen-riker-44ccee", "next-gen-la-forge-ee6d76"),
    ] {
        // The second entry in each pair shares the captain's group and adds
        // the source's different-class synergy entry (+10 percentage points).
        for (bridge, shot_damage) in [(vec![], 105.0), (vec![synergy_officer.into()], 115.0)] {
            let crew = resolve_crew_to_buff_set(
                captain,
                &bridge,
                &[],
                &officers,
                &ResolveOptions::default(),
            )
            .crew;
            config.trace_mode = TraceMode::Events;
            let npc = simulate_combat_with_defender_faction_and_defender_crew(
                &attacker,
                &defender,
                &config,
                &crew,
                OpponentFactionTag::Unknown,
                ShipType::Battleship,
                ShipType::Battleship,
                true,
                false,
                &empty,
            );
            let shots: Vec<_> = npc
                .events
                .iter()
                .filter(|event| event.event_type == "damage_application" && event.phase == "damage")
                .map(|event| {
                    (
                        event.round_index,
                        event.values["applied_weapon_damage"].as_f64().unwrap(),
                    )
                })
                .collect();
            assert_eq!(
                shots,
                [
                    (1, shot_damage),
                    (1, shot_damage),
                    (2, 100.0),
                    (2, 100.0),
                    (3, 100.0),
                    (3, 100.0)
                ],
                "{captain} with {bridge:?}"
            );
            assert_eq!(npc.attacker_hull_remaining, 9_400.0);

            config.trace_mode = TraceMode::Off;
            let pvp = simulate_combat_with_defender_faction_and_defender_crew(
                &attacker,
                &defender,
                &config,
                &empty,
                OpponentFactionTag::Unknown,
                ShipType::Battleship,
                ShipType::Battleship,
                false,
                true,
                &crew,
            );
            assert_eq!(pvp.total_damage, 600.0);
            let expected_hull = attacker.hull_health - 400.0 - 2.0 * shot_damage;
            assert!(
                (pvp.attacker_hull_remaining - expected_hull).abs() < 1e-9,
                "{captain} with {bridge:?} on the PvP defender, expected {expected_hull}, got {}",
                pvp.attacker_hull_remaining
            );
        }
    }
}
