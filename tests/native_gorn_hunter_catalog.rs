//! Public native supplement through the production registry and full hostile catalog.

use kobayashi::combat::types::WeaponType;
use kobayashi::combat::{AbilityEffect, DefenderStats, ShipType, TimingWindow};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};

#[test]
fn native_gorn_hunter_registry_preserves_component_rows_and_only_verified_effects() {
    let registry = DataRegistry::load().expect("public static registry");
    let record = registry
        .resolve_hostile("2978535446")
        .expect("native Hunter record");
    assert_eq!(record.hostile_name, "V'ger Gorn Hunter");
    assert_eq!(record.level, 62);
    // These are retained native float32 component values, not substituted
    // integer HP observed in a private battle journal.
    assert_eq!(record.hull_health, 12_048_861_184.0);
    assert_eq!(record.shield_health, 22_032_201_728.0);
    assert_eq!(record.armor, 516_380.0);
    assert_eq!(record.shield_deflection, 1_824_542.0);
    assert_eq!(record.dodge, 206_552.0);
    assert_eq!(record.apex_barrier, 0.0);
    assert_eq!(record.isolytic_defense, 0.0);

    let mut components: Vec<_> = record
        .components
        .iter()
        .filter(|c| c["data"]["tag"] == "Weapon")
        .collect();
    components.sort_by_key(|c| c["order"].as_u64().unwrap());
    assert_eq!(
        components
            .iter()
            .map(|c| c["id"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![
            3_804_869_128,
            3_804_869_128,
            3_804_869_128,
            3_804_869_128,
            404_883_461,
            404_883_461
        ]
    );
    assert_eq!(
        components
            .iter()
            .map(|c| c["order"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![7, 8, 9, 10, 11, 12]
    );
    let weapons = record.weapons_from_components();
    let raw = record.counter_weapon_attacker_stats();
    assert_eq!(weapons.len(), 6);
    assert_eq!(raw.len(), 6);
    assert_eq!(record.crit_damage_is_bonus, Some(true));
    assert_eq!(record.combat_crit_multiplier(), 2.5);
    let counter_weapons = record.weapons_for_counter_attack(
        ShipType::Explorer,
        DefenderStats {
            armor: 100.0,
            shield_deflection: 100.0,
            dodge: 100.0,
        },
    );
    for (index, (weapon, stats)) in weapons.iter().zip(&raw).enumerate() {
        let kinetic = index < 4;
        assert_eq!(
            weapon.weapon_type,
            if kinetic {
                WeaponType::Kinetic
            } else {
                WeaponType::Energy
            }
        );
        assert_eq!(
            weapon.attack,
            if kinetic { 14_975_012.5 } else { 6_540_810.0 }
        );
        assert_eq!(weapon.shots, Some(2));
        assert_eq!(weapon.crit_chance, Some(0.009999999776482582));
        assert_eq!(
            weapon.crit_multiplier,
            Some(2.5),
            "native1.5 is a bonus fraction"
        );
        assert_eq!(stats.armor_piercing, 58_523.0);
        assert_eq!(
            stats.shield_piercing,
            if kinetic { 585_230.0 } else { 58_523.0 }
        );
        assert_eq!(stats.accuracy, if kinetic { 58_523.0 } else { 585_230.0 });
        assert_eq!(
            counter_weapons[index].pierce,
            Some(0.0),
            "raw stats must not add damage-through"
        );
    }

    let catalog =
        hostile_ability_catalog_for_default_path().expect("full maintained hostile catalog");
    let crew = hostile_abilities_to_defender_crew(&record.ability, Some(catalog), record.level);
    assert_eq!(
        crew.seats.len(),
        3,
        "unknown passive/activated rows must not add combat effects"
    );
    let mut ids: Vec<_> = crew.seats.iter().map(|s| s.ability.name.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["1761765323", "4173861882", "4196543321"]);
    for seat in &crew.seats {
        assert!(!seat.ability.boostable);
        assert!(seat.ability.condition.is_none());
        match seat.ability.name.as_str() {
            "1761765323" => {
                assert_eq!(seat.ability.timing, TimingWindow::CombatBegin);
                assert_eq!(
                    seat.ability.effect,
                    AbilityEffect::HostileIsolyticVulnerability
                );
            }
            "4196543321" => {
                assert_eq!(seat.ability.timing, TimingWindow::CombatBegin);
                assert_eq!(
                    seat.ability.effect,
                    AbilityEffect::IsolyticDamageBonus(14.91)
                );
            }
            "4173861882" => {
                assert_eq!(seat.ability.timing, TimingWindow::RoundStart);
                assert_eq!(
                    seat.ability.effect,
                    AbilityEffect::CumulativeOpponentShieldMitigationDebuff {
                        per_round: 0.01,
                        cap: f64::MAX,
                    }
                );
            }
            id => panic!("unexpected modeled Hunter source {id}"),
        }
    }
}
