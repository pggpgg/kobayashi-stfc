//! Closed-form expected hull damage for analytical pre-filtering (see docs/DESIGN.md §6.2).
//!
//! This is a **ranking proxy**, not a win-rate predictor: it estimates total hull damage the
//! attacker would deal over `input.rounds` using only static combatant stats (post-profile and
//! post–static LCARS buffs from the built `CombatSimulationInput`). It intentionally ignores
//! per-round ability modifiers, shots/crit/proc variance, morale, burning, and defender return
//! fire — those are left to full simulation.

use crate::combat::{
    apply_shield_hull_split, compute_apex_damage_factor, compute_damage_through_factor,
    compute_isolytic_taken,
};
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::CombatSimulationInput;

/// Expected total hull damage to the defender over `input.rounds`, using expected values for crit
/// and officer proc. Returns a finite non-negative `f32` suitable for sorting candidates.
pub(crate) fn expected_damage(input: &CombatSimulationInput) -> f32 {
    let v = expected_hull_damage_total(input);
    if v.is_finite() && v >= 0.0 {
        v as f32
    } else {
        0.0
    }
}

fn expected_hull_damage_total(input: &CombatSimulationInput) -> f64 {
    let attacker = &input.attacker;
    let defender = &input.defender;
    let mitigation_mult = (1.0_f64 - defender.mitigation).max(0.0);
    let dtf = compute_damage_through_factor(mitigation_mult, attacker.pierce, 0.0);
    let apex = compute_apex_damage_factor(attacker.apex_shred, defender.apex_barrier);
    let e_crit = 1.0 + attacker.crit_chance * (attacker.crit_multiplier - 1.0);
    let e_proc = 1.0 + attacker.proc_chance * (attacker.proc_multiplier - 1.0);
    let shield_mit_base = defender.shield_mitigation.clamp(0.0, 1.0);

    let mut shield_rem = defender.shield_health.max(0.0);
    let mut total_hull = 0.0;

    for _ in 0..input.rounds {
        for wi in 0..attacker.weapon_count() {
            let shots = attacker.weapon_base_shots(wi);
            let Some(w_atk) = attacker.weapon_attack(wi) else {
                continue;
            };
            for _ in 0..shots {
                let pre = w_atk * dtf * e_crit * e_proc;
                let iso_taken = compute_isolytic_taken(
                    pre,
                    attacker.isolytic_damage,
                    defender.isolytic_defense,
                    0.0,
                );
                let after_apex = (pre + iso_taken) * apex;
                let sm = if shield_rem > 0.0 {
                    shield_mit_base
                } else {
                    0.0
                };
                let (sd, hd) = apply_shield_hull_split(after_apex, sm, shield_rem);
                shield_rem = (shield_rem - sd).max(0.0);
                total_hull += hd;
            }
        }
    }

    total_hull
}

/// Drop candidates whose closed-form expected hull damage falls below
/// `hull_fraction × defender.hull_health`. These crews deal negligible damage and
/// cannot kill the hostile in any realistic number of rounds.
///
/// The analytical formula underestimates (ignores crit/proc variance, morale, burning)
/// so it produces false negatives rather than false positives. A conservative threshold
/// (e.g. 0.05) eliminates only truly hopeless crews.
///
/// Returns the filtered list and the count of crews dropped.
pub(crate) fn prune_candidates_by_expected_hull_damage(
    shared: &crate::optimizer::monte_carlo::scenario::SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    hull_fraction: f64,
) -> (Vec<CrewCandidate>, usize) {
    use crate::optimizer::monte_carlo::scenario::scenario_to_combat_input_from_shared;

    let n_before = candidates.len();
    let filtered: Vec<CrewCandidate> = candidates
        .into_iter()
        .filter(|c| {
            let input = scenario_to_combat_input_from_shared(shared, c, seed);
            let damage = expected_damage(&input) as f64;
            let threshold = input.defender_hull * hull_fraction;
            damage >= threshold
        })
        .collect();
    let dropped = n_before - filtered.len();
    (filtered, dropped)
}

/// Drop candidates where static ability gates fail against the current hostile.
///
/// Evaluates each crew's ability conditions against the shared scenario (defender faction,
/// ship type, PvE/PvP, etc.) and drops crews whose fraction of failing conditional abilities
/// exceeds `max_gated_fraction`.  A conservative threshold (e.g. 0.95) only eliminates crews
/// where nearly every conditional ability is mismatched (e.g. all Borg-locked officers
/// against a non-Borg hostile).
///
/// Returns the filtered list and the count of crews dropped.
pub(crate) fn prune_candidates_by_static_gates(
    shared: &crate::optimizer::monte_carlo::scenario::SharedScenarioData,
    candidates: Vec<CrewCandidate>,
    seed: u64,
    max_gated_fraction: f64,
) -> (Vec<CrewCandidate>, usize) {
    use crate::optimizer::monte_carlo::scenario::scenario_to_combat_input_from_shared;

    let n_before = candidates.len();
    let filtered: Vec<CrewCandidate> = candidates
        .into_iter()
        .filter(|c| {
            let input = scenario_to_combat_input_from_shared(shared, c, seed);
            let frac =
                crate::optimizer::matchup_priors::static_gate_fail_fraction(shared, &input.crew)
                    as f64;
            frac <= max_gated_fraction
        })
        .collect();
    let dropped = n_before - filtered.len();
    (filtered, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Combatant, CrewConfiguration, EnemyTypes, OpponentFactionTag};

    fn minimal_input(attacker_attack: f64) -> CombatSimulationInput {
        CombatSimulationInput {
            attacker: Combatant {
                id: "a".to_string(),
                attack: attacker_attack,
                mitigation: 0.0,
                armor: 0.0,
                shield_deflection: 0.0,
                dodge: 0.0,
                damage_reduction: 0.0,
                pierce: 0.1,
                crit_chance: 0.2,
                crit_multiplier: 2.0,
                crit_damage_floor: 0.0,
                proc_chance: 0.1,
                proc_multiplier: 1.5,
                end_of_round_damage: 0.0,
                hull_health: 1000.0,
                shield_health: 0.0,
                shield_mitigation: 0.8,
                apex_barrier: 0.0,
                apex_shred: 0.0,
                isolytic_damage: 0.0,
                isolytic_defense: 0.0,
                weapons: vec![],
                hostile_mitigation_params: None,
            },
            defender: Combatant {
                id: "d".to_string(),
                attack: 0.0,
                mitigation: 0.5,
                armor: 0.0,
                shield_deflection: 0.0,
                dodge: 0.0,
                damage_reduction: 0.0,
                pierce: 0.0,
                crit_chance: 0.0,
                crit_multiplier: 1.0,
                crit_damage_floor: 0.0,
                proc_chance: 0.0,
                proc_multiplier: 1.0,
                end_of_round_damage: 0.0,
                hull_health: 500.0,
                shield_health: 0.0,
                shield_mitigation: 0.8,
                apex_barrier: 0.0,
                apex_shred: 0.0,
                isolytic_damage: 0.0,
                isolytic_defense: 0.0,
                weapons: vec![],
                hostile_mitigation_params: None,
            },
            defender_crew: CrewConfiguration { seats: vec![] },
            crew: CrewConfiguration { seats: vec![] },
            rounds: 3,
            defender_hull: 500.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            base_seed: 0,
            engagement_enemy_types: EnemyTypes::default(),
            defender_level: None,
            attacker_roster_officer_ids: Vec::new(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            officer_stat_round: None,
        }
    }

    #[test]
    fn expected_damage_increases_with_weapon_attack() {
        let low = minimal_input(50.0);
        let high = minimal_input(150.0);
        assert!(expected_damage(&high) > expected_damage(&low));
    }

    #[test]
    fn expected_damage_non_negative() {
        let x = minimal_input(10.0);
        assert!(expected_damage(&x) >= 0.0);
    }
}
