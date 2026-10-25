//! Native Orlop Defense rank units and below-decks-only rating scope.
use std::collections::HashMap;

use kobayashi::combat::{Combatant, CrewOfficerStatTotals};
use kobayashi::data::import::ResearchEntry;
use kobayashi::data::profile::{
    apply_profile_to_attacker, compute_officer_stat_runtime_bonus,
    merge_research_bonuses_into_profile, validate_player_profile_payload,
    OfficerStatConditionContext, PlayerProfile,
};
use kobayashi::data::research::ResearchCatalog;
use kobayashi::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};

fn imported_orlop(rank: i64) -> PlayerProfile {
    let catalog: ResearchCatalog =
        serde_json::from_str(include_str!("../data/research_catalog.json")).unwrap();
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(
        &mut profile,
        &[ResearchEntry {
            rid: 282179003,
            level: rank,
        }],
        &catalog,
        None,
    );
    profile
}

fn defense_bonus(profile: &PlayerProfile, bridge: f64, below: f64) -> f64 {
    let ship = ShipRecord {
        ship_class: "explorer".into(),
        shield_deflection: 100.0,
        officer_bonus: OfficerBonusTable {
            defense: vec![OfficerBonusBreakpoint {
                value: 150.0,
                bonus: 1.0,
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let runtime = compute_officer_stat_runtime_bonus(
        CrewOfficerStatTotals {
            defense: bridge + below,
            ..Default::default()
        },
        CrewOfficerStatTotals {
            defense: bridge,
            ..Default::default()
        },
        &ship,
        profile,
        None,
        &HashMap::new(),
        &[],
        &OfficerStatConditionContext::default(),
        &[],
    );
    assert_eq!(runtime.attack_bonus, 0.0);
    assert_eq!(runtime.health_bonus, 0.0);
    runtime.defense_shield_deflection_add
}

#[test]
fn final_native_rank_is_one_hundred_percent_with_a_scoped_stat_key() {
    let profile = imported_orlop(10);
    assert_eq!(profile.bonuses["officer_defense_below_decks"], 1.0);
    assert!(!profile.bonuses.contains_key("officer_defense"));
    assert!(validate_player_profile_payload(profile, None).is_ok());
    assert_eq!(
        imported_orlop(1).bonuses["officer_defense_below_decks"],
        0.2
    );
}

#[test]
fn moving_officer_stats_between_bridge_and_below_decks_changes_orlop_eligibility() {
    let profile = imported_orlop(10);
    // All arrangements have the same raw Defense of 100. The 150 breakpoint
    // is reached only when at least 50 Defense is actually below decks.
    for (bridge, below, expected) in [
        (100.0, 0.0, 0.0),
        (80.0, 20.0, 0.0),
        (50.0, 50.0, 100.0),
        (0.0, 100.0, 100.0),
    ] {
        assert_eq!(defense_bonus(&profile, bridge, below), expected);
    }
}

#[test]
fn below_decks_research_is_not_direct_ship_mitigation() {
    let before = Combatant {
        shield_mitigation: 0.8,
        hull_health: 500.0,
        shield_health: 600.0,
        ..Default::default()
    };
    let after = apply_profile_to_attacker(
        before.clone(),
        &imported_orlop(10),
        None,
        Default::default(),
    );
    assert_eq!(after, before);
}
