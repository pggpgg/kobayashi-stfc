use kobayashi::data::import::{GlobalBuffEntry, ResearchEntry};
use kobayashi::data::native_research_buffs::research_bonuses_for_ship;
use kobayashi::data::profile::{
    merge_research_bonuses_into_profile, replace_research_setup_bonuses_with_native_snapshot,
    PlayerProfile,
};
use kobayashi::data::research::ResearchCatalog;
use std::collections::HashMap;

fn entry(bid: i64, level: i64) -> GlobalBuffEntry {
    GlobalBuffEntry {
        bid,
        level,
        expiry_time: None,
    }
}

#[test]
fn active_piercing_and_officer_sources_keep_native_ranks_and_scope() {
    let rows = [
        entry(949755120, 2),   // all piercing +0.6
        entry(3089064337, 2),  // Explorer accuracy +0.35
        entry(1994654654, 2),  // grade 4+ all piercing +0.24
        entry(1380237269, 2),  // crew-wide stats +0.3
        entry(1895868262, 10), // Orlop below-decks Defense +1
        entry(624709978, 10),  // unverified below-decks Attack must stay excluded
        entry(113240864, 2),   // unsupported grade gate on modifier 7
    ];
    let bonuses = research_bonuses_for_ship(&rows, Some("explorer"), Some(4), 100).unwrap();
    for (key, expected) in [
        ("accuracy", 1.19),
        ("armor_piercing", 0.84),
        ("shield_piercing", 0.84),
        ("officer_attack", 0.3),
        ("officer_defense", 0.3),
        ("officer_health", 0.3),
        ("officer_defense_below_decks", 1.0),
    ] {
        assert!(
            (bonuses[key] - expected).abs() < 1e-12,
            "{key}: {}",
            bonuses[key]
        );
    }
    for grade in [None, Some(3)] {
        let other = research_bonuses_for_ship(&rows, Some("interceptor"), grade, 100).unwrap();
        assert_eq!(other["accuracy"], 0.6);
        assert_eq!(other["armor_piercing"], 0.6);
        assert_eq!(other["officer_defense_below_decks"], 1.0);
    }
}

#[test]
fn active_source_replacement_preserves_manual_bonuses_and_excludes_inactive_research() {
    let keys = [
        "accuracy",
        "armor_piercing",
        "shield_piercing",
        "officer_attack",
        "officer_defense",
        "officer_health",
        "officer_defense_below_decks",
    ];
    let bonuses: Vec<_> = keys
        .iter()
        .map(|key| serde_json::json!({"stat":key,"value":2.0}))
        .collect();
    let catalog: ResearchCatalog = serde_json::from_value(serde_json::json!({"items":[{
        "rid":123, "level_value_mode":"snapshot", "levels":[{"level":1,"bonuses":bonuses}]
    }]}))
    .unwrap();
    let imported = [ResearchEntry { rid: 123, level: 1 }];
    let mut profile = PlayerProfile {
        bonuses: keys.iter().map(|k| (k.to_string(), 0.5)).collect(),
        ..Default::default()
    };
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    let native = research_bonuses_for_ship(&[entry(1380237269, 2)], None, None, 100).unwrap();
    replace_research_setup_bonuses_with_native_snapshot(
        &mut profile,
        &imported,
        &catalog,
        None,
        None,
        None,
        &native,
    );
    for key in keys {
        let expected = if ["officer_attack", "officer_defense", "officer_health"].contains(&key) {
            0.8
        } else {
            0.5
        };
        assert!((profile.bonuses[key] - expected).abs() < 1e-12, "{key}");
    }
    let expired = GlobalBuffEntry {
        expiry_time: Some(100),
        ..entry(1380237269, 2)
    };
    let absent =
        research_bonuses_for_ship(&[entry(434613423, 10), expired], None, None, 100).unwrap();
    assert!(!absent.contains_key("officer_attack"));
    let zero = research_bonuses_for_ship(&[entry(1895868262, 99)], None, None, 100).unwrap();
    assert_eq!(zero, HashMap::new(), "explicit trailing zero stays zero");
}

#[test]
fn fleet_commander_temporary_weapon_bonus_requires_a_current_active_buff() {
    use kobayashi::combat::{
        simulate_combat, Combatant, CrewConfiguration, SimulationConfig, WeaponStats,
    };
    use kobayashi::data::profile::{apply_profile_to_attacker, OfficerStatRuntimeBonus};

    let catalog: ResearchCatalog =
        serde_json::from_str(include_str!("../data/research_catalog.json")).unwrap();
    let owned = [
        ResearchEntry {
            rid: 4126790711,
            level: 1,
        }, // Lorca's Fist: +20.0 while active
        ResearchEntry {
            rid: 151318080,
            level: 1,
        }, // Weapon Intel: +0.65, always on
    ];
    let active = GlobalBuffEntry {
        expiry_time: Some(100),
        ..entry(251122208, 1)
    };
    let current = [active.clone(), entry(3616876360, 1)];

    for (buffs, now, expected) in [
        (&[][..], 99, 165.0),
        (std::slice::from_ref(&active), 100, 165.0),
        (&current[..], 99, 2165.0),
    ] {
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &owned, &catalog, None);
        if let Some(native) = research_bonuses_for_ship(buffs, None, None, now) {
            replace_research_setup_bonuses_with_native_snapshot(
                &mut profile,
                &owned,
                &catalog,
                None,
                None,
                None,
                &native,
            );
        }
        let attacker = apply_profile_to_attacker(
            Combatant {
                hull_health: 10_000.0,
                weapons: vec![WeaponStats {
                    attack: 100.0,
                    ..Default::default()
                }],
                ..Default::default()
            },
            &profile,
            None,
            OfficerStatRuntimeBonus::default(),
        );
        let defender = Combatant {
            hull_health: 10_000.0,
            ..Default::default()
        };
        let result = simulate_combat(
            &attacker,
            &defender,
            &SimulationConfig {
                rounds: 1,
                ..Default::default()
            },
            &CrewConfiguration::default(),
        );
        assert!(
            (result.total_damage - expected).abs() < 1e-9,
            "buffs={buffs:?}, now={now}: got {}, expected {expected}",
            result.total_damage,
        );
    }
}
