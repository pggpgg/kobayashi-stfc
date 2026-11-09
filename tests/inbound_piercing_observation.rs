//! The independent no-Tom Jem'Hadar control measured mitigation with raw
//! defense and the firing weapon's piercing (see the observation fixture).
use kobayashi::combat::{
    simulate_combat, AttackerStats, Combatant, CrewConfiguration, DefenderStats,
    HostileMitigationParams, ShipType, SimulationConfig, TraceMode, WeaponStats,
};

#[test]
fn sphere_suppressor_csv_clean_hit_partitions_raw_damage() {
    // Sanitized combat columns from the first noncritical incoming hit.
    // CSV SHA-256: ce4169f010ece3b1ec8529e70eaac27e89971186697af39b27b22bad3c3340db.
    let raw = 9_431_434.0;
    let hull = 78_182.0;
    let shield = 368_571.0;
    let regular_mitigation = 6_715_181.0;
    let apex = 2_269_500.0;
    assert_eq!(raw, hull + shield + regular_mitigation + apex);
    assert!((regular_mitigation / raw - 0.712_f64).abs() < 1e-6);
}

#[test]
fn raw_inbound_piercing_is_not_added_a_second_time_after_mitigation() {
    let player = Combatant {
        id: "player".into(),
        attack: 1.0,
        hull_health: 100_000.0,
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: true,
            defender_stats: DefenderStats {
                armor: 125.0,
                shield_deflection: 963.0,
                dodge: 42.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 803_768.0,
                shield_piercing: 1_298_802.0,
                accuracy: 765_496.0,
            },
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Explorer,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let enemy = Combatant {
        id: "hostile".into(),
        hull_health: 100_000.0,
        weapons: vec![WeaponStats {
            attack: 2_000.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    };
    let result = simulate_combat(
        &player,
        &enemy,
        &SimulationConfig {
            rounds: 1,
            trace_mode: TraceMode::Off,
            ..Default::default()
        },
        &CrewConfiguration::default(),
    );
    let applied_fraction = (player.hull_health - result.attacker_hull_remaining) / 2_000.0;
    // Captured clean-hit mitigation interval, derived from its damage ledger;
    // applying a separate 25%-of-through pierce term misses by about 0.21.
    assert!(
        (1.0 - 0.1617052216..=1.0 - 0.1617042808).contains(&applied_fraction),
        "unexpected inbound damage-through fraction {applied_fraction}"
    );
}
