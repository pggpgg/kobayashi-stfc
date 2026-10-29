//! Native selected research setup families, using global active buff membership.
//!
//! Owning a project is not sufficient: Fleet Commander On Duty projects can be
//! owned while their linked buff is absent from the active global list. Native
//! FleetPlayerData.EvaluateBuffs consumes globalBuffs, independently of research
//! ownership. Canonical status/timing seats and other research families remain on
//! their existing paths; this compiler handles only known deterministic setup rows.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

use crate::combat::CrewSeatContext;
use crate::data::import::{GlobalBuffEntry, ResearchEntry};
use crate::data::native_consumable_buffs::{
    bonuses_from_catalog_for_ship, combat_start_defense_seats_from_catalog_for_group,
    NativeConsumableBuffCatalog,
};
use serde::Deserialize;

const EMBEDDED_CATALOG: &str =
    include_str!("../../data/research/client_buff_semantics_2026-10-20.json");

fn embedded_catalog() -> &'static NativeConsumableBuffCatalog {
    static CATALOG: OnceLock<NativeConsumableBuffCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        NativeConsumableBuffCatalog::from_json(EMBEDDED_CATALOG)
            .expect("reviewed embedded native research catalog must parse")
    })
}

/// `None` permits a legacy fallback when no active recognized research source is
/// present. `Some`, including an empty map, is authoritative for selected setup
/// families: absent/ineligible buffs cannot be revived by project ownership or
/// prose inference. Imported levels are one-based and expiry is in Unix seconds.
pub fn research_bonuses_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> Option<HashMap<String, f64>> {
    research_bonuses_for_ship(imported, attacker_ship_class, None, now_unix_seconds)
}

/// Scenario-aware variant retaining the verified Direct Hit minimum-grade gate.
pub fn research_bonuses_for_ship(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    ship_grade: Option<u32>,
    now_unix_seconds: i64,
) -> Option<HashMap<String, f64>> {
    let catalog = embedded_catalog();
    let recognized = imported.iter().any(|buff| {
        buff.bid > 0
            && buff.level > 0
            && buff
                .expiry_time
                .is_none_or(|expiry| expiry > now_unix_seconds)
            && catalog.contains_setup_family_buff(buff.bid)
    });
    recognized.then(|| {
        bonuses_from_catalog_for_ship(
            catalog,
            66,
            imported,
            attacker_ship_class,
            None,
            ship_grade,
            now_unix_seconds,
        )
    })
}

#[derive(Deserialize)]
struct ResearchCoverage {
    rows: Vec<ResearchCoverageRow>,
}

#[derive(Deserialize)]
struct ResearchCoverageRow {
    buff_id: i64,
    modifier_code: i64,
    exclusion_reason: Option<String>,
    projects: Vec<ResearchCoverageProject>,
}

#[derive(Deserialize)]
struct ResearchCoverageProject {
    rid: i64,
    name: String,
}

fn quarantined_apex_rows() -> &'static [ResearchCoverageRow] {
    static ROWS: OnceLock<Vec<ResearchCoverageRow>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let coverage: ResearchCoverage = serde_json::from_str(include_str!(
            "../../data/research/client_buff_coverage_2026-10-20.json"
        ))
        .expect("reviewed native research coverage must parse");
        coverage
            .rows
            .into_iter()
            .filter(|row| row.modifier_code == 67001 && row.exclusion_reason.is_some())
            .collect()
    })
}

/// Diagnostic for research Apex effects excluded because their native combat
/// gates or targets are unknown. Owning a project is not proof that its buff
/// applies; active rank values are raw source values, not a stacked total.
pub fn quarantined_apex_research_warning(
    research: Option<&[ResearchEntry]>,
    buffs: Option<&[GlobalBuffEntry]>,
    now_unix_seconds: i64,
) -> Option<String> {
    let owned: BTreeSet<i64> = research
        .into_iter()
        .flatten()
        .filter(|entry| entry.rid > 0 && entry.level > 0)
        .map(|entry| entry.rid)
        .collect();
    let mut active = BTreeMap::<i64, i64>::new();
    for buff in buffs.into_iter().flatten() {
        if buff.bid > 0
            && buff.level > 0
            && buff
                .expiry_time
                .is_none_or(|expiry| expiry > now_unix_seconds)
        {
            active
                .entry(buff.bid)
                .and_modify(|level| *level = (*level).max(buff.level))
                .or_insert(buff.level);
        }
    }
    let rows = quarantined_apex_rows();
    let projects: BTreeSet<(i64, &str)> = rows
        .iter()
        .flat_map(|row| row.projects.iter())
        .filter(|project| owned.contains(&project.rid))
        .map(|project| (project.rid, project.name.as_str()))
        .collect();
    let active_values: Vec<(i64, i64, Option<f64>)> = rows
        .iter()
        .filter_map(|row| {
            let level = *active.get(&row.buff_id)?;
            let value = embedded_catalog().raw_apex_rank_value(row.buff_id, level);
            Some((row.buff_id, level, value))
        })
        .collect();
    if projects.is_empty() && active_values.is_empty() && research.is_some() && buffs.is_some() {
        return None;
    }
    let mut details = Vec::new();
    if !projects.is_empty() {
        let names = projects
            .iter()
            .take(3)
            .map(|(_, name)| *name)
            .collect::<Vec<_>>()
            .join(", ");
        details.push(format!(
            "{} owned project(s), including {names}",
            projects.len()
        ));
    }
    if !active_values.is_empty() {
        let values = active_values
            .iter()
            .take(3)
            .map(|(bid, level, value)| match value {
                Some(value) => format!("buff {bid} rank {level}: {value}"),
                None => format!("buff {bid} rank {level}: source value unavailable"),
            })
            .collect::<Vec<_>>()
            .join(", ");
        details.push(format!(
            "{} active buff(s); raw rank values include {values}",
            active_values.len()
        ));
    }
    if research.is_none() {
        details.push("owned research snapshot unavailable".to_string());
    }
    if buffs.is_none() {
        details.push("active buff snapshot unavailable".to_string());
    }
    Some(format!(
        "Conditional Apex Barrier research is omitted where its native gates or targets cannot be evaluated: {}. Raw rank values are not an applicable or stacked total.",
        details.join("; ")
    ))
}

/// Load the caller's snapshots without conflating absent files with empty lists.
pub fn quarantined_apex_research_warning_for_profile(profile_id: Option<&str>) -> Option<String> {
    use crate::data::profile_index::{
        profile_path, resolve_profile_id_for_api, BUFFS_IMPORTED, RESEARCH_IMPORTED,
    };
    let id = resolve_profile_id_for_api(profile_id);
    let research = crate::data::import::load_imported_research(
        &profile_path(&id, RESEARCH_IMPORTED).to_string_lossy(),
    );
    let buffs = crate::data::import::load_imported_buffs(
        &profile_path(&id, BUFFS_IMPORTED).to_string_lossy(),
    );
    quarantined_apex_research_warning(
        research.as_deref(),
        buffs.as_deref(),
        chrono::Utc::now().timestamp(),
    )
}

/// Native research CombatStart ModAllDefenses effects, kept outside launch pools.
pub fn combat_start_defense_seats_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    combat_start_defense_seats_from_catalog_for_group(
        embedded_catalog(),
        66,
        imported,
        attacker_ship_class,
        now_unix_seconds,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(bid: i64, level: i64) -> GlobalBuffEntry {
        GlobalBuffEntry {
            bid,
            level,
            expiry_time: None,
        }
    }

    #[test]
    fn apex_audit_membership_does_not_enable_unrelated_setup_replacement() {
        // Native source 11214279 is conditional player-only Apex Barrier.
        // Its presence in the audit catalog is not authority to replace hull,
        // shields, damage or any other legacy setup-family research.
        assert!(
            research_bonuses_for_imported_buffs(&[entry(11214279, 1)], Some("explorer"), 0)
                .is_none()
        );
        let baseline =
            research_bonuses_for_imported_buffs(&[entry(434613423, 10)], Some("explorer"), 0);
        let with_apex = research_bonuses_for_imported_buffs(
            &[entry(434613423, 10), entry(11214279, 1)],
            Some("explorer"),
            0,
        );
        assert_eq!(baseline, with_apex);
    }

    #[test]
    fn apex_scope_warning_distinguishes_owned_active_expired_and_missing_snapshots() {
        let research = [ResearchEntry {
            rid: 2708500806,
            level: 1,
        }];
        let active = [entry(11214279, 1)];
        let warning = quarantined_apex_research_warning(Some(&research), Some(&active), 100)
            .expect("owned and active conditional research must warn");
        assert!(warning.contains("1 owned project(s)"));
        assert!(warning.contains("1 active buff(s)"));
        assert!(warning.contains("buff 11214279 rank 1: 300"));
        assert!(warning.contains("not an applicable or stacked total"));

        let mut expired = entry(11214279, 1);
        expired.expiry_time = Some(100);
        let warning =
            quarantined_apex_research_warning(Some(&research), Some(&[expired]), 100).unwrap();
        assert!(warning.contains("1 owned project(s)"));
        assert!(!warning.contains("active buff(s)"));
        assert!(quarantined_apex_research_warning(Some(&[]), Some(&[]), 100).is_none());
        let missing = quarantined_apex_research_warning(None, None, 100).unwrap();
        assert!(missing.contains("owned research snapshot unavailable"));
        assert!(missing.contains("active buff snapshot unavailable"));
    }

    #[test]
    fn owned_but_inactive_project_cannot_enter_selected_setup_pools() {
        // Buff748660265 links to Fleet Commander Fortified Rampart. The raw
        // BuffSpec itself is unconditional, so global membership is necessary.
        assert!(research_bonuses_for_imported_buffs(&[], Some("explorer"), 0).is_none());
        let without =
            research_bonuses_for_imported_buffs(&[entry(434613423, 10)], Some("explorer"), 0)
                .unwrap();
        assert_eq!(without.get("hull_hp_flat"), Some(&3400.0));
        assert!(!without.contains_key("hull_hp"));
        let active = research_bonuses_for_imported_buffs(
            &[entry(434613423, 10), entry(748660265, 3)],
            Some("explorer"),
            0,
        )
        .unwrap();
        assert_eq!(active.get("hull_hp"), Some(&0.4));
    }

    #[test]
    fn source_absence_expiry_and_authoritative_unsupported_scope_remain_distinct() {
        assert!(research_bonuses_for_imported_buffs(&[entry(488045092, 20)], None, 0).is_none());
        assert_eq!(
            research_bonuses_for_imported_buffs(&[entry(1898558353, 1)], Some("explorer"), 0),
            Some(HashMap::new())
        );
        let mut expired = entry(748660265, 3);
        expired.expiry_time = Some(100);
        assert!(research_bonuses_for_imported_buffs(&[expired.clone()], None, 100).is_none());
        assert_eq!(
            research_bonuses_for_imported_buffs(&[expired], None, 99)
                .unwrap()
                .get("hull_hp"),
            Some(&0.4)
        );
    }

    #[test]
    fn research_flat_hp_and_critical_additions_keep_native_rank_units() {
        let out = research_bonuses_for_imported_buffs(
            &[
                entry(4191797822, 10),
                entry(434613423, 10),
                entry(1195169860, 8),
            ],
            Some("explorer"),
            0,
        )
        .unwrap();
        assert_eq!(out.get("shield_hp_flat"), Some(&4000.0));
        assert_eq!(out.get("hull_hp_flat"), Some(&3400.0));
        assert_eq!(out.get("crit_damage_flat"), Some(&0.1));
        assert!(!out.contains_key("crit_damage"));
    }

    #[test]
    fn research_isolytic_add_uses_active_native_ranks_and_quarantines_scope_leaks() {
        let out = research_bonuses_for_imported_buffs(
            &[
                entry(3778854141, 2),
                entry(3536940873, 5),
                entry(2807517802, 14),
                entry(1037099981, 4),
                entry(3087616214, 3), // hostile/grade condition, formerly leaked +1.2
                entry(1934800456, 6), // faction attribute, formerly leaked +2.0
            ],
            Some("explorer"),
            0,
        )
        .unwrap();
        assert!((out["isolytic_damage"] - 0.305).abs() < 1e-12);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn deployed_realta_isolytic_defense_uses_native_add_ranks_and_explorer_gate() {
        let rows = [
            entry(4212632622, 5),
            entry(3883139316, 7),
            entry(3386276092, 14),
            entry(1149751341, 3),
            entry(833661631, 4),
            entry(3919113722, 19), // unsupported condition 28
            entry(2852465399, 10), // unsupported condition 124
        ];
        let explorer = research_bonuses_for_imported_buffs(&rows, Some("explorer"), 0).unwrap();
        assert!((explorer["isolytic_defense"] - 1.43).abs() < 1e-12);
        assert_eq!(explorer.len(), 1);
        let interceptor =
            research_bonuses_for_imported_buffs(&rows, Some("interceptor"), 0).unwrap();
        assert!((interceptor["isolytic_defense"] - 1.23).abs() < 1e-12);
    }

    #[test]
    fn realta_launch_defense_research_selects_only_proven_ranks_and_scope() {
        let rows = [
            entry(4281890425, 5),  // all defenses 1.1, Explorer
            entry(4205456019, 8),  // all defenses 2.7
            entry(2976909288, 10), // all defenses 1.5, Explorer
            entry(1616818672, 15), // all defenses 2.0
            entry(1398325008, 17), // all defenses 1.75
            entry(711050273, 1),   // all defenses 1.0
            entry(411115104, 5),   // plating 2.0
            entry(1255158479, 5),  // absorption 2.0
            entry(1742688098, 8),  // absorption 1.0, Explorer
            entry(951774677, 5),   // dodge 2.0
            entry(721399596, 61),  // dodge 0.4
            entry(2817985671, 15), // combat-start all defenses, outside launch pool
            entry(1019365535, 6),  // unsupported condition 123
        ];
        let explorer = research_bonuses_for_imported_buffs(&rows, Some("explorer"), 0).unwrap();
        assert!((explorer["armor"] - 12.05).abs() < 1e-12);
        assert!((explorer["shield_deflection"] - 13.05).abs() < 1e-12);
        assert!((explorer["dodge"] - 12.45).abs() < 1e-12);
        assert_eq!(explorer.len(), 3);
        let interceptor =
            research_bonuses_for_imported_buffs(&rows, Some("interceptor"), 0).unwrap();
        assert!((interceptor["armor"] - 9.45).abs() < 1e-12);
        assert!((interceptor["shield_deflection"] - 9.45).abs() < 1e-12);
        assert!((interceptor["dodge"] - 9.85).abs() < 1e-12);
    }

    #[test]
    fn combat_start_defense_research_is_separate_from_launch_pool() {
        let rows = [
            entry(2817985671, 15), // unconditional 1.75
            entry(4289628197, 15), // NPC, non-Armada 4.5
            entry(1265887920, 10), // Explorer, NPC, non-Armada 3.0
            entry(2708670536, 10), // localized Hostile Explorer 0.75
            entry(4205456019, 8),  // launch 2.7, must not become a seat
        ];
        let seats = combat_start_defense_seats_for_imported_buffs(&rows, Some("explorer"), 0);
        assert_eq!(seats.len(), 4);
        let values: Vec<f64> = seats
            .iter()
            .map(|seat| match seat.ability.effect {
                crate::combat::AbilityEffect::BaseMitigationStatsBonus(value) => value,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(values, vec![3.0, 0.75, 1.75, 4.5]);
        assert_eq!(values.iter().sum::<f64>(), 10.0);
        let launch = research_bonuses_for_imported_buffs(&rows, Some("explorer"), 0).unwrap();
        assert_eq!(launch.get("armor"), Some(&2.7));
    }

    #[test]
    fn eclipse_combat_start_research_uses_active_global_buff_and_stays_out_of_launch_pool() {
        let active = [entry(1238121830, 10)];
        let seats = combat_start_defense_seats_for_imported_buffs(&active, Some("explorer"), 0);
        assert_eq!(seats.len(), 1);
        assert_eq!(
            seats[0].ability.effect,
            crate::combat::AbilityEffect::BaseMitigationStatsBonus(7.0)
        );
        assert!(matches!(
            seats[0].ability.condition,
            Some(crate::combat::AbilityCondition::DefenderHullFactionIdIs(
                1_750_120_904
            ))
        ));
        assert_eq!(
            research_bonuses_for_imported_buffs(&active, Some("explorer"), 0)
                .unwrap()
                .get("armor"),
            None
        );
        assert!(combat_start_defense_seats_for_imported_buffs(&[], Some("explorer"), 0).is_empty());
    }
}
