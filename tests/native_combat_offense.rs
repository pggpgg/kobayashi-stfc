//! Native offensive account buffs retain their units, phase and opponent gates.
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TimingWindow, TraceMode,
};
use kobayashi::data::{import::GlobalBuffEntry, native_consumable_buffs};

fn entry(bid: i64, expiry_time: Option<i64>) -> GlobalBuffEntry {
    GlobalBuffEntry {
        bid,
        level: 1,
        expiry_time,
    }
}

#[test]
fn native_offense_is_additive_scoped_and_stable_across_rounds() {
    let rows = [entry(44509805, None), entry(907388503, None)];
    let mut crew = CrewConfiguration {
        seats: native_consumable_buffs::combat_start_offense_seats_for_imported_buffs(&rows, 100),
    };
    assert_eq!(crew.seats.len(), 2);
    crew.seats.push(CrewSeatContext::legacy(
        CrewSeat::Bridge,
        Ability {
            name: "independent_critical_multiplier".into(),
            class: AbilityClass::BridgeAbility,
            timing: TimingWindow::CombatBegin,
            boostable: false,
            effect: AbilityEffect::CritDamageMultiplier(1.5),
            condition: None,
            weapon_scope: Default::default(),
        },
        false,
    ));
    let attacker = Combatant {
        attack: 300.0, // base 100 with an existing +200% profile pool
        hull_health: 1e8,
        crit_chance: 1.0,
        crit_multiplier: 2.5,
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: 1e8,
        ..Default::default()
    };
    for (opponent, pvp, weapon_bonus, crit_bonus) in [
        (ShipType::Explorer, false, 0.4, 0.03),
        (ShipType::Armada, false, 0.0, 0.0),
        (ShipType::Explorer, true, 0.0, 0.0),
    ] {
        let config = SimulationConfig {
            rounds: 3,
            seed: 77,
            trace_mode: TraceMode::Events,
            weapon_damage_profile_additive_pool: Some(2.0),
            profile_weapon_damage_fraction: 2.0,
            ..Default::default()
        };
        let run = || {
            simulate_combat_with_defender_faction_and_defender_crew(
                &attacker,
                &defender,
                &config,
                &crew,
                OpponentFactionTag::Unknown,
                opponent,
                ShipType::Explorer,
                !pvp,
                pvp,
                &CrewConfiguration::default(),
            )
        };
        let result = run();
        assert_eq!(
            result,
            run(),
            "complete input and seed must remain deterministic"
        );
        let attacks: Vec<_> = result
            .events
            .iter()
            .filter(|e| e.event_type == "attack_roll" && e.phase == "attack")
            .collect();
        let criticals: Vec<_> = result
            .events
            .iter()
            .filter(|e| e.event_type == "crit_resolution" && e.phase == "attack")
            .collect();
        assert_eq!(attacks.len(), 3);
        assert_eq!(criticals.len(), 3);
        for attack in attacks {
            assert!(
                (attack.values["effective_attack"].as_f64().unwrap()
                    - 100.0 * (3.0 + weapon_bonus))
                    .abs()
                    < 1e-6
            );
        }
        for critical in criticals {
            assert!(
                (critical.values["multiplier"].as_f64().unwrap() - (2.5 + crit_bonus) * 1.5).abs()
                    < 1e-6
            );
        }
    }
}

#[test]
fn expired_absent_and_duplicate_sources_do_not_add_offense() {
    assert!(
        native_consumable_buffs::combat_start_offense_seats_for_imported_buffs(&[], 100).is_empty()
    );
    let rows = [
        entry(44509805, Some(100)),
        entry(907388503, None),
        entry(907388503, None),
    ];
    let seats = native_consumable_buffs::combat_start_offense_seats_for_imported_buffs(&rows, 100);
    assert_eq!(seats.len(), 1);
    assert_eq!(
        seats[0].ability.effect,
        AbilityEffect::BaseCritDamageBonus(0.03)
    );
    assert!(
        native_consumable_buffs::bonuses_for_imported_buffs(&rows, Some("explorer"), 100)
            .is_empty(),
        "combat-start sources must not enter launch pools too"
    );
}
