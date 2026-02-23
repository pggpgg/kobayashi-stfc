use std::collections::HashMap;

use rayon::prelude::*;

use crate::combat::{
    mitigation, pierce_damage_through_bonus, simulate_combat, Ability, AbilityClass, AbilityEffect,
    AttackerStats, Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats,
    ShipType, SimulationConfig, TimingWindow, TraceMode,
};
use crate::data::loader::{resolve_hostile, resolve_ship};
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::profile::{apply_profile_to_attacker, load_profile, PlayerProfile, DEFAULT_PROFILE_PATH};
use crate::optimizer::crew_generator::{CrewCandidate, BRIDGE_SLOTS, BELOW_DECKS_SLOTS};

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
    run_monte_carlo_with_parallelism(ship, hostile, candidates, iterations, seed, false)
}

/// Like [run_monte_carlo] but distributes candidates across all CPU cores via Rayon.
/// Use for large candidate lists (e.g. optimizer sweeps). Results order matches input order.
pub fn run_monte_carlo_parallel(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
) -> Vec<SimulationResult> {
    run_monte_carlo_with_parallelism(ship, hostile, candidates, iterations, seed, true)
}

fn run_monte_carlo_with_parallelism(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
    parallel: bool,
) -> Vec<SimulationResult> {
    let officer_index = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .ok()
        .map(index_officers_by_name)
        .unwrap_or_default();
    let profile = load_profile(DEFAULT_PROFILE_PATH);

    let run_one = |candidate: &CrewCandidate| {
        let input = scenario_to_combat_input(ship, hostile, candidate, seed, &officer_index, &profile);
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
            candidate: candidate.clone(),
            win_rate,
            avg_hull_remaining,
        }
    };

    if parallel {
        candidates.par_iter().map(run_one).collect()
    } else {
        candidates.iter().map(run_one).collect()
    }
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
    profile: &PlayerProfile,
) -> CombatSimulationInput {
    let base_seed = stable_seed(ship, hostile, &candidate.captain, &candidate.bridge, &candidate.below_decks, seed);

    let crew_seats = build_crew_seats(candidate, officers_by_name);

    if let (Some(ship_rec), Some(hostile_rec)) = (resolve_ship(ship), resolve_hostile(hostile)) {
        let defender_mitigation = computed_defender_mitigation(ship, hostile);
        let defender_hull = hostile_rec.hull_health;
        let rounds = 100u32.min(10u32.saturating_add(hostile_rec.level as u32));
        return CombatSimulationInput {
            attacker: apply_profile_to_attacker(
                Combatant {
                    id: ship.to_string(),
                    attack: ship_rec.attack,
                    mitigation: 0.0,
                    pierce: pierce_damage_through_bonus(
                        hostile_rec.to_defender_stats(),
                        ship_rec.to_attacker_stats(),
                        hostile_rec.ship_type(),
                    ),
                    crit_chance: ship_rec.crit_chance,
                    crit_multiplier: ship_rec.crit_damage,
                    proc_chance: 0.0,
                    proc_multiplier: 1.0,
                    end_of_round_damage: 0.0,
                    hull_health: ship_rec.hull_health,
                    shield_health: ship_rec.shield_health,
                    shield_mitigation: ship_rec.shield_mitigation.unwrap_or(0.8),
                    apex_barrier: 0.0,
                    apex_shred: ship_rec.apex_shred,
                },
                profile,
            ),
            defender: Combatant {
                id: hostile.to_string(),
                attack: 0.0,
                mitigation: defender_mitigation,
                pierce: 0.0,
                crit_chance: 0.0,
                crit_multiplier: 1.0,
                proc_chance: 0.0,
                proc_multiplier: 1.0,
                end_of_round_damage: 0.0,
                hull_health: defender_hull,
                shield_health: hostile_rec.shield_health,
                shield_mitigation: hostile_rec.shield_mitigation.unwrap_or(0.8),
                apex_barrier: hostile_rec.apex_barrier,
                apex_shred: 0.0,
            },
            crew: CrewConfiguration { seats: crew_seats.clone() },
            rounds,
            defender_hull,
            base_seed,
        };
    }

    let ship_hash = hash_identifier(ship);
    let hostile_hash = hash_identifier(hostile);
    let defender_hull = 260.0 + ((hostile_hash >> 16) % 280) as f64;
    let defender_mitigation = computed_defender_mitigation(ship, hostile);

    CombatSimulationInput {
        attacker: apply_profile_to_attacker(
            Combatant {
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
                shield_health: 0.0,
                shield_mitigation: 0.8,
                apex_barrier: 0.0,
                apex_shred: 0.0,
            },
            profile,
        ),
        defender: Combatant {
            id: hostile.to_string(),
            attack: 0.0,
            mitigation: defender_mitigation,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: defender_hull,
            shield_health: 400.0,
            shield_mitigation: 0.8,
            apex_barrier: 0.0,
            apex_shred: 0.0,
        },
        crew: CrewConfiguration { seats: crew_seats },
        rounds: 3 + (hostile_hash % 4) as u32,
        defender_hull,
        base_seed,
    }
}

fn synthetic_ship_type(identifier_hash: u64) -> ShipType {
    match identifier_hash % 4 {
        0 => ShipType::Battleship,
        1 => ShipType::Explorer,
        2 => ShipType::Interceptor,
        _ => ShipType::Survey,
    }
}

fn synthetic_defender_stats(hostile_hash: u64) -> DefenderStats {
    DefenderStats {
        armor: 120.0 + (hostile_hash % 260) as f64,
        shield_deflection: 110.0 + ((hostile_hash >> 11) % 240) as f64,
        dodge: 90.0 + ((hostile_hash >> 23) % 220) as f64,
    }
}

fn synthetic_attacker_stats(ship_hash: u64) -> AttackerStats {
    AttackerStats {
        armor_piercing: 85.0 + (ship_hash % 220) as f64,
        shield_piercing: 80.0 + ((ship_hash >> 9) % 210) as f64,
        accuracy: 75.0 + ((ship_hash >> 21) % 200) as f64,
    }
}

fn computed_defender_mitigation(ship: &str, hostile: &str) -> f64 {
    if let (Some(ship_rec), Some(hostile_rec)) = (resolve_ship(ship), resolve_hostile(hostile)) {
        return mitigation(
            hostile_rec.to_defender_stats(),
            ship_rec.to_attacker_stats(),
            hostile_rec.ship_type(),
        );
    }
    let attacker = synthetic_attacker_stats(hash_identifier(ship));
    let defender_hash = hash_identifier(hostile);
    let defender = synthetic_defender_stats(defender_hash);
    let ship_type = synthetic_ship_type(defender_hash);
    mitigation(defender, attacker, ship_type)
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
            },
            boosted: false,
        });
    }
    out
}

fn build_crew_seats(
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

fn stable_seed(
    ship: &str,
    hostile: &str,
    captain: &str,
    bridge: &[String],
    below_decks: &[String],
    seed: u64,
) -> u64 {
    let mut acc = seed;
    for s in [ship, hostile, captain]
        .into_iter()
        .chain(bridge.iter().map(String::as_str))
        .chain(below_decks.iter().map(String::as_str))
    {
        for b in s.bytes() {
            acc = acc.wrapping_mul(37).wrapping_add(u64::from(b));
        }
    }
    acc
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
    fn computed_mitigation_changes_with_defense_and_piercing_inputs() {
        let ship_hash = hash_identifier("USS Enterprise");
        let hostile_hash = hash_identifier("Hostile D4");
        let ship_type = synthetic_ship_type(hostile_hash);

        let base_defender = synthetic_defender_stats(hostile_hash);
        let base_attacker = synthetic_attacker_stats(ship_hash);
        let base = mitigation(base_defender, base_attacker, ship_type);

        let stronger_defender = mitigation(
            DefenderStats {
                armor: base_defender.armor * 1.4,
                shield_deflection: base_defender.shield_deflection * 1.4,
                dodge: base_defender.dodge * 1.4,
            },
            base_attacker,
            ship_type,
        );
        let stronger_attacker = mitigation(
            base_defender,
            AttackerStats {
                armor_piercing: base_attacker.armor_piercing * 1.4,
                shield_piercing: base_attacker.shield_piercing * 1.4,
                accuracy: base_attacker.accuracy * 1.4,
            },
            ship_type,
        );

        assert_ne!(base, stronger_defender);
        assert_ne!(base, stronger_attacker);
    }

    #[test]
    fn computed_mitigation_is_bounded_between_zero_and_one() {
        let samples = [
            ("Mayflower", "Borg Cube"),
            ("Saladin", "Klingon Patrol"),
            ("Kelvin", "Romulan Interceptor"),
            ("Defiant", "Dominion Cruiser"),
        ];

        for (ship, hostile) in samples {
            let value = computed_defender_mitigation(ship, hostile);
            assert!(
                (0.0..=1.0).contains(&value),
                "mitigation={value} for {ship} vs {hostile}"
            );
        }
    }

    #[test]
    fn computed_mitigation_is_deterministic_for_same_inputs() {
        let first = computed_defender_mitigation("Franklin", "Hostile Miner");
        let second = computed_defender_mitigation("Franklin", "Hostile Miner");
        assert_eq!(first, second);

        let candidate = CrewCandidate {
            captain: "Kirk".to_string(),
            bridge: vec!["Spock".to_string(), "Spock".to_string()],
            below_decks: vec!["Scotty".to_string(), "Scotty".to_string(), "Scotty".to_string()],
        };
        let officers = HashMap::new();
        let profile = PlayerProfile::default();

        let one = scenario_to_combat_input("Franklin", "Hostile Miner", &candidate, 7, &officers, &profile);
        let two = scenario_to_combat_input("Franklin", "Hostile Miner", &candidate, 7, &officers, &profile);
        assert_eq!(one.defender.mitigation, two.defender.mitigation);
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
