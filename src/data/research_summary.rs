//! Read-only summary of synced research levels and effective ship-combat bonuses (profile + research catalog).

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::data::import;
use crate::data::profile::{
    combat_research_bonuses_from_import, combat_research_owner_faction_bonuses_from_import,
};
use crate::data::profile_index::{profile_path, RESEARCH_IMPORTED};
use crate::data::research::{
    load_research_canonical_overrides, ResearchCatalog, DEFAULT_RESEARCH_CANONICAL_PATH,
};

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
    /// Owner-hull-faction-gated research for this row only (`faction_slug` → stat → value).
    #[serde(default, skip_serializing_if = "owner_faction_nested_empty")]
    pub combat_owner_faction_bonuses_from_row: HashMap<String, HashMap<String, f64>>,
}

fn combat_bonuses_empty(m: &HashMap<String, f64>) -> bool {
    m.is_empty()
}

fn owner_faction_nested_empty(m: &HashMap<String, HashMap<String, f64>>) -> bool {
    m.is_empty() || m.values().all(|inner| inner.is_empty())
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
    /// Cumulative owner-faction-gated research (`faction_slug` → stat → value); same merge as
    /// [`crate::data::profile::PlayerProfile::research_owner_faction_bonuses`].
    #[serde(default, skip_serializing_if = "owner_faction_nested_empty")]
    pub combat_owner_faction_bonuses_from_research: HashMap<String, HashMap<String, f64>>,
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
    let research_path = profile_path(profile_id, RESEARCH_IMPORTED)
        .to_string_lossy()
        .to_string();
    let imported = import::load_imported_research(&research_path).unwrap_or_default();

    research_combat_summary_from_imported(profile_id, &imported, catalog)
}

fn research_combat_summary_from_imported(
    profile_id: &str,
    imported: &[import::ResearchEntry],
    catalog: Option<&ResearchCatalog>,
) -> ResearchCombatSummary {
    let catalog_nonempty = catalog.filter(|c| !c.items.is_empty());
    let catalog_by_rid: Option<HashMap<i64, &crate::data::research::ResearchRecord>> =
        catalog_nonempty.map(by_rid);
    let canonical_overrides = load_research_canonical_overrides(DEFAULT_RESEARCH_CANONICAL_PATH);
    let exclude_catalog_rids: HashSet<i64> = canonical_overrides.keys().copied().collect();

    let mut rows: Vec<ResearchSummaryRow> = imported
        .iter()
        .map(|e| {
            let (
                catalog_record_present,
                research_name,
                combat_bonuses_from_row,
                combat_owner_faction_bonuses_from_row,
            ) = match catalog_by_rid.as_ref() {
                None => (false, None, HashMap::new(), HashMap::new()),
                Some(map) => {
                    let present = map.contains_key(&e.rid);
                    let name = map.get(&e.rid).and_then(|r| r.name.clone());
                    let lvl_u32 = effective_level_u32(e.level);
                    let (combat_bonuses_from_row, combat_owner_faction_bonuses_from_row) =
                        if present && lvl_u32 > 0 {
                            let one = [e.clone()];
                            match catalog_nonempty {
                                Some(cat) => (
                                    combat_research_bonuses_from_import(
                                        &one,
                                        cat,
                                        Some(&exclude_catalog_rids),
                                    ),
                                    combat_research_owner_faction_bonuses_from_import(
                                        &one,
                                        cat,
                                        Some(&exclude_catalog_rids),
                                    ),
                                ),
                                None => (HashMap::new(), HashMap::new()),
                            }
                        } else {
                            (HashMap::new(), HashMap::new())
                        };
                    (
                        present,
                        name,
                        combat_bonuses_from_row,
                        combat_owner_faction_bonuses_from_row,
                    )
                }
            };
            ResearchSummaryRow {
                rid: e.rid,
                level: e.level,
                research_name,
                catalog_record_present,
                combat_bonuses_from_row,
                combat_owner_faction_bonuses_from_row,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.rid.cmp(&b.rid));

    let mut unmapped_rids: Vec<i64> = imported
        .iter()
        .filter(|e| {
            catalog_by_rid
                .as_ref()
                .map(|m| !m.contains_key(&e.rid))
                .unwrap_or(true)
        })
        .map(|e| e.rid)
        .collect();
    unmapped_rids.sort_unstable();
    unmapped_rids.dedup();

    // Fortify-gated Titan `rid`s are never in this aggregate (they fold into Fortify static in scenario).
    let combat_bonuses_from_research = catalog_nonempty
        .map(|cat| {
            combat_research_bonuses_from_import(imported, cat, Some(&exclude_catalog_rids))
        })
        .unwrap_or_default();

    let combat_owner_faction_bonuses_from_research = catalog_nonempty
        .map(|cat| {
            combat_research_owner_faction_bonuses_from_import(
                imported,
                cat,
                Some(&exclude_catalog_rids),
            )
        })
        .unwrap_or_default();

    let error = catalog_nonempty.is_none().then(|| {
        "missing or empty research catalog (data/research_catalog.json); combat bonuses from research are not applied. Synced rid/level pairs remain stored in research.imported.json.".to_string()
    });

    ResearchCombatSummary {
        profile_id: profile_id.to_string(),
        error,
        synced_research_count: imported.len(),
        unmapped_rids,
        combat_bonuses_from_research,
        combat_owner_faction_bonuses_from_research,
        research: rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::import::ResearchEntry;
    use crate::data::profile::{
        merge_research_bonuses_into_profile, PlayerProfile,
        TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS,
    };
    use crate::data::research::{
        ResearchBonusConditionKey, ResearchBonusEntry, ResearchLevel, ResearchRecord,
    };

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
                        condition: Default::default(),
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
            ResearchEntry {
                rid: 99999,
                level: 5,
            },
        ];
        let mut scratch = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut scratch, &imported, &cat, None);
        assert_eq!(scratch.bonuses.get("weapon_damage").copied(), Some(0.03));

        let catalog_by_rid = by_rid(&cat);
        let unmapped: Vec<i64> = imported
            .iter()
            .filter(|e| !catalog_by_rid.contains_key(&e.rid))
            .map(|e| e.rid)
            .collect();
        assert_eq!(unmapped, vec![99999]);
    }

    #[test]
    fn summary_aggregate_matches_profile_merge_for_representative_import() {
        let gated_rid = TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS[0];
        let cat = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![
                ResearchRecord {
                    rid: 42,
                    name: Some("Weapon Lab".to_string()),
                    data_version: None,
                    source_note: None,
                    levels: vec![
                        ResearchLevel {
                            level: 1,
                            bonuses: vec![ResearchBonusEntry {
                                stat: "weapon_damage".to_string(),
                                value: 0.03,
                                operator: "add".to_string(),
                                condition: Default::default(),
                            }],
                        },
                        ResearchLevel {
                            level: 2,
                            bonuses: vec![ResearchBonusEntry {
                                stat: "weapon_damage".to_string(),
                                value: 0.02,
                                operator: "add".to_string(),
                                condition: Default::default(),
                            }],
                        },
                    ],
                },
                ResearchRecord {
                    rid: 7,
                    name: Some("Hull Lab".to_string()),
                    data_version: None,
                    source_note: None,
                    levels: vec![ResearchLevel {
                        level: 1,
                        bonuses: vec![ResearchBonusEntry {
                            stat: "hull_hp".to_string(),
                            value: 100.0,
                            operator: "add".to_string(),
                            condition: Default::default(),
                        }],
                    }],
                },
                ResearchRecord {
                    rid: gated_rid,
                    name: Some("Titan Gated Lab".to_string()),
                    data_version: None,
                    source_note: None,
                    levels: vec![ResearchLevel {
                        level: 1,
                        bonuses: vec![ResearchBonusEntry {
                            stat: "weapon_damage".to_string(),
                            value: 9.99,
                            operator: "add".to_string(),
                            condition: Default::default(),
                        }],
                    }],
                },
            ],
        };
        let imported = vec![
            ResearchEntry { rid: 42, level: 1 },
            ResearchEntry { rid: 42, level: 2 },
            ResearchEntry { rid: 7, level: 1 },
            ResearchEntry {
                rid: gated_rid,
                level: 1,
            },
            ResearchEntry {
                rid: 99999,
                level: 5,
            },
        ];

        let summary = research_combat_summary_from_imported("higgsbozo", &imported, Some(&cat));
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported, &cat, None);

        assert_eq!(summary.profile_id, "higgsbozo");
        assert_eq!(summary.synced_research_count, imported.len());
        assert_eq!(summary.unmapped_rids, vec![99999]);
        assert_eq!(summary.combat_bonuses_from_research, profile.bonuses);
        assert_eq!(
            summary.combat_owner_faction_bonuses_from_research,
            profile.research_owner_faction_bonuses
        );
        assert_eq!(
            summary.combat_bonuses_from_research.get("weapon_damage"),
            Some(&0.05)
        );
        assert_eq!(
            summary.combat_bonuses_from_research.get("hull_hp"),
            Some(&100.0)
        );
        assert!(
            summary
                .research
                .iter()
                .find(|row| row.rid == gated_rid)
                .expect("gated research row present")
                .combat_bonuses_from_row
                .is_empty(),
            "support-buff-gated research must not appear in default profile research summaries"
        );
    }

    #[test]
    fn summary_owner_faction_row_matches_merge() {
        let cat = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 9001,
                name: Some("Fed-only lab".into()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "shield_deflection".into(),
                        value: 0.04,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            attacker_faction: Some("federation".into()),
                            ..Default::default()
                        },
                    }],
                }],
            }],
        };
        let imported = vec![ResearchEntry { rid: 9001, level: 1 }];
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported, &cat, None);
        let s = research_combat_summary_from_imported("p", &imported, Some(&cat));
        assert!(!profile.bonuses.contains_key("shield_deflection"));
        assert_eq!(
            profile
                .research_owner_faction_bonuses
                .get("federation")
                .and_then(|m| m.get("shield_deflection"))
                .copied(),
            Some(0.04)
        );
        assert_eq!(
            s.combat_owner_faction_bonuses_from_research,
            profile.research_owner_faction_bonuses
        );
        let row = s.research.iter().find(|r| r.rid == 9001).unwrap();
        assert_eq!(
            row.combat_owner_faction_bonuses_from_row,
            profile.research_owner_faction_bonuses
        );
    }
}
