//! Component-base damage bonuses must not multiply an already scaled weapon.
use kobayashi::combat::{
    abilities::AbilityCondition, hostile_tags::HOSTILE_TAG_MASK_CONQUEROR_BORG, simulate_combat,
    Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat, CrewSeatContext,
    SimulationConfig, TimingWindow, TraceMode, WeaponStats,
};

#[test]
fn component_bonus_uses_raw_weapon_base_and_respects_opponent_gate() {
    let attacker = Combatant {
        hull_health: 1e8,
        weapons: vec![WeaponStats {
            attack: 300.0, // 100 component base with pre-existing profile/officer buffs
            component_base_attack: Some(100.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e12,
        ..Default::default()
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::Ship,
            Ability {
                name: "component bonus".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ComponentBaseWeaponDamageBonus(2.0),
                condition: Some(AbilityCondition::DefenderHostileTagsAllPresent {
                    required_mask: HOSTILE_TAG_MASK_CONQUEROR_BORG,
                }),
                weapon_scope: Default::default(),
            },
            false,
        )],
    };
    for (tag, raw, expected) in [
        (HOSTILE_TAG_MASK_CONQUEROR_BORG, Some(100.0), 500.0),
        (0, Some(100.0), 300.0),
        (HOSTILE_TAG_MASK_CONQUEROR_BORG, None, 300.0),
    ] {
        let mut a = attacker.clone();
        a.weapons[0].component_base_attack = raw;
        let config = SimulationConfig {
            rounds: 1,
            seed: 42,
            trace_mode: TraceMode::Events,
            defender_hostile_tag_mask: tag,
            ..Default::default()
        };
        let result = simulate_combat(&a, &defender, &config, &crew);
        assert_eq!(result, simulate_combat(&a, &defender, &config, &crew));
        let attack = result
            .events
            .iter()
            .find(|e| e.event_type == "attack_roll" && e.phase == "attack")
            .unwrap();
        assert!((attack.values["effective_attack"].as_f64().unwrap() - expected).abs() < 1e-8);
    }
}

#[test]
fn profile_scaling_preserves_component_base() {
    use kobayashi::data::profile::{
        apply_profile_to_attacker, OfficerStatRuntimeBonus, PlayerProfile,
    };
    let attacker = Combatant {
        weapons: vec![WeaponStats {
            attack: 100.0,
            component_base_attack: Some(100.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut profile = PlayerProfile::default();
    profile.bonuses.insert("weapon_damage".into(), 2.0);
    let scaled =
        apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
    assert_eq!(scaled.weapons[0].attack, 300.0);
    assert_eq!(scaled.weapons[0].component_base_attack, Some(100.0));
}
