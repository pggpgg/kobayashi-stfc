//! Syndicate reputation combat bonuses: resolve (syndicate_level, ops_level) to cumulative
//! engine stat bonuses. Uses Section > Subsection > Bracket labels from the imported sheet.

use std::collections::HashMap;

use crate::data::syndicate_reputation::SyndicateReputationList;

/// Player operations level to bracket string (10-19, 20-29, ..., 61-70).
pub fn ops_level_to_band(ops_level: u32) -> &'static str {
    match ops_level {
        1..=19 => "10-19",
        20..=29 => "20-29",
        30..=39 => "30-39",
        40..=50 => "40-50",
        51..=60 => "51-60",
        _ => "61-70",
    }
}

/// Returns the engine/LCARS stat key(s) for a syndicate stat name, if it is a combat stat.
/// Officer Attack and Combat > Damage are separate columns in the sheet; only Combat > Damage
/// is "weapon damage" in-game. Officer Attack is kept as officer_attack to avoid double-counting.
fn syndicate_stat_to_engine_keys(stat: &str) -> Option<&'static [&'static str]> {
    let s = stat;
    if s.contains("Officer_Stats_>_Officer_Attack") {
        return Some(&["officer_attack"]);
    }
    if s.contains("Officer_Stats_>_Officer_Defense") {
        return Some(&["shield_mitigation"]);
    }
    if s.contains("Officer_Stats_>_Officer_Health") {
        return Some(&["hull_hp"]);
    }
    if s.contains("Ship_Stats_>_Piercing") {
        return Some(&["armor_piercing", "shield_piercing"]);
    }
    if s.contains("Ship_Stats_>_Mitigation") {
        return Some(&["shield_mitigation"]);
    }
    if s.contains("Ship_Stats_>_HHP_") || s.contains("HHP_(Hull_Health_Points)") {
        return Some(&["hull_hp"]);
    }
    if s.contains("Ship_Stats_>_SHP_") || s.contains("SHP_(Shield_Health_Points)") {
        return Some(&["shield_hp"]);
    }
    // Only the main "Combat > Damage" column is in-game weapon damage (e.g. +35% at 17, +50% at 23, +80% at 25).
    // Do not fold Defense_Platform_Damage or Damage_to_Stations into weapon_damage.
    if s.contains("Combat_>_Damage_>") && !s.contains("Defense_Platform") && !s.contains("Damage_to_Stations") {
        return Some(&["weapon_damage"]);
    }
    if s.contains("Combat_>_Defense_Platform_Damage") {
        return Some(&["defense_platform_damage"]);
    }
    if s.contains("Combat_>_Damage_to_Stations") {
        return Some(&["damage_to_stations"]);
    }
    if s.contains("Critical_Damage") {
        return Some(&["crit_damage"]);
    }
    if s.contains("Isolytic_Damage") {
        return Some(&["isolytic_damage"]);
    }
    if s.contains("Isolytic_Mitigation") {
        return Some(&["isolytic_defense"]);
    }
    if s.contains("PvP_Apex_Barrier") || s.contains("PvE_Apex_Barrier") {
        return Some(&["apex_barrier"]);
    }
    None
}

/// Returns cumulative combat stat bonuses from syndicate reputation for the given levels.
/// - `syndicate_level`: max syndicate level (levels 1..=syndicate_level are summed).
/// - `ops_level`: player operations level; used to select the bracket (10-19, ..., 51-60, 61-70).
/// Keys are engine/LCARS stat names (e.g. weapon_damage, hull_hp); values are additive totals.
pub fn cumulative_combat_bonuses(
    data: &SyndicateReputationList,
    syndicate_level: u32,
    ops_level: u32,
) -> HashMap<String, f64> {
    let band = ops_level_to_band(ops_level);
    let mut out: HashMap<String, f64> = HashMap::new();

    for entry in data.levels.iter() {
        if entry.level < 1 || entry.level > syndicate_level {
            continue;
        }
        for b in &entry.bonuses {
            if !b.stat.contains(band) {
                continue;
            }
            let Some(engine_keys) = syndicate_stat_to_engine_keys(&b.stat) else {
                continue;
            };
            let value = b.value;
            let is_multiply = b.operator.eq_ignore_ascii_case("multiply");
            for &key in engine_keys {
                let current = out.get(key).copied().unwrap_or(0.0);
                let new_value = if is_multiply {
                    (1.0 + current) * (1.0 + value) - 1.0
                } else {
                    current + value
                };
                out.insert(key.to_string(), new_value);
            }
        }
    }

    out
}
