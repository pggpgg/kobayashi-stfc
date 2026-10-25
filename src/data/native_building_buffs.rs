//! Selected native StarbaseBuffs(68) definitions, evaluated from synced global
//! active buff levels. This replaces only the corresponding legacy building
//! families at the scenario callsite; research, equipment and manual sources
//! remain separate. Unsupported account/grade/target predicates are quarantined.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::combat::CrewSeatContext;
use crate::data::import::GlobalBuffEntry;
use crate::data::native_consumable_buffs::{
    bonuses_from_catalog_for_group, combat_start_defense_seats_from_catalog_for_group,
    NativeConsumableBuffCatalog,
};

const EMBEDDED_CATALOG: &str = include_str!("../../data/native_building_buff_catalog.json");

fn embedded_catalog() -> &'static NativeConsumableBuffCatalog {
    static CATALOG: OnceLock<NativeConsumableBuffCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        NativeConsumableBuffCatalog::from_json(EMBEDDED_CATALOG)
            .expect("reviewed embedded native building catalog must parse")
    })
}

/// `None` means no active recognized building BuffSpec is present, permitting a
/// legacy fallback. `Some`, including an empty map, is authoritative: unsupported
/// native conditions must not revive older building name/description inference.
pub fn building_bonuses_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    syndicate_level: Option<u32>,
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
        bonuses_for_imported_buffs(
            imported,
            attacker_ship_class,
            syndicate_level,
            now_unix_seconds,
        )
    })
}

/// One-based imported current levels; expiry timestamps are Unix seconds.
/// Known no/single-class deterministic setup scopes are supported. Account,
/// grade, combat-state and opponent conditions must never become unconditional.
pub fn bonuses_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    syndicate_level: Option<u32>,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    bonuses_from_catalog_for_group(
        embedded_catalog(),
        68,
        imported,
        attacker_ship_class,
        syndicate_level,
        now_unix_seconds,
    )
}

/// Native building CombatStart ModAllDefenses effects, kept outside launch pools.
pub fn combat_start_defense_seats_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    combat_start_defense_seats_from_catalog_for_group(
        embedded_catalog(),
        68,
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
    fn native_building_values_are_raw_rank_snapshots_and_class_gates_survive() {
        let rows = [
            entry(1474824297, 48),
            entry(753685799, 60),
            entry(919263260, 61),
        ];
        let out = bonuses_for_imported_buffs(&rows, Some("explorer"), None, 0);
        assert_eq!(out.get("hull_hp"), Some(&3.19));
        assert_eq!(out.get("shield_hp"), Some(&1.8));
        assert_eq!(out.get("weapon_damage"), Some(&0.65));
        assert!(
            !bonuses_for_imported_buffs(&rows, Some("interceptor"), None, 0)
                .contains_key("shield_hp")
        );
    }

    #[test]
    fn loyalty_grade_faction_and_opponent_scopes_are_not_flattened() {
        let rows = [
            entry(1944989778, 60), // loyalty30 condition203
            entry(1974652540, 61), // loyalty33 condition204
            entry(119659407, 60),  // loyalty35 condition205
            entry(3012721624, 36), // FKR / Grade5 / opponent-hostile scope
            entry(1328584071, 60), // PvP critical damage
        ];
        assert!(bonuses_for_imported_buffs(&rows, Some("explorer"), None, 0).is_empty());
    }

    #[test]
    fn other_native_source_ids_cannot_be_counted_as_buildings() {
        let rows = [
            entry(434613423, 10),
            entry(4191797822, 10),
            entry(488045092, 20),
            entry(3207768523, 20),
        ];
        assert!(bonuses_for_imported_buffs(&rows, Some("explorer"), Some(45), 0).is_empty());
        assert!(bonuses_from_catalog_for_group(
            embedded_catalog(),
            116,
            &[entry(1474824297, 48)],
            None,
            None,
            0
        )
        .is_empty());
    }

    #[test]
    fn known_syndicate_minima_gate_building_bonus_pools() {
        let rows = [
            entry(1944989778, 60),
            entry(1974652540, 61),
            entry(119659407, 60),
        ];
        assert!(bonuses_for_imported_buffs(&rows, Some("explorer"), Some(29), 0).is_empty());
        let at30 = bonuses_for_imported_buffs(&rows, Some("explorer"), Some(30), 0);
        assert_eq!(at30.get("hull_hp"), Some(&3.5));
        assert!(!at30.contains_key("shield_hp"));
        let at33 = bonuses_for_imported_buffs(&rows, Some("explorer"), Some(33), 0);
        assert_eq!(at33.get("shield_hp"), Some(&4.15));
        assert!(!at33.contains_key("weapon_damage"));
        let at35 = bonuses_for_imported_buffs(&rows, Some("explorer"), Some(35), 0);
        assert_eq!(at35.get("weapon_damage"), Some(&4.0));
        assert_eq!(
            at35,
            bonuses_for_imported_buffs(&rows, Some("explorer"), Some(45), 0)
        );
    }

    #[test]
    fn building_isolytic_add_uses_active_native_ranks_and_quarantines_scope_leaks() {
        let rows = [
            entry(2337243763, 49),
            entry(1639441900, 29),
            entry(96795919, 38),
            entry(301108821, 33), // compound faction/grade condition
        ];
        let out = bonuses_for_imported_buffs(&rows, Some("explorer"), Some(45), 0);
        assert!((out["isolytic_damage"] - 0.875).abs() < 1e-12);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn deployed_realta_isolytic_defense_building_is_selected_without_conditional_leak() {
        let rows = [
            entry(469165341, 49),
            entry(866732757, 36), // compound unknown conditions
        ];
        let out = bonuses_for_imported_buffs(&rows, Some("explorer"), Some(45), 0);
        assert_eq!(out.get("isolytic_defense"), Some(&0.73));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn realta_launch_defense_buildings_keep_all_defenses_and_single_axes() {
        let rows = [
            entry(1776779724, 51), // all defenses 4.65
            entry(864495595, 48),  // all defenses 3.19
            entry(3707508878, 60), // plating 1.4
            entry(3089815719, 60), // absorption 1.4
            entry(1400315411, 60), // dodge 1.4
        ];
        let out = bonuses_for_imported_buffs(&rows, Some("explorer"), Some(45), 0);
        for stat in ["armor", "shield_deflection", "dodge"] {
            assert!((out[stat] - 9.24).abs() < 1e-12);
        }
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn absent_source_and_authoritative_empty_scope_are_distinct() {
        assert!(building_bonuses_for_imported_buffs(&[], None, None, 0).is_none());
        assert!(
            building_bonuses_for_imported_buffs(&[entry(434613423, 10)], None, None, 0).is_none()
        );
        assert_eq!(
            building_bonuses_for_imported_buffs(
                &[entry(3012721624, 36)],
                Some("explorer"),
                Some(45),
                0
            ),
            Some(HashMap::new())
        );
        let mut expired = entry(1474824297, 48);
        expired.expiry_time = Some(10);
        assert!(building_bonuses_for_imported_buffs(&[expired], None, None, 10).is_none());
    }

    #[test]
    fn current_building_catalog_has_no_proven_combat_start_defense_seat() {
        let rows = [entry(1776779724, 51), entry(864495595, 48)];
        assert!(
            combat_start_defense_seats_for_imported_buffs(&rows, Some("explorer"), 0,).is_empty()
        );
    }
}
