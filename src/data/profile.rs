//! Player profile: effective_bonuses applied as pre-combat modifier layer (DESIGN §5).
//! Keys match engine/LCARS stats: weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce, etc.
//! Bonuses from synced forbidden/chaos tech (by fid) are merged in when [merge_forbidden_tech_bonuses_into_profile] is used.
//! Bonuses from synced buildings (by bid) are merged in when [merge_building_bonuses_into_profile] is used.
//! Bonuses from synced research (by rid) are merged in when [merge_research_bonuses_into_profile] is used.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::Combatant;
use crate::data::building::{self, BuildingBonusContext, BuildingIndex};
use crate::data::forbidden_chaos::ForbiddenChaosList;
use crate::data::import::{BuildingEntry, ForbiddenTechEntry, ResearchEntry};
use crate::data::research::{cumulative_research_bonuses, ResearchCatalog};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerProfile {
    #[serde(default)]
    pub bonuses: HashMap<String, f64>,
    /// Optional Operations Center level override. When set, building bonus context uses this
    /// instead of inferring from synced buildings (ops_center level). Lets you simulate without sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ops_level: Option<u32>,
    /// When set and non-empty, these fids are used instead of synced forbidden_tech.imported.json
    /// to merge forbidden-tech bonuses. Enables UI to choose "Custom" tech set per profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbidden_tech_override: Option<Vec<i64>>,
    /// When set and non-empty, these fids are used instead of synced chaos tech from
    /// forbidden_tech.imported.json. Enables UI to choose "Custom" chaos tech set per profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chaos_tech_override: Option<Vec<i64>>,
}

pub const DEFAULT_PROFILE_PATH: &str = "data/profile.json";

/// Applies one bonus to profile (add or mult). Mult: (1+current)*(1+value)-1; else additive.
fn accumulate_forbidden_tech_bonus(out: &mut HashMap<String, f64>, stat: &str, operator: &str, value: f64) {
    let current = out.get(stat).copied().unwrap_or(0.0);
    let is_mult = operator.eq_ignore_ascii_case("mult")
        || operator.eq_ignore_ascii_case("multiply")
        || operator.eq_ignore_ascii_case("mul");
    let new_value = if is_mult {
        (1.0 + current) * (1.0 + value) - 1.0
    } else {
        current + value
    };
    out.insert(stat.to_string(), new_value);
}

/// Merges bonuses from player's synced forbidden/chaos tech into `profile.bonuses`.
/// For each imported tech entry (by `fid`), looks up the catalog by `fid`; if the catalog
/// has a matching `fid`, applies that record's bonuses (additive for "add", multiplicative for "mult").
/// Catalog entries without `fid` are skipped for sync-based lookup.
pub fn merge_forbidden_tech_bonuses_into_profile(
    profile: &mut PlayerProfile,
    imported_ft: &[ForbiddenTechEntry],
    catalog: &ForbiddenChaosList,
) {
    let fids: Vec<i64> = imported_ft.iter().map(|e| e.fid).collect();
    merge_tech_fids_into_profile(profile, &fids, catalog);
}

/// Resolves effective tech fids from profile overrides or imported entries, split by tech_type.
/// Forbidden: use forbidden_tech_override if set, else imported entries matching tech_type "forbidden".
/// Chaos: use chaos_tech_override if set, else imported entries matching tech_type "chaos".
/// Items with empty tech_type are treated as forbidden for backward compatibility.
pub fn resolve_effective_tech_fids(
    profile: &PlayerProfile,
    imported_ft: &[ForbiddenTechEntry],
    catalog: &ForbiddenChaosList,
) -> Vec<i64> {
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();

    let is_forbidden = |r: &&crate::data::forbidden_chaos::ForbiddenChaosRecord| {
        r.tech_type.is_empty() || r.tech_type.eq_ignore_ascii_case("forbidden")
    };
    let is_chaos = |r: &&crate::data::forbidden_chaos::ForbiddenChaosRecord| {
        r.tech_type.eq_ignore_ascii_case("chaos")
    };

    let forbidden_fids: Vec<i64> = if profile
        .forbidden_tech_override
        .as_ref()
        .map_or(false, |v| !v.is_empty())
    {
        profile.forbidden_tech_override.as_ref().unwrap().clone()
    } else {
        imported_ft
            .iter()
            .filter(|e| by_fid.get(&e.fid).map_or(false, is_forbidden))
            .map(|e| e.fid)
            .collect()
    };

    let chaos_fids: Vec<i64> = if profile
        .chaos_tech_override
        .as_ref()
        .map_or(false, |v| !v.is_empty())
    {
        profile.chaos_tech_override.as_ref().unwrap().clone()
    } else {
        imported_ft
            .iter()
            .filter(|e| by_fid.get(&e.fid).map_or(false, is_chaos))
            .map(|e| e.fid)
            .collect()
    };

    let mut out = forbidden_fids;
    out.extend(chaos_fids);
    out
}

/// Merges bonuses from tech catalog into profile for the given fids.
pub fn merge_tech_fids_into_profile(
    profile: &mut PlayerProfile,
    fids: &[i64],
    catalog: &ForbiddenChaosList,
) {
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    for &fid in fids {
        let Some(record) = by_fid.get(&fid) else {
            continue;
        };
        for bonus in &record.bonuses {
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            accumulate_forbidden_tech_bonus(&mut profile.bonuses, &bonus.stat, op, bonus.value);
        }
    }
}

fn normalize_profile_combat_stat(stat: &str) -> Option<&'static str> {
    match stat {
        "weapon_damage" => Some("weapon_damage"),
        "hull_hp" => Some("hull_hp"),
        "shield_hp" => Some("shield_hp"),
        "crit_chance" => Some("crit_chance"),
        "crit_damage" => Some("crit_damage"),
        "pierce" | "armor_pierce" | "shield_pierce" => Some("pierce"),
        "shield_mitigation" => Some("shield_mitigation"),
        "armor" => Some("armor"),
        "dodge" => Some("dodge"),
        "damage_reduction" => Some("damage_reduction"),
        _ => None,
    }
}

/// Merges combat stat bonuses from player's synced buildings into `profile.bonuses`.
/// Resolves bid → building id via `bid_to_id`, loads building records, computes cumulative
/// bonuses, and adds only combat keys (weapon_damage, hull_hp, etc.). armor_pierce and
/// shield_pierce are folded into pierce.
pub fn merge_building_bonuses_into_profile(
    profile: &mut PlayerProfile,
    imported_buildings: &[BuildingEntry],
    bid_to_id: &HashMap<i64, String>,
    _building_index: &BuildingIndex,
    data_dir: &Path,
    context: &BuildingBonusContext,
) {
    if imported_buildings.is_empty() || bid_to_id.is_empty() {
        return;
    }

    let mut levels_by_id: HashMap<String, u32> = HashMap::new();
    for entry in imported_buildings {
        let Some(id) = bid_to_id.get(&entry.bid) else {
            continue;
        };
        let level = if entry.level >= 0 {
            entry.level.min(i64::from(u32::MAX)) as u32
        } else {
            0
        };
        levels_by_id.insert(id.clone(), level);
    }
    if levels_by_id.is_empty() {
        return;
    }

    let mut records: Vec<building::BuildingRecord> = Vec::new();
    for id in levels_by_id.keys() {
        if let Some(rec) = building::load_building_record(data_dir, id) {
            records.push(rec);
        }
    }
    if records.is_empty() {
        return;
    }

    let bonuses =
        building::cumulative_building_bonuses_with_context(&records, &levels_by_id, context);

    for (stat, value) in bonuses {
        let Some(key) = normalize_profile_combat_stat(&stat) else {
            continue;
        };
        let current = profile.bonuses.get(key).copied().unwrap_or(0.0);
        profile.bonuses.insert(key.to_string(), current + value);
    }
}

/// Merges combat stat bonuses from player's synced research into `profile.bonuses`.
/// For each imported research entry (rid, level), looks up the catalog by rid and sums
/// cumulative bonuses for levels 1..=level. Only combat stats are applied (same keys as buildings).
pub fn merge_research_bonuses_into_profile(
    profile: &mut PlayerProfile,
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
) {
    if imported_research.is_empty() || catalog.items.is_empty() {
        return;
    }

    let mut levels_by_rid: HashMap<i64, u32> = HashMap::new();
    for entry in imported_research {
        let level = if entry.level > 0 {
            entry.level.min(i64::from(u32::MAX)) as u32
        } else {
            0
        };
        if level > 0 {
            levels_by_rid.insert(entry.rid, level);
        }
    }
    if levels_by_rid.is_empty() {
        return;
    }

    let records: Vec<&crate::data::research::ResearchRecord> = catalog
        .items
        .iter()
        .filter(|r| levels_by_rid.contains_key(&r.rid))
        .collect();
    if records.is_empty() {
        return;
    }

    let bonuses = cumulative_research_bonuses(&records, &levels_by_rid);

    for (stat, value) in bonuses {
        let Some(key) = normalize_profile_combat_stat(&stat) else {
            continue;
        };
        let current = profile.bonuses.get(key).copied().unwrap_or(0.0);
        profile.bonuses.insert(key.to_string(), current + value);
    }
}

/// Load profile from JSON file. Returns default (empty bonuses) if file missing or invalid.
pub fn load_profile(path: &str) -> PlayerProfile {
    let path = Path::new(path);
    if !path.exists() {
        return PlayerProfile::default();
    }
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        _ => return PlayerProfile::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn get_bonus(profile: &PlayerProfile, key: &str) -> f64 {
    profile.bonuses.get(key).copied().unwrap_or(0.0)
}

/// Apply LCARS/officer static buffs to a Combatant (e.g. from [BuffSet::static_buffs]).
/// Intended for use when building a Combatant from ship/hostile + crew where crew is resolved via
/// [crate::lcars::resolve_crew_to_buff_set]. Keys applied: isolytic_damage, isolytic_defense,
/// shield_mitigation (additive; shield_mitigation clamped to [0, 1]), weapon_damage (mult to attack),
/// hull_hp, shield_hp (mult), shield_pierce/armor_pierce (add to pierce), crit_chance (add), crit_damage (mult).
pub fn apply_static_buffs_to_combatant(
    combatant: Combatant,
    static_buffs: &HashMap<String, f64>,
) -> Combatant {
    if static_buffs.is_empty() {
        return combatant;
    }
    let isolytic_damage_add = static_buffs.get("isolytic_damage").copied().unwrap_or(0.0);
    let isolytic_defense_add = static_buffs.get("isolytic_defense").copied().unwrap_or(0.0);
    let shield_mitigation_add = static_buffs
        .get("shield_mitigation")
        .copied()
        .unwrap_or(0.0);
    let weapon_mult = static_buffs.get("weapon_damage").copied().unwrap_or(1.0);
    let hull_mult = static_buffs.get("hull_hp").copied().unwrap_or(1.0);
    let shield_mult = static_buffs.get("shield_hp").copied().unwrap_or(1.0);
    let pierce_add = static_buffs.get("shield_pierce").copied().unwrap_or(0.0)
        + static_buffs.get("armor_pierce").copied().unwrap_or(0.0);
    let crit_chance_add = static_buffs.get("crit_chance").copied().unwrap_or(0.0);
    let crit_damage_mult = static_buffs.get("crit_damage").copied().unwrap_or(1.0);
    let armor_add = static_buffs.get("armor").copied().unwrap_or(0.0);
    let damage_reduction_add = static_buffs.get("damage_reduction").copied().unwrap_or(0.0);
    let dodge_add = static_buffs.get("dodge").copied().unwrap_or(0.0);

    Combatant {
        attack: combatant.attack * weapon_mult,
        hull_health: combatant.hull_health * hull_mult,
        shield_health: combatant.shield_health * shield_mult,
        pierce: (combatant.pierce + pierce_add).max(0.0),
        crit_chance: (combatant.crit_chance + crit_chance_add).max(0.0).min(1.0),
        crit_multiplier: (combatant.crit_multiplier * crit_damage_mult).max(0.0),
        isolytic_damage: (combatant.isolytic_damage + isolytic_damage_add).max(0.0),
        isolytic_defense: (combatant.isolytic_defense + isolytic_defense_add).max(0.0),
        shield_mitigation: (combatant.shield_mitigation + shield_mitigation_add)
            .max(0.0)
            .min(1.0),
        mitigation: (combatant.mitigation + armor_add + damage_reduction_add + dodge_add)
            .max(0.0)
            .min(1.0),
        ..combatant
    }
}

/// Apply effective_bonuses to attacker Combatant (multipliers and additive bonuses).
/// Keys: weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce (additive),
/// shield_mitigation (additive to base), armor/dodge/damage_reduction (additive to mitigation).
pub fn apply_profile_to_attacker(attacker: Combatant, profile: &PlayerProfile) -> Combatant {
    if profile.bonuses.is_empty() {
        return attacker;
    }
    let weapon = 1.0 + get_bonus(profile, "weapon_damage");
    let hull_hp = 1.0 + get_bonus(profile, "hull_hp");
    let shield_hp = 1.0 + get_bonus(profile, "shield_hp");
    let crit_chance_add = get_bonus(profile, "crit_chance");
    let crit_damage_mult = 1.0 + get_bonus(profile, "crit_damage");
    let pierce_add = get_bonus(profile, "pierce");
    let shield_mit_add = get_bonus(profile, "shield_mitigation");
    let mitigation_add = get_bonus(profile, "armor")
        + get_bonus(profile, "dodge")
        + get_bonus(profile, "damage_reduction");

    Combatant {
        attack: attacker.attack * weapon,
        hull_health: attacker.hull_health * hull_hp,
        shield_health: attacker.shield_health * shield_hp,
        crit_chance: (attacker.crit_chance + crit_chance_add).max(0.0).min(1.0),
        crit_multiplier: (attacker.crit_multiplier * crit_damage_mult).max(0.0),
        pierce: (attacker.pierce + pierce_add).max(0.0),
        mitigation: (attacker.mitigation + mitigation_add).max(0.0).min(1.0),
        shield_mitigation: (attacker.shield_mitigation + shield_mit_add)
            .max(0.0)
            .min(1.0),
        ..attacker
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::combat::Combatant;
    use crate::data::building::{
        BuildingBonusContext, BuildingIndex, BuildingIndexEntry, BuildingMode,
    };
    use crate::data::import::BuildingEntry;

    use super::*;

    #[test]
    fn merge_building_bonuses_into_profile_adds_only_combat_keys() {
        let mut profile = PlayerProfile::default();
        let imported_buildings = vec![BuildingEntry { bid: 1, level: 1 }];
        let mut bid_to_id = HashMap::new();
        bid_to_id.insert(1i64, "test_weapon_building".to_string());
        let building_index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "test_weapon_building".to_string(),
                building_name: "Test".to_string(),
                file: None,
            }],
        };
        let data_dir = std::env::temp_dir().join("kobayashi_profile_building_test");
        let _ = std::fs::create_dir_all(&data_dir);
        let building_json = r#"{
            "id": "test_weapon_building",
            "building_name": "Test",
            "levels": [{
                "level": 1,
                "bonuses": [
                    {"stat": "weapon_damage", "value": 0.05, "operator": "add"},
                    {"stat": "buff_123", "value": 1.0, "operator": "add"}
                ]
            }]
        }"#;
        std::fs::write(data_dir.join("test_weapon_building.json"), building_json).unwrap();

        merge_building_bonuses_into_profile(
            &mut profile,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext::default(),
        );

        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.05));
        assert!(profile.bonuses.get("buff_123").is_none());
    }

    #[test]
    fn merge_building_bonuses_into_profile_maps_pierce_and_mitigation_stats() {
        let mut profile = PlayerProfile::default();
        let imported_buildings = vec![BuildingEntry { bid: 1, level: 1 }];
        let mut bid_to_id = HashMap::new();
        bid_to_id.insert(1i64, "test_weapon_building".to_string());
        let building_index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "test_weapon_building".to_string(),
                building_name: "Test".to_string(),
                file: None,
            }],
        };
        let data_dir = std::env::temp_dir().join("kobayashi_profile_building_test_stats");
        let _ = std::fs::create_dir_all(&data_dir);
        let building_json = r#"{
            "id": "test_weapon_building",
            "building_name": "Test",
            "levels": [{
                "level": 1,
                "bonuses": [
                    {"stat": "armor_pierce", "value": 0.07, "operator": "add"},
                    {"stat": "shield_pierce", "value": 0.03, "operator": "add"},
                    {"stat": "armor", "value": 0.04, "operator": "add"},
                    {"stat": "dodge", "value": 0.05, "operator": "add"},
                    {"stat": "damage_reduction", "value": 0.06, "operator": "add"}
                ]
            }]
        }"#;
        std::fs::write(data_dir.join("test_weapon_building.json"), building_json).unwrap();

        merge_building_bonuses_into_profile(
            &mut profile,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext {
                ops_level: Some(30),
                mode: BuildingMode::ShipCombat,
            },
        );

        assert_eq!(profile.bonuses.get("pierce"), Some(&0.10));
        assert_eq!(profile.bonuses.get("armor"), Some(&0.04));
        assert_eq!(profile.bonuses.get("dodge"), Some(&0.05));
        assert_eq!(profile.bonuses.get("damage_reduction"), Some(&0.06));
    }

    #[test]
    fn merge_research_bonuses_into_profile_adds_only_combat_keys() {
        use crate::data::research::{
            ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
        };

        let mut profile = PlayerProfile::default();
        let imported_research = vec![ResearchEntry { rid: 1, level: 1 }];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 1,
                name: Some("Combat I".to_string()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![
                        ResearchBonusEntry {
                            stat: "weapon_damage".to_string(),
                            value: 0.05,
                            operator: "add".to_string(),
                        },
                        ResearchBonusEntry {
                            stat: "buff_unknown".to_string(),
                            value: 1.0,
                            operator: "add".to_string(),
                        },
                    ],
                }],
            }],
        };
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog);
        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.05));
        assert!(profile.bonuses.get("buff_unknown").is_none());
    }

    #[test]
    fn merge_research_bonuses_into_profile_skips_unknown_rid() {
        use crate::data::research::{ResearchCatalog, ResearchRecord};

        let mut profile = PlayerProfile::default();
        let imported_research = vec![ResearchEntry { rid: 99999, level: 5 }];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 1,
                name: None,
                data_version: None,
                source_note: None,
                levels: vec![],
            }],
        };
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog);
        assert!(profile.bonuses.is_empty());
    }

    fn combatant_with(
        isolytic_damage: f64,
        isolytic_defense: f64,
        shield_mitigation: f64,
    ) -> Combatant {
        Combatant {
            id: "test".to_string(),
            attack: 0.0,
            mitigation: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation,
            apex_barrier: 0.0,
            weapons: vec![],
            apex_shred: 0.0,
            isolytic_damage,
            isolytic_defense,
        }
    }

    #[test]
    fn apply_static_buffs_to_combatant_applies_and_clamps() {
        let c = combatant_with(0.0, 0.0, 0.5);
        let mut buffs = HashMap::new();
        buffs.insert("isolytic_damage".to_string(), 0.1);
        buffs.insert("isolytic_defense".to_string(), 10.0);
        buffs.insert("shield_mitigation".to_string(), 0.3);
        let out = apply_static_buffs_to_combatant(c, &buffs);
        assert_eq!(out.isolytic_damage, 0.1);
        assert_eq!(out.isolytic_defense, 10.0);
        assert_eq!(out.shield_mitigation, 0.8);

        let c2 = combatant_with(0.0, 0.0, 0.9);
        let mut buffs2 = HashMap::new();
        buffs2.insert("shield_mitigation".to_string(), 0.5);
        let out2 = apply_static_buffs_to_combatant(c2, &buffs2);
        assert_eq!(
            out2.shield_mitigation, 1.0,
            "shield_mitigation should clamp to 1.0"
        );
    }

    #[test]
    fn apply_profile_to_attacker_applies_mitigation_stats() {
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.10,
            pierce: 0.05,
            crit_chance: 0.10,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 500.0,
            shield_mitigation: 0.2,
            apex_barrier: 0.0,
            weapons: vec![],
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
        };
        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("armor".to_string(), 0.04);
        profile.bonuses.insert("dodge".to_string(), 0.03);
        profile.bonuses.insert("damage_reduction".to_string(), 0.02);

        let out = apply_profile_to_attacker(attacker, &profile);
        assert!((out.mitigation - 0.19).abs() < 1e-9);
    }
}
