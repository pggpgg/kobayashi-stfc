//! Track D2: ship hull abilities that debuff hostiles or buff the player vs hostiles.

use kobayashi::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, SimulationConfig, TimingWindow, TraceMode, WeaponStats,
};

fn high_crit_defender() -> Combatant {
    Combatant {
        id: "defender".into(),
        attack: 200.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.5,
        crit_chance: 1.0,
        crit_multiplier: 2.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 100_000.0,
        shield_health: 50_000.0,
        shield_mitigation: 0.5,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 200.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn weak_attacker() -> Combatant {
    Combatant {
        id: "attacker".into(),
        attack: 1.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 80_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 1.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn default_config(rounds: u32) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed: 42,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: kobayashi::combat::OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        emit_state_snapshots: false,
    }
}

#[test]
fn hostile_counter_stat_debuff_preserves_more_attacker_hull_than_plain() {
    let attacker = weak_attacker();
    let defender = high_crit_defender();
    let config = default_config(3);
    let plain = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: "701705952".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileCounterStatDebuff {
                    reduction: 0.5,
                    duration_rounds: 5,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: kobayashi::combat::NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let debuffed = simulate_combat(&attacker, &defender, &config, &crew);
    assert!(
        debuffed.attacker_hull_remaining > plain.attacker_hull_remaining,
        "counter pierce debuff should reduce hostile crit damage; plain={} debuffed={}",
        plain.attacker_hull_remaining,
        debuffed.attacker_hull_remaining
    );
}

#[test]
fn defender_shield_drain_per_round_reduces_defender_shields() {
    let attacker = Combatant {
        attack: 5000.0,
        pierce: 0.9,
        weapons: vec![WeaponStats {
            attack: 5000.0,
            shots: None,
            ..Default::default()
        }],
        ..weak_attacker()
    };
    let defender = high_crit_defender();
    let config = default_config(2);
    let plain = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: "1379978713".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::DefenderShieldDrainPerRound {
                    fraction: 0.25,
                    duration_rounds: 5,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: kobayashi::combat::NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let drained = simulate_combat(&attacker, &defender, &config, &crew);
    assert!(
        drained.defender_shield_remaining < plain.defender_shield_remaining,
        "Sanctus-style drain should leave fewer defender shields; plain={} drained={}",
        plain.defender_shield_remaining,
        drained.defender_shield_remaining
    );
}

#[test]
fn hostile_engagement_defensive_preserves_more_attacker_hull() {
    let attacker = weak_attacker();
    let defender = high_crit_defender();
    let config = default_config(3);
    let plain = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: "1463338054".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileEngagementDefensiveBonus(0.5),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: kobayashi::combat::NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let buffed = simulate_combat(&attacker, &defender, &config, &crew);
    assert!(
        buffed.attacker_hull_remaining > plain.attacker_hull_remaining,
        "Intrepid-style defensive bonus should mitigate counter-fire; plain={} buffed={}",
        plain.attacker_hull_remaining,
        buffed.attacker_hull_remaining
    );
}
