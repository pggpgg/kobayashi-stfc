use kobayashi::combat::hostile_tags::HOSTILE_TAG_MASK_XINDI_AQUATIC;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration, ShipType,
    SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

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
