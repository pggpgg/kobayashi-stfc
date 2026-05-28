//! Read-only summary of synced research levels and effective ship-combat bonuses (profile + research catalog).

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::combat::types::{OpponentFactionTag, ShipType};
use crate::data::import;
use crate::data::profile::{
    combat_research_bonuses_from_import, combat_research_conditional_bonuses_from_import,
    combat_research_owner_faction_bonuses_from_import, is_support_buff_gated_research_rid,
    research_levels_by_rid_from_import,
};
use crate::data::profile_index::{profile_path, RESEARCH_IMPORTED};
use crate::data::research::{
    cumulative_dual_gate_hull_shield_research_fractions, cumulative_research_level_conditional_bonuses,
    load_research_canonical_overrides, ResearchBonusConditionKey, ResearchCatalog, ResearchRecord,
    DEFAULT_RESEARCH_CANONICAL_PATH,
};

/// Resolved ship + hostile for scenario-effective research totals (optional query params).
#[derive(Debug, Clone, Serialize)]
pub struct ResearchSummaryScenarioContext {
    pub ship_id: String,
    pub hostile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_faction: Option<String>,
    pub defender_faction: String,
    pub defender_ship_class: String,
}

/// One conditional research bonus line (attack-phase seat source, not flat profile merge).
#[derive(Debug, Clone, Serialize)]
pub struct ResearchConditionalBonusLine {
    pub stat: String,
    pub value: f64,
    #[serde(flatten)]
    pub condition: ResearchBonusConditionKey,
    /// True when morale, burning, or hull breach must be active in combat (not inferred from scenario ids alone).
    pub requires_runtime_state: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition_label: Option<String>,
}

/// Synced research row with no catalog entry (sorted by level for triage).
#[derive(Debug, Clone, Serialize)]
pub struct UnmappedResearchEntry {
    pub rid: i64,
    pub level: i64,
}

/// One row from `research.imported.json` with catalog resolution and per-row combat slice.
#[derive(Debug, Clone, Serialize)]
pub struct ResearchSummaryRow {
    pub rid: i64,
    pub level: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research_name: Option<String>,
    /// True when `research_catalog.json` contains this `rid`.
    pub catalog_record_present: bool,
    /// How this row contributes to combat: `unmapped`, `non_combat`, `flat`, `owner_faction`, `conditional`, `mixed`, `support_buff_gated`.
    pub combat_kind: String,
    /// Combat-relevant bonuses from this research only at the synced level (same rules as merge).
    #[serde(default, skip_serializing_if = "combat_bonuses_empty")]
    pub combat_bonuses_from_row: HashMap<String, f64>,
    /// Owner-hull-faction-gated research for this row only (`faction_slug` → stat → value).
    #[serde(default, skip_serializing_if = "owner_faction_nested_empty")]
    pub combat_owner_faction_bonuses_from_row: HashMap<String, HashMap<String, f64>>,
    /// Conditional / seat-derived bonuses for this row only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combat_conditional_bonuses_from_row: Vec<ResearchConditionalBonusLine>,
}

fn combat_bonuses_empty(m: &HashMap<String, f64>) -> bool {
    m.is_empty()
}

fn owner_faction_nested_empty(m: &HashMap<String, HashMap<String, f64>>) -> bool {
    m.is_empty() || m.values().all(|inner| inner.is_empty())
}

/// Optional ship + hostile ids for scenario-effective totals (`GET …?ship_id=&hostile_id=`).
#[derive(Debug, Clone, Default)]
pub struct ResearchSummaryOptions {
    pub ship_id: Option<String>,
    pub hostile_id: Option<String>,
}

/// Effective research-derived combat bonuses for the active profile (same merge as scenario / optimize).
#[derive(Debug, Clone, Serialize)]
pub struct ResearchCombatSummary {
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub synced_research_count: usize,
    /// `rid` values present in sync with no catalog entry (legacy flat list; see `unmapped_research`).
    pub unmapped_rids: Vec<i64>,
    /// Unmapped rows with levels, sorted by level descending (actionable triage list).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmapped_research: Vec<UnmappedResearchEntry>,
    /// Aggregated combat stat bonuses from all synced research (engine keys).
    #[serde(default, skip_serializing_if = "combat_bonuses_empty")]
    pub combat_bonuses_from_research: HashMap<String, f64>,
    /// Cumulative owner-faction-gated research (`faction_slug` → stat → value); same merge as
    /// [`crate::data::profile::PlayerProfile::research_owner_faction_bonuses`].
    #[serde(default, skip_serializing_if = "owner_faction_nested_empty")]
    pub combat_owner_faction_bonuses_from_research: HashMap<String, HashMap<String, f64>>,
    /// Conditional research compiled to attack-phase seats (not in flat `combat_bonuses_from_research`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combat_conditional_bonuses_from_research: Vec<ResearchConditionalBonusLine>,
    /// When `ship_id` + `hostile_id` query params resolve, describes the scenario lens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario_context: Option<ResearchSummaryScenarioContext>,
    /// Flat research totals for the scenario: unconditional + owner-faction slice for the ship + dual-gate hull/shield fractions.
    #[serde(default, skip_serializing_if = "combat_bonuses_empty")]
    pub combat_bonuses_scenario_effective: HashMap<String, f64>,
    /// Conditional lines whose static gates (faction / class / owner hull) match the scenario; runtime gates flagged on each line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combat_conditional_scenario_active: Vec<ResearchConditionalBonusLine>,
    pub research: Vec<ResearchSummaryRow>,
}

fn by_rid(catalog: &ResearchCatalog) -> HashMap<i64, &ResearchRecord> {
    catalog.items.iter().map(|r| (r.rid, r)).collect()
}

fn effective_level_u32(level: i64) -> u32 {
    if level > 0 {
        level.min(i64::from(u32::MAX)) as u32
    } else {
        0
    }
}

fn condition_requires_runtime_state(key: &ResearchBonusConditionKey) -> bool {
    key.requires_morale || key.requires_defender_burning || key.requires_defender_hull_breach
}

fn format_condition_label(key: &ResearchBonusConditionKey) -> String {
    let mut parts: Vec<String> = Vec::new();
    if key.requires_morale {
        parts.push("morale".into());
    }
    if key.requires_defender_burning {
        parts.push("defender burning".into());
    }
    if key.requires_defender_hull_breach {
        parts.push("defender hull breach".into());
    }
    if let Some(ref s) = key.defender_faction {
        parts.push(format!("vs {s}"));
    }
    if let Some(ref s) = key.defender_ship_class {
        parts.push(format!("vs {s}"));
    }
    if !key.attacker_factions.is_empty() {
        parts.push(format!("{} hull", key.attacker_factions.join("/")));
    } else if let Some(ref s) = key.attacker_faction {
        parts.push(format!("{s} hull"));
    }
    if parts.is_empty() {
        "conditional".into()
    } else {
        parts.join(", ")
    }
}

fn conditional_line(
    key: &ResearchBonusConditionKey,
    stat: &str,
    value: f64,
) -> ResearchConditionalBonusLine {
    ResearchConditionalBonusLine {
        stat: stat.to_string(),
        value,
        condition: key.clone(),
        requires_runtime_state: condition_requires_runtime_state(key),
        condition_label: Some(format_condition_label(key)),
    }
}

fn conditional_lines_from_tuples(
    tuples: &[(ResearchBonusConditionKey, String, f64)],
) -> Vec<ResearchConditionalBonusLine> {
    tuples
        .iter()
        .map(|(key, stat, value)| conditional_line(key, stat, *value))
        .collect()
}

fn classify_combat_kind(
    catalog_record_present: bool,
    rid: i64,
    flat: &HashMap<String, f64>,
    owner: &HashMap<String, HashMap<String, f64>>,
    conditional: &[ResearchConditionalBonusLine],
) -> String {
    if !catalog_record_present {
        return "unmapped".to_string();
    }
    if is_support_buff_gated_research_rid(rid) {
        return "support_buff_gated".to_string();
    }
    let has_flat = !flat.is_empty();
    let has_owner = !owner.is_empty();
    let has_cond = !conditional.is_empty();
    match (has_flat, has_owner, has_cond) {
        (false, false, false) => "non_combat".to_string(),
        (true, false, false) => "flat".to_string(),
        (false, true, false) => "owner_faction".to_string(),
        (false, false, true) => "conditional".to_string(),
        _ => "mixed".to_string(),
    }
}

fn owner_faction_keys_match_slug(
    key: &ResearchBonusConditionKey,
    owner_lc: &str,
) -> bool {
    if !key.attacker_factions.is_empty() {
        return key.attacker_factions.iter().any(|raw| {
            raw.trim()
                .to_ascii_lowercase()
                .eq_ignore_ascii_case(owner_lc)
        });
    }
    key.attacker_faction
        .as_ref()
        .is_some_and(|raw| raw.trim().to_ascii_lowercase() == owner_lc)
}

fn research_condition_static_parts_match_scenario(
    key: &ResearchBonusConditionKey,
    ship_faction: Option<&str>,
    defender_faction: OpponentFactionTag,
    defender_ship_type: ShipType,
) -> bool {
    if let Some(ref slug) = key.defender_faction {
        let Some(tag) = OpponentFactionTag::from_data_slug(slug) else {
            return false;
        };
        if tag != defender_faction {
            return false;
        }
    }
    if let Some(ref slug) = key.defender_ship_class {
        let Some(st) = ShipType::from_data_slug(slug) else {
            return false;
        };
        if st != defender_ship_type {
            return false;
        }
    }
    let needs_owner = !key.attacker_factions.is_empty() || key.attacker_faction.is_some();
    if needs_owner {
        let Some(owner_lc) = ship_faction
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
        else {
            return false;
        };
        if !owner_faction_keys_match_slug(key, owner_lc.as_str()) {
            return false;
        }
    }
    true
}

struct ScenarioResolved {
    context: ResearchSummaryScenarioContext,
    defender_faction: OpponentFactionTag,
    defender_ship_type: ShipType,
}

fn build_scenario_effective(
    imported: &[import::ResearchEntry],
    catalog: &ResearchCatalog,
    exclude: &HashSet<i64>,
    flat: &HashMap<String, f64>,
    owner_nested: &HashMap<String, HashMap<String, f64>>,
    conditional: &[ResearchConditionalBonusLine],
    scenario: &ScenarioResolved,
) -> (HashMap<String, f64>, Vec<ResearchConditionalBonusLine>) {
    let mut effective = flat.clone();
    if let Some(ref faction) = scenario.context.ship_faction {
        let fk = faction.trim().to_ascii_lowercase();
        if let Some(inner) = owner_nested.get(&fk) {
            for (stat, value) in inner {
                let cur = effective.get(stat).copied().unwrap_or(0.0);
                effective.insert(stat.clone(), cur + value);
            }
        }
    }

    let levels_by_rid = research_levels_by_rid_from_import(imported);
    let records: Vec<&ResearchRecord> = catalog
        .items
        .iter()
        .filter(|r| levels_by_rid.contains_key(&r.rid) && !exclude.contains(&r.rid))
        .collect();
    let (hull_frac, shield_frac) = cumulative_dual_gate_hull_shield_research_fractions(
        &records,
        &levels_by_rid,
        scenario.context.ship_faction.as_deref(),
        scenario.defender_faction,
    );
    if hull_frac != 0.0 {
        let cur = effective.get("hull_hp").copied().unwrap_or(0.0);
        effective.insert("hull_hp".into(), cur + hull_frac);
    }
    if shield_frac != 0.0 {
        let cur = effective.get("shield_hp").copied().unwrap_or(0.0);
        effective.insert("shield_hp".into(), cur + shield_frac);
    }

    let active_conditional: Vec<ResearchConditionalBonusLine> = conditional
        .iter()
        .filter(|line| {
            research_condition_static_parts_match_scenario(
                &line.condition,
                scenario.context.ship_faction.as_deref(),
                scenario.defender_faction,
                scenario.defender_ship_type,
            )
        })
        .cloned()
        .collect();

    (effective, active_conditional)
}

fn row_conditional_lines_for_record(
    record: &ResearchRecord,
    level: u32,
    exclude_rid: bool,
) -> Vec<ResearchConditionalBonusLine> {
    if exclude_rid || level == 0 {
        return Vec::new();
    }
    let partial = cumulative_research_level_conditional_bonuses(record, level);
    let mut out: Vec<ResearchConditionalBonusLine> = Vec::new();
    for ((key, stat), value) in partial {
        if value == 0.0 {
            continue;
        }
        out.push(conditional_line(&key, &stat, value));
    }
    out.sort_by(|a, b| a.stat.cmp(&b.stat));
    out
}

/// Builds a summary for `profiles/{profile_id}/` using the same paths and merge rules as the optimizer.
pub fn research_combat_summary_for_profile(
    profile_id: &str,
    catalog: Option<&ResearchCatalog>,
) -> ResearchCombatSummary {
    research_combat_summary_for_profile_with_options(profile_id, catalog, &ResearchSummaryOptions::default(), None)
}

/// Same as [`research_combat_summary_for_profile`] with optional scenario lens from resolved ship + hostile.
pub fn research_combat_summary_for_profile_with_scenario(
    profile_id: &str,
    catalog: Option<&ResearchCatalog>,
    ship_id: Option<&str>,
    hostile_id: Option<&str>,
    ship_faction: Option<String>,
    defender_faction: Option<OpponentFactionTag>,
    defender_ship_class: Option<&str>,
) -> ResearchCombatSummary {
    let options = ResearchSummaryOptions {
        ship_id: ship_id.map(str::to_string),
        hostile_id: hostile_id.map(str::to_string),
    };
    let scenario = match (ship_id, hostile_id, defender_faction, defender_ship_class) {
        (Some(ship), Some(hostile), Some(df), Some(sc)) => Some(resolve_research_summary_scenario(
            ship,
            hostile,
            ship_faction,
            df,
            sc,
        )),
        _ => None,
    };
    research_combat_summary_for_profile_with_options(profile_id, catalog, &options, scenario)
}

/// Same as [`research_combat_summary_for_profile`] with optional scenario lens and pre-resolved context.
fn research_combat_summary_for_profile_with_options(
    profile_id: &str,
    catalog: Option<&ResearchCatalog>,
    options: &ResearchSummaryOptions,
    scenario: Option<ScenarioResolved>,
) -> ResearchCombatSummary {
    let research_path = profile_path(profile_id, RESEARCH_IMPORTED)
        .to_string_lossy()
        .to_string();
    let imported = import::load_imported_research(&research_path).unwrap_or_default();

    research_combat_summary_from_imported(profile_id, &imported, catalog, options, scenario)
}

fn research_combat_summary_from_imported(
    profile_id: &str,
    imported: &[import::ResearchEntry],
    catalog: Option<&ResearchCatalog>,
    _options: &ResearchSummaryOptions,
    scenario: Option<ScenarioResolved>,
) -> ResearchCombatSummary {
    let catalog_nonempty = catalog.filter(|c| !c.items.is_empty());
    let catalog_by_rid: Option<HashMap<i64, &ResearchRecord>> = catalog_nonempty.map(by_rid);
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
                combat_conditional_bonuses_from_row,
            ) = match catalog_by_rid.as_ref() {
                None => (
                    false,
                    None,
                    HashMap::new(),
                    HashMap::new(),
                    Vec::new(),
                ),
                Some(map) => {
                    let present = map.contains_key(&e.rid);
                    let name = map.get(&e.rid).and_then(|r| r.name.clone());
                    let lvl_u32 = effective_level_u32(e.level);
                    let exclude_rid = exclude_catalog_rids.contains(&e.rid);
                    let (combat_bonuses_from_row, combat_owner_faction_bonuses_from_row) =
                        if present && lvl_u32 > 0 && !exclude_rid {
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
                    let combat_conditional_bonuses_from_row =
                        if present && lvl_u32 > 0 && !exclude_rid {
                            map.get(&e.rid)
                                .map(|rec| row_conditional_lines_for_record(rec, lvl_u32, false))
                                .unwrap_or_default()
                        } else if present && exclude_rid {
                            map.get(&e.rid)
                                .map(|rec| row_conditional_lines_for_record(rec, lvl_u32, true))
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                    (
                        present,
                        name,
                        combat_bonuses_from_row,
                        combat_owner_faction_bonuses_from_row,
                        combat_conditional_bonuses_from_row,
                    )
                }
            };
            let combat_kind = classify_combat_kind(
                catalog_record_present,
                e.rid,
                &combat_bonuses_from_row,
                &combat_owner_faction_bonuses_from_row,
                &combat_conditional_bonuses_from_row,
            );
            ResearchSummaryRow {
                rid: e.rid,
                level: e.level,
                research_name,
                catalog_record_present,
                combat_kind,
                combat_bonuses_from_row,
                combat_owner_faction_bonuses_from_row,
                combat_conditional_bonuses_from_row,
            }
        })
        .collect();
    rows.sort_by_key(|a| a.rid);

    let levels_by_rid = research_levels_by_rid_from_import(imported);
    let mut unmapped_research: Vec<UnmappedResearchEntry> = levels_by_rid
        .iter()
        .filter(|(rid, _)| {
            catalog_by_rid
                .as_ref()
                .map(|m| !m.contains_key(rid))
                .unwrap_or(true)
        })
        .map(|(&rid, &level)| UnmappedResearchEntry {
            rid,
            level: i64::from(level),
        })
        .collect();
    unmapped_research.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.rid.cmp(&b.rid)));

    let unmapped_rids: Vec<i64> = unmapped_research.iter().map(|e| e.rid).collect();

    let combat_bonuses_from_research = catalog_nonempty
        .map(|cat| combat_research_bonuses_from_import(imported, cat, Some(&exclude_catalog_rids)))
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

    let conditional_tuples = catalog_nonempty
        .map(|cat| {
            combat_research_conditional_bonuses_from_import(
                imported,
                cat,
                Some(&exclude_catalog_rids),
            )
        })
        .unwrap_or_default();
    let combat_conditional_bonuses_from_research =
        conditional_lines_from_tuples(&conditional_tuples);

    let (scenario_context, combat_bonuses_scenario_effective, combat_conditional_scenario_active) =
        if let Some(ref sc) = scenario {
            let (eff, active) = build_scenario_effective(
                imported,
                catalog_nonempty.expect("scenario requires catalog"),
                &exclude_catalog_rids,
                &combat_bonuses_from_research,
                &combat_owner_faction_bonuses_from_research,
                &combat_conditional_bonuses_from_research,
                sc,
            );
            (
                Some(sc.context.clone()),
                eff,
                active,
            )
        } else {
            (None, HashMap::new(), Vec::new())
        };

    let error = catalog_nonempty.is_none().then(|| {
        "missing or empty research catalog (data/research_catalog.json); combat bonuses from research are not applied. Synced rid/level pairs remain stored in research.imported.json.".to_string()
    });

    ResearchCombatSummary {
        profile_id: profile_id.to_string(),
        error,
        synced_research_count: imported.len(),
        unmapped_rids,
        unmapped_research,
        combat_bonuses_from_research,
        combat_owner_faction_bonuses_from_research,
        combat_conditional_bonuses_from_research,
        scenario_context,
        combat_bonuses_scenario_effective,
        combat_conditional_scenario_active,
        research: rows,
    }
}

/// Resolve optional scenario query params into ship/hostile context.
fn resolve_research_summary_scenario(
    ship_id: &str,
    hostile_id: &str,
    ship_faction: Option<String>,
    defender_faction: OpponentFactionTag,
    defender_ship_class: &str,
) -> ScenarioResolved {
    let defender_ship_type =
        ShipType::from_data_slug(defender_ship_class).unwrap_or(ShipType::Battleship);
    let defender_faction_slug = serde_json::to_value(defender_faction)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "unknown".into());
    ScenarioResolved {
        context: ResearchSummaryScenarioContext {
            ship_id: ship_id.to_string(),
            hostile_id: hostile_id.to_string(),
            ship_faction,
            defender_faction: defender_faction_slug,
            defender_ship_class: defender_ship_class.to_string(),
        },
        defender_faction,
        defender_ship_type,
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
        ResearchBonusEntry, ResearchLevel, ResearchRecord,
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
        assert!(s.unmapped_research.is_empty());
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
    fn unmapped_research_sorted_by_level_desc() {
        let cat = tiny_catalog();
        let imported = vec![
            ResearchEntry {
                rid: 100,
                level: 3,
            },
            ResearchEntry {
                rid: 200,
                level: 10,
            },
            ResearchEntry { rid: 42, level: 1 },
        ];
        let s = research_combat_summary_from_imported(
            "p",
            &imported,
            Some(&cat),
            &ResearchSummaryOptions::default(),
            None,
        );
        assert_eq!(s.unmapped_research.len(), 2);
        assert_eq!(s.unmapped_research[0].rid, 200);
        assert_eq!(s.unmapped_research[0].level, 10);
        assert_eq!(s.unmapped_research[1].rid, 100);
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

        let summary = research_combat_summary_from_imported(
            "higgsbozo",
            &imported,
            Some(&cat),
            &ResearchSummaryOptions::default(),
            None,
        );
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
        let gated_row = summary
            .research
            .iter()
            .find(|row| row.rid == gated_rid)
            .expect("gated research row present");
        assert_eq!(gated_row.combat_kind, "support_buff_gated");
        assert!(
            gated_row.combat_bonuses_from_row.is_empty(),
            "support-buff-gated research must not appear in default profile research summaries"
        );
        assert_eq!(
            summary
                .research
                .iter()
                .find(|r| r.rid == 42)
                .unwrap()
                .combat_kind,
            "flat"
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
        let imported = vec![ResearchEntry {
            rid: 9001,
            level: 1,
        }];
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported, &cat, None);
        let s = research_combat_summary_from_imported(
            "p",
            &imported,
            Some(&cat),
            &ResearchSummaryOptions::default(),
            None,
        );
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
        assert_eq!(row.combat_kind, "owner_faction");
        assert_eq!(
            row.combat_owner_faction_bonuses_from_row,
            profile.research_owner_faction_bonuses
        );
    }

    #[test]
    fn summary_conditional_row_classified_and_listed() {
        let cat = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 501,
                name: Some("Crit vs Explorer".into()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "crit_chance".into(),
                        value: 0.05,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            defender_ship_class: Some("explorer".into()),
                            ..Default::default()
                        },
                    }],
                }],
            }],
        };
        let imported = vec![ResearchEntry { rid: 501, level: 1 }];
        let s = research_combat_summary_from_imported(
            "p",
            &imported,
            Some(&cat),
            &ResearchSummaryOptions::default(),
            None,
        );
        assert!(s.combat_bonuses_from_research.is_empty());
        assert_eq!(s.combat_conditional_bonuses_from_research.len(), 1);
        assert_eq!(s.combat_conditional_bonuses_from_research[0].stat, "crit_chance");
        let row = s.research.iter().find(|r| r.rid == 501).unwrap();
        assert_eq!(row.combat_kind, "conditional");
        assert_eq!(row.combat_conditional_bonuses_from_row.len(), 1);
    }

    #[test]
    fn scenario_effective_includes_owner_faction_for_ship() {
        let cat = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 9001,
                name: Some("Fed-only".into()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".into(),
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
        let imported = vec![ResearchEntry {
            rid: 9001,
            level: 1,
        }];
        let scenario = resolve_research_summary_scenario(
            "test_ship",
            "test_hostile",
            Some("federation".into()),
            OpponentFactionTag::Klingon,
            "battleship",
        );
        let s = research_combat_summary_from_imported(
            "p",
            &imported,
            Some(&cat),
            &ResearchSummaryOptions {
                ship_id: Some("test_ship".into()),
                hostile_id: Some("test_hostile".into()),
            },
            Some(scenario),
        );
        assert_eq!(
            s.combat_bonuses_scenario_effective.get("weapon_damage").copied(),
            Some(0.04)
        );
        assert!(s.scenario_context.is_some());
    }
}
