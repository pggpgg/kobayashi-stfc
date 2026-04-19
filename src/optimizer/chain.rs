//! Sequential chain-grind trials: attacker HHP carries over, SHP full each link (see DESIGN.md).

use crate::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, OpponentFactionTag, SimulationConfig,
    TraceMode,
};
use serde::{Deserialize, Serialize};

use crate::optimizer::monte_carlo::scenario::{CombatSimulationInput, SharedScenarioData};

/// How to break ties among crews with similar chain-primary success (conditional on primary hit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainSecondaryObjective {
    /// Maximize mean attacker hull fraction after the Nth kill (minimize hull damage over the chain).
    MinHullDamage,
    /// Placeholder: `kills / (1 - hull_frac + eps)` until real loot is modeled in combat.
    MaxLootPerHullProxy,
}

#[derive(Debug, Clone)]
pub struct ChainGrindParams {
    pub kills_target: u32,
    pub secondary: ChainSecondaryObjective,
}

/// Aggregated chain-grind Monte Carlo (attached to optimizer [`super::monte_carlo::SimulationResult`] when enabled).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChainSimulationSummary {
    pub kills_target: u32,
    pub secondary_objective: ChainSecondaryObjective,
    pub primary_success_rate: f64,
    pub primary_ci_low: f64,
    pub primary_ci_high: f64,
    pub secondary_mean_given_primary: f64,
    pub secondary_ci_low: f64,
    pub secondary_ci_high: f64,
    pub n_primary_successes: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ChainTrialOutcome {
    pub primary_success: bool,
    /// Set when `primary_success` (hull remaining / max hull after last fight).
    pub hull_fraction_end: Option<f64>,
    /// True if chain failed because a link ended in round-limit stall (attacker "won" by tie-break).
    pub failed_on_stall: bool,
    /// First link was a clean win in round 1 (no round-limit).
    pub first_link_r1_clean_win: bool,
    /// Defender hull fraction remaining after the last simulated fight in this trial.
    pub last_defender_hull_frac: f64,
}

const LOOT_PROXY_EPS: f64 = 1e-9;
const FIGHT_SEED_STRIDE: u64 = 982_451_653;

/// One Monte Carlo trial: up to `kills_target` consecutive clean wins with HHP carry-over and full SHP per link.
pub(crate) fn run_chain_trial(
    shared: &SharedScenarioData,
    input: &CombatSimulationInput,
    params: &ChainGrindParams,
    trial_seed: u64,
) -> ChainTrialOutcome {
    let n = params.kills_target.max(1);
    let max_hull = input.attacker.hull_health.max(1.0);
    let def_max = input.defender.hull_health.max(1.0);
    let mut initial_hull_damage = 0.0_f64;
    let mut first_link_r1_clean_win = false;
    let mut last_def_frac = 1.0_f64;

    let defender_faction = shared
        .hostile_rec
        .as_ref()
        .map(|h| h.opponent_faction_tag())
        .unwrap_or(OpponentFactionTag::Unknown);
    let defender_ship_type = shared.defender_ship_type_for_combat();
    let attacker_ship_type = shared.attacker_ship_type_for_combat();

    for link in 0..n {
        let fight_seed = trial_seed.wrapping_add((link as u64).wrapping_mul(FIGHT_SEED_STRIDE));
        let combat_config = SimulationConfig {
            rounds: input.rounds,
            seed: fight_seed,
            trace_mode: TraceMode::Off,
            initial_attacker_hull_damage: initial_hull_damage,
            weapon_damage_profile_additive_pool: input.weapon_damage_profile_additive_pool,
            profile_weapon_damage_fraction: input.profile_weapon_damage_fraction,
            defender_hull_faction_id: shared
                .hostile_rec
                .as_ref()
                .and_then(|h| h.faction.as_ref().map(|f| f.id))
                .unwrap_or(0),
            defender_hostile_tag_mask: shared.defender_hostile_tag_mask_for_combat(),
        };

        let result = simulate_combat_with_defender_faction_and_defender_crew(
            &input.attacker,
            &input.defender,
            combat_config,
            &input.crew,
            defender_faction,
            defender_ship_type,
            attacker_ship_type,
            shared.defender_opponent.defender_is_npc_hostile(),
            shared.defender_opponent.defender_is_player_ship(),
            &input.defender_crew,
        );

        last_def_frac = (result.defender_hull_remaining / def_max).clamp(0.0, 1.0);

        if result.winner_by_round_limit {
            return ChainTrialOutcome {
                primary_success: false,
                hull_fraction_end: None,
                failed_on_stall: true,
                first_link_r1_clean_win,
                last_defender_hull_frac: last_def_frac,
            };
        }
        if !result.attacker_won {
            return ChainTrialOutcome {
                primary_success: false,
                hull_fraction_end: None,
                failed_on_stall: false,
                first_link_r1_clean_win,
                last_defender_hull_frac: last_def_frac,
            };
        }

        if link == 0 && result.rounds_simulated == 1 {
            first_link_r1_clean_win = true;
        }

        let hull_frac = (result.attacker_hull_remaining / max_hull).clamp(0.0, 1.0);

        if link + 1 == n {
            return ChainTrialOutcome {
                primary_success: true,
                hull_fraction_end: Some(hull_frac),
                failed_on_stall: false,
                first_link_r1_clean_win,
                last_defender_hull_frac: last_def_frac,
            };
        }

        initial_hull_damage = (max_hull - result.attacker_hull_remaining).clamp(0.0, max_hull);
    }

    ChainTrialOutcome {
        primary_success: false,
        hull_fraction_end: None,
        failed_on_stall: false,
        first_link_r1_clean_win,
        last_defender_hull_frac: last_def_frac,
    }
}

pub(crate) fn secondary_draw(
    secondary: ChainSecondaryObjective,
    kills_target: u32,
    hull_fraction_end: f64,
) -> f64 {
    match secondary {
        ChainSecondaryObjective::MinHullDamage => hull_fraction_end,
        ChainSecondaryObjective::MaxLootPerHullProxy => {
            let k = kills_target.max(1) as f64;
            k / (1.0 - hull_fraction_end + LOOT_PROXY_EPS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_loot_proxy_monotone() {
        let a = secondary_draw(ChainSecondaryObjective::MaxLootPerHullProxy, 3, 0.9);
        let b = secondary_draw(ChainSecondaryObjective::MaxLootPerHullProxy, 3, 0.5);
        assert!(
            a > b,
            "more hull left => higher placeholder loot/hull proxy"
        );
    }
}
