//! Xindi hostile ability catalog → combat integration (crit debuff, Kemocite, No Mercy, Ibix bypass).

use kobayashi::combat::abilities::AbilityEffect;
use kobayashi::combat::types::{OpponentFactionTag, ShipType};
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, CombatEvent, Combatant,
    CrewConfiguration, DefenderStats, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::hostile::HostileRecord;
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::loader::resolve_hostile;

fn tanky_attacker(high_crit: bool) -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 50_000.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 1.0,
        crit_chance: if high_crit { 1.0 } else { 0.0 },
        crit_multiplier: 2.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 500_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 50_000.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn weak_attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 0.0,
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
        hull_health: 100_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn cfg(rounds: u32, seed: u64) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
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
fn xindi_doomed_species_catalog_builds_crit_and_particle_beam_lethal_seats() {
    let rec = resolve_hostile("4012373729").expect("doomed species xindi");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileCritDamageReduction {
                    duration_rounds: 2,
                    reduction,
                    additive_percentage_points: true,
                    stacks: true,
                } if (reduction - 5.0).abs() < 1e-9
            )
        }),
        "expected round-start crit debuff seat"
    );
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileLethalEndOfRound {
                    round_interval: 1,
                    shots: 1,
                    prevent_when_defender_assimilated: false,
                }
            )
        }),
        "expected Xindi Weaponry particle-beam lethal extra seat"
    );
}

#[test]
fn be_like_water_catalog_has_no_lethal_extra_seat() {
    let rec = resolve_hostile("3988400401").expect("be like water only xindi");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileCritDamageReduction {
                    additive_percentage_points: true,
                    stacks: false,
                    ..
                }
            )
        }),
        "expected Be Like Water crit debuff seat"
    );
    assert!(
        !crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileLethalEndOfRound { .. }
            )
        }),
        "Xindi Might text must not add a catalog lethal seat"
    );
    assert!(
        !crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileDenticleBladeHeavyArtillery { .. }
            )
        }),
        "Be Like Water-only row must not add Denticle Blade seat"
    );
    let blw_reduction = crew
        .seats
        .iter()
        .find_map(|s| match s.ability.effect {
            AbilityEffect::HostileCritDamageReduction {
                reduction,
                stacks: false,
                ..
            } => Some(reduction),
            _ => None,
        })
        .expect("BLW reduction value");
    assert!(
        (blw_reduction - 25.0).abs() < 1e-9,
        "upstream value at L51 should be 25"
    );
}

#[test]
fn denticle_blade_catalog_builds_combat_begin_seat() {
    let rec = resolve_hostile("1043112405").expect("denticle xindi");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileDenticleBladeHeavyArtillery {
                    proc_chance,
                    weapon_index_one_based: 5,
                } if (proc_chance - 0.3).abs() < 1e-9
            )
        }),
        "expected Denticle Blade combat-begin seat"
    );
}

fn denticle_hostile_defender(rec: &HostileRecord) -> Combatant {
    let player_defender = DefenderStats::default();
    let weapons = rec.weapons_for_counter_attack(ShipType::Explorer, player_defender);
    Combatant {
        id: rec.id.clone(),
        attack: 0.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: rec.counter_pierce_damage_through_bonus(ShipType::Explorer, player_defender),
        crit_chance: rec.crit_chance.clamp(0.0, 1.0),
        crit_multiplier: rec.crit_damage.max(1.0),
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: rec.hull_health,
        shield_health: 0.0,
        shield_mitigation: rec.shield_mitigation.unwrap_or(0.8),
        apex_barrier: rec.apex_barrier,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: rec.isolytic_defense,
        weapons,
        hostile_mitigation_params: None,
    }
}

fn denticle_roll_triggered(events: &[CombatEvent]) -> Option<bool> {
    events
        .iter()
        .find(|e| e.event_type == "denticle_blade_roll")
        .and_then(|e| e.values.get("triggered").and_then(|v| v.as_bool()))
}

fn counter_fired_at_weapon_index(events: &[CombatEvent], weapon_index: u32) -> bool {
    events.iter().any(|e| {
        e.phase == "counter"
            && e.weapon_index == Some(weapon_index)
            && (e.event_type == "damage" || e.event_type == "mitigation_calc")
    })
}

#[test]
fn denticle_blade_gates_weapon_five_until_proc() {
    let rec = resolve_hostile("1043112405").expect("denticle xindi");
    let catalog = hostile_ability_catalog_for_default_path();
    let defender_crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let attacker = weak_attacker();
    let defender = denticle_hostile_defender(&rec);

    let mut fail_seed = None;
    let mut success_seed = None;
    for seed in 0..10_000u64 {
        let mut config = cfg(1, seed);
        config.trace_mode = TraceMode::Events;
        let result = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &config,
            &CrewConfiguration { seats: vec![] },
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Explorer,
            true,
            false,
            &defender_crew,
        );
        let triggered = denticle_roll_triggered(&result.events);
        let fired_w5 = counter_fired_at_weapon_index(&result.events, 4);
        if triggered == Some(false) && !fired_w5 {
            fail_seed = Some(seed);
        }
        if triggered == Some(true) && fired_w5 {
            success_seed = Some(seed);
        }
        if fail_seed.is_some() && success_seed.is_some() {
            break;
        }
    }
    assert!(
        fail_seed.is_some(),
        "expected a seed where Denticle proc fails and weapon 5 counter is skipped"
    );
    assert!(
        success_seed.is_some(),
        "expected a seed where Denticle proc succeeds and weapon 5 counter fires"
    );
}

#[test]
fn denticle_blade_fires_weapon_five_when_proc_succeeds() {
    let rec = resolve_hostile("1043112405").expect("denticle xindi");
    let catalog = hostile_ability_catalog_for_default_path();
    let defender_crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let attacker = weak_attacker();
    let defender = denticle_hostile_defender(&rec);

    let mut success_seed = None;
    for seed in 0..10_000u64 {
        let mut config = cfg(1, seed);
        config.trace_mode = TraceMode::Events;
        let result = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &config,
            &CrewConfiguration { seats: vec![] },
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Explorer,
            true,
            false,
            &defender_crew,
        );
        if denticle_roll_triggered(&result.events) == Some(true)
            && counter_fired_at_weapon_index(&result.events, 4)
        {
            success_seed = Some(seed);
            break;
        }
    }
    let seed = success_seed.expect("expected a proc-success seed within 10k");
    let mut config = cfg(1, seed);
    config.trace_mode = TraceMode::Events;
    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &defender_crew,
    );
    assert!(
        counter_fired_at_weapon_index(&result.events, 4),
        "weapon slot 5 counter should fire when Denticle proc succeeds"
    );
    assert!(
        result.attacker_hull_remaining < attacker.hull_health,
        "heavy artillery counter should damage attacker hull"
    );
}

#[test]
fn doomed_species_particle_beam_lethal_kills_at_round_one_end() {
    let rec = resolve_hostile("4012373729").expect("doomed species xindi");
    let catalog = hostile_ability_catalog_for_default_path();
    let defender_crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let attacker = weak_attacker();
    let defender = Combatant {
        id: "xindi".into(),
        attack: 0.0,
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(1, 7),
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &defender_crew,
    );
    assert!(
        result.attacker_hull_remaining <= 0.0,
        "particle-beam lethal round-end should zero attacker hull, got {}",
        result.attacker_hull_remaining
    );
}

#[test]
fn no_mercy_lethal_kills_at_round_eight() {
    let rec = resolve_hostile("2634260020").expect("xindi group armada");
    let catalog = hostile_ability_catalog_for_default_path();
    let defender_crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let attacker = weak_attacker();
    let defender = Combatant {
        id: "xindi_armada".into(),
        attack: 0.0,
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let round_seven = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(7, 7),
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &defender_crew,
    );
    let round_eight = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(8, 7),
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &defender_crew,
    );
    assert!(
        round_seven.attacker_hull_remaining > 0.0,
        "No Mercy should not fire before round 8"
    );
    assert!(
        round_eight.attacker_hull_remaining <= 0.0,
        "No Mercy lethal should zero attacker hull on round 8, got {}",
        round_eight.attacker_hull_remaining
    );
}

#[test]
fn no_mercy_lethal_is_not_undone_by_attacker_round_end_hull_regen() {
    use kobayashi::combat::abilities::{
        Ability, AbilityClass, AbilityEffect, CrewSeat, CrewSeatContext, TimingWindow,
        NO_EXPLICIT_CONTRIBUTION_BATCH,
    };

    let rec = resolve_hostile("2634260020").expect("xindi group armada");
    let catalog = hostile_ability_catalog_for_default_path();
    let defender_crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let attacker_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "test_regen".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::RoundEnd,
                boostable: false,
                effect: AbilityEffect::HullRegen(500_000.0),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let attacker = weak_attacker();
    let defender = Combatant {
        id: "xindi".into(),
        attack: 0.0,
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(8, 99),
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &defender_crew,
    );
    assert!(
        result.attacker_hull_remaining <= 0.0,
        "lethal must kill even with large round-end hull regen; remaining {}",
        result.attacker_hull_remaining
    );
}

#[test]
fn xindi_crit_debuff_reduces_player_crit_damage_on_outbound() {
    use kobayashi::combat::abilities::{
        Ability, AbilityClass, AbilityEffect, CrewSeat, CrewSeatContext, TimingWindow,
        NO_EXPLICIT_CONTRIBUTION_BATCH,
    };

    let crit_debuff_only = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "1271329828".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::HostileCritDamageReduction {
                    reduction: 5.0,
                    duration_rounds: 2,
                    additive_percentage_points: true,
                    stacks: true,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let attacker = tanky_attacker(true);
    let defender = Combatant {
        id: "xindi".into(),
        attack: 100.0,
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
        hull_health: 10_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let without = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(1, 11),
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    let with_debuff = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(1, 11),
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &crit_debuff_only,
    );
    assert!(
        with_debuff.total_damage < without.total_damage,
        "crit debuff should reduce outbound crit damage ({} vs {})",
        with_debuff.total_damage,
        without.total_damage
    );
}

#[test]
fn ibix_strength_catalog_builds_full_shield_bypass_seat() {
    let rec = resolve_hostile("1080638426").expect("xindi interceptor");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::ShieldMitigationBypassFraction(v) if (v - 1.0).abs() < 1e-9
            )
        }),
        "expected combat_begin 100% shield bypass seat from Strength of the Ibix"
    );
}

#[test]
fn ibix_shield_bypass_routes_counter_damage_to_attacker_hull() {
    use kobayashi::data::hostile_ability_resolve::HostileAbilityCatalog;

    let rec = resolve_hostile("1080638426").expect("xindi interceptor");
    let catalog = hostile_ability_catalog_for_default_path();
    let with_ibix = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let noop_catalog = HostileAbilityCatalog {
        description: None,
        entries: std::collections::HashMap::new(),
    };
    let without_ibix =
        hostile_abilities_to_defender_crew(&rec.ability, Some(&noop_catalog), rec.level);

    let attacker = Combatant {
        id: "att".into(),
        attack: 0.0,
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
        hull_health: 500_000.0,
        shield_health: 1_000_000.0,
        shield_mitigation: 0.95,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "xindi".into(),
        attack: 0.0,
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
        hull_health: 10_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let attacker_crew = CrewConfiguration { seats: vec![] };
    let baseline = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(1, 21),
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &without_ibix,
    );
    let bypass = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(1, 21),
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &with_ibix,
    );
    assert!(
        bypass.attacker_hull_remaining < baseline.attacker_hull_remaining,
        "100% shield bypass should increase counter hull damage (baseline hull {}, bypass {})",
        baseline.attacker_hull_remaining,
        bypass.attacker_hull_remaining
    );
    assert!(
        bypass.attacker_shield_remaining > baseline.attacker_shield_remaining,
        "bypass should spare attacker shields (baseline shield {}, bypass {})",
        baseline.attacker_shield_remaining,
        bypass.attacker_shield_remaining
    );
}

#[test]
fn kemocite_catalog_builds_round_end_weaponry_seat() {
    let rec = resolve_hostile("2634260020").expect("xindi group armada");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileKemociteWeaponry {
                    growth_per_stack,
                } if (growth_per_stack - 0.3).abs() < 1e-9
            ) && s.ability.timing == kobayashi::combat::abilities::TimingWindow::RoundEnd
        }),
        "expected Kemocite round-end seat with 30% growth"
    );
}

#[test]
fn kemocite_stacking_increases_counter_damage_over_rounds() {
    use kobayashi::data::hostile_ability_resolve::HostileAbilityCatalog;

    let rec = resolve_hostile("2634260020").expect("xindi group armada");
    let catalog = hostile_ability_catalog_for_default_path();
    let with_kemocite = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let noop_catalog = HostileAbilityCatalog {
        description: None,
        entries: std::collections::HashMap::new(),
    };
    let without_kemocite =
        hostile_abilities_to_defender_crew(&rec.ability, Some(&noop_catalog), rec.level);

    let attacker = Combatant {
        id: "att".into(),
        attack: 0.0,
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
        hull_health: 5_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "xindi_armada".into(),
        attack: 0.0,
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
        hull_health: 10_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 50_000.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let attacker_crew = CrewConfiguration { seats: vec![] };
    let one_round = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(1, 31),
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &with_kemocite,
    );
    let five_rounds = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(5, 31),
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &with_kemocite,
    );
    let five_rounds_no_kemocite = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg(5, 31),
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &without_kemocite,
    );
    let one_round_att_hull_lost = attacker.hull_health - one_round.attacker_hull_remaining;
    let five_rounds_att_hull_lost = attacker.hull_health - five_rounds.attacker_hull_remaining;
    let five_rounds_no_kemocite_att_hull_lost =
        attacker.hull_health - five_rounds_no_kemocite.attacker_hull_remaining;
    assert!(
        five_rounds_att_hull_lost > one_round_att_hull_lost,
        "Kemocite stacks should increase counter damage by round 5"
    );
    assert!(
        five_rounds_att_hull_lost > five_rounds_no_kemocite_att_hull_lost,
        "Kemocite crew should out-damage noop catalog over 5 rounds"
    );
}

#[test]
fn no_mercy_catalog_builds_assimilated_gated_lethal_seat() {
    let rec = resolve_hostile("2634260020").expect("xindi group armada");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileLethalEndOfRound {
                    round_interval: 8,
                    shots: 1,
                    prevent_when_defender_assimilated: true,
                }
            )
        }),
        "expected No Mercy lethal seat with assimilated gate"
    );
}
