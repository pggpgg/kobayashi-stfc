//! Native selected research setup families, using global active buff membership.
//!
//! Owning a project is not sufficient: Fleet Commander On Duty projects can be
//! owned while their linked buff is absent from the active global list. Native
//! FleetPlayerData.EvaluateBuffs consumes globalBuffs, independently of research
//! ownership. Canonical status/timing seats and other research families remain on
//! their existing paths; this compiler handles only known deterministic setup rows.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::combat::CrewSeatContext;
use crate::data::import::GlobalBuffEntry;
use crate::data::native_consumable_buffs::{
    bonuses_from_catalog_for_ship, combat_start_defense_seats_from_catalog_for_group,
    NativeConsumableBuffCatalog,
};

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
            && catalog.contains_buff(buff.bid)
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
