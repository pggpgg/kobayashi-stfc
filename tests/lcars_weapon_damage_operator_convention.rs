//! Dynamic (non-static) LCARS `weapon_damage` rows must land in the engine's additive
//! pre-attack modifier with LCARS operator folding: `multiply v` ⇒ `+(v − 1)`, `add v` ⇒ `+v`,
//! `sub v` ⇒ `−v`. Before the 2026-07-10 fix, `compile_officer_combat_spec_impl` emitted the
//! full multiplicative factor (`multiply 1.2` → `AttackMultiplier(1.2)`), which the accumulator
//! then ADDED as a modifier — so a ×1.2 round-start buff dealt ×2.2 damage and a −20% debuff
//! (`sub 0.2` → 0.8) dealt ×1.8.

use kobayashi::combat::abilities::{AbilityClass, CrewSeat};
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::lcars::{
    resolve_officer_ability, LcarsAbility, LcarsEffect, LcarsOfficer, ResolveOptions,
};

fn weapon_damage_officer(operator: &str, value: f64) -> LcarsOfficer {
    LcarsOfficer {
        id: "wd-conv-officer".to_string(),
        name: "Weapon Damage Convention Officer".to_string(),
        faction: None,
        rarity: None,
        group: None,
        captain_ability: None,
        bridge_ability: Some(LcarsAbility {
            name: "wd_conv".to_string(),
            effects: vec![LcarsEffect {
                effect_type: "stat_modify".to_string(),
                stat: Some("weapon_damage".to_string()),
                target: None,
                operator: Some(operator.to_string()),
                value: Some(value),
                trigger: Some("on_round_start".to_string()),
                duration: None,
                scaling: None,
                condition: None,
                chance: None,
                multiplier: None,
                tag: None,
                accumulate: None,
                decay: None,
            }],
        }),
        below_decks_ability: None,
        stats: Vec::new(),
        max_level_by_rank: Vec::new(),
    }
}

fn crew_for(operator: &str, value: f64) -> CrewConfiguration {
    let officer = weapon_damage_officer(operator, value);
    let ability = officer.bridge_ability.clone().expect("bridge ability");
    let seats = resolve_officer_ability(
        &officer,
        &ability,
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        &ResolveOptions::default(),
        0,
    );
    assert_eq!(seats.len(), 1, "expected exactly one dynamic seat");
    CrewConfiguration { seats }
}

fn attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 800.0,
        hull_health: 50_000.0,
        crit_multiplier: 1.0,
        proc_multiplier: 1.0,
        weapons: vec![WeaponStats {
            attack: 2_500.0,
            shots: None,
            ..Default::default()
        }],
        ..Combatant::default()
    }
}

fn defender() -> Combatant {
    Combatant {
        id: "def".into(),
        hull_health: 500_000.0,
        ..attacker()
    }
}

/// One-round outbound damage with the officer, divided by the no-crew baseline.
fn damage_ratio(operator: &str, value: f64) -> f64 {
    let attacker = attacker();
    let defender = defender();
    let config = SimulationConfig {
        rounds: 1,
        seed: 42,
        trace_mode: TraceMode::Off,
        ..Default::default()
    };
    let with_setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &crew_for(operator, value),
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let baseline_setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with = simulate_combat_from_setup(&with_setup, config.seed);
    let without = simulate_combat_from_setup(&baseline_setup, config.seed);
    assert!(without.total_damage > 0.0, "baseline dealt no damage");
    with.total_damage / without.total_damage
}

#[test]
fn multiply_operator_scales_damage_by_the_stated_factor() {
    let ratio = damage_ratio("multiply", 1.2);
    assert!(
        (ratio - 1.2).abs() < 1e-6,
        "multiply 1.2 must deal ×1.2 damage, got ×{ratio}"
    );
}

#[test]
fn add_operator_adds_the_stated_bonus_fraction() {
    let ratio = damage_ratio("add", 0.2);
    assert!(
        (ratio - 1.2).abs() < 1e-6,
        "add 0.2 must deal ×1.2 damage, got ×{ratio}"
    );
}

#[test]
fn sub_operator_reduces_damage_by_the_stated_fraction() {
    let ratio = damage_ratio("sub", 0.2);
    assert!(
        (ratio - 0.8).abs() < 1e-6,
        "sub 0.2 must deal ×0.8 damage, got ×{ratio}"
    );
}
