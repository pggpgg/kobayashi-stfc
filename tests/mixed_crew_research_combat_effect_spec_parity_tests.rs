//! Composition smoke tests: LCARS-resolved bridge seats plus conditional research attack-phase seats.
//! Research rows always come from [`kobayashi::data::profile::research_derived_attack_phase_seats`]
//! (delegates to the CombatEffectSpec adapter).

use kobayashi::combat::abilities::{AbilityClass, CrewSeat};
use kobayashi::data::import::ResearchEntry;
use kobayashi::data::profile::{research_derived_attack_phase_seats, SupportBuffResearchGateState};
use kobayashi::data::research::{
    ResearchBonusConditionKey, ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
};
use kobayashi::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec;
use kobayashi::lcars::{
    resolve_officer_ability, LcarsAbility, LcarsEffect, LcarsOfficer, ResolveOptions,
};

fn seat_signature(c: &kobayashi::combat::CrewSeatContext) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}",
        c.seat, c.ability.timing, c.ability.effect, c.ability.condition
    )
}

fn sorted_combined_signatures(
    lcars: &[kobayashi::combat::CrewSeatContext],
    research: &[kobayashi::combat::CrewSeatContext],
) -> Vec<String> {
    let mut v: Vec<String> = lcars
        .iter()
        .chain(research.iter())
        .map(seat_signature)
        .collect();
    v.sort();
    v
}

fn sample_bridge_weapon_damage_effect() -> LcarsEffect {
    LcarsEffect {
        effect_type: "stat_modify".into(),
        stat: Some("weapon_damage".into()),
        target: None,
        operator: Some("add".into()),
        value: Some(0.07),
        trigger: Some("on_attack".into()),
        duration: None,
        scaling: None,
        condition: None,
        chance: None,
        multiplier: None,
        tag: None,
        accumulate: None,
        decay: None,
    }
}

fn sample_officer_with_bridge(effect: LcarsEffect) -> LcarsOfficer {
    LcarsOfficer {
        id: "mixed_parity_officer".into(),
        name: "Mixed Parity".into(),
        faction: None,
        rarity: None,
        group: None,
        captain_ability: None,
        bridge_ability: Some(LcarsAbility {
            name: "bridge_strike".into(),
            effects: vec![effect],
        }),
        below_decks_ability: None,
    }
}

fn lcars_bridge_seats(officer: &LcarsOfficer) -> Vec<kobayashi::combat::CrewSeatContext> {
    let ability = officer.bridge_ability.as_ref().expect("bridge");
    resolve_officer_ability(
        officer,
        ability,
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        &ResolveOptions::default(),
        0,
    )
}

#[test]
fn mixed_bridge_officer_plus_burning_gated_research_combines() {
    let officer = sample_officer_with_bridge(sample_bridge_weapon_damage_effect());
    let lcars = lcars_bridge_seats(&officer);
    assert_eq!(lcars.len(), 1, "fixture should resolve one bridge seat");

    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![ResearchRecord {
            rid: 99001001,
            name: Some("Mixed parity burning WD".into()),
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".into(),
                    value: 0.04,
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
        rid: 99001001,
        level: 1,
    }];

    let gates = SupportBuffResearchGateState::default();
    let research = research_derived_attack_phase_seats(&imported, &catalog, &gates);
    let from_spec = research_derived_attack_phase_seats_from_spec(&imported, &catalog);
    assert!(!research.is_empty());
    assert_eq!(
        sorted_combined_signatures(&lcars, &research),
        sorted_combined_signatures(&lcars, &from_spec),
        "public API matches adapter when merged with LCARS"
    );
}

#[test]
fn mixed_bridge_officer_plus_two_research_rids_combines() {
    let mut effect = sample_bridge_weapon_damage_effect();
    effect.value = Some(0.03);
    let officer = sample_officer_with_bridge(effect);
    let lcars = lcars_bridge_seats(&officer);
    assert_eq!(lcars.len(), 1);

    let catalog = ResearchCatalog {
        source: None,
        last_updated: None,
        items: vec![
            ResearchRecord {
                rid: 99001002,
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
                rid: 99001003,
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
            rid: 99001002,
            level: 1,
        },
        ResearchEntry {
            rid: 99001003,
            level: 1,
        },
    ];

    let gates = SupportBuffResearchGateState::default();
    let research = research_derived_attack_phase_seats(&imported, &catalog, &gates);
    let from_spec = research_derived_attack_phase_seats_from_spec(&imported, &catalog);
    assert_eq!(research.len(), from_spec.len());

    assert_eq!(
        sorted_combined_signatures(&lcars, &research),
        sorted_combined_signatures(&lcars, &from_spec),
    );
}
