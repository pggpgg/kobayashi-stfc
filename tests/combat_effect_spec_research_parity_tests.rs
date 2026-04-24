//! Golden fixtures: [`kobayashi::data::profile::research_derived_attack_phase_seats`] must match
//! [`kobayashi::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec`]
//! (order-independent seat signatures). The public API delegates to the adapter; these tests lock behavior.

use kobayashi::data::import::ResearchEntry;
use kobayashi::data::profile::{research_derived_attack_phase_seats, SupportBuffResearchGateState};
use kobayashi::data::research::{
    ResearchBonusConditionKey, ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
};
use kobayashi::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec;

fn seat_signature(c: &kobayashi::combat::CrewSeatContext) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}",
        c.seat, c.ability.timing, c.ability.effect, c.ability.condition
    )
}

fn assert_public_matches_adapter(imported: &[ResearchEntry], catalog: &ResearchCatalog) {
    let gates = SupportBuffResearchGateState::default();
    let public = research_derived_attack_phase_seats(imported, catalog, &gates);
    let via_spec = research_derived_attack_phase_seats_from_spec(imported, catalog);
    assert_eq!(public.len(), via_spec.len(), "seat count mismatch");
    let mut a: Vec<String> = public.iter().map(seat_signature).collect();
    let mut b: Vec<String> = via_spec.iter().map(seat_signature).collect();
    a.sort();
    b.sort();
    assert_eq!(
        a, b,
        "research_derived_attack_phase_seats vs from_spec: same attack-phase effects & conditions"
    );
}

#[test]
fn parity_empty_import_yields_no_seats() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 1,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".into(),
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
    assert_public_matches_adapter(&[], &catalog);
}

#[test]
fn parity_crit_damage_morale_gated() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 88001001,
            name: Some("Golden crit damage + morale".into()),
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "crit_damage".into(),
                    value: 0.25,
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
        rid: 88001001,
        level: 1,
    }];
    assert_public_matches_adapter(&imported, &catalog);
}

#[test]
fn parity_weapon_damage_burning_and_explorer_and_faction() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 88001002,
            name: Some("Golden multi-gate weapon_damage".into()),
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".into(),
                    value: 0.03,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        requires_defender_burning: true,
                        defender_ship_class: Some("explorer".into()),
                        defender_faction: Some("borg".into()),
                        ..Default::default()
                    },
                }],
            }],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 88001002,
        level: 1,
    }];
    assert_public_matches_adapter(&imported, &catalog);
}

#[test]
fn parity_two_rids_two_conditional_rows() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![
            ResearchRecord {
                rid: 88001003,
                name: None,
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "crit_chance".into(),
                        value: 0.04,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            requires_defender_hull_breach: true,
                            ..Default::default()
                        },
                    }],
                }],
            },
            ResearchRecord {
                rid: 88001004,
                name: None,
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".into(),
                        value: 0.02,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            defender_ship_class: Some("interceptor".into()),
                            ..Default::default()
                        },
                    }],
                }],
            },
        ],
    };
    let imported = vec![
        ResearchEntry {
            rid: 88001003,
            level: 1,
        },
        ResearchEntry {
            rid: 88001004,
            level: 1,
        },
    ];
    assert_public_matches_adapter(&imported, &catalog);
}

#[test]
fn parity_cumulative_level_stacks_same_rid() {
    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 88001005,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![
                ResearchLevel {
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
                },
                ResearchLevel {
                    level: 2,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".into(),
                        value: 0.02,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            requires_defender_burning: true,
                            ..Default::default()
                        },
                    }],
                },
            ],
        }],
    };
    let imported = vec![ResearchEntry {
        rid: 88001005,
        level: 2,
    }];
    assert_public_matches_adapter(&imported, &catalog);
}
