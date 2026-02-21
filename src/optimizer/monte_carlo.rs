use crate::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, SimulationConfig, TimingWindow, TraceMode,
};
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
    candidates
        .iter()
        .cloned()
        .map(|candidate| {
            let input = scenario_to_combat_input(ship, hostile, &candidate, seed);
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

                if result.total_damage >= effective_hull {
                    wins += 1;
                    let remaining =
                        ((result.total_damage - effective_hull) / effective_hull).clamp(0.0, 1.0);
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
        },
        crew: CrewConfiguration {
            seats: vec![
                seat_from_officer(
                    &candidate.captain,
                    CrewSeat::Captain,
                    AbilityClass::CaptainManeuver,
                ),
                seat_from_officer(
                    &candidate.bridge,
                    CrewSeat::Bridge,
                    AbilityClass::BridgeAbility,
                ),
                seat_from_officer(
                    &candidate.below_decks,
                    CrewSeat::BelowDeck,
                    AbilityClass::BelowDeck,
                ),
            ],
        },
        rounds: 3 + (hostile_hash % 4) as u32,
        defender_hull: 260.0 + ((hostile_hash >> 16) % 280) as f64,
        base_seed,
    }
}

fn seat_from_officer(id: &str, seat: CrewSeat, class: AbilityClass) -> CrewSeatContext {
    let hash = hash_identifier(id);
    let effect = if hash % 2 == 0 {
        AbilityEffect::AttackMultiplier(0.05 + ((hash >> 8) % 12) as f64 / 100.0)
    } else {
        AbilityEffect::PierceBonus(0.01 + ((hash >> 8) % 12) as f64 / 100.0)
    };

    CrewSeatContext {
        seat,
        ability: Ability {
            name: format!(
                "{}_{}",
                id.to_ascii_lowercase(),
                format!("{seat:?}").to_ascii_lowercase()
            ),
            class,
            timing: TimingWindow::AttackPhase,
            boostable: true,
            effect,
        },
        boosted: hash % 5 == 0,
    }
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
