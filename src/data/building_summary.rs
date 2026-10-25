//! Read-only summary of synced building levels and effective ship-combat bonuses (profile + building catalog).

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::data::building::{self, BuildingBonusContext, BuildingIndex, BuildingMode};
use crate::data::building_bid_resolver::{
    load_bid_to_building_id, DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
};
use crate::data::import::{self, BuildingEntry};
use crate::data::profile::{
    load_profile, merge_building_bonuses_with_native_snapshot, PlayerProfile,
};
use crate::data::profile_index::{profile_path, BUFFS_IMPORTED, BUILDINGS_IMPORTED, PROFILE_JSON};

/// One row from `buildings.imported.json` with catalog resolution.
#[derive(Debug, Clone, Serialize)]
pub struct BuildingSummaryRow {
    pub bid: i64,
    pub level: i64,
    /// Kobayashi building id when bid maps via translations / index fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kobayashi_building_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub building_name: Option<String>,
    /// True when a building JSON record exists for the resolved id.
    pub catalog_record_present: bool,
}

/// Effective building-derived combat bonuses for the active profile (same merge as scenario / optimize).
#[derive(Debug, Clone, Serialize)]
pub struct BuildingCombatSummary {
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// `profile.json` ops override (used for building context when set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops_level_profile_override: Option<u32>,
    /// Ops inferred from synced Operations Center (bid → ops_center) when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops_level_inferred_from_sync: Option<u32>,
    /// Ops passed to building bonus context: override, else inferred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops_level_effective: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syndicate_level: Option<u32>,
    pub synced_building_count: usize,
    pub buildings: Vec<BuildingSummaryRow>,
    /// Synced `bid` values with no bid→catalog mapping.
    pub unmapped_bids: Vec<i64>,
    /// Additive combat stat bonuses from buildings only (engine keys, e.g. weapon_damage).
    #[serde(default, skip_serializing_if = "combat_bonuses_empty")]
    pub combat_bonuses_from_buildings: HashMap<String, f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_class: Option<String>,
    pub setup_bonus_source: String,
    pub source_limitations: Vec<String>,
}

fn combat_bonuses_empty(m: &HashMap<String, f64>) -> bool {
    m.is_empty()
}

fn infer_ops_level(
    imported_buildings: &[BuildingEntry],
    bid_to_id: &HashMap<i64, String>,
) -> Option<u32> {
    imported_buildings.iter().find_map(|entry| {
        let id = bid_to_id.get(&entry.bid)?;
        if id != "ops_center" {
            return None;
        }
        if entry.level < 0 {
            return Some(0);
        }
        Some(entry.level.min(i64::from(u32::MAX)) as u32)
    })
}

fn building_name_for_id(index: &BuildingIndex, id: &str) -> Option<String> {
    index
        .buildings
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.building_name.clone())
}

/// Builds a summary for `profiles/{profile_id}/` using the same paths and merge rules as the optimizer.
pub fn building_combat_summary_for_profile(profile_id: &str) -> BuildingCombatSummary {
    building_combat_summary_for_profile_with_ship(profile_id, None)
}

/// A selected hull supplies class and grade gates; unknown hulls never infer these gates.
pub fn building_combat_summary_for_profile_with_ship(
    profile_id: &str,
    ship_id: Option<&str>,
) -> BuildingCombatSummary {
    let profile_json = profile_path(profile_id, PROFILE_JSON);
    let player = load_profile(&profile_json.to_string_lossy());
    let buildings_path = profile_path(profile_id, BUILDINGS_IMPORTED)
        .to_string_lossy()
        .to_string();
    let imported = import::load_imported_buildings(&buildings_path).unwrap_or_default();
    let ship = ship_id.and_then(crate::data::loader::resolve_ship);
    let resolved_ship_id = ship.as_ref().and(ship_id).map(str::to_string);
    let ship_class = ship.as_ref().map(|ship| ship.ship_class.clone());
    let global_buffs =
        import::load_imported_buffs(&profile_path(profile_id, BUFFS_IMPORTED).to_string_lossy())
            .unwrap_or_default();
    let native = crate::data::native_building_buffs::building_bonuses_for_imported_buffs(
        &global_buffs,
        ship_class.as_deref(),
        player.syndicate_level,
        chrono::Utc::now().timestamp(),
    );
    let setup_bonus_source = if native.is_some() {
        "native_active_buffs"
    } else {
        "owned_catalog_fallback"
    }
    .to_string();
    let mut source_limitations = vec![
        "Totals cover reviewed setup families. Opponent, faction, assignment and other unsupported conditions remain excluded or use existing catalog rules.".to_string(),
    ];
    if ship.is_none() {
        source_limitations.push("Select a recognized ship in the scenario lens to include its class-specific bonuses; unknown class and grade gates are excluded.".to_string());
    }
    if player.syndicate_level.is_none() {
        source_limitations.push(
            "Syndicate level is unknown; native loyalty-gated bonuses are excluded.".to_string(),
        );
    }
    if native.is_none() {
        source_limitations.push("No recognized active building snapshot is available; totals use the owned building catalog fallback.".to_string());
    }

    let Some(building_index) =
        building::load_building_index(building::DEFAULT_BUILDINGS_INDEX_PATH)
    else {
        return BuildingCombatSummary {
            profile_id: profile_id.to_string(),
            error: Some(format!(
                "missing or invalid building index at {}",
                building::DEFAULT_BUILDINGS_INDEX_PATH
            )),
            ops_level_profile_override: player.ops_level,
            ops_level_inferred_from_sync: None,
            ops_level_effective: player.ops_level,
            syndicate_level: player.syndicate_level,
            synced_building_count: imported.len(),
            buildings: Vec::new(),
            unmapped_bids: imported.iter().map(|e| e.bid).collect(),
            combat_bonuses_from_buildings: HashMap::new(),
            ship_id: resolved_ship_id,
            ship_class,
            setup_bonus_source,
            source_limitations,
        };
    };

    let Some(bid_to_id) =
        load_bid_to_building_id(DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH, &building_index)
    else {
        return BuildingCombatSummary {
            profile_id: profile_id.to_string(),
            error: Some(format!(
                "could not load bid map from {}",
                DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH
            )),
            ops_level_profile_override: player.ops_level,
            ops_level_inferred_from_sync: None,
            ops_level_effective: player.ops_level,
            syndicate_level: player.syndicate_level,
            synced_building_count: imported.len(),
            buildings: Vec::new(),
            unmapped_bids: imported.iter().map(|e| e.bid).collect(),
            combat_bonuses_from_buildings: HashMap::new(),
            ship_id: resolved_ship_id,
            ship_class,
            setup_bonus_source,
            source_limitations,
        };
    };

    let ops_inferred = infer_ops_level(&imported, &bid_to_id);
    let ops_effective = player.ops_level.or(ops_inferred);
    let data_dir = Path::new(building::DEFAULT_BUILDINGS_INDEX_PATH)
        .parent()
        .unwrap_or_else(|| Path::new("data/buildings"));

    let mut rows: Vec<BuildingSummaryRow> = imported
        .iter()
        .map(|e| {
            let kid = bid_to_id.get(&e.bid).cloned();
            let building_name = kid
                .as_deref()
                .and_then(|id| building_name_for_id(&building_index, id));
            let catalog_record_present = kid
                .as_deref()
                .map(|id| building::load_building_record(data_dir, id).is_some())
                .unwrap_or(false);
            BuildingSummaryRow {
                bid: e.bid,
                level: e.level,
                kobayashi_building_id: kid,
                building_name,
                catalog_record_present,
            }
        })
        .collect();
    rows.sort_by_key(|a| a.bid);

    let unmapped_bids: Vec<i64> = imported
        .iter()
        .filter(|e| !bid_to_id.contains_key(&e.bid))
        .map(|e| e.bid)
        .collect();

    let context = BuildingBonusContext {
        ops_level: ops_effective,
        mode: BuildingMode::ShipCombat,
        defender_opponent: crate::data::building::BuildingDefenderOpponent::Unknown,
        attacker_faction: crate::data::building::BuildingAttackerFaction::Unknown,
        attacker_tal_assigned_captain_or_bridge: false,
        attacker_ship_type: ship_class
            .as_deref()
            .and_then(crate::combat::types::ShipType::from_data_slug),
        attacker_ship_grade: ship.as_ref().and_then(|ship| ship.grade),
        ..Default::default()
    };
    let mut scratch = PlayerProfile {
        ops_level: player.ops_level,
        bonuses: HashMap::new(),
        support_buffs: Vec::new(),
        forbidden_tech_override: None,
        chaos_tech_override: None,
        ..Default::default()
    };
    merge_building_bonuses_with_native_snapshot(
        &mut scratch,
        &imported,
        &bid_to_id,
        &building_index,
        data_dir,
        &context,
        native.as_ref(),
    );

    BuildingCombatSummary {
        profile_id: profile_id.to_string(),
        error: None,
        ops_level_profile_override: player.ops_level,
        ops_level_inferred_from_sync: ops_inferred,
        ops_level_effective: ops_effective,
        syndicate_level: player.syndicate_level,
        synced_building_count: imported.len(),
        buildings: rows,
        unmapped_bids,
        combat_bonuses_from_buildings: scratch.bonuses,
        ship_id: resolved_ship_id,
        ship_class,
        setup_bonus_source,
        source_limitations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_summary_matches_selected_class_and_known_syndicate_setup_pools() {
        let pid = format!("summary_native_{}", uuid::Uuid::new_v4().as_simple());
        let root = profile_path(&pid, PROFILE_JSON)
            .parent()
            .unwrap()
            .to_path_buf();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(PROFILE_JSON), r#"{"syndicate_level":45}"#).unwrap();
        std::fs::write(
            root.join(BUILDINGS_IMPORTED),
            r#"{"buildings":[{"bid":0,"level":60}]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join(BUFFS_IMPORTED),
            r#"{"buffs":[
            {"bid":1474824297,"level":48},{"bid":753685799,"level":60},
            {"bid":919263260,"level":61},{"bid":1944989778,"level":60},
            {"bid":1974652540,"level":61},{"bid":119659407,"level":60}
        ]}"#,
        )
        .unwrap();
        let selected = building_combat_summary_for_profile_with_ship(&pid, Some("realta"));
        assert!(selected.error.is_none());
        assert_eq!(selected.ship_class.as_deref(), Some("explorer"));
        assert_eq!(selected.setup_bonus_source, "native_active_buffs");
        assert!((selected.combat_bonuses_from_buildings["hull_hp"] - 6.69).abs() < 1e-9);
        assert!((selected.combat_bonuses_from_buildings["shield_hp"] - 5.95).abs() < 1e-9);
        assert!((selected.combat_bonuses_from_buildings["weapon_damage"] - 4.65).abs() < 1e-9);
        let unknown = building_combat_summary_for_profile_with_ship(&pid, Some("unknown_hull"));
        assert_eq!(unknown.ship_class, None);
        assert!((unknown.combat_bonuses_from_buildings["shield_hp"] - 4.15).abs() < 1e-9);
        for filename in [PROFILE_JSON, BUILDINGS_IMPORTED, BUFFS_IMPORTED] {
            std::fs::remove_file(root.join(filename)).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}
