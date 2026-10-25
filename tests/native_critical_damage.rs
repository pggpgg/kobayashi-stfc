//! Native source units are converted at the data boundary, never on Combatant itself.

use kobayashi::combat::Combatant;
use kobayashi::data::hostile::{load_hostile_record, HostileRecord};
use kobayashi::data::ship::{load_extended_ship_record, ShipRecord, WeaponRecord};
use std::path::Path;

#[test]
fn native_ship_scalar_and_weapon_critical_bonus_convert_once() {
    let extended = load_extended_ship_record(Path::new("data/ships_extended"), "realta")
        .expect("native Realta fixture");
    assert_eq!(extended.crit_damage_is_bonus, Some(true));
    let ship = extended.to_ship_record(Some(4), Some(20)).unwrap();
    assert_eq!(ship.crit_damage, 1.5, "retain the native display value");
    assert_eq!(ship.combat_crit_multiplier(), 2.5);
    assert_eq!(ship.to_weapons()[0].crit_multiplier, Some(2.5));
    assert_eq!(
        ship.to_weapons()[0].crit_multiplier,
        Some(2.5),
        "projection is pure"
    );
    let compiled = kobayashi::data::profile::apply_static_buffs_to_combatant(
        Combatant {
            crit_multiplier: ship.combat_crit_multiplier(),
            weapons: ship.to_weapons(),
            ..Default::default()
        },
        &std::collections::HashMap::from([("crit_damage_flat".to_string(), 2.14)]),
    );
    assert!((compiled.crit_multiplier - 4.64).abs() < 1e-12);
    assert!((compiled.weapon_crit_multiplier(0) - 4.64).abs() < 1e-12);

    let legacy = ShipRecord {
        crit_damage: 1.5,
        weapons: Some(vec![WeaponRecord {
            attack: 10.0,
            crit_multiplier: Some(1.5),
            ..Default::default()
        }]),
        ..Default::default()
    };
    assert_eq!(legacy.combat_crit_multiplier(), 1.5);
    assert_eq!(legacy.to_weapons()[0].crit_multiplier, Some(1.5));
    let synthetic = Combatant {
        crit_multiplier: 1.5,
        ..Default::default()
    };
    assert_eq!(synthetic.weapon_crit_multiplier(0), 1.5);
}

#[test]
fn native_hostile_critical_bonus_converts_scalar_and_components() {
    let hostile = load_hostile_record(Path::new("data/hostiles"), "898210950")
        .expect("native Scouting Bandit fixture");
    assert_eq!(hostile.crit_damage_is_bonus, Some(true));
    assert_eq!(hostile.crit_damage, 1.5);
    assert_eq!(hostile.combat_crit_multiplier(), 2.5);
    assert!(hostile
        .weapons_from_components()
        .iter()
        .all(|weapon| weapon.crit_multiplier == Some(2.5)));
    assert!(hostile
        .weapons_for_counter_attack(
            kobayashi::combat::ShipType::Explorer,
            kobayashi::combat::DefenderStats {
                armor: 1.0,
                shield_deflection: 1.0,
                dodge: 1.0
            }
        )
        .iter()
        .all(|weapon| weapon.crit_multiplier == Some(2.5)));

    let mut manual: HostileRecord =
        serde_json::from_str(&std::fs::read_to_string("data/hostiles/898210950.json").unwrap())
            .unwrap();
    assert_eq!(
        manual.crit_damage_is_bonus, None,
        "bare serde has no source inference"
    );
    assert_eq!(manual.combat_crit_multiplier(), 1.5);
    manual.crit_damage_is_bonus = Some(false);
    assert_eq!(manual.combat_crit_multiplier(), 1.5);
}

#[test]
fn explicit_legacy_units_override_known_native_source_membership() {
    let temporary = std::env::temp_dir().join(format!(
        "kobayashi-native-critical-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&temporary).unwrap();
    for (directory, id) in [
        ("data/hostiles", "898210950"),
        ("data/ships_extended", "realta"),
    ] {
        let mut json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{directory}/{id}.json")).unwrap(),
        )
        .unwrap();
        json["crit_damage_is_bonus"] = serde_json::json!(false);
        std::fs::write(
            temporary.join(format!("{id}.json")),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
    }
    let hostile = load_hostile_record(&temporary, "898210950").unwrap();
    assert_eq!(hostile.combat_crit_multiplier(), 1.5);
    let ship = load_extended_ship_record(&temporary, "realta")
        .unwrap()
        .to_ship_record(Some(4), Some(20))
        .unwrap();
    assert_eq!(ship.combat_crit_multiplier(), 1.5);
    assert_eq!(ship.to_weapons()[0].crit_multiplier, Some(1.5));
    std::fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn zero_native_critical_bonus_keeps_the_noncritical_damage_factor() {
    let ship = ShipRecord {
        crit_damage: 0.0,
        crit_damage_is_bonus: Some(true),
        weapons: Some(vec![WeaponRecord {
            attack: 10.0,
            crit_multiplier: Some(0.0),
            ..Default::default()
        }]),
        ..Default::default()
    };
    assert_eq!(ship.combat_crit_multiplier(), 1.0);
    assert_eq!(ship.to_weapons()[0].crit_multiplier, Some(1.0));
    let mut hostile = load_hostile_record(Path::new("data/hostiles"), "898210950").unwrap();
    hostile.crit_damage = 0.0;
    for component in &mut hostile.components {
        if component["data"]["tag"] == "Weapon" {
            component["data"]["crit_modifier"] = serde_json::json!(0.0);
        }
    }
    assert_eq!(hostile.combat_crit_multiplier(), 1.0);
    assert!(hostile
        .weapons_from_components()
        .iter()
        .all(|weapon| weapon.crit_multiplier == Some(1.0)));
}
