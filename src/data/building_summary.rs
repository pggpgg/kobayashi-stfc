//! Read-only summary of synced building levels and effective ship-combat bonuses (profile + building catalog).

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::data::building::{self, BuildingBonusContext, BuildingIndex, BuildingMode};
use crate::data::building_bid_resolver::{
    load_bid_to_building_id, DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
};
use crate::data::import::{self, BuildingEntry};
use crate::data::profile::{load_profile, merge_building_bonuses_into_profile, PlayerProfile};
use crate::data::profile_index::{profile_path, BUILDINGS_IMPORTED, PROFILE_JSON};

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
    pub synced_building_count: usize,
    pub buildings: Vec<BuildingSummaryRow>,
    /// Synced `bid` values with no bid→catalog mapping.
    pub unmapped_bids: Vec<i64>,
    /// Additive combat stat bonuses from buildings only (engine keys, e.g. weapon_damage).
    #[serde(default, skip_serializing_if = "combat_bonuses_empty")]
    pub combat_bonuses_from_buildings: HashMap<String, f64>,
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
    let profile_json = profile_path(profile_id, PROFILE_JSON);
    let player = load_profile(&profile_json.to_string_lossy());
    let buildings_path = profile_path(profile_id, BUILDINGS_IMPORTED)
        .to_string_lossy()
        .to_string();
    let imported = import::load_imported_buildings(&buildings_path).unwrap_or_default();

    let Some(building_index) = building::load_building_index(building::DEFAULT_BUILDINGS_INDEX_PATH)
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
            synced_building_count: imported.len(),
            buildings: Vec::new(),
            unmapped_bids: imported.iter().map(|e| e.bid).collect(),
            combat_bonuses_from_buildings: HashMap::new(),
        };
    };

    let Some(bid_to_id) = load_bid_to_building_id(
        DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
        &building_index,
    ) else {
        return BuildingCombatSummary {
            profile_id: profile_id.to_string(),
            error: Some(format!(
                "could not load bid map from {}",
                DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH
            )),
            ops_level_profile_override: player.ops_level,
            ops_level_inferred_from_sync: None,
            ops_level_effective: player.ops_level,
            synced_building_count: imported.len(),
            buildings: Vec::new(),
            unmapped_bids: imported.iter().map(|e| e.bid).collect(),
            combat_bonuses_from_buildings: HashMap::new(),
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
    rows.sort_by(|a, b| a.bid.cmp(&b.bid));

    let unmapped_bids: Vec<i64> = imported
        .iter()
        .filter(|e| !bid_to_id.contains_key(&e.bid))
        .map(|e| e.bid)
        .collect();

    let context = BuildingBonusContext {
        ops_level: ops_effective,
        mode: BuildingMode::ShipCombat,
    };
    let mut scratch = PlayerProfile {
        ops_level: player.ops_level,
        bonuses: HashMap::new(),
        forbidden_tech_override: None,
        chaos_tech_override: None,
    };
    merge_building_bonuses_into_profile(
        &mut scratch,
        &imported,
        &bid_to_id,
        &building_index,
        data_dir,
        &context,
    );

    BuildingCombatSummary {
        profile_id: profile_id.to_string(),
        error: None,
        ops_level_profile_override: player.ops_level,
        ops_level_inferred_from_sync: ops_inferred,
        ops_level_effective: ops_effective,
        synced_building_count: imported.len(),
        buildings: rows,
        unmapped_bids,
        combat_bonuses_from_buildings: scratch.bonuses,
    }
}
