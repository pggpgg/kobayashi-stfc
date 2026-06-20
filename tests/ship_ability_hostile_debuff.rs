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
        attacker_hyperthermic_decay_fraction: 0.0,
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

// ── Breach-gated cumulative crit hull abilities (Hegh'ta / Rotarran) ──

/// A multi-shot attacker that cannot crit on its own (crit_chance 0). Paired with a deterministic
/// hull-breach proc so the breach-gated crit abilities have something to ramp against.
fn multishot_breacher() -> Combatant {
    Combatant {
        attack: 400.0,
        crit_chance: 0.0,
        crit_multiplier: 2.0,
        weapons: vec![WeaponStats {
            attack: 400.0,
            shots: Some(8),
            ..Default::default()
        }],
        ..weak_attacker()
    }
}

fn breach_seat() -> CrewSeatContext {
    CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: "breach_proc".into(),
            class: AbilityClass::ShipAbility,
            timing: TimingWindow::AttackPhase,
            boostable: false,
            effect: AbilityEffect::HullBreach {
                chance: 1.0,
                duration_rounds: 10,
                requires_critical: false,
            },
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: kobayashi::combat::NO_EXPLICIT_CONTRIBUTION_BATCH,
    }
}

fn ship_seat(name: &str, effect: AbilityEffect) -> CrewSeatContext {
    CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: name.into(),
            class: AbilityClass::ShipAbility,
            timing: TimingWindow::AttackPhase,
            boostable: false,
            effect,
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: kobayashi::combat::NO_EXPLICIT_CONTRIBUTION_BATCH,
    }
}

#[test]
fn breach_cumulative_crit_chance_increases_damage_only_while_breached() {
    let attacker = multishot_breacher();
    let defender = Combatant {
        hull_health: 5_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        crit_chance: 0.0,
        attack: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: None,
            ..Default::default()
        }],
        ..high_crit_defender()
    };
    let config = default_config(4);

    // Breach only (Hegh'ta ability absent) — attacker never crits on its own.
    let breach_only = CrewConfiguration {
        seats: vec![breach_seat()],
    };
    let base = simulate_combat(&attacker, &defender, &config, &breach_only);

    // Breach + Hegh'ta "Open the Wound": per-hit crit chance ramps while breached, so crits land.
    let with_ability = CrewConfiguration {
        seats: vec![
            breach_seat(),
            ship_seat(
                "3432906971",
                AbilityEffect::BreachCumulativeCritChancePerHit(0.05),
            ),
        ],
    };
    let ramped = simulate_combat(&attacker, &defender, &config, &with_ability);

    assert!(
        ramped.total_damage > base.total_damage,
        "per-hit crit-chance ramp should land crits and raise damage; base={} ramped={}",
        base.total_damage,
        ramped.total_damage
    );
}

#[test]
fn breach_cumulative_crit_chance_inert_without_breach() {
    // No breach proc: the per-hit crit chance ability must contribute nothing (it is gated on the
    // opponent being hull breached), so damage matches a plain crew.
    let attacker = multishot_breacher();
    let defender = Combatant {
        hull_health: 5_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        crit_chance: 0.0,
        attack: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: None,
            ..Default::default()
        }],
        ..high_crit_defender()
    };
    let config = default_config(4);

    let plain = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    let with_ability = CrewConfiguration {
        seats: vec![ship_seat(
            "3432906971",
            AbilityEffect::BreachCumulativeCritChancePerHit(0.2),
        )],
    };
    let gated = simulate_combat(&attacker, &defender, &config, &with_ability);

    assert!(
        (gated.total_damage - plain.total_damage).abs() < 1e-6,
        "crit-chance ability must be inert without hull breach; plain={} gated={}",
        plain.total_damage,
        gated.total_damage
    );
}

#[test]
fn breach_cumulative_crit_damage_increases_damage_while_breached() {
    // Attacker always crits (crit_chance 1.0), so Rotarran's per-crit crit-damage ramp compounds.
    let attacker = Combatant {
        crit_chance: 1.0,
        ..multishot_breacher()
    };
    let defender = Combatant {
        hull_health: 50_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        crit_chance: 0.0,
        attack: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: None,
            ..Default::default()
        }],
        ..high_crit_defender()
    };
    let config = default_config(4);

    let breach_only = CrewConfiguration {
        seats: vec![breach_seat()],
    };
    let base = simulate_combat(&attacker, &defender, &config, &breach_only);

    let with_ability = CrewConfiguration {
        seats: vec![
            breach_seat(),
            ship_seat(
                "2195955652",
                AbilityEffect::BreachCumulativeCritDamagePerCrit(0.1),
            ),
        ],
    };
    let ramped = simulate_combat(&attacker, &defender, &config, &with_ability);

    assert!(
        ramped.total_damage > base.total_damage,
        "per-crit crit-damage ramp should raise total damage; base={} ramped={}",
        base.total_damage,
        ramped.total_damage
    );
}
