//! Scope guards for hull abilities assembled from the shipped ship catalog.
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Combatant, CrewConfiguration,
    EnemyType, OpponentFactionTag, ShipType, SimulationConfig, TraceMode,
};
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::profile::PlayerProfile;
use kobayashi::data::ship_ability_resolve::{
    ship_abilities_to_crew_seat_contexts,
    sum_combat_begin_accuracy_from_ship_abilities_for_opponent,
};
use kobayashi::optimizer::monte_carlo::mitigation_and_pierce_for_player_vs_player;
use std::collections::HashMap;

fn crew_for_ship(ship: &str, tier: u32, level: u32) -> CrewConfiguration {
    let record = resolve_ship_with_tier_level(ship, Some(tier), Some(level)).unwrap();
    CrewConfiguration {
        seats: ship_abilities_to_crew_seat_contexts(record.abilities.as_deref().unwrap()),
    }
}

fn one_shot(
    crew: &CrewConfiguration,
    defender: &Combatant,
    defender_type: ShipType,
    npc: bool,
    wave: bool,
    trace_mode: TraceMode,
) -> kobayashi::combat::SimulationResult {
    let attacker = Combatant {
        id: "scope_attacker".into(),
        attack: 1_000.0,
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    let mut config = SimulationConfig {
        rounds: 1,
        seed: 17,
        trace_mode,
        ..Default::default()
    };
    if wave {
        config.engagement_enemy_types = EnemyType::Waves.into();
    }
    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        defender,
        &config,
        crew,
        OpponentFactionTag::Unknown,
        defender_type,
        ShipType::Explorer,
        npc,
        !npc,
        &CrewConfiguration::default(),
    )
}

#[test]
fn gladius_secret_weapon_applies_only_against_npc_hostiles() {
    let defender = Combatant {
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    for (tier, level, expected_npc_damage) in [(1, 1, 1_350.0), (9, 45, 1_800.0)] {
        let crew = crew_for_ship("gladius", tier, level);
        for (npc, expected) in [(true, expected_npc_damage), (false, 1_000.0)] {
            let result = one_shot(
                &crew,
                &defender,
                ShipType::Explorer,
                npc,
                false,
                TraceMode::Events,
            );
            let plain = one_shot(
                &crew,
                &defender,
                ShipType::Explorer,
                npc,
                false,
                TraceMode::Off,
            );
            assert_eq!(result.total_damage, expected, "Gladius L{level}, NPC={npc}");
            assert_eq!(result.total_damage, plain.total_damage);
            assert_eq!(
                result.defender_hull_remaining,
                plain.defender_hull_remaining
            );
        }
    }
}

#[test]
fn gs_31_wave_only_shred_does_not_weaken_barrier_in_other_fights() {
    let crew = crew_for_ship("gs_31", 15, 75);
    let defender = Combatant {
        hull_health: 1_000_000.0,
        apex_barrier: 10_000.0,
        ..Default::default()
    };
    let raider = resolve_hostile("80126960").expect("Apex Raiders Orion Master");
    assert_eq!(raider.faction.as_ref().unwrap().id, 3_505_432_050);
    for (defender_type, npc, wave) in [
        (ShipType::Battleship, true, false),
        (ShipType::Armada, true, false),
        (ShipType::Battleship, false, false),
        // The generic Waves flag cannot establish Solo Wave Defense.
        (raider.ship_type(), true, true),
    ] {
        let result = one_shot(
            &crew,
            &defender,
            defender_type,
            npc,
            wave,
            TraceMode::Events,
        );
        let plain = one_shot(&crew, &defender, defender_type, npc, wave, TraceMode::Off);
        assert_eq!(
            result.total_damage, 500.0,
            "{defender_type:?}, NPC={npc}, Waves={wave}"
        );
        assert_eq!(result.total_damage, plain.total_damage);
        assert_eq!(
            result.defender_hull_remaining,
            plain.defender_hull_remaining
        );
        assert!(!result.events.iter().any(|event| {
            event.event_type == "ability_activation"
                && event.source.ship_ability_id.as_deref() == Some("1773930736")
        }));
    }
}

fn counter_shot(
    crew: &CrewConfiguration,
    defender_attack: f64,
    defender_isolytic_damage: f64,
    defender_type: ShipType,
    npc: bool,
    trace_mode: TraceMode,
) -> kobayashi::combat::SimulationResult {
    let attacker = Combatant {
        id: "scope_attacker".into(),
        attack: 1_000.0,
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    let defender = Combatant {
        id: "scope_defender".into(),
        attack: defender_attack,
        hull_health: 1_000_000.0,
        isolytic_damage: defender_isolytic_damage,
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 19,
        trace_mode,
        ..Default::default()
    };
    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        crew,
        OpponentFactionTag::Unknown,
        defender_type,
        ShipType::Interceptor,
        npc,
        !npc,
        &CrewConfiguration::default(),
    )
}

#[test]
fn grade7_isolytic_ward_applies_only_to_non_armada_npc_hostiles() {
    for (ship, ability_id, expected_l1, expected_l75) in [
        ("i_k_s_ak_voh", "1947674592", 20.0, 35.0),
        ("i_k_s_boreth", "1457441777", 5.0, 20.0),
        ("i_k_s_gre_thor", "88812954", 35.0, 50.0),
    ] {
        let rec_l1 = resolve_ship_with_tier_level(ship, Some(1), Some(1)).unwrap();
        let rec_l75 = resolve_ship_with_tier_level(ship, Some(15), Some(75)).unwrap();
        let ab1 = rec_l1
            .abilities
            .as_deref()
            .unwrap()
            .iter()
            .find(|a| a.id == ability_id)
            .unwrap();
        let ab75 = rec_l75
            .abilities
            .as_deref()
            .unwrap()
            .iter()
            .find(|a| a.id == ability_id)
            .unwrap();
        assert!((ab1.value - expected_l1).abs() < 1e-9, "{ship} L1");
        assert!((ab75.value - expected_l75).abs() < 1e-9, "{ship} L75");
        assert!(ab1.condition_defender_is_npc_hostile, "{ship} npc gate");
        assert_eq!(
            ab1.condition_opponent_ship_class_not.as_deref(),
            Some("armada"),
            "{ship} non-armada gate"
        );

        let crew = crew_for_ship(ship, 15, 75);
        // Non-armada NPC hostile triggers Isolytic Ward and cuts incoming isolytic damage.
        let vs_npc = counter_shot(
            &crew,
            1_000.0,
            2.0,
            ShipType::Battleship,
            true,
            TraceMode::Events,
        );
        let vs_npc_plain = counter_shot(
            &crew,
            1_000.0,
            2.0,
            ShipType::Battleship,
            true,
            TraceMode::Off,
        );
        assert_eq!(
            vs_npc.attacker_hull_remaining,
            vs_npc_plain.attacker_hull_remaining
        );
        // Armada and player ships must NOT trigger Isolytic Ward.
        let vs_armada = counter_shot(
            &crew,
            1_000.0,
            2.0,
            ShipType::Armada,
            true,
            TraceMode::Events,
        );
        let vs_player = counter_shot(
            &crew,
            1_000.0,
            2.0,
            ShipType::Battleship,
            false,
            TraceMode::Events,
        );
        assert!(
            vs_npc.attacker_hull_remaining > vs_armada.attacker_hull_remaining,
            "{ship} should take less isolytic damage vs non-armada NPC than vs Armada"
        );
        assert!(
            !vs_armada.events.iter().any(|e| {
                e.event_type == "ability_activation"
                    && e.source.ship_ability_id.as_deref() == Some(ability_id)
            }),
            "{ship} Isolytic Ward must not fire vs Armada"
        );
        assert!(
            !vs_player.events.iter().any(|e| {
                e.event_type == "ability_activation"
                    && e.source.ship_ability_id.as_deref() == Some(ability_id)
            }),
            "{ship} Isolytic Ward must not fire vs Player"
        );
    }
}

#[test]
fn grade7_through_the_veil_stays_inactive_without_veil_selector() {
    let crew = crew_for_ship("i_k_s_boreth", 15, 75);
    let defender = Combatant {
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    for (defender_type, npc) in [
        (ShipType::Battleship, true),
        (ShipType::Armada, true),
        (ShipType::Battleship, false),
    ] {
        let result = one_shot(
            &crew,
            &defender,
            defender_type,
            npc,
            false,
            TraceMode::Events,
        );
        let plain = one_shot(&crew, &defender, defender_type, npc, false, TraceMode::Off);
        assert_eq!(result.total_damage, 1_000.0, "{defender_type:?}, NPC={npc}");
        assert_eq!(result.total_damage, plain.total_damage);
        assert!(!result.events.iter().any(|e| {
            e.event_type == "ability_activation"
                && e.source.ship_ability_id.as_deref() == Some("2500322687")
        }));
    }
}

#[test]
fn grade7_apex_aegis_activates_only_against_player_ships() {
    let rec_l1 = resolve_ship_with_tier_level("i_k_s_gre_thor", Some(1), Some(1)).unwrap();
    let rec_l75 = resolve_ship_with_tier_level("i_k_s_gre_thor", Some(15), Some(75)).unwrap();
    let ab1 = rec_l1
        .abilities
        .as_deref()
        .unwrap()
        .iter()
        .find(|a| a.id == "390344944")
        .unwrap();
    let ab75 = rec_l75
        .abilities
        .as_deref()
        .unwrap()
        .iter()
        .find(|a| a.id == "390344944")
        .unwrap();
    assert!((ab1.value - 10_000.0).abs() < 1e-9);
    assert!((ab75.value - 20_000.0).abs() < 1e-9);
    assert!(ab1.condition_defender_is_player_ship);
    assert!(!ab1.condition_defender_is_npc_hostile);

    let crew = crew_for_ship("i_k_s_gre_thor", 15, 75);
    // Against NPC hostile or Armada: Apex Aegis (+20,000 Apex Barrier) stays inert, full 1,000 hit lands.
    for defender_type in [ShipType::Battleship, ShipType::Armada] {
        let vs_npc = counter_shot(&crew, 1_000.0, 0.0, defender_type, true, TraceMode::Events);
        let vs_npc_plain = counter_shot(&crew, 1_000.0, 0.0, defender_type, true, TraceMode::Off);
        assert_eq!(
            vs_npc.attacker_hull_remaining, 999_000.0,
            "NPC {defender_type:?}"
        );
        assert_eq!(
            vs_npc.attacker_hull_remaining,
            vs_npc_plain.attacker_hull_remaining
        );
        assert!(!vs_npc.events.iter().any(|e| {
            e.event_type == "ability_activation"
                && e.source.ship_ability_id.as_deref() == Some("390344944")
        }));
    }
    // Against player ship: +20,000 Apex Barrier cuts incoming damage to 1,000 / (1 + 20,000/10,000) = 333.33...
    let vs_player = counter_shot(
        &crew,
        1_000.0,
        0.0,
        ShipType::Battleship,
        false,
        TraceMode::Events,
    );
    let vs_player_plain = counter_shot(
        &crew,
        1_000.0,
        0.0,
        ShipType::Battleship,
        false,
        TraceMode::Off,
    );
    assert_eq!(
        vs_player.attacker_hull_remaining,
        vs_player_plain.attacker_hull_remaining
    );
    let damage_taken = 1_000_000.0 - vs_player.attacker_hull_remaining;
    assert!(
        (damage_taken - (1_000.0 / 3.0)).abs() < 1.0,
        "expected ~333.33 damage after +20,000 Apex Barrier in PvP, got {damage_taken}"
    );
}

#[test]
fn kos_karii_true_aim_preserves_fraction_and_is_inert_in_pvp() {
    let rec_l1 = resolve_ship_with_tier_level("kos_karii", Some(1), Some(1)).unwrap();
    let rec_l90 = resolve_ship_with_tier_level("kos_karii", Some(18), Some(90)).unwrap();
    let ab1 = rec_l1
        .abilities
        .as_deref()
        .unwrap()
        .iter()
        .find(|a| a.id == "1800726742")
        .unwrap();
    let ab90 = rec_l90
        .abilities
        .as_deref()
        .unwrap()
        .iter()
        .find(|a| a.id == "1800726742")
        .unwrap();
    assert!(
        (ab1.value - 10.0).abs() < 1e-9,
        "L1 True Aim should be 10.0 (+1000%)"
    );
    assert!(
        (ab90.value - 15.0).abs() < 1e-9,
        "L90 True Aim should be 15.0 (+1500%)"
    );
    assert!(ab1.condition_defender_is_npc_hostile);

    let npc_bonus = sum_combat_begin_accuracy_from_ship_abilities_for_opponent(
        rec_l90.abilities.as_deref().unwrap(),
        ShipType::Interceptor,
        true,
        false,
    );
    let pvp_bonus = sum_combat_begin_accuracy_from_ship_abilities_for_opponent(
        rec_l90.abilities.as_deref().unwrap(),
        ShipType::Interceptor,
        false,
        true,
    );
    assert!((npc_bonus - 15.0).abs() < 1e-9);
    assert_eq!(pvp_bonus, 0.0, "True Aim must stay inert in PvP");

    // Verify player-vs-player mitigation path ignores True Aim (matches a copy without abilities).
    let mut rec_no_abilities = rec_l90.clone();
    rec_no_abilities.abilities = None;
    let defender_ship = resolve_ship_with_tier_level("i_k_s_gre_thor", Some(1), Some(1)).unwrap();
    let profile = PlayerProfile::default();
    let empty_buffs = HashMap::new();
    let pvp_with = mitigation_and_pierce_for_player_vs_player(
        &rec_l90,
        &defender_ship,
        &profile,
        &empty_buffs,
    );
    let pvp_without = mitigation_and_pierce_for_player_vs_player(
        &rec_no_abilities,
        &defender_ship,
        &profile,
        &empty_buffs,
    );
    assert_eq!(
        pvp_with, pvp_without,
        "Kos'Karii True Aim must not alter PvP mitigation or pierce"
    );
}
