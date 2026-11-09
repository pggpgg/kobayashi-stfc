//! Monaveen buff 49906243: native base damage MULT, enemy hull faction 2796195869.
use kobayashi::combat::hostile_tags::HOSTILE_TAG_MASK_TEXAS_CLASS;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::hostile::HostileRecord;
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship::ShipAbility;
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

fn ability(level: u32) -> ShipAbility {
    resolve_ship_with_tier_level("monaveen", Some(12), Some(level))
        .expect("Monaveen")
        .abilities
        .unwrap()
        .into_iter()
        .find(|a| a.id == "49906243")
        .expect("Modernity is Overrated")
}

#[test]
fn native_fraction_uses_observed_ship_level_without_percent_rescaling() {
    for (level, expected) in [(1, 520.0), (54, 2375.0), (55, 2435.0), (60, 2800.0)] {
        let a = ability(level);
        assert_eq!(a.effect_type, "component_base_weapon_damage");
        assert_eq!(a.value, expected, "native rank is ship level minus one");
        assert!(a.condition_defender_is_npc_hostile);
        assert_eq!(
            a.condition_opponent_hostile_tags,
            Some(vec!["texas_class".into()])
        );
    }
}

#[test]
fn texas_membership_uses_exact_native_faction_not_name_or_ship_class() {
    let actual = resolve_hostile("569324075").unwrap();
    assert_ne!(actual.hostile_tag_mask() & HOSTILE_TAG_MASK_TEXAS_CLASS, 0);
    let mut unrelated = actual.clone();
    unrelated.hostile_name = "Texas-class Sentry".into();
    unrelated.faction = None;
    unrelated.hostile_tags.clear();
    assert_eq!(
        unrelated.hostile_tag_mask() & HOSTILE_TAG_MASK_TEXAS_CLASS,
        0
    );
    let generic: HostileRecord = serde_json::from_value(serde_json::json!({
        "id":"synthetic","hostile_name":"Untranslated","level":1,"ship_class":"battleship",
        "armor":0,"dodge":0,"shield_deflection":0,"hull_health":100,"shield_health":0,
        "faction":{"id":2796195869_i64}
    }))
    .unwrap();
    assert_ne!(generic.hostile_tag_mask() & HOSTILE_TAG_MASK_TEXAS_CLASS, 0);
}

fn first_attack(
    mask: u32,
    npc: bool,
    component: Option<f64>,
    mode: TraceMode,
) -> kobayashi::combat::SimulationResult {
    let attacker = Combatant {
        id: "monaveen".into(),
        hull_health: 1e12,
        shield_health: 0.0,
        crit_chance: 0.0,
        weapons: vec![WeaponStats {
            attack: 2000.0,
            component_base_attack: component,
            shots: Some(1),
            crit_chance: Some(0.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        id: "target".into(),
        hull_health: 1e12,
        shield_health: 0.0,
        attack: 0.0,
        mitigation: 0.0,
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 17,
        trace_mode: mode,
        defender_hostile_tag_mask: mask,
        ..Default::default()
    };
    let crew = CrewConfiguration {
        seats: vec![ship_ability_to_crew_seat_context(&ability(54)).unwrap()],
    };
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
        &CrewConfiguration { seats: vec![] },
    );
    simulate_combat_from_setup(&setup, 17)
}

#[test]
fn bonus_adds_component_base_once_and_respects_texas_and_npc_gates() {
    let active = first_attack(
        HOSTILE_TAG_MASK_TEXAS_CLASS,
        true,
        Some(100.0),
        TraceMode::Events,
    );
    let roll = active
        .events
        .iter()
        .find(|e| e.event_type == "attack_roll")
        .unwrap();
    assert_eq!(roll.values["effective_attack"], 239500.0);
    for (mask, npc, component) in [
        (0, true, Some(100.0)),
        (HOSTILE_TAG_MASK_TEXAS_CLASS, false, Some(100.0)),
        (HOSTILE_TAG_MASK_TEXAS_CLASS, true, None),
    ] {
        let result = first_attack(mask, npc, component, TraceMode::Events);
        assert_eq!(
            result
                .events
                .iter()
                .find(|e| e.event_type == "attack_roll")
                .unwrap()
                .values["effective_attack"],
            2000.0
        );
    }
    let off = first_attack(
        HOSTILE_TAG_MASK_TEXAS_CLASS,
        true,
        Some(100.0),
        TraceMode::Off,
    );
    assert_eq!(active.total_damage, off.total_damage);
    assert_eq!(active.attacker_hull_remaining, off.attacker_hull_remaining);
}
