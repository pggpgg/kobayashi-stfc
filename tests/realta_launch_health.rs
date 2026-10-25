use std::{collections::HashMap, path::Path};

use kobayashi::{
    combat::Combatant,
    data::{
        profile::{apply_profile_to_attacker, OfficerStatRuntimeBonus, PlayerProfile},
        ship::load_extended_ship_record,
    },
};

#[test]
fn realta_ship_bonus_adds_level_selected_shield_points_before_shared_hp_pool() {
    let extended = load_extended_ship_record(Path::new("data/ships_extended"), "realta").unwrap();
    let low = extended.to_ship_record(Some(4), Some(1)).unwrap();
    let high = extended.to_ship_record(Some(4), Some(20)).unwrap();
    assert!((low.health_with_launch_bonuses().1 - low.shield_health - 10.0).abs() < 1e-9);
    assert!((high.health_with_launch_bonuses().1 - high.shield_health - 110.0).abs() < 1e-9);
    let (hull, shield) = high.health_with_launch_bonuses();
    let profile = PlayerProfile {
        bonuses: HashMap::from([
            ("hull_hp_flat".into(), 3400.0),
            ("shield_hp_flat".into(), 4000.0),
            ("hull_hp".into(), 54.9 + 75.15 + 49.15 + 1.0),
            ("shield_hp".into(), 59.9 + 65.15 + 45.12 + 1.0),
        ]),
        ..Default::default()
    };
    // Independently reconstructed native pools from the historical journal.
    // No final-stat override or fitted correction is used.
    let compiled = apply_profile_to_attacker(
        Combatant {
            hull_health: hull,
            shield_health: shield,
            ..Default::default()
        },
        &profile,
        None,
        OfficerStatRuntimeBonus {
            health_bonus: 1.0,
            ..Default::default()
        },
    );
    assert!((compiled.hull_health - 652159.392).abs() < 1e-7);
    assert!((compiled.shield_health - 748506.5446).abs() < 1e-7);
    assert_eq!(compiled.hull_health.round(), 652159.0);
    assert_eq!(compiled.shield_health.round(), 748507.0);
}
