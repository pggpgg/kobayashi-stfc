//! Building data: normalized level → bonuses (from STFCcommunity or manual).
//! For advanced player profile: building level → stat bonuses.
//! Each bonus uses engine/LCARS stat keys and additive vs multiplicative
//! semantics consistent with syndicate combat bonuses.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingMode {
    Unknown,
    ShipCombat,
    StationDefense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingDefenderOpponent {
    Unknown,
    NpcHostile,
    PlayerShip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingAttackerFaction {
    Unknown,
    Federation,
    Klingon,
    Romulan,
}

impl BuildingAttackerFaction {
    pub fn from_ship_faction_slug(slug: &str) -> Self {
        match slug.trim().to_ascii_lowercase().as_str() {
            "federation" | "fed" => Self::Federation,
            "klingon" | "klg" => Self::Klingon,
            "romulan" | "rom" => Self::Romulan,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingBonusContext {
    pub ops_level: Option<u32>,
    pub mode: BuildingMode,
    pub defender_opponent: BuildingDefenderOpponent,
    pub attacker_faction: BuildingAttackerFaction,
    pub attacker_tal_assigned_captain_or_bridge: bool,
}

impl Default for BuildingBonusContext {
    fn default() -> Self {
        Self {
            ops_level: None,
            mode: BuildingMode::Unknown,
            defender_opponent: BuildingDefenderOpponent::Unknown,
            attacker_faction: BuildingAttackerFaction::Unknown,
            attacker_tal_assigned_captain_or_bridge: false,
        }
    }
}

/// Normalized building record (KOBAYASHI schema). One per building.
/// Stats are stored as fractional bonuses where applicable (e.g. 0.35 = +35%).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingRecord {
    pub id: String,
    pub building_name: String,
    /// Optional provenance for this building file (e.g. "stfccommunity-main").
    #[serde(default)]
    pub data_version: Option<String>,
    /// Human-readable source description (e.g. "STFCcommunity baseline", "Manual corrections").
    #[serde(default)]
    pub source_note: Option<String>,
    pub levels: Vec<BuildingLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingLevel {
    pub level: u32,
    /// Optional Operations level band for which this row applies.
    #[serde(default)]
    pub ops_min: Option<u32>,
    #[serde(default)]
    pub ops_max: Option<u32>,
    pub bonuses: Vec<BonusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonusEntry {
    /// Engine/LCARS stat key (e.g. "weapon_damage", "hull_hp").
    pub stat: String,
    /// Bonus value (fractional where applicable: 0.35 = +35%).
    pub value: f64,
    /// Aggregation operator. "add" sums linearly; "multiply" stacks like
    /// (1 + current) * (1 + value) - 1. Empty defaults to "add".
    #[serde(default)]
    pub operator: String,
    /// Optional tags describing when this bonus applies (e.g. "defense_platform_only").
    #[serde(default)]
    pub conditions: Vec<String>,
    /// Optional free-form notes for edge cases or non-combat effects.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Index of all buildings. Includes data_version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub buildings: Vec<BuildingIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingIndexEntry {
    pub id: String,
    pub building_name: String,
    /// Optional filename stem (without .json) when using bid_name scheme, e.g. "0_ops_center".
    #[serde(default)]
    pub file: Option<String>,
    /// Upstream starbase module id (`bid`) when known — used with sync import and [`crate::data::building_bid_resolver`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid: Option<i64>,
}

pub const DEFAULT_BUILDINGS_INDEX_PATH: &str = "data/buildings/index.json";

pub fn load_building_index(path: &str) -> Option<BuildingIndex> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Load a building record by id. Tries `{id}.json` first, then (if index has a
/// `file` field for this id) `{file}.json`, so both legacy and bid_name naming work.
pub fn load_building_record(data_dir: &Path, id: &str) -> Option<BuildingRecord> {
    let path = data_dir.join(format!("{}.json", id));
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(rec) = serde_json::from_str(&data) {
            return Some(rec);
        }
    }
    let index_path = data_dir.join("index.json");
    let index: BuildingIndex = serde_json::from_str(&fs::read_to_string(index_path).ok()?).ok()?;
    let entry = index.buildings.iter().find(|e| e.id == id)?;
    let file_stem = entry.file.as_deref().unwrap_or(id);
    let path = data_dir.join(format!("{}.json", file_stem));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn accumulate_bonus(out: &mut HashMap<String, f64>, stat: &str, operator: &str, value: f64) {
    let key = stat.to_string();
    let current = out.get(&key).copied().unwrap_or(0.0);
    let is_multiply = operator.eq_ignore_ascii_case("multiply")
        || operator.eq_ignore_ascii_case("mul")
        || operator.eq_ignore_ascii_case("mult");
    let new_value = if is_multiply {
        (1.0 + current) * (1.0 + value) - 1.0
    } else {
        current + value
    };
    out.insert(key, new_value);
}

fn normalize_condition(condition: &str) -> String {
    condition
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
}

fn level_matches_context(level: &BuildingLevel, context: &BuildingBonusContext) -> bool {
    let Some(ops_level) = context.ops_level else {
        return true;
    };

    if let Some(min) = level.ops_min {
        if ops_level < min {
            return false;
        }
    }
    if let Some(max) = level.ops_max {
        if ops_level > max {
            return false;
        }
    }
    true
}

fn condition_matches_mode(condition: &str, mode: BuildingMode) -> bool {
    let normalized = normalize_condition(condition);
    match mode {
        BuildingMode::Unknown => true,
        BuildingMode::ShipCombat => !matches!(
            normalized.as_str(),
            "station_defense"
                | "station_defense_only"
                | "starbase_defense"
                | "defense_platform"
                | "defense_platform_only"
                | "platform_only"
                | "base_defense"
        ),
        BuildingMode::StationDefense => !matches!(
            normalized.as_str(),
            "ship_combat_only" | "ship_combat" | "ships_only" | "space_combat_only"
        ),
    }
}

fn condition_matches_defender(condition: &str, opponent: BuildingDefenderOpponent) -> bool {
    let normalized = normalize_condition(condition);
    match normalized.as_str() {
        "defender_is_npc_hostile" => opponent == BuildingDefenderOpponent::NpcHostile,
        "defender_is_player_ship" => opponent == BuildingDefenderOpponent::PlayerShip,
        _ => true,
    }
}

fn condition_matches_attacker_tal(condition: &str, tal_assigned: bool) -> bool {
    let normalized = normalize_condition(condition);
    match normalized.as_str() {
        "attacker_officer_tal_not_on_bridge" => !tal_assigned,
        _ => true,
    }
}

fn condition_matches_attacker_faction(
    condition: &str,
    attacker_faction: BuildingAttackerFaction,
) -> bool {
    let normalized = normalize_condition(condition);
    match normalized.as_str() {
        "attacker_ship_faction_any_fed_klg_rom" => matches!(
            attacker_faction,
            BuildingAttackerFaction::Federation
                | BuildingAttackerFaction::Klingon
                | BuildingAttackerFaction::Romulan
        ),
        _ => true,
    }
}

fn bonus_matches_context(bonus: &BonusEntry, context: &BuildingBonusContext) -> bool {
    bonus.conditions.iter().all(|condition| {
        condition_matches_mode(condition, context.mode)
            && condition_matches_defender(condition, context.defender_opponent)
            && condition_matches_attacker_tal(
                condition,
                context.attacker_tal_assigned_captain_or_bridge,
            )
            && condition_matches_attacker_faction(condition, context.attacker_faction)
    })
}

/// Maximum level defined in this building record (highest level in `levels`).
/// Returns 0 if levels is empty.
pub fn max_level(record: &BuildingRecord) -> u32 {
    record.levels.iter().map(|l| l.level).max().unwrap_or(0)
}

/// Combat bonuses from one building at `synced_level`, using catalog rows as **tier snapshots**.
///
/// Imported STFC building JSON lists each module tier’s **current totals** (replacing lower tiers),
/// not per-tier deltas. We therefore take bonuses only from the **highest** catalog tier that is
/// both ≤ `synced_level` (capped to the catalog’s max tier) and allowed by `ops_min`/`ops_max`
/// when ops context is set—not the sum of every lower tier row.
fn effective_building_bonuses_at_synced_level_with_context(
    record: &BuildingRecord,
    synced_level: u32,
    context: &BuildingBonusContext,
) -> HashMap<String, f64> {
    let capped = synced_level.min(max_level(record));
    let eligible: Vec<&BuildingLevel> = record
        .levels
        .iter()
        .filter(|l| l.level <= capped && level_matches_context(l, context))
        .collect();
    let Some(best_level) = eligible.iter().map(|l| l.level).max() else {
        return HashMap::new();
    };

    let mut out: HashMap<String, f64> = HashMap::new();
    for lvl in eligible.iter().filter(|l| l.level == best_level) {
        for bonus in &lvl.bonuses {
            if !bonus_matches_context(bonus, context) {
                continue;
            }
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            accumulate_bonus(&mut out, &bonus.stat, op, bonus.value);
        }
    }
    out
}

/// Returns effective combat bonuses from a single building at the synced module tier.
/// See [`effective_building_bonuses_at_synced_level_with_context`].
pub fn cumulative_building_level_bonuses(
    record: &BuildingRecord,
    level: u32,
) -> HashMap<String, f64> {
    cumulative_building_level_bonuses_with_context(record, level, &BuildingBonusContext::default())
}

pub fn cumulative_building_level_bonuses_with_context(
    record: &BuildingRecord,
    level: u32,
    context: &BuildingBonusContext,
) -> HashMap<String, f64> {
    effective_building_bonuses_at_synced_level_with_context(record, level, context)
}

/// Returns effective combat bonuses from multiple buildings (tier snapshots per building).
/// Missing buildings or levels are ignored.
pub fn cumulative_building_bonuses(
    records: &[BuildingRecord],
    levels_by_id: &HashMap<String, u32>,
) -> HashMap<String, f64> {
    cumulative_building_bonuses_with_context(
        records,
        levels_by_id,
        &BuildingBonusContext::default(),
    )
}

pub fn cumulative_building_bonuses_with_context(
    records: &[BuildingRecord],
    levels_by_id: &HashMap<String, u32>,
    context: &BuildingBonusContext,
) -> HashMap<String, f64> {
    let mut out: HashMap<String, f64> = HashMap::new();
    let index: HashMap<&str, &BuildingRecord> =
        records.iter().map(|r| (r.id.as_str(), r)).collect();

    for (id, level) in levels_by_id {
        let Some(rec) = index.get(id.as_str()) else {
            continue;
        };
        let tier_bonuses =
            effective_building_bonuses_at_synced_level_with_context(rec, *level, context);
        for (stat, value) in tier_bonuses {
            accumulate_bonus(&mut out, &stat, "add", value);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record() -> BuildingRecord {
        BuildingRecord {
            id: "ops_center".to_string(),
            building_name: "Operations Center".to_string(),
            data_version: None,
            source_note: None,
            levels: vec![
                BuildingLevel {
                    level: 1,
                    ops_min: None,
                    ops_max: None,
                    bonuses: vec![BonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.05,
                        operator: "add".to_string(),
                        conditions: vec![],
                        notes: None,
                    }],
                },
                BuildingLevel {
                    level: 2,
                    ops_min: Some(20),
                    ops_max: Some(30),
                    bonuses: vec![
                        BonusEntry {
                            stat: "weapon_damage".to_string(),
                            value: 0.05,
                            operator: "add".to_string(),
                            conditions: vec![],
                            notes: None,
                        },
                        BonusEntry {
                            stat: "shield_hp".to_string(),
                            value: 0.10,
                            operator: "add".to_string(),
                            conditions: vec![],
                            notes: None,
                        },
                    ],
                },
                BuildingLevel {
                    level: 3,
                    ops_min: Some(25),
                    ops_max: None,
                    bonuses: vec![
                        BonusEntry {
                            stat: "weapon_damage".to_string(),
                            value: 0.05,
                            operator: "add".to_string(),
                            conditions: vec![],
                            notes: None,
                        },
                        BonusEntry {
                            stat: "shield_hp".to_string(),
                            value: 0.10,
                            operator: "add".to_string(),
                            conditions: vec![],
                            notes: None,
                        },
                        BonusEntry {
                            stat: "crit_chance".to_string(),
                            value: 0.02,
                            operator: "add".to_string(),
                            conditions: vec!["ship_combat".to_string()],
                            notes: None,
                        },
                        BonusEntry {
                            stat: "crit_damage".to_string(),
                            value: 0.10,
                            operator: "add".to_string(),
                            conditions: vec!["defense_platform_only".to_string()],
                            notes: None,
                        },
                        BonusEntry {
                            stat: "hull_hp".to_string(),
                            value: 0.10,
                            operator: "multiply".to_string(),
                            conditions: vec![],
                            notes: None,
                        },
                        BonusEntry {
                            stat: "hull_hp".to_string(),
                            value: 0.20,
                            operator: "multiply".to_string(),
                            conditions: vec![],
                            notes: None,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn ops_range_is_respected_when_context_has_ops_level() {
        let record = test_record();
        let out = cumulative_building_level_bonuses_with_context(
            &record,
            3,
            &BuildingBonusContext {
                ops_level: Some(10),
                mode: BuildingMode::ShipCombat,
                defender_opponent: BuildingDefenderOpponent::Unknown,
                attacker_faction: BuildingAttackerFaction::Unknown,
                attacker_tal_assigned_captain_or_bridge: false,
            },
        );
        assert_eq!(out.get("weapon_damage"), Some(&0.05));
        assert!(!out.contains_key("shield_hp"));
    }

    #[test]
    fn ship_combat_filters_station_only_conditions() {
        let record = test_record();
        let out = cumulative_building_level_bonuses_with_context(
            &record,
            3,
            &BuildingBonusContext {
                ops_level: Some(25),
                mode: BuildingMode::ShipCombat,
                defender_opponent: BuildingDefenderOpponent::Unknown,
                attacker_faction: BuildingAttackerFaction::Unknown,
                attacker_tal_assigned_captain_or_bridge: false,
            },
        );
        assert_eq!(out.get("shield_hp"), Some(&0.10));
        assert_eq!(out.get("crit_chance"), Some(&0.02));
        assert!(!out.contains_key("crit_damage"));
    }

    #[test]
    fn unknown_mode_keeps_conditional_bonuses_for_compatibility() {
        let record = test_record();
        let out = cumulative_building_level_bonuses(&record, 3);
        assert_eq!(out.get("crit_damage"), Some(&0.10));
    }

    #[test]
    fn multiply_bonuses_stack_multiplicatively() {
        let record = test_record();
        let out = cumulative_building_level_bonuses_with_context(
            &record,
            3,
            &BuildingBonusContext {
                ops_level: Some(25),
                mode: BuildingMode::ShipCombat,
                defender_opponent: BuildingDefenderOpponent::Unknown,
                attacker_faction: BuildingAttackerFaction::Unknown,
                attacker_tal_assigned_captain_or_bridge: false,
            },
        );
        let hull = out.get("hull_hp").copied().unwrap_or_default();
        assert!(
            (hull - 0.32).abs() < 1e-9,
            "expected multiplicative stacking, got {hull}"
        );
    }

    #[test]
    fn defender_and_tal_conditions_gate_bonus_rows() {
        let record = BuildingRecord {
            id: "test_conditional".to_string(),
            building_name: "Conditional".to_string(),
            data_version: None,
            source_note: None,
            levels: vec![BuildingLevel {
                level: 1,
                ops_min: None,
                ops_max: None,
                bonuses: vec![
                    BonusEntry {
                        stat: "crit_damage".to_string(),
                        value: 0.1,
                        operator: "add".to_string(),
                        conditions: vec!["defender_is_npc_hostile".to_string()],
                        notes: None,
                    },
                    BonusEntry {
                        stat: "apex_barrier".to_string(),
                        value: 0.2,
                        operator: "add".to_string(),
                        conditions: vec![
                            "defender_is_player_ship".to_string(),
                            "attacker_officer_tal_not_on_bridge".to_string(),
                        ],
                        notes: None,
                    },
                ],
            }],
        };
        let hostile_ctx = BuildingBonusContext {
            ops_level: Some(1),
            mode: BuildingMode::ShipCombat,
            defender_opponent: BuildingDefenderOpponent::NpcHostile,
            attacker_faction: BuildingAttackerFaction::Unknown,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        let player_with_tal_ctx = BuildingBonusContext {
            ops_level: Some(1),
            mode: BuildingMode::ShipCombat,
            defender_opponent: BuildingDefenderOpponent::PlayerShip,
            attacker_faction: BuildingAttackerFaction::Unknown,
            attacker_tal_assigned_captain_or_bridge: true,
        };
        let player_without_tal_ctx = BuildingBonusContext {
            ops_level: Some(1),
            mode: BuildingMode::ShipCombat,
            defender_opponent: BuildingDefenderOpponent::PlayerShip,
            attacker_faction: BuildingAttackerFaction::Unknown,
            attacker_tal_assigned_captain_or_bridge: false,
        };
        let hostile_out = cumulative_building_level_bonuses_with_context(&record, 1, &hostile_ctx);
        assert_eq!(hostile_out.get("crit_damage"), Some(&0.1));
        assert!(!hostile_out.contains_key("apex_barrier"));

        let player_with_tal =
            cumulative_building_level_bonuses_with_context(&record, 1, &player_with_tal_ctx);
        assert!(!player_with_tal.contains_key("crit_damage"));
        assert!(!player_with_tal.contains_key("apex_barrier"));

        let player_without_tal =
            cumulative_building_level_bonuses_with_context(&record, 1, &player_without_tal_ctx);
        assert_eq!(player_without_tal.get("apex_barrier"), Some(&0.2));
    }

    #[test]
    fn attacker_faction_any_fed_klg_rom_condition_is_enforced() {
        let record = BuildingRecord {
            id: "test_faction".to_string(),
            building_name: "Faction".to_string(),
            data_version: None,
            source_note: None,
            levels: vec![BuildingLevel {
                level: 1,
                ops_min: None,
                ops_max: None,
                bonuses: vec![BonusEntry {
                    stat: "hull_hp".to_string(),
                    value: 0.12,
                    operator: "add".to_string(),
                    conditions: vec!["attacker_ship_faction_any_fed_klg_rom".to_string()],
                    notes: None,
                }],
            }],
        };

        for faction in [
            BuildingAttackerFaction::Federation,
            BuildingAttackerFaction::Klingon,
            BuildingAttackerFaction::Romulan,
        ] {
            let out = cumulative_building_level_bonuses_with_context(
                &record,
                1,
                &BuildingBonusContext {
                    ops_level: Some(1),
                    mode: BuildingMode::ShipCombat,
                    defender_opponent: BuildingDefenderOpponent::Unknown,
                    attacker_faction: faction,
                    attacker_tal_assigned_captain_or_bridge: false,
                },
            );
            assert_eq!(out.get("hull_hp"), Some(&0.12));
        }

        let out_unknown = cumulative_building_level_bonuses_with_context(
            &record,
            1,
            &BuildingBonusContext {
                ops_level: Some(1),
                mode: BuildingMode::ShipCombat,
                defender_opponent: BuildingDefenderOpponent::Unknown,
                attacker_faction: BuildingAttackerFaction::Unknown,
                attacker_tal_assigned_captain_or_bridge: false,
            },
        );
        assert!(!out_unknown.contains_key("hull_hp"));
    }
}
