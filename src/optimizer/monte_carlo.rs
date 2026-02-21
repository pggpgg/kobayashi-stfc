use std::collections::HashMap;

use crate::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, SimulationConfig, TimingWindow, TraceMode,
};
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::optimizer::crew_generator::CrewCandidate;

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub candidate: CrewCandidate,
    pub win_rate: f64,
    pub avg_hull_remaining: f64,
}

pub fn run_monte_carlo(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
) -> Vec<SimulationResult> {
    let officer_index = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .ok()
        .map(index_officers_by_name)
        .unwrap_or_default();

    candidates
        .iter()
        .cloned()
        .map(|candidate| {
            let input = scenario_to_combat_input(ship, hostile, &candidate, seed, &officer_index);
            let mut wins = 0usize;
            let mut surviving_hull_sum = 0.0;

            for iteration in 0..iterations {
                let iteration_seed = input.base_seed.wrapping_add(iteration as u64);
                let result = simulate_combat(
                    &input.attacker,
                    &input.defender,
                    SimulationConfig {
                        rounds: input.rounds,
                        seed: iteration_seed,
                        trace_mode: TraceMode::Off,
                    },
                    &input.crew,
                );
                let effective_hull = input.defender_hull * seeded_variance(iteration_seed);

                if result.attacker_won {
                    wins += 1;
                    let remaining = if result.winner_by_round_limit {
                        (result.attacker_hull_remaining / input.attacker.hull_health.max(1.0))
                            .clamp(0.0, 1.0)
                    } else {
                        ((result.total_damage - effective_hull) / effective_hull).clamp(0.0, 1.0)
                    };
                    surviving_hull_sum += remaining;
                }
            }

            let win_rate = if iterations == 0 {
                0.0
            } else {
                wins as f64 / iterations as f64
            };
            let avg_hull_remaining = if iterations == 0 {
                0.0
            } else {
                surviving_hull_sum / iterations as f64
            };

            SimulationResult {
                candidate,
                win_rate,
                avg_hull_remaining,
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct CombatSimulationInput {
    attacker: Combatant,
    defender: Combatant,
    crew: CrewConfiguration,
    rounds: u32,
    defender_hull: f64,
    base_seed: u64,
}

fn scenario_to_combat_input(
    ship: &str,
    hostile: &str,
    candidate: &CrewCandidate,
    seed: u64,
    officers_by_name: &HashMap<String, Officer>,
) -> CombatSimulationInput {
    let base_seed = stable_seed(
        ship,
        hostile,
        &candidate.captain,
        &candidate.bridge,
        &candidate.below_decks,
        seed,
    );
    let ship_hash = hash_identifier(ship);
    let hostile_hash = hash_identifier(hostile);
    let defender_hull = 260.0 + ((hostile_hash >> 16) % 280) as f64;

    CombatSimulationInput {
        attacker: Combatant {
            id: ship.to_string(),
            attack: 95.0 + (ship_hash % 70) as f64,
            mitigation: 0.0,
            pierce: 0.08 + ((ship_hash >> 8) % 14) as f64 / 100.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
        },
        defender: Combatant {
            id: hostile.to_string(),
            attack: 0.0,
            mitigation: 0.25 + (hostile_hash % 35) as f64 / 100.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: defender_hull,
        },
        crew: CrewConfiguration {
            seats: vec![
                seat_from_officer(
                    &candidate.captain,
                    CrewSeat::Captain,
                    AbilityClass::CaptainManeuver,
                    officers_by_name,
                ),
                seat_from_officer(
                    &candidate.bridge,
                    CrewSeat::Bridge,
                    AbilityClass::BridgeAbility,
                    officers_by_name,
                ),
                seat_from_officer(
                    &candidate.below_decks,
                    CrewSeat::BelowDeck,
                    AbilityClass::BelowDeck,
                    officers_by_name,
                ),
            ],
        },
        rounds: 3 + (hostile_hash % 4) as u32,
        defender_hull,
        base_seed,
    }
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
        },
        boosted: hash % 5 == 0,
    }
}

fn index_officers_by_name(officers: Vec<Officer>) -> HashMap<String, Officer> {
    officers
        .into_iter()
        .map(|officer| (normalize_lookup_key(&officer.name), officer))
        .collect()
}

fn normalize_lookup_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn split_name_and_tier(input: &str) -> (String, Option<u8>) {
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

fn hash_identifier(value: &str) -> u64 {
    value.bytes().fold(14695981039346656037u64, |acc, b| {
        (acc ^ u64::from(b)).wrapping_mul(1099511628211)
    })
}

fn seeded_variance(seed: u64) -> f64 {
    let mixed = seed
        .wrapping_add(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let unit = (mixed as f64) / (u64::MAX as f64);
    0.85 + (unit * 0.30)
}

fn stable_seed(
    ship: &str,
    hostile: &str,
    captain: &str,
    bridge: &str,
    below_decks: &str,
    seed: u64,
) -> u64 {
    [ship, hostile, captain, bridge, below_decks]
        .into_iter()
        .flat_map(str::bytes)
        .fold(seed, |acc, b| {
            acc.wrapping_mul(37).wrapping_add(u64::from(b))
        })
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
