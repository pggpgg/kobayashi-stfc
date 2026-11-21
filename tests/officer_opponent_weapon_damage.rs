use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, AbilityEffect, Combatant,
    CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig, WeaponStats, WeaponType,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

#[test]
fn opponent_damage_captains_cannot_raise_friendly_weapon_damage() {
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
        for (attacking_crew, defending_crew) in [(&crew, &empty), (&empty, &crew)] {
            let result = simulate_combat_with_defender_faction_and_defender_crew(
                &player,
                &opponent,
                &config,
                attacking_crew,
                OpponentFactionTag::Unknown,
                ShipType::Battleship,
                ShipType::Battleship,
                false,
                true,
                defending_crew,
            );
            assert_eq!(result.total_damage, 200.0, "{id} outbound");
            assert_eq!(result.attacker_hull_remaining, 9_800.0, "{id} inbound");
        }
    }
}

#[test]
fn npc_weapon_reductions_preserve_ranked_diagnostics_without_buffing_either_weapon() {
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
        for (level, class) in [
            (51, ShipType::Battleship),
            (52, ShipType::Battleship),
            (51, ShipType::Armada),
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
            assert_eq!(result.attacker_hull_remaining, 9_800.0, "{id} inbound");
        }
        assert_eq!(crew.seats.len(), 1, "{id}");
        assert_eq!(
            crew.seats[0].ability.effect,
            AbilityEffect::UnsupportedOfficerEffect {
                coefficient: rank_four,
                chance: 1.0
            },
            "{id}",
        );
    }
}
