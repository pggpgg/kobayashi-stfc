use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TimingWindow, TraceMode, WeaponStats, WeaponType,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;

#[test]
fn krencha_does_not_trigger_disintegrate_on_incoming_damage() {
    assert_no_named_weapon_stacks("krencha", "394860549");
}

#[test]
fn scimitar_does_not_trigger_designed_for_war_on_incoming_damage() {
    assert_no_named_weapon_stacks("scimitar", "407462527");
}

#[test]
fn enterprise_e_morale_does_not_trigger_drawing_the_line_without_a_decimator_hit() {
    assert_no_named_weapon_stacks("uss_enterprise_e", "4132047263");
}

fn assert_no_named_weapon_stacks(ship: &str, ability_id: &str) {
    let attacker = Combatant {
        id: "attacker".into(),
        hull_health: 100_000.0,
        shield_health: 0.0,
        crit_chance: 0.0,
        weapons: vec![
            WeaponStats {
                attack: 1000.0,
                shots: Some(1),
                crit_chance: Some(0.0),
                weapon_type: WeaponType::Kinetic,
                ..Default::default()
            },
            WeaponStats {
                attack: 1000.0,
                shots: Some(1),
                crit_chance: Some(0.0),
                weapon_type: WeaponType::Energy,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let defender = Combatant {
        id: "target".into(),
        hull_health: 100_000.0,
        shield_health: 0.0,
        attack: 100.0,
        crit_chance: 0.0,
        mitigation: 0.0,
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 3,
        seed: 17,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };

    let record = resolve_ship_with_tier_level(ship, Some(1), Some(1)).unwrap();
    let abilities = record.abilities.as_deref().unwrap();
    assert!(abilities.iter().any(|ability| ability.id == ability_id));
    let mut crew = CrewConfiguration {
        seats: ship_abilities_to_crew_seat_contexts(abilities),
    };
    // A guaranteed Morale source exercises Enterprise-E's old round-start proxy.
    crew.seats.push(CrewSeatContext {
        seat: CrewSeat::Captain,
        ability: Ability {
            name: "synthetic morale".into(),
            class: AbilityClass::CaptainManeuver,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::Morale(1.0),
            condition: None,
            weapon_scope: Default::default(),
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });

    for npc in [true, false] {
        let setup = build_combat_setup(
            &attacker,
            &defender,
            &config,
            &crew,
            OpponentFactionTag::Federation,
            ShipType::Explorer,
            ShipType::Battleship,
            npc,
            !npc,
            &CrewConfiguration::default(),
        );
        let result = simulate_combat_from_setup(&setup, config.seed);
        let attacks: Vec<f64> = result
            .events
            .iter()
            .filter(|event| event.event_type == "attack_roll")
            .map(|event| event.values["effective_attack"].as_f64().unwrap())
            .collect();
        let activations: Vec<_> = result
            .events
            .iter()
            .filter(|event| {
                event.event_type == "ability_activation"
                    && event.source.ship_ability_id.as_deref() == Some(ability_id)
            })
            .map(|event| (event.round_index, event.phase.as_str()))
            .collect();
        assert!(result.attacker_hull_remaining < attacker.hull_health);
        assert_eq!(
            (attacks, activations),
            (vec![1000.0; 6], vec![]),
            "{ship} ({ability_id}), npc={npc}"
        );
    }
}
