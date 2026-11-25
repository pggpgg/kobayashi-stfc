//! Hostile ability catalog → defender_crew integration (real hostile records).

use kobayashi::combat::abilities::AbilityEffect;
use kobayashi::combat::types::{OpponentFactionTag, ShipType};
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Combatant, CrewConfiguration,
    SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
    HostileAbilityCatalog,
};
use kobayashi::data::loader::resolve_hostile;

fn weak_attacker_no_shields() -> Combatant {
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
        hull_health: 500_000.0,
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

fn pve_config(seed: u64) -> SimulationConfig {
    SimulationConfig {
        rounds: 1,
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
        ..Default::default()
    }
}

#[test]
fn real_hostile_isolytic_ability_builds_defender_crew_seat() {
    // Federation Elite hostile carries ability 3172395625 (Elite Assassin Training).
    let hostile_id = "3279772514";
    let rec = resolve_hostile(hostile_id).expect("elite hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        !crew.seats.is_empty(),
        "expected at least one defender crew seat from hostile abilities"
    );
    assert!(
        crew.seats.iter().any(|s| {
            matches!(s.ability.effect, AbilityEffect::IsolyticDamageBonus(v) if v > 0.0)
        }),
        "expected IsolyticDamageBonus seat from catalog"
    );
}

#[test]
fn real_hostile_isolytic_ability_increases_counter_fire_damage() {
    let hostile_id = "3279772514";
    let rec = resolve_hostile(hostile_id).expect("elite hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let with_abilities = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let noop_catalog = HostileAbilityCatalog {
        description: None,
        entries: std::collections::HashMap::new(),
    };
    let without_abilities =
        hostile_abilities_to_defender_crew(&rec.ability, Some(&noop_catalog), rec.level);

    let attacker = weak_attacker_no_shields();
    // Fixed weak defender so counter-fire is measurable (avoid multi-million DPR from the real record).
    let defender = Combatant {
        id: hostile_id.into(),
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
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let cfg = pve_config(42);
    let attacker_crew = CrewConfiguration { seats: vec![] };

    let baseline = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &without_abilities,
    );
    let boosted = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &with_abilities,
    );

    assert!(
        boosted.attacker_hull_remaining < baseline.attacker_hull_remaining,
        "isolytic defender crew should increase counter-fire damage (baseline hull {}, boosted {})",
        baseline.attacker_hull_remaining,
        boosted.attacker_hull_remaining
    );
}

#[test]
fn real_hostile_apex_barrier_ability_builds_defender_crew_seat() {
    // Augment Exile hostiles carry apex_barrier ability 1782396999 (Not So Wounded).
    let hostile_id = "1061963239";
    let rec = resolve_hostile(hostile_id).expect("augment hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats
            .iter()
            .any(|s| { matches!(s.ability.effect, AbilityEffect::ApexBarrierBonus(v) if v > 0.0) }),
        "expected ApexBarrierBonus seat from catalog"
    );
}

fn isolytic_boundary_combatant(id: &str) -> Combatant {
    let mut ship = weak_attacker_no_shields();
    ship.id = id.into();
    ship.weapons[0].attack = 100.0;
    ship.isolytic_damage = 1.0;
    ship
}

#[test]
fn romulan_sedes_virtutis_uses_both_combat_start_isolytic_bonuses() {
    let rec = resolve_hostile("23791327").expect("Romulan Sedes Virtutis");
    assert_eq!(rec.ship_type_for_combat(), ShipType::Armada);
    let crew = hostile_abilities_to_defender_crew(
        &rec.ability,
        hostile_ability_catalog_for_default_path(),
        rec.level,
    );
    let attacker = isolytic_boundary_combatant("player");
    let defender = isolytic_boundary_combatant(&rec.id);
    let simulate = |ship: &Combatant, abilities: &CrewConfiguration, trace_mode| {
        simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            ship,
            &SimulationConfig {
                rounds: 2,
                trace_mode,
                ..pve_config(42)
            },
            &CrewConfiguration::default(),
            OpponentFactionTag::Romulan,
            rec.ship_type_for_combat(),
            ShipType::Battleship,
            true,
            false,
            abilities,
        )
    };
    let mut expected = defender.clone();
    expected.isolytic_damage += 1.5;
    expected.isolytic_defense += 0.5;
    let expected = simulate(&expected, &CrewConfiguration::default(), TraceMode::Off);
    let observed = simulate(&defender, &crew, TraceMode::Off);
    let traced = simulate(&defender, &crew, TraceMode::Events);
    assert_eq!(observed.rounds_simulated, 2);
    assert_eq!(
        observed.attacker_hull_remaining,
        expected.attacker_hull_remaining
    );
    assert_eq!(
        observed.total_isolytic_damage,
        expected.total_isolytic_damage
    );
    assert_eq!(
        traced.attacker_hull_remaining,
        observed.attacker_hull_remaining
    );
    assert_eq!(traced.total_isolytic_damage, observed.total_isolytic_damage);
}

#[test]
fn hostile_combat_start_isolytic_defense_only_protects_the_hostile() {
    let rec = resolve_hostile("26336879").expect("Venari Ral");
    let mut crew = hostile_abilities_to_defender_crew(
        &rec.ability,
        hostile_ability_catalog_for_default_path(),
        rec.level,
    );
    crew.seats.retain(|seat| {
        seat.ability.name == "2936293636"
            && matches!(seat.ability.effect, AbilityEffect::IsolyticDefenseBonus(_))
    });
    assert_eq!(crew.seats.len(), 1);
    crew.seats[0].ability.condition = Some(
        kobayashi::combat::AbilityCondition::AttackerShipTypeIs(ShipType::Battleship),
    );
    let attacker = isolytic_boundary_combatant("player");
    let defender = isolytic_boundary_combatant(&rec.id);
    let simulate = |ship: &Combatant, abilities: &CrewConfiguration, attacker_type| {
        simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            ship,
            &pve_config(42),
            &CrewConfiguration::default(),
            OpponentFactionTag::Unknown,
            rec.ship_type_for_combat(),
            attacker_type,
            true,
            false,
            abilities,
        )
    };
    let baseline = simulate(
        &defender,
        &CrewConfiguration::default(),
        ShipType::Battleship,
    );
    let active = simulate(&defender, &crew, ShipType::Battleship);
    let mut expected = defender.clone();
    expected.isolytic_defense = 10.0;
    let expected = simulate(
        &expected,
        &CrewConfiguration::default(),
        ShipType::Battleship,
    );
    assert_eq!(active.total_isolytic_damage, expected.total_isolytic_damage);
    assert_eq!(
        active.attacker_hull_remaining,
        baseline.attacker_hull_remaining
    );

    let gated = simulate(&defender, &crew, ShipType::Explorer);
    let gated_baseline = simulate(&defender, &CrewConfiguration::default(), ShipType::Explorer);
    assert_eq!(
        gated.total_isolytic_damage,
        gated_baseline.total_isolytic_damage
    );
    assert_eq!(
        gated.attacker_hull_remaining,
        gated_baseline.attacker_hull_remaining
    );
}
