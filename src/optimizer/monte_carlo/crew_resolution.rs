//! Crew resolution from officer names and candidate → crew seats/contexts.

use std::collections::HashMap;

use crate::combat::{
    Ability, AbilityClass, AbilityEffect, CrewConfiguration, CrewSeat, CrewSeatContext,
    TimingWindow,
};
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::optimizer::crew_generator::{CrewCandidate, BRIDGE_SLOTS, BELOW_DECKS_SLOTS};

/// Build a [CrewConfiguration] from officer names (e.g. from a fight export).
/// Convention: captain = Officer One, bridge = Officer Two then Officer Three, below_decks = [].
/// Empty or "--" names are skipped. Uses canonical officers from [DEFAULT_CANONICAL_OFFICERS_PATH].
pub fn crew_from_officer_names(
    captain: Option<&str>,
    bridge: Vec<String>,
    below_decks: Vec<String>,
) -> CrewConfiguration {
    let captain_str = captain
        .filter(|s| !is_empty_or_placeholder(s))
        .unwrap_or("")
        .to_string();
    let bridge_filtered: Vec<String> = bridge
        .into_iter()
        .filter(|s| !is_empty_or_placeholder(s))
        .collect();
    let below_filtered: Vec<String> = below_decks
        .into_iter()
        .filter(|s| !is_empty_or_placeholder(s))
        .collect();
    let candidate = CrewCandidate {
        captain: captain_str,
        bridge: bridge_filtered,
        below_decks: below_filtered,
    };
    let officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH).unwrap_or_default();
    let officers_by_name = index_officers_by_name(officers);
    let seats = build_crew_seats(&candidate, &officers_by_name);
    CrewConfiguration { seats }
}

pub(crate) fn build_crew_seats(
    candidate: &CrewCandidate,
    officers_by_name: &HashMap<String, Officer>,
) -> Vec<CrewSeatContext> {
    let mut seats = Vec::with_capacity(1 + BRIDGE_SLOTS + BELOW_DECKS_SLOTS);
    seats.push(seat_from_officer(
        &candidate.captain,
        CrewSeat::Captain,
        AbilityClass::CaptainManeuver,
        officers_by_name,
    ));
    seats.extend(apex_ability_contexts(
        &candidate.captain,
        CrewSeat::Captain,
        AbilityClass::CaptainManeuver,
        officers_by_name,
    ));
    for i in 0..BRIDGE_SLOTS {
        let name = candidate
            .bridge
            .get(i)
            .or_else(|| candidate.bridge.first())
            .map(String::as_str)
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        seats.push(seat_from_officer(
            name,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            officers_by_name,
        ));
        seats.extend(apex_ability_contexts(
            name,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            officers_by_name,
        ));
    }
    for i in 0..BELOW_DECKS_SLOTS {
        let name = candidate
            .below_decks
            .get(i)
            .or_else(|| candidate.below_decks.first())
            .map(String::as_str)
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        seats.push(seat_from_officer(
            name,
            CrewSeat::BelowDeck,
            AbilityClass::BelowDeck,
            officers_by_name,
        ));
        seats.extend(apex_ability_contexts(
            name,
            CrewSeat::BelowDeck,
            AbilityClass::BelowDeck,
            officers_by_name,
        ));
    }
    seats
}

fn seat_from_officer(
    id: &str,
    seat: CrewSeat,
    class: AbilityClass,
    officers_by_name: &HashMap<String, Officer>,
) -> CrewSeatContext {
    let hash = hash_identifier(id);
    let (lookup_name, tier) = split_name_and_tier(id);
    let officer = officers_by_name.get(&normalize_lookup_key(&lookup_name));
    let morale_chance = officer.and_then(|officer| {
        officer
            .abilities
            .iter()
            .find(|ability| ability.is_round_start_trigger() && ability.applies_morale_state())
            .map(|ability| ability.morale_chance_for_tier(tier))
    });
    let assimilated = officer.and_then(|officer| {
        officer
            .abilities
            .iter()
            .find(|ability| ability.applies_assimilated_state())
            .map(|ability| {
                let timing = if ability.is_round_start_trigger() {
                    TimingWindow::RoundStart
                } else {
                    TimingWindow::AttackPhase
                };
                (
                    timing,
                    AbilityEffect::Assimilated {
                        chance: ability.morale_chance_for_tier(tier),
                        duration_rounds: ability.state_duration_rounds(),
                    },
                )
            })
    });
    let hull_breach = officer.and_then(|officer| {
        officer
            .abilities
            .iter()
            .find(|ability| ability.applies_hull_breach_state())
            .map(|ability| {
                let timing = if ability.is_round_start_trigger() {
                    TimingWindow::RoundStart
                } else {
                    TimingWindow::AttackPhase
                };
                (
                    timing,
                    AbilityEffect::HullBreach {
                        chance: ability.morale_chance_for_tier(tier),
                        duration_rounds: ability.state_duration_rounds(),
                        requires_critical: ability.triggers_on_critical_shot(),
                    },
                )
            })
    });
    let burning = officer.and_then(|officer| {
        officer
            .abilities
            .iter()
            .find(|ability| ability.applies_burning_state())
            .map(|ability| {
                let timing = if ability.is_round_start_trigger() {
                    TimingWindow::RoundStart
                } else {
                    TimingWindow::AttackPhase
                };
                (
                    timing,
                    AbilityEffect::Burning {
                        chance: ability.morale_chance_for_tier(tier),
                        duration_rounds: ability.state_duration_rounds(),
                    },
                )
            })
    });

    let (timing, effect) = if let Some((timing, effect)) = assimilated {
        (timing, effect)
    } else if let Some((timing, effect)) = hull_breach {
        (timing, effect)
    } else if let Some((timing, effect)) = burning {
        (timing, effect)
    } else if let Some(chance) = morale_chance {
        (TimingWindow::RoundStart, AbilityEffect::Morale(chance))
    } else if hash % 2 == 0 {
        (
            TimingWindow::AttackPhase,
            AbilityEffect::AttackMultiplier(0.05 + ((hash >> 8) % 12) as f64 / 100.0),
        )
    } else {
        (
            TimingWindow::AttackPhase,
            AbilityEffect::PierceBonus(0.01 + ((hash >> 8) % 12) as f64 / 100.0),
        )
    };

    let ability_name = officer
        .map(|officer| officer.name.clone())
        .unwrap_or_else(|| id.to_string());

    CrewSeatContext {
        seat,
        ability: Ability {
            name: ability_name,
            class,
            timing,
            boostable: true,
            effect,
            condition: None,
        },
        boosted: hash % 5 == 0,
    }
}

pub(crate) fn index_officers_by_name(officers: Vec<Officer>) -> HashMap<String, Officer> {
    officers
        .into_iter()
        .map(|officer| (normalize_lookup_key(&officer.name), officer))
        .collect()
}

fn is_empty_or_placeholder(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.eq_ignore_ascii_case("--")
}

pub(crate) fn normalize_lookup_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn split_name_and_tier(input: &str) -> (String, Option<u8>) {
    let trimmed = input.trim();
    if let Some(open) = trimmed.rfind('(') {
        if trimmed.ends_with(')') {
            let inner = &trimmed[open + 1..trimmed.len() - 1];
            if let Some(rest) = inner.strip_prefix('T').or_else(|| inner.strip_prefix('t')) {
                if let Ok(tier) = rest.parse::<u8>() {
                    return (trimmed[..open].trim().to_string(), Some(tier));
                }
            }
        }
    }
    (trimmed.to_string(), None)
}

pub(crate) fn hash_identifier(value: &str) -> u64 {
    value.bytes().fold(14695981039346656037u64, |acc, b| {
        (acc ^ u64::from(b)).wrapping_mul(1099511628211)
    })
}

pub(crate) fn seeded_variance(seed: u64) -> f64 {
    let mixed = seed
        .wrapping_add(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let unit = (mixed as f64) / (u64::MAX as f64);
    0.85 + (unit * 0.30)
}

fn trigger_to_timing_window(trigger: Option<&str>) -> Option<TimingWindow> {
    match trigger.as_ref().and_then(|t| Some(t.trim())) {
        Some("CombatStart") => Some(TimingWindow::CombatBegin),
        Some("RoundStart") => Some(TimingWindow::RoundStart),
        _ => None,
    }
}

fn apex_ability_contexts(
    officer_id: &str,
    seat: CrewSeat,
    class: AbilityClass,
    officers_by_name: &HashMap<String, Officer>,
) -> Vec<CrewSeatContext> {
    let (lookup_name, tier) = split_name_and_tier(officer_id);
    let Some(officer) = officers_by_name.get(&normalize_lookup_key(&lookup_name)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ability in &officer.abilities {
        let Some(timing) = trigger_to_timing_window(ability.trigger.as_deref()) else {
            continue;
        };
        let (effect, name_suffix) = if ability.modifier_is_apex_shred() {
            (
                AbilityEffect::ApexShredBonus(ability.value_for_tier(tier)),
                " (Apex Shred)",
            )
        } else if ability.modifier_is_apex_barrier() {
            (
                AbilityEffect::ApexBarrierBonus(ability.value_for_tier(tier)),
                " (Apex Barrier)",
            )
        } else {
            continue;
        };
        out.push(CrewSeatContext {
            seat,
            ability: Ability {
                name: format!("{}{}", officer.name, name_suffix),
                class,
                timing,
                boostable: false,
                effect,
                condition: None,
            },
            boosted: false,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::officer::OfficerAbility;

    #[test]
    fn seat_from_officer_interprets_round_start_morale_from_profile() {
        let mut officers = HashMap::new();
        officers.insert(
            normalize_lookup_key("Harry Kim"),
            Officer {
                id: "harry-kim-a79fdf".to_string(),
                name: "Harry Kim".to_string(),
                slot: Some("science".to_string()),
                abilities: vec![OfficerAbility {
                    slot: "officer".to_string(),
                    trigger: Some("RoundStart".to_string()),
                    modifier: Some("AddState".to_string()),
                    attributes: Some("num_rounds=1, state=8".to_string()),
                    description: Some("Apply Morale".to_string()),
                    chance_by_rank: vec![0.1, 0.15, 0.3, 0.6, 1.0],
                    value_by_rank: vec![],
                }],
            },
        );

        let seat = seat_from_officer(
            "Harry Kim (T5)",
            CrewSeat::BelowDeck,
            AbilityClass::BelowDeck,
            &officers,
        );

        assert_eq!(seat.ability.timing, TimingWindow::RoundStart);
        assert!(matches!(seat.ability.effect, AbilityEffect::Morale(1.0)));
    }

    #[test]
    fn seat_from_officer_interprets_assimilated_profiles_including_below_decks() {
        let mut officers = HashMap::new();
        officers.insert(
            normalize_lookup_key("Dezoc"),
            Officer {
                id: "dezoc".to_string(),
                name: "Dezoc".to_string(),
                slot: Some("science".to_string()),
                abilities: vec![OfficerAbility {
                    slot: "officer".to_string(),
                    trigger: Some("RoundStart".to_string()),
                    modifier: Some("AddState".to_string()),
                    attributes: Some("num_rounds=4, state=64".to_string()),
                    description: Some("Apply Assimilate".to_string()),
                    chance_by_rank: vec![0.4, 0.45, 0.5],
                    value_by_rank: vec![],
                }],
            },
        );

        let seat = seat_from_officer(
            "Dezoc (T2)",
            CrewSeat::BelowDeck,
            AbilityClass::BelowDeck,
            &officers,
        );

        assert_eq!(seat.ability.timing, TimingWindow::RoundStart);
        assert!(matches!(
            seat.ability.effect,
            AbilityEffect::Assimilated {
                chance,
                duration_rounds: 4
            } if (chance - 0.45).abs() < 1e-12
        ));
    }

    #[test]
    fn seat_from_officer_interprets_hull_breach_profiles() {
        let mut officers = HashMap::new();
        officers.insert(
            normalize_lookup_key("Lorca"),
            Officer {
                id: "lorca".to_string(),
                name: "Lorca".to_string(),
                slot: Some("officer".to_string()),
                abilities: vec![OfficerAbility {
                    slot: "officer".to_string(),
                    trigger: Some("RoundStart".to_string()),
                    modifier: Some("AddState".to_string()),
                    attributes: Some("num_rounds=2, state=4".to_string()),
                    description: Some("Apply Hull Breach".to_string()),
                    chance_by_rank: vec![0.5, 0.6, 0.7],
                    value_by_rank: vec![],
                }],
            },
        );

        let lorca = seat_from_officer(
            "Lorca (T2)",
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &officers,
        );
        assert_eq!(lorca.ability.timing, TimingWindow::RoundStart);
        assert!(matches!(
            lorca.ability.effect,
            AbilityEffect::HullBreach {
                chance,
                duration_rounds: 2,
                requires_critical: false
            } if (chance - 0.6).abs() < 1e-12
        ));

        officers.insert(
            normalize_lookup_key("Gorkon"),
            Officer {
                id: "gorkon".to_string(),
                name: "Gorkon".to_string(),
                slot: Some("officer".to_string()),
                abilities: vec![OfficerAbility {
                    slot: "officer".to_string(),
                    trigger: Some("CriticalShotFired".to_string()),
                    modifier: Some("AddState".to_string()),
                    attributes: Some("num_rounds=3, state=4".to_string()),
                    description: Some("Hull Breach on critical hit".to_string()),
                    chance_by_rank: vec![0.7, 0.75, 0.8],
                    value_by_rank: vec![],
                }],
            },
        );

        let gorkon = seat_from_officer(
            "Gorkon (T1)",
            CrewSeat::Captain,
            AbilityClass::CaptainManeuver,
            &officers,
        );
        assert_eq!(gorkon.ability.timing, TimingWindow::AttackPhase);
        assert!(matches!(
            gorkon.ability.effect,
            AbilityEffect::HullBreach {
                chance,
                duration_rounds: 3,
                requires_critical: true
            } if (chance - 0.7).abs() < 1e-12
        ));

        officers.insert(
            normalize_lookup_key("B'Elanna Torres"),
            Officer {
                id: "belanna".to_string(),
                name: "B'Elanna Torres".to_string(),
                slot: Some("below_decks".to_string()),
                abilities: vec![OfficerAbility {
                    slot: "officer".to_string(),
                    trigger: Some("RoundStart".to_string()),
                    modifier: Some("AddState".to_string()),
                    attributes: Some("num_rounds=1, state=4".to_string()),
                    description: Some("Chance to apply Hull Breach".to_string()),
                    chance_by_rank: vec![0.1, 0.15, 0.3],
                    value_by_rank: vec![],
                }],
            },
        );

        let belanna = seat_from_officer(
            "B'Elanna Torres (T3)",
            CrewSeat::BelowDeck,
            AbilityClass::BelowDeck,
            &officers,
        );
        assert_eq!(belanna.ability.timing, TimingWindow::RoundStart);
        assert!(matches!(
            belanna.ability.effect,
            AbilityEffect::HullBreach {
                chance,
                duration_rounds: 1,
                requires_critical: false
            } if (chance - 0.3).abs() < 1e-12
        ));
    }

    #[test]
    fn seat_from_officer_interprets_burning_profiles() {
        let mut officers = HashMap::new();
        officers.insert(
            normalize_lookup_key("Nero"),
            Officer {
                id: "nero".to_string(),
                name: "Nero".to_string(),
                slot: Some("captain".to_string()),
                abilities: vec![OfficerAbility {
                    slot: "officer".to_string(),
                    trigger: Some("EnemyTakesHit".to_string()),
                    modifier: Some("AddState".to_string()),
                    attributes: Some("num_rounds=2, state=2".to_string()),
                    description: Some("Apply Burning".to_string()),
                    chance_by_rank: vec![0.25, 0.3, 0.35],
                    value_by_rank: vec![],
                }],
            },
        );

        let nero = seat_from_officer(
            "Nero (T2)",
            CrewSeat::Captain,
            AbilityClass::CaptainManeuver,
            &officers,
        );

        assert_eq!(nero.ability.timing, TimingWindow::AttackPhase);
        assert!(matches!(
            nero.ability.effect,
            AbilityEffect::Burning {
                chance,
                duration_rounds: 2
            } if (chance - 0.3).abs() < 1e-12
        ));
    }

    #[test]
    fn seat_from_officer_uses_tiered_morale_chance() {
        let mut officers = HashMap::new();
        officers.insert(
            normalize_lookup_key("Harry Kim"),
            Officer {
                id: "harry-kim-a79fdf".to_string(),
                name: "Harry Kim".to_string(),
                slot: Some("science".to_string()),
                abilities: vec![OfficerAbility {
                    slot: "officer".to_string(),
                    trigger: Some("RoundStart".to_string()),
                    modifier: Some("AddState".to_string()),
                    attributes: Some("num_rounds=1, state=8".to_string()),
                    description: Some("Apply Morale".to_string()),
                    chance_by_rank: vec![0.1, 0.15, 0.3, 0.6, 1.0],
                    value_by_rank: vec![],
                }],
            },
        );

        let seat = seat_from_officer(
            "Harry Kim (T2)",
            CrewSeat::BelowDeck,
            AbilityClass::BelowDeck,
            &officers,
        );

        assert!(
            matches!(seat.ability.effect, AbilityEffect::Morale(chance) if (chance - 0.15).abs() < 1e-12)
        );
    }
}
