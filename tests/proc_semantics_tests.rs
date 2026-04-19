//! Integration checks for weapon intrinsic proc **per hit** (see `src/combat/proc.rs`).

use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode, WeaponStats,
};

#[test]
fn weapon_intrinsic_proc_triggers_once_per_outbound_hit() {
    let crew = CrewConfiguration::default();
    let attacker = Combatant {
        id: "a".into(),
        attack: 50.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.25,
        proc_multiplier: 1.5,
        end_of_round_damage: 0.0,
        hull_health: 10_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 50.0,
            shots: Some(3),
            ..Default::default()
        }],
    };
    let defender = Combatant {
        id: "d".into(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
    };
    let r = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            seed: 20260411,
            trace_mode: TraceMode::Events,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Default::default(),
        },
        &crew,
    );
    let n = r
        .events
        .iter()
        .filter(|e| e.event_type == "proc_triggers" && e.round_index == 1)
        .count();
    assert_eq!(
        n, 3,
        "three outbound hits → three intrinsic proc rolls (see combat::proc)"
    );
}
