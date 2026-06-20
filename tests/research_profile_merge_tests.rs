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
fn merge_research_hull_hp_stacks_fractions_like_buildings() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 597105669,
            name: Some("Valor of Starfleet".into()),
            data_version: None,
            source_note: None,
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
