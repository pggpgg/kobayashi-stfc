//! Read-only summary of synced research levels and effective ship-combat bonuses (profile + research catalog).

use std::collections::HashMap;

use serde::Serialize;

use crate::data::import;
use crate::data::profile::{
    accumulate_combat_only_bonuses_from_raw, load_profile, merge_research_bonuses_into_profile,
    PlayerProfile,
};
use crate::data::profile_index::{profile_path, PROFILE_JSON, RESEARCH_IMPORTED};
use crate::data::research::{cumulative_research_level_bonuses, ResearchCatalog};

/// One row from `research.imported.json` with catalog resolution and per-row combat slice.
#[derive(Debug, Clone, Serialize)]
pub struct ResearchSummaryRow {
    pub rid: i64,
    pub level: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research_name: Option<String>,
    /// True when `research_catalog.json` contains this `rid`.
    pub catalog_record_present: bool,
    /// Combat-relevant bonuses from this research only at the synced level (same rules as merge).
    #[serde(default, skip_serializing_if = "combat_bonuses_empty")]
    pub combat_bonuses_from_row: HashMap<String, f64>,
}

fn combat_bonuses_empty(m: &HashMap<String, f64>) -> bool {
    m.is_empty()
}

/// Effective research-derived combat bonuses for the active profile (same merge as scenario / optimize).
#[derive(Debug, Clone, Serialize)]
pub struct ResearchCombatSummary {
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub synced_research_count: usize,
    /// `rid` values present in sync with no catalog entry.
    pub unmapped_rids: Vec<i64>,
    /// Aggregated combat stat bonuses from all synced research (engine keys).
    #[serde(default, skip_serializing_if = "combat_bonuses_empty")]
    pub combat_bonuses_from_research: HashMap<String, f64>,
    pub research: Vec<ResearchSummaryRow>,
}

fn by_rid(catalog: &ResearchCatalog) -> HashMap<i64, &crate::data::research::ResearchRecord> {
    catalog.items.iter().map(|r| (r.rid, r)).collect()
}

fn effective_level_u32(level: i64) -> u32 {
    if level > 0 {
        level.min(i64::from(u32::MAX)) as u32
    } else {
        0
    }
}

/// Builds a summary for `profiles/{profile_id}/` using the same paths and merge rules as the optimizer.
pub fn research_combat_summary_for_profile(
    profile_id: &str,
    catalog: Option<&ResearchCatalog>,
) -> ResearchCombatSummary {
    let profile_json = profile_path(profile_id, PROFILE_JSON);
    let player = load_profile(&profile_json.to_string_lossy());
    let research_path = profile_path(profile_id, RESEARCH_IMPORTED)
        .to_string_lossy()
        .to_string();
    let imported = import::load_imported_research(&research_path).unwrap_or_default();

    let Some(catalog) = catalog.filter(|c| !c.items.is_empty()) else {
        return ResearchCombatSummary {
            profile_id: profile_id.to_string(),
            error: Some(
                "missing or empty research catalog (data/research_catalog.json)".to_string(),
            ),
            synced_research_count: imported.len(),
            unmapped_rids: imported.iter().map(|e| e.rid).collect(),
            combat_bonuses_from_research: HashMap::new(),
            research: Vec::new(),
        };
    };

    let catalog_by_rid = by_rid(catalog);

    let mut rows: Vec<ResearchSummaryRow> = imported
        .iter()
        .map(|e| {
            let catalog_record_present = catalog_by_rid.contains_key(&e.rid);
            let research_name = catalog_by_rid
                .get(&e.rid)
                .and_then(|r| r.name.clone());
            let lvl_u32 = effective_level_u32(e.level);
            let combat_bonuses_from_row = if catalog_record_present && lvl_u32 > 0 {
                let rec = catalog_by_rid.get(&e.rid).copied().unwrap();
                let raw = cumulative_research_level_bonuses(rec, lvl_u32);
                let mut slice = PlayerProfile::default();
                accumulate_combat_only_bonuses_from_raw(&mut slice, &raw);
                slice.bonuses
            } else {
                HashMap::new()
            };
            ResearchSummaryRow {
                rid: e.rid,
                level: e.level,
                research_name,
                catalog_record_present,
                combat_bonuses_from_row,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.rid.cmp(&b.rid));

    let unmapped_rids: Vec<i64> = imported
        .iter()
        .filter(|e| !catalog_by_rid.contains_key(&e.rid))
        .map(|e| e.rid)
        .collect();

    let mut scratch = PlayerProfile {
        ops_level: player.ops_level,
        bonuses: HashMap::new(),
        forbidden_tech_override: None,
        chaos_tech_override: None,
    };
    merge_research_bonuses_into_profile(&mut scratch, &imported, catalog);

    ResearchCombatSummary {
        profile_id: profile_id.to_string(),
        error: None,
        synced_research_count: imported.len(),
        unmapped_rids,
        combat_bonuses_from_research: scratch.bonuses,
        research: rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::import::ResearchEntry;
    use crate::data::research::{ResearchBonusEntry, ResearchLevel, ResearchRecord};

    fn tiny_catalog() -> ResearchCatalog {
        ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 42,
                name: Some("Test Lab".to_string()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.03,
                        operator: "add".to_string(),
                    }],
                }],
            }],
        }
    }

    #[test]
    fn summary_empty_import_no_error_when_catalog_ok() {
        let cat = tiny_catalog();
        let s = research_combat_summary_for_profile("nonexistent-profile-xyz", Some(&cat));
        assert!(s.error.is_none());
        assert_eq!(s.synced_research_count, 0);
        assert!(s.combat_bonuses_from_research.is_empty());
    }

    #[test]
    fn summary_without_catalog_sets_error() {
        let s = research_combat_summary_for_profile("p", None);
        assert!(s.error.is_some());
        assert!(s.combat_bonuses_from_research.is_empty());
    }

    #[test]
    fn merge_with_fixture_matches_row_unmapped_logic() {
        let cat = tiny_catalog();
        let imported = vec![
            ResearchEntry { rid: 42, level: 1 },
            ResearchEntry { rid: 99999, level: 5 },
        ];
        let mut scratch = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut scratch, &imported, &cat);
        assert_eq!(scratch.bonuses.get("weapon_damage").copied(), Some(0.03));

        let catalog_by_rid = by_rid(&cat);
        let unmapped: Vec<i64> = imported
            .iter()
            .filter(|e| !catalog_by_rid.contains_key(&e.rid))
            .map(|e| e.rid)
            .collect();
        assert_eq!(unmapped, vec![99999]);
    }
}
