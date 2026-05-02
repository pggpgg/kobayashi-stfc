//! Deterministic tests for research catalog → profile merge (no dependency on `data/research_catalog.json`).
//!
//! `accuracy` merges into `profile.bonuses` and scales ship base accuracy for hostile dodge mitigation
//! (see `effective_attacker_stats_for_mitigation` tests in `src/optimizer/monte_carlo/scenario.rs`).

use kobayashi::combat::{
    AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, ShipType, TimingWindow,
};
use kobayashi::data::import::ResearchEntry;
use kobayashi::data::profile::{
    merge_research_bonuses_into_profile, research_derived_attack_phase_seats, PlayerProfile,
    SupportBuffResearchGateState,
};
use kobayashi::data::research::{
    ResearchBonusConditionKey, ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
};

#[test]
fn merge_research_applies_fixture_catalog_weapon_damage() {
    let catalog: ResearchCatalog = serde_json::from_str(include_str!(
        "fixtures/research/research_catalog_fixture.json"
    ))
    .expect("parse fixture research catalog");

    let imported = vec![ResearchEntry {
        rid: 99000001,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);

    let w = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
    assert!(
        (w - 0.12).abs() < 1e-9,
        "expected weapon_damage 0.12 from fixture rid, got {w}"
    );
}

#[test]
fn merge_research_accuracy_from_fixture_catalog() {
    let catalog: ResearchCatalog = serde_json::from_str(include_str!(
        "fixtures/research/research_catalog_fixture.json"
    ))
    .expect("parse fixture research catalog");

    let imported = vec![ResearchEntry {
        rid: 99000003,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);

    let a = profile.bonuses.get("accuracy").copied().unwrap_or(0.0);
    assert!(
        (a - 0.08).abs() < 1e-9,
        "expected accuracy bonus 0.08 (fractional mult on ship base in scenario), got {a}"
    );
}

#[test]
fn merge_research_stacks_accuracy_across_two_rids_additively() {
    let catalog: ResearchCatalog = serde_json::from_str(include_str!(
        "fixtures/research/research_catalog_fixture.json"
    ))
    .expect("parse fixture research catalog");

    // 0.08 (fixture rid 99000003) + 0.02 (inline rid 99000004) → 0.10 additive in profile.bonuses["accuracy"].
    use kobayashi::data::research::{ResearchBonusEntry, ResearchLevel, ResearchRecord};
    let mut catalog = catalog;
    catalog.items.push(ResearchRecord {
        rid: 99000004,
        name: Some("Extra accuracy".into()),
        data_version: None,
        source_note: None,
        levels: vec![ResearchLevel {
            level: 1,
            bonuses: vec![ResearchBonusEntry {
                stat: "accuracy".into(),
                value: 0.02,
                operator: "add".into(),
                condition: Default::default(),
            }],
        }],
    });

    let imported = vec![
        ResearchEntry {
            rid: 99000003,
            level: 1,
        },
        ResearchEntry {
            rid: 99000004,
            level: 1,
        },
    ];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);

    let a = profile.bonuses.get("accuracy").copied().unwrap_or(0.0);
    assert!(
        (a - 0.10).abs() < 1e-9,
        "expected stacked accuracy 0.10, got {a}"
    );
}

#[test]
fn merge_research_skips_unknown_rid_in_fixture_catalog() {
    let catalog: ResearchCatalog = serde_json::from_str(include_str!(
        "fixtures/research/research_catalog_fixture.json"
    ))
    .expect("parse fixture research catalog");

    let imported = vec![ResearchEntry {
        rid: 99999999,
        level: 5,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);
    assert!(profile.bonuses.is_empty());
}

#[test]
fn merge_research_duplicate_rid_uses_max_level() {
    use kobayashi::data::research::{ResearchBonusEntry, ResearchLevel, ResearchRecord};

    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 1,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![
                ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.05,
                        operator: "add".to_string(),
                        condition: Default::default(),
                    }],
                },
                ResearchLevel {
                    level: 2,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.07,
                        operator: "add".to_string(),
                        condition: Default::default(),
                    }],
                },
            ],
        }],
    };

    // Last-wins on insert order would keep level 1 only (0.05); max(level) must use 2 → 0.12.
    let imported = vec![
        ResearchEntry { rid: 1, level: 2 },
        ResearchEntry { rid: 1, level: 1 },
    ];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);

    let w = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
    assert!(
        (w - 0.12).abs() < 1e-9,
        "expected weapon_damage 0.12 from max level 2, got {w}"
    );
}

#[test]
fn merge_research_conditional_crit_is_attack_phase_seat_not_profile_crit() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 99000005,
            name: Some("Gated crit lab".into()),
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "crit_chance".into(),
                    value: 0.06,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        defender_ship_class: Some("explorer".into()),
                        ..Default::default()
                    },
                }],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 99000005,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);
    assert!(
        !profile.bonuses.contains_key("crit_chance"),
        "conditional crit must not merge into profile.bonuses"
    );

    let gates = SupportBuffResearchGateState::default();
    let seats = research_derived_attack_phase_seats(
        &imported,
        &catalog,
        &gates,
        &std::collections::HashMap::new(),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].seat, CrewSeat::Ship);
    assert_eq!(seats[0].ability.class, AbilityClass::ShipAbility);
    assert_eq!(seats[0].ability.timing, TimingWindow::AttackPhase);
    match &seats[0].ability.effect {
        AbilityEffect::CritChanceBonus(v) => assert!((v - 0.06).abs() < 1e-12),
        e => panic!("expected CritChanceBonus, got {e:?}"),
    }
    assert_eq!(
        seats[0].ability.condition,
        Some(AbilityCondition::DefenderShipTypeIs(ShipType::Explorer))
    );
}

#[test]
fn merge_research_ns_burning_damage_is_burning_gated_attack_multiplier_not_flat_weapon() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 365419690,
            name: Some("NS Burning Damage".into()),
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".into(),
                    value: 0.01,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        requires_defender_burning: true,
                        ..Default::default()
                    },
                }],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 365419690,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog);
    assert!(
        !profile.bonuses.contains_key("weapon_damage"),
        "burning-gated weapon_damage must not merge into profile.bonuses"
    );

    let gates = SupportBuffResearchGateState::default();
    let seats = research_derived_attack_phase_seats(
        &imported,
        &catalog,
        &gates,
        &std::collections::HashMap::new(),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].ability.timing, TimingWindow::AttackPhase);
    match seats[0].ability.effect {
        AbilityEffect::AttackMultiplier(v) => assert!((v - 0.01).abs() < 1e-12),
        ref e => panic!("expected AttackMultiplier, got {e:?}"),
    }
    assert_eq!(
        seats[0].ability.condition,
        Some(AbilityCondition::DefenderBurning)
    );
}
