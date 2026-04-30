//! Integration: officer-stat building bonuses merge into distinct profile buckets and compound on [`Combatant`] attack.

use std::collections::HashMap;
use std::path::Path;

use kobayashi::combat::Combatant;
use kobayashi::data::building::{BuildingBonusContext, BuildingIndex};
use kobayashi::data::import::BuildingEntry;
use kobayashi::data::profile::{
    apply_profile_to_attacker, merge_building_bonuses_into_profile, PlayerProfile,
};

#[test]
fn command_center_level_80_officer_attack_multiplies_when_weapon_damage_absent() {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/buildings");
    let mut profile = PlayerProfile::default();
    let imported = vec![BuildingEntry { bid: 71, level: 80 }];
    let mut bid_to_id = HashMap::new();
    bid_to_id.insert(71_i64, "building_71".to_string());
    let building_index = BuildingIndex {
        data_version: None,
        source_note: None,
        buildings: vec![],
    };

    merge_building_bonuses_into_profile(
        &mut profile,
        &imported,
        &bid_to_id,
        &building_index,
        data_dir.as_path(),
        &BuildingBonusContext::default(),
    );

    let wd = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
    assert!(
        wd.abs() < 1e-9,
        "Command Center merge alone should not add weapon_damage (got {wd})"
    );

    let oa = profile
        .bonuses
        .get("officer_attack")
        .copied()
        .expect("Command Center L80 should contribute officer_attack");
    assert!(oa > 0.0, "expected positive officer_attack from CC bonuses");

    let attacker = Combatant {
        id: "probe".to_string(),
        attack: 1000.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 100.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let out = apply_profile_to_attacker(attacker, &profile);
    let expected = 1000.0 * (1.0 + wd) * (1.0 + oa);
    assert!(
        (out.attack - expected).abs() < 1e-3,
        "attack {} expected {} (oa={oa}, wd={wd})",
        out.attack,
        expected
    );
}
