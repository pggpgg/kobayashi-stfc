//! Data-driven guard for the Track D2 hostile-debuff hull abilities (Quv'Sompek, Sanctus, B'Rel,
//! U.S.S. Intrepid). These resolve out of the real `data/ships_extended` records, so this catches
//! two failure modes that previously left them dormant (see SHIP_ABILITY_COMBAT_NOOP_AUDIT.md §6.1):
//!   1. `ships_extended` not regenerated after a catalog change (effect stays `combat_noop`).
//!   2. `value_is_percentage` mis-set so an already-fractional value is scaled ×0.01 (100× too small).
//!
//! The lower bound on the resolved value guards specifically against (2).

use kobayashi::combat::{
    mitigation, simulate_combat_with_defender_faction_and_defender_crew, AbilityEffect,
    AttackerStats, Combatant, CrewConfiguration, CrewSeatContext, DefenderStats,
    HostileMitigationParams, OpponentFactionTag, ShipType, SimulationConfig, TraceMode,
    WeaponStats,
};
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

fn seat_for(ship: &str, ability_id: &str, level: u32) -> CrewSeatContext {
    let record = resolve_ship_with_tier_level(ship, Some(9), Some(level))
        .unwrap_or_else(|| panic!("ship {ship} should resolve from data/ships_extended"));
    let ability = record
        .abilities
        .iter()
        .flatten()
        .find(|a| a.id == ability_id)
        .unwrap_or_else(|| panic!("ship {ship} should carry ability {ability_id}"));
    ship_ability_to_crew_seat_context(ability)
        .unwrap_or_else(|| panic!("ability {ability_id} should resolve to a seat context"))
}

fn effect_for(ship: &str, ability_id: &str) -> AbilityEffect {
    seat_for(ship, ability_id, 90).ability.effect
}

/// A resolved debuff/bonus fraction should be a meaningful percentage (>= 5%), not the ~0.1%
/// that a double-applied `value_is_percentage` would produce.
fn assert_meaningful(label: &str, v: f64) {
    assert!(
        v >= 0.05,
        "{label} resolved to {v}; expected a meaningful fraction (>= 0.05). \
         A value near 0.001 means value_is_percentage double-scaled an already-fractional row."
    );
}

#[test]
fn quv_sompek_counter_stat_debuff_is_meaningful() {
    match effect_for("quv_sompek", "701705952") {
        AbilityEffect::HostileCounterStatDebuff {
            reduction,
            duration_rounds,
        } => {
            assert_eq!(duration_rounds, 5);
            assert_meaningful("Quv'Sompek counter debuff", reduction);
        }
        other => panic!("expected HostileCounterStatDebuff, got {other:?}"),
    }
}

#[test]
fn sanctus_shield_drain_is_meaningful() {
    match effect_for("sanctus", "1379978713") {
        AbilityEffect::DefenderShieldDrainPerRound {
            fraction,
            duration_rounds,
        } => {
            assert_eq!(duration_rounds, 5);
            assert_meaningful("Sanctus shield drain", fraction);
        }
        other => panic!("expected DefenderShieldDrainPerRound, got {other:?}"),
    }
}

#[test]
fn b_rel_counter_stat_debuff_is_meaningful() {
    match effect_for("b_rel", "2441576367") {
        AbilityEffect::HostileCounterStatDebuff {
            reduction,
            duration_rounds,
        } => {
            assert_eq!(duration_rounds, 1);
            assert_meaningful("B'Rel counter debuff", reduction);
        }
        other => panic!("expected HostileCounterStatDebuff, got {other:?}"),
    }
}

#[test]
fn hostile_counter_stat_debuff_reduces_the_firing_weapons_raw_stats_only_during_its_window() {
    let defense = DefenderStats {
        armor: 820.0,
        shield_deflection: 580.0,
        dodge: 960.0,
    };
    let weapon_stats = [
        AttackerStats {
            armor_piercing: 720.0,
            shield_piercing: 520.0,
            accuracy: 480.0,
        },
        AttackerStats {
            armor_piercing: 320.0,
            shield_piercing: 930.0,
            accuracy: 710.0,
        },
    ];
    let player = Combatant {
        hull_health: 10_000.0,
        attack: 1.0,
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: defense,
            weapon_pierce_defender_stats: None,
            base_attacker_stats: weapon_stats[0],
            base_attacker_weapon_stats: weapon_stats.to_vec(),
            ship_type: ShipType::Explorer,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let opponent = Combatant {
        hull_health: 10_000.0,
        weapons: vec![
            WeaponStats {
                attack: 100.0,
                ..Default::default()
            };
            weapon_stats.len()
        ],
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 6,
        seed: 42,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let run = |crew: &CrewConfiguration, npc, config: &SimulationConfig| {
        simulate_combat_with_defender_faction_and_defender_crew(
            &player,
            &opponent,
            config,
            crew,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Explorer,
            npc,
            !npc,
            &CrewConfiguration::default(),
        )
    };
    for (ship, id, level, reduction, duration) in [
        ("b_rel", "2441576367", 1, 0.15, 1),
        ("b_rel", "2441576367", 45, 0.35, 1),
        ("quv_sompek", "701705952", 1, 0.12, 5),
    ] {
        let seat = seat_for(ship, id, level);
        let crew = CrewConfiguration { seats: vec![seat] };
        let result = run(&crew, true, &config);
        let plain = run(&CrewConfiguration::default(), true, &config);
        let raw_mitigations: Vec<_> = result
            .events
            .iter()
            .filter(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
            .collect();
        assert_eq!(
            raw_mitigations.len(),
            config.rounds as usize * weapon_stats.len()
        );
        for e in raw_mitigations {
            let stats = weapon_stats[e.weapon_index.unwrap() as usize];
            let factor = if e.round_index <= duration {
                1.0 - reduction
            } else {
                1.0
            };
            let expected = mitigation(
                defense,
                AttackerStats {
                    armor_piercing: stats.armor_piercing * factor,
                    shield_piercing: stats.shield_piercing * factor,
                    accuracy: stats.accuracy * factor,
                },
                ShipType::Explorer,
            );
            let actual = e.values["base_stat_mitigation_unrounded"].as_f64().unwrap();
            assert!(
                (expected - actual).abs() < 1e-12,
                "{ship}, level {level}, round {}, weapon {:?}: expected {expected}, got {actual}",
                e.round_index,
                e.weapon_index,
            );
        }
        assert!(result.attacker_hull_remaining > plain.attacker_hull_remaining);
        assert_eq!(
            result.defender_hull_remaining,
            plain.defender_hull_remaining
        );
        assert_eq!(
            run(&crew, false, &config),
            run(&CrewConfiguration::default(), false, &config)
        );
        let untraced = run(
            &crew,
            true,
            &SimulationConfig {
                trace_mode: TraceMode::Off,
                ..config.clone()
            },
        );
        assert_eq!(
            result.attacker_hull_remaining,
            untraced.attacker_hull_remaining
        );
    }
}

#[test]
fn counter_stat_debuff_hulls_require_an_npc_opponent() {
    for (ship, id) in [("b_rel", "2441576367"), ("quv_sompek", "701705952")] {
        assert_eq!(
            seat_for(ship, id, 1).ability.condition,
            Some(kobayashi::combat::AbilityCondition::DefenderIsNpcHostile),
            "{ship} must leave player piercing unchanged"
        );
    }
}

#[test]
fn intrepid_engagement_defensive_is_meaningful() {
    match effect_for("uss_intrepid", "1463338054") {
        AbilityEffect::HostileEngagementDefensiveBonus(v) => {
            assert_meaningful("Intrepid engagement defensive", v);
        }
        other => panic!("expected HostileEngagementDefensiveBonus, got {other:?}"),
    }
}
