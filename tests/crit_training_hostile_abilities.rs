//! Critical Training / Diverted Power hostile ability integration.

use kobayashi::combat::abilities::{
    hostile_crit_damage_floor_bonus_from_defender_crew, Ability, AbilityClass, AbilityEffect,
    CrewSeat, CrewSeatContext, TimingWindow, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
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

fn approx_eq(actual: f64, expected: f64, eps: f64) {
    assert!(
        (actual - expected).abs() <= eps,
        "expected {expected}, got {actual}"
    );
}

fn cfg(seed: u64) -> SimulationConfig {
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
    }
}

fn attacker(hull: f64) -> Combatant {
    Combatant {
        id: "attacker".into(),
        hull_health: hull,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn defender(attack: f64, crit_chance: f64, crit_multiplier: f64, crit_floor: f64) -> Combatant {
    Combatant {
        id: "defender".into(),
        attack,
        crit_chance,
        crit_multiplier,
        crit_damage_floor: crit_floor,
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        weapons: vec![WeaponStats {
            attack,
            shots: Some(1),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn sim(
    attacker: &Combatant,
    defender: &Combatant,
    attacker_crew: &CrewConfiguration,
    defender_crew: &CrewConfiguration,
    seed: u64,
) -> kobayashi::combat::SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        &cfg(seed),
        attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        defender_crew,
    )
}

fn hostile_cdr_crew(reduction: f64) -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "test_hostile_cdr".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileCritDamageReduction {
                    reduction,
                    duration_rounds: 5,
                    additive_percentage_points: false,
                    stacks: false,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    }
}

#[test]
fn critical_training_catalog_resolves_three_crit_seats() {
    let rec = resolve_hostile("1006581066").expect("critical training hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);

    assert!(
        crew.seats.iter().any(|s| {
            matches!(s.ability.effect, AbilityEffect::CritChanceBonus(v) if (v - 1.0).abs() < 1e-9)
        }),
        "expected always-crit seat"
    );
    assert!(
        crew.seats.iter().any(|s| {
            matches!(s.ability.effect, AbilityEffect::CritDamageMultiplier(v) if (v - 51.0).abs() < 1e-9)
        }),
        "expected x51 crit-damage seat"
    );
    assert!(
        crew.seats.iter().any(|s| {
            matches!(s.ability.effect, AbilityEffect::HostileCritDamageFloorBonus(v) if (v - 50.0).abs() < 1e-9)
        }),
        "expected x50 hostile crit-damage floor seat"
    );
}

#[test]
fn diverted_power_catalog_resolves_floor_seats() {
    let catalog = hostile_ability_catalog_for_default_path();
    for hostile_id in ["1007969491", "1022287607"] {
        let rec = resolve_hostile(hostile_id).expect("diverted power hostile");
        let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
        assert!(
            crew.seats.iter().any(|s| {
                matches!(s.ability.effect, AbilityEffect::HostileCritDamageFloorBonus(v) if (v - 1.5).abs() < 1e-9)
            }),
            "expected x1.5 floor for hostile {hostile_id}"
        );
    }
}

#[test]
fn hostile_crit_damage_floor_bonus_sums_defender_crew() {
    let catalog = hostile_ability_catalog_for_default_path();
    let rec_a = resolve_hostile("1007969491").expect("diverted power hostile");
    let rec_b = resolve_hostile("1022287607").expect("diverted power hostile");
    let mut crew = hostile_abilities_to_defender_crew(&rec_a.ability, catalog, rec_a.level);
    crew.seats
        .extend(hostile_abilities_to_defender_crew(&rec_b.ability, catalog, rec_b.level).seats);

    approx_eq(
        hostile_crit_damage_floor_bonus_from_defender_crew(&crew),
        3.0,
        1e-9,
    );
}

#[test]
fn critical_training_increases_counter_damage_vs_empty_catalog() {
    let rec = resolve_hostile("1006581066").expect("critical training hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let empty_catalog = HostileAbilityCatalog {
        description: Some("empty".into()),
        entries: Default::default(),
    };
    let noop_crew =
        hostile_abilities_to_defender_crew(&rec.ability, Some(&empty_catalog), rec.level);

    let attacker = attacker(1_000_000.0);
    let defender = defender(100.0, 0.0, 1.0, 0.0);
    let empty_attacker_crew = CrewConfiguration::default();

    let with_training = sim(&attacker, &defender, &empty_attacker_crew, &crew, 99);
    let baseline = sim(&attacker, &defender, &empty_attacker_crew, &noop_crew, 99);
    let boosted_damage = attacker.hull_health - with_training.attacker_hull_remaining;
    let baseline_damage = attacker.hull_health - baseline.attacker_hull_remaining;

    assert!(
        boosted_damage > baseline_damage * 40.0,
        "expected Critical Training counter damage to jump sharply (boosted={boosted_damage}, baseline={baseline_damage})"
    );
}

#[test]
fn hostile_floor_clamps_counter_crit_after_player_cdr() {
    let attacker = attacker(1_000_000.0);
    let empty_defender_crew = CrewConfiguration::default();
    let cdr = hostile_cdr_crew(0.9);

    let no_floor = defender(100.0, 1.0, 2.0, 0.0);
    let with_floor = defender(100.0, 1.0, 2.0, 1.5);
    let no_floor_result = sim(&attacker, &no_floor, &cdr, &empty_defender_crew, 7);
    let floor_result = sim(&attacker, &with_floor, &cdr, &empty_defender_crew, 7);
    let no_floor_damage = attacker.hull_health - no_floor_result.attacker_hull_remaining;
    let floor_damage = attacker.hull_health - floor_result.attacker_hull_remaining;

    approx_eq(floor_damage / no_floor_damage, 7.5, 1e-9);
}

#[test]
fn hostile_floor_below_reduced_counter_crit_changes_nothing() {
    let attacker = attacker(1_000_000.0);
    let empty_defender_crew = CrewConfiguration::default();
    let cdr = hostile_cdr_crew(0.1);

    let no_floor = defender(100.0, 1.0, 2.0, 0.0);
    let low_floor = defender(100.0, 1.0, 2.0, 1.5);
    let no_floor_result = sim(&attacker, &no_floor, &cdr, &empty_defender_crew, 11);
    let low_floor_result = sim(&attacker, &low_floor, &cdr, &empty_defender_crew, 11);

    approx_eq(
        no_floor_result.attacker_hull_remaining,
        low_floor_result.attacker_hull_remaining,
        1e-9,
    );
}

#[test]
fn hostile_floor_counter_path_is_deterministic_for_same_seed() {
    let attacker = attacker(1_000_000.0);
    let defender = defender(100.0, 1.0, 2.0, 1.5);
    let cdr = hostile_cdr_crew(0.9);
    let empty_defender_crew = CrewConfiguration::default();

    let a = sim(&attacker, &defender, &cdr, &empty_defender_crew, 12345);
    let b = sim(&attacker, &defender, &cdr, &empty_defender_crew, 12345);

    assert_eq!(a.attacker_hull_remaining, b.attacker_hull_remaining);
    assert_eq!(a.defender_hull_remaining, b.defender_hull_remaining);
}
