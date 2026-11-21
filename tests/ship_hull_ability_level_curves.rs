//! Per-level value curves for ship hull abilities (auto-detected by the normalizer).
//!
//! Upstream `values[]` on a hull ability is a fixed-width array whose live (non-zero-tail)
//! portion is indexed by ship level − 1 (live length matches the ship's max level, not its
//! tier count). The normalizer emits `level_scaled_values` for every catalogued, non-noop,
//! non-overridden ability whose live entries vary, truncated to the ship's max level;
//! `to_ship_record` resolves the curve to a concrete `value` at the requested level.

use kobayashi::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, SimulationConfig, TimingWindow, TraceMode,
};
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;
use serde_json::Value;

fn extended_ability(ship_file: &str, ability_id: &str) -> Value {
    let raw = std::fs::read_to_string(format!("data/ships_extended/{ship_file}.json"))
        .expect("read extended ship json");
    let root: Value = serde_json::from_str(&raw).expect("parse extended ship json");
    root["abilities"]
        .as_array()
        .expect("abilities array")
        .iter()
        .find(|a| a["id"] == ability_id)
        .unwrap_or_else(|| panic!("ability {ability_id} on {ship_file}"))
        .clone()
}

fn resolved_ability_value(ship_id: &str, tier: u32, level: u32, ability_id: &str) -> f64 {
    let rec = resolve_ship_with_tier_level(ship_id, Some(tier), Some(level)).expect("ship record");
    let ability = rec
        .abilities
        .as_ref()
        .expect("abilities")
        .iter()
        .find(|a| a.id == ability_id)
        .unwrap_or_else(|| panic!("ability {ability_id} on {ship_id}"))
        .clone();
    assert!(
        ability.level_scaled_values.is_none(),
        "to_ship_record must collapse the curve into value"
    );
    ability.value
}

/// Augur `3459465041` (accumulating_attack_multiplier, percentage): upstream curve runs
/// 0.5 → 1.4 over the ship's 45 levels (×0.01 percentage scaling → 0.005 → 0.014).
#[test]
fn augur_ability_scales_with_level_matching_upstream_curve_ends() {
    let l1 = resolved_ability_value("augur", 1, 1, "3459465041");
    let l45 = resolved_ability_value("augur", 9, 45, "3459465041");
    assert!((l1 - 0.005).abs() < 1e-9, "L1 value {l1} != 0.005");
    assert!((l45 - 0.014).abs() < 1e-9, "L45 value {l45} != 0.014");
    assert!(l45 > l1);

    let curve = extended_ability("augur", "3459465041");
    let entries = curve["level_scaled_values"]
        .as_array()
        .expect("augur ability carries a level curve");
    assert_eq!(entries.len(), 45, "curve truncated to max level");
}

#[test]
fn d4_relentless_hunting_uses_fractional_level_curve() {
    let raw = std::fs::read_to_string("data/upstream/data-stfc-space/ships/957865248.json")
        .expect("cached D4 ship detail");
    let detail: Value = serde_json::from_str(&raw).expect("parse D4 ship detail");
    let ability = detail["ability"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == 957865248_u64)
        .expect("Relentless Hunting source");
    let ranks = ability["values"].as_array().expect("rank values");
    // The source translation's % format displays 0.01 as 1% and 0.25 as 25%.
    assert_eq!(ranks[0]["value"], 0.01);
    assert_eq!(ranks[44]["value"], 0.25);
    for level in 1..=45 {
        let expected = ranks[level as usize - 1]["value"].as_f64().unwrap();
        let actual = resolved_ability_value("d4_class", 9, level, "957865248");
        assert_eq!(actual, expected, "D4 at level {level}");
    }
}

#[test]
fn d4_relentless_hunting_contributes_each_round_during_hull_breach() {
    let ship = resolve_ship_with_tier_level("d4_class", Some(9), Some(45)).expect("D4 ship");
    let ability = ship
        .abilities
        .as_ref()
        .unwrap()
        .iter()
        .find(|a| a.id == "957865248")
        .unwrap();
    let hull_seat = ship_ability_to_crew_seat_context(ability).expect("Relentless Hunting");
    let breach_seat = CrewSeatContext::legacy(
        CrewSeat::Bridge,
        Ability {
            name: "Synthetic Hull Breach".into(),
            class: AbilityClass::BridgeAbility,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::HullBreach {
                chance: 1.0,
                duration_rounds: 2,
                requires_critical: false,
            },
            condition: None,
            weapon_scope: Default::default(),
        },
        false,
    );
    let attacker = Combatant {
        id: ship.id,
        attack: 100.0,
        hull_health: 1_000.0,
        crit_multiplier: 1.0,
        ..Combatant::default()
    };
    let defender = Combatant {
        hull_health: 1_000.0,
        ..Combatant::default()
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
        trace_mode: TraceMode::Off,
        ..SimulationConfig::default()
    };
    for (seats, expected) in [
        (vec![hull_seat.clone()], 200.0),
        (vec![breach_seat.clone()], 200.0),
        (vec![breach_seat, hull_seat], 275.0),
    ] {
        let crew = CrewConfiguration { seats };
        let result = simulate_combat(&attacker, &defender, &config, &crew);
        let traced = simulate_combat(
            &attacker,
            &defender,
            &SimulationConfig {
                trace_mode: TraceMode::Events,
                ..config.clone()
            },
            &crew,
        );
        assert_eq!(result.rounds_simulated, 2);
        assert_eq!(traced.total_damage, result.total_damage);
        assert_eq!(result.total_damage, expected);
    }
}

/// Levels past the ship's cap resolve to the max-level value (last-positive fallback over
/// the truncated curve), not to an inflated unreachable upstream entry.
#[test]
fn out_of_range_level_resolves_to_max_level_value() {
    let l45 = resolved_ability_value("augur", 9, 45, "3459465041");
    let l90 = resolved_ability_value("augur", 9, 90, "3459465041");
    assert!((l90 - l45).abs() < 1e-12, "L90 {l90} != L45 {l45}");
}

/// A catalog `value_override` suppresses curve emission entirely: a leaked curve would
/// overwrite the override at ship resolution time (borg_sphere `2425475474` is overridden
/// to 0.0 while its upstream values[] exists).
#[test]
fn value_override_suppresses_curve_emission() {
    let ability = extended_ability("borg_sphere", "2425475474");
    assert!(
        ability["level_scaled_values"].is_null(),
        "overridden ability must not carry a curve"
    );
    let resolved = resolved_ability_value("borg_sphere", 1, 45, "2425475474");
    assert_eq!(resolved, 0.0, "override value must survive at high level");
}

/// Abilities whose live upstream values are constant stay scalar — no curve churn
/// (borg_sphere `1781715001` apex_barrier is 50000 at every level).
#[test]
fn constant_ability_stays_scalar() {
    let ability = extended_ability("borg_sphere", "1781715001");
    assert!(
        ability["level_scaled_values"].is_null(),
        "constant ability must not carry a curve"
    );
    let l1 = resolved_ability_value("borg_sphere", 1, 1, "1781715001");
    let high = resolved_ability_value("borg_sphere", 1, 45, "1781715001");
    assert_eq!(l1, 50000.0);
    assert_eq!(l1, high);
}

#[test]
fn orion_corvette_enhanced_hull_adds_flat_health_at_each_level() {
    let detail: Value = serde_json::from_str(include_str!(
        "../data/upstream/data-stfc-space/ships/34867572.json"
    ))
    .unwrap();
    let ability = &detail["ability"][0];
    assert_eq!(ability["id"], 34867572);
    assert_eq!(ability["value_is_percentage"], false);

    for level in 1_u32..=20 {
        let expected_bonus = ability["values"][level as usize - 1]["value"]
            .as_f64()
            .unwrap();
        let ship =
            resolve_ship_with_tier_level("orion_corvette", Some(level.div_ceil(5)), Some(level))
                .expect("Orion Corvette at a supported tier and level");
        let (hull, shield) = ship.health_with_launch_bonuses();
        assert_eq!(hull, ship.hull_health + expected_bonus, "level {level}");
        assert_eq!(shield, ship.shield_health, "level {level}");
    }
}
