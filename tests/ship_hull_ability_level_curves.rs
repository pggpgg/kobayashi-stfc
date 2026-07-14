//! Per-level value curves for ship hull abilities (auto-detected by the normalizer).
//!
//! Upstream `values[]` on a hull ability is a fixed-width array whose live (non-zero-tail)
//! portion is indexed by ship level − 1 (live length matches the ship's max level, not its
//! tier count). The normalizer emits `level_scaled_values` for every catalogued, non-noop,
//! non-overridden ability whose live entries vary, truncated to the ship's max level;
//! `to_ship_record` resolves the curve to a concrete `value` at the requested level.

use kobayashi::data::loader::resolve_ship_with_tier_level;
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
