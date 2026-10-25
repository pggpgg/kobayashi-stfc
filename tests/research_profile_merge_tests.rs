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
    ResearchBonusConditionKey, ResearchBonusEntry, ResearchCatalog, ResearchLevel,
    ResearchLevelValueMode, ResearchRecord,
};

#[test]
fn imported_owned_level_selects_snapshot_and_canonical_exclusion_still_wins() {
    let catalog: ResearchCatalog = serde_json::from_str(
        r#"{"items":[{"rid":42,"level_value_mode":"snapshot","levels":[
          {"level":1,"bonuses":[{"stat":"accuracy","value":0.21}]},
          {"level":2,"bonuses":[{"stat":"accuracy","value":0.24}]}]}]}"#,
    )
    .unwrap();
    let imported = vec![
        ResearchEntry { rid: 42, level: 2 },
        ResearchEntry { rid: 42, level: 1 },
    ];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    assert_eq!(profile.bonuses["accuracy"], 0.24);
    let mut excluded_profile = PlayerProfile::default();
    let excluded = std::collections::HashSet::from([42]);
    merge_research_bonuses_into_profile(
        &mut excluded_profile,
        &imported,
        &catalog,
        Some(&excluded),
    );
    assert!(excluded_profile.bonuses.is_empty());
}

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
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);

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
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);

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
        level_value_mode: Default::default(),
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
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);

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
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
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
            level_value_mode: Default::default(),
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
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);

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
            level_value_mode: Default::default(),
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
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
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
            level_value_mode: Default::default(),
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
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
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

#[test]
fn merge_research_morale_isolytic_is_round_start_seat_not_flat_profile() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 4133019450,
            name: Some("NS Morale Isolytic Damage".into()),
            data_version: None,
            source_note: None,
            level_value_mode: Default::default(),
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "isolytic_damage".into(),
                    value: 0.05,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        requires_morale: true,
                        ..Default::default()
                    },
                }],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 4133019450,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    assert!(
        !profile.bonuses.contains_key("isolytic_damage"),
        "morale-gated isolytic must not merge into profile.bonuses"
    );

    let gates = SupportBuffResearchGateState::default();
    let seats = research_derived_attack_phase_seats(
        &imported,
        &catalog,
        &gates,
        &std::collections::HashMap::new(),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].ability.timing, TimingWindow::RoundStart);
    match &seats[0].ability.effect {
        AbilityEffect::IsolyticDamageBonus(v) => assert!((*v - 0.05).abs() < 1e-12),
        e => panic!("expected IsolyticDamageBonus, got {e:?}"),
    }
    assert_eq!(
        seats[0].ability.condition,
        Some(AbilityCondition::MoraleActive)
    );
}

#[test]
fn merge_research_burning_hb_isolytic_dual_gate_seat() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 2047743532,
            name: Some("NS Burning HB Isolytic Damage".into()),
            data_version: None,
            source_note: None,
            level_value_mode: Default::default(),
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "isolytic_damage".into(),
                    value: 0.005,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        requires_defender_burning: true,
                        requires_defender_hull_breach: true,
                        ..Default::default()
                    },
                }],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 2047743532,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    assert!(!profile.bonuses.contains_key("isolytic_damage"));

    let gates = SupportBuffResearchGateState::default();
    let seats = research_derived_attack_phase_seats(
        &imported,
        &catalog,
        &gates,
        &std::collections::HashMap::new(),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].ability.timing, TimingWindow::AttackPhase);
    match &seats[0].ability.effect {
        AbilityEffect::IsolyticDamageBonus(v) => assert!((*v - 0.005).abs() < 1e-12),
        e => panic!("expected IsolyticDamageBonus, got {e:?}"),
    }
    assert_eq!(
        seats[0].ability.condition,
        Some(AbilityCondition::And(vec![
            AbilityCondition::DefenderBurning,
            AbilityCondition::DefenderHullBreach,
        ]))
    );
}

#[test]
fn canonical_override_takes_priority_over_catalog_for_ns_burning_damage() {
    use kobayashi::data::combat_effect_spec::{
        AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec,
    };
    use kobayashi::data::research::{ResearchCanonicalEffectEntry, ResearchCanonicalOverride};
    use kobayashi::data::research_effect_spec_adapter::{
        incoming_shield_mitigation_for_combat, research_derived_attack_phase_seats_from_spec,
    };
    use std::collections::HashMap;

    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 365419690,
            name: Some("NS Burning Damage".into()),
            data_version: None,
            source_note: None,
            level_value_mode: ResearchLevelValueMode::Snapshot,
            levels: vec![ResearchLevel {
                level: 2,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".into(),
                    value: 0.99,
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
        level: 2,
    }];
    let mut overrides = HashMap::new();
    overrides.insert(
        365419690,
        ResearchCanonicalOverride {
            rid: 365419690,
            name: None,
            source_note: None,
            effects: vec![ResearchCanonicalEffectEntry {
                id: "test_ns_burning".into(),
                modifier: AbilityModifierSpec::WeaponDamage,
                operation: AbilityOperationSpec::Add,
                by_level: vec![0.01, 0.0145],
                conditions: vec![AbilityConditionSpec::DefenderBurning],
                trigger: None,
                category: None,
                confidence: None,
                source_ref: None,
                snapshot_by_level: false,
                incoming_shield_mitigation_rounds: None,
            }],
        },
    );

    let seats = research_derived_attack_phase_seats_from_spec(&imported, &catalog, &overrides);
    assert_eq!(seats.len(), 1);
    match seats[0].ability.effect {
        AbilityEffect::AttackMultiplier(v) => {
            assert!(
                (v - 0.0245).abs() < 1e-12,
                "canonical cumulative 0.01+0.0145 expected, got {v}"
            );
        }
        ref e => panic!("expected AttackMultiplier, got {e:?}"),
    }

    let (sm, rounds) = incoming_shield_mitigation_for_combat(&imported, &overrides);
    assert_eq!(sm, 0.0);
    assert_eq!(rounds, 0);

    // Canonical definitions retain their independent per-effect aggregation contract.
    overrides.get_mut(&365419690).unwrap().effects[0].snapshot_by_level = true;
    let seats = research_derived_attack_phase_seats_from_spec(&imported, &catalog, &overrides);
    assert_eq!(seats.len(), 1);
    match seats[0].ability.effect {
        AbilityEffect::AttackMultiplier(value) => assert!((value - 0.0145).abs() < 1e-12),
        ref effect => panic!("expected AttackMultiplier, got {effect:?}"),
    }
}

#[test]
fn ksg_incoming_shield_mitigation_from_canonical_fixture() {
    use kobayashi::data::research::load_research_canonical_overrides;
    use kobayashi::data::research_effect_spec_adapter::incoming_shield_mitigation_for_combat;

    let overrides = load_research_canonical_overrides("data/research_canonical.json");
    let imported = vec![ResearchEntry {
        rid: 2392190200,
        level: 5,
    }];
    let (bonus, rounds) = incoming_shield_mitigation_for_combat(&imported, &overrides);
    assert!(
        (bonus - 0.025).abs() < 1e-9,
        "KSG tier-5 snapshot SM expected 2.5%, got {bonus}"
    );
    assert_eq!(rounds, 2);
}

#[test]
fn bundled_canonical_isolytic_uses_one_source_rank_instead_of_summing_prior_ranks() {
    use kobayashi::data::research::load_research_canonical_overrides;
    use kobayashi::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec;

    let overrides = load_research_canonical_overrides("data/research_canonical.json");
    let upstream: serde_json::Value = serde_json::from_str(include_str!(
        "../data/upstream/data-stfc-space/research/2580836593.json"
    ))
    .unwrap();
    let source_buff = upstream["buffs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|buff| buff["id"].as_i64() == Some(1855248137))
        .unwrap();
    assert_eq!(source_buff["loca_id"].as_i64(), Some(70100));
    let max_rank = overrides[&2580836593].effects[0].by_level.len();
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: Vec::new(),
    };
    for level in [0, 1, 2, max_rank as i64, 99] {
        let imported = [ResearchEntry {
            rid: 2580836593,
            level,
        }];
        let seats = research_derived_attack_phase_seats_from_spec(&imported, &catalog, &overrides);
        if level == 0 {
            assert!(seats.is_empty());
            continue;
        }
        let source_rank = (level as usize).min(max_rank) - 1;
        let expected = source_buff["values"][source_rank]["value"]
            .as_f64()
            .unwrap();
        assert_eq!(seats.len(), 1);
        assert_eq!(seats[0].ability.timing, TimingWindow::RoundStart);
        assert_eq!(
            seats[0].ability.condition,
            Some(AbilityCondition::DefenderBurning)
        );
        match seats[0].ability.effect {
            AbilityEffect::IsolyticDamageBonus(value) => assert!(
                (value - expected).abs() < 1e-12,
                "rank {level} should select source value {expected}, got {value}"
            ),
            ref effect => panic!("expected IsolyticDamageBonus, got {effect:?}"),
        }
    }
}

#[test]
fn verified_defense_and_repair_compile_identically_from_canonical_and_catalog_fallback() {
    use kobayashi::data::research::{load_research_canonical_overrides, load_research_catalog};
    use kobayashi::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec;
    use std::collections::HashMap;

    let overrides = load_research_canonical_overrides("data/research_canonical.json");
    let catalog = load_research_catalog("data/research_catalog.json").unwrap();
    let evidence: serde_json::Value = serde_json::from_str(include_str!(
        "../data/research/client_verified_research_2026-10-20.json"
    ))
    .unwrap();
    let no_overrides = HashMap::new();
    for (rid, condition) in [
        (2047743532, AbilityCondition::DefenderBurning),
        (535909811, AbilityCondition::MoraleActive),
        (1233598019, AbilityCondition::DefenderHullBreach),
        (3288570685, AbilityCondition::MoraleActive),
    ] {
        let source = evidence["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["rid"].as_i64() == Some(rid))
            .unwrap();
        let values = source["buff"]["ranked_values"].as_array().unwrap();
        for level in [0, 1, 2, values.len() as i64, 99] {
            let imported = [ResearchEntry { rid, level }];
            let mut profile = PlayerProfile::default();
            merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
            assert!(
                profile.bonuses.is_empty(),
                "round-start rows must not merge flat"
            );
            assert!(profile.research_owner_faction_bonuses.is_empty());
            for selected_overrides in [&overrides, &no_overrides] {
                let seats = research_derived_attack_phase_seats_from_spec(
                    &imported,
                    &catalog,
                    selected_overrides,
                );
                if level == 0 {
                    assert!(seats.is_empty());
                    continue;
                }
                assert_eq!(seats.len(), 1, "RID {rid} rank {level}");
                let expected = values[(level as usize).min(values.len()) - 1]
                    .as_f64()
                    .unwrap();
                assert_eq!(seats[0].ability.timing, TimingWindow::RoundStart);
                assert_eq!(seats[0].ability.condition, Some(condition.clone()));
                match seats[0].ability.effect {
                    AbilityEffect::BaseMitigationStatsBonus(value) if rid != 3288570685 => {
                        assert!((value - expected).abs() < 1e-12);
                    }
                    AbilityEffect::ShieldRegenMaxFraction(value) if rid == 3288570685 => {
                        assert!((value - expected).abs() < 1e-12);
                    }
                    ref effect => panic!("RID {rid} compiled to unexpected {effect:?}"),
                }
            }
        }
    }
}

#[test]
fn noncombat_empty_override_prevents_stale_catalog_combat_resurrection() {
    use kobayashi::data::research::load_research_canonical_overrides;
    use kobayashi::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec;
    use std::collections::{HashMap, HashSet};

    let overrides = load_research_canonical_overrides("data/research_canonical.json");
    assert!(overrides[&851540444].effects.is_empty());
    let catalog: ResearchCatalog = serde_json::from_str(
        r#"{"items":[{"rid":851540444,"level_value_mode":"snapshot","levels":[{"level":1,"bonuses":[
            {"stat":"accuracy","value":0.9},
            {"stat":"weapon_damage","value":0.8,"requires_morale":true}
        ]}]}]}"#,
    )
    .unwrap();
    let imported = [ResearchEntry {
        rid: 851540444,
        level: 1,
    }];
    assert_eq!(
        research_derived_attack_phase_seats_from_spec(&imported, &catalog, &HashMap::new()).len(),
        1
    );
    assert!(
        research_derived_attack_phase_seats_from_spec(&imported, &catalog, &overrides).is_empty()
    );
    let excluded: HashSet<i64> = overrides.keys().copied().collect();
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, Some(&excluded));
    assert!(profile.bonuses.is_empty());
}

#[test]
fn merge_research_hull_hp_stacks_fractions_like_buildings() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 597105669,
            name: Some("Valor of Starfleet".into()),
            data_version: None,
            source_note: None,
            level_value_mode: Default::default(),
            levels: vec![
                ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "hull_hp".into(),
                        value: 0.08,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            attacker_faction: Some("federation".into()),
                            ..Default::default()
                        },
                    }],
                },
                ResearchLevel {
                    level: 2,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "hull_hp".into(),
                        value: 0.10,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            attacker_faction: Some("federation".into()),
                            ..Default::default()
                        },
                    }],
                },
            ],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 597105669,
        level: 2,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);

    let owner = profile
        .research_owner_faction_bonuses
        .get("federation")
        .and_then(|m| m.get("hull_hp"))
        .copied()
        .unwrap_or(0.0);
    assert!(
        (owner - 0.18).abs() < 1e-9,
        "expected cumulative owner-faction hull_hp 0.18 at level 2, got {owner}"
    );
    assert!(
        !profile.bonuses.contains_key("hull_hp"),
        "owner-faction hull_hp must not flat-merge into profile.bonuses"
    );
}

#[test]
fn merge_research_morale_apex_barrier_is_round_start_seat_not_flat_profile() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 292902554,
            name: Some("Sarek's Vulcan Necklace".into()),
            data_version: None,
            source_note: None,
            level_value_mode: Default::default(),
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "apex_barrier".into(),
                    value: 250.0,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        requires_morale: true,
                        ..Default::default()
                    },
                }],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 292902554,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    assert!(
        !profile.bonuses.contains_key("apex_barrier"),
        "morale-gated apex_barrier must not merge into profile.bonuses"
    );

    let gates = SupportBuffResearchGateState::default();
    let seats = research_derived_attack_phase_seats(
        &imported,
        &catalog,
        &gates,
        &std::collections::HashMap::new(),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].ability.timing, TimingWindow::RoundStart);
    match &seats[0].ability.effect {
        AbilityEffect::ApexBarrierBonus(v) => assert!((*v - 250.0).abs() < 1e-9),
        e => panic!("expected ApexBarrierBonus, got {e:?}"),
    }
    assert_eq!(
        seats[0].ability.condition,
        Some(AbilityCondition::MoraleActive)
    );
}

#[test]
fn merge_research_dual_gate_hull_shield_skips_flat_and_owner_faction_maps() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 9001,
            name: Some("Fed vs Klingon hull".into()),
            data_version: None,
            source_note: None,
            level_value_mode: Default::default(),
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![
                    ResearchBonusEntry {
                        stat: "hull_hp".into(),
                        value: 0.12,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            attacker_faction: Some("federation".into()),
                            defender_faction: Some("klingon".into()),
                            ..Default::default()
                        },
                    },
                    ResearchBonusEntry {
                        stat: "shield_hp".into(),
                        value: 0.08,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            attacker_faction: Some("romulan".into()),
                            defender_faction: Some("federation".into()),
                            ..Default::default()
                        },
                    },
                ],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 9001,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    assert!(
        !profile.bonuses.contains_key("hull_hp"),
        "dual-gate hull must not flat-merge"
    );
    assert!(
        !profile.bonuses.contains_key("shield_hp"),
        "dual-gate shield must not flat-merge"
    );
    assert!(
        profile.research_owner_faction_bonuses.is_empty(),
        "dual-gate hull/shield must not merge into owner_faction map"
    );
}

#[test]
fn merge_research_morale_gated_hull_hp_round_start_seat() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 9003,
            name: Some("Morale hull".into()),
            data_version: None,
            source_note: None,
            level_value_mode: Default::default(),
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "hull_hp".into(),
                    value: 0.15,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        requires_morale: true,
                        ..Default::default()
                    },
                }],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 9003,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    assert!(!profile.bonuses.contains_key("hull_hp"));

    let gates = SupportBuffResearchGateState::default();
    let seats = research_derived_attack_phase_seats(
        &imported,
        &catalog,
        &gates,
        &std::collections::HashMap::new(),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].ability.timing, TimingWindow::RoundStart);
    match &seats[0].ability.effect {
        AbilityEffect::HullHpMultiplier(v) => assert!((*v - 0.15).abs() < 1e-12),
        e => panic!("expected HullHpMultiplier, got {e:?}"),
    }
}

#[test]
fn merge_research_burning_gated_shield_hp_attack_phase_seat() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 9004,
            name: Some("Burning shield".into()),
            data_version: None,
            source_note: None,
            level_value_mode: Default::default(),
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "shield_hp".into(),
                    value: 0.1,
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
        rid: 9004,
        level: 1,
    }];
    let mut profile = PlayerProfile::default();
    merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
    assert!(!profile.bonuses.contains_key("shield_hp"));

    let gates = SupportBuffResearchGateState::default();
    let seats = research_derived_attack_phase_seats(
        &imported,
        &catalog,
        &gates,
        &std::collections::HashMap::new(),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].ability.timing, TimingWindow::AttackPhase);
    match &seats[0].ability.effect {
        AbilityEffect::ShieldHpMultiplier(v) => assert!((*v - 0.1).abs() < 1e-12),
        e => panic!("expected ShieldHpMultiplier, got {e:?}"),
    }
}
