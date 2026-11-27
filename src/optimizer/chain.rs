//! Sequential chain-grind trials: attacker HHP carries over, SHP full each link (see DESIGN.md).

use crate::combat::engine::{
    build_combat_setup_with_officer_stat, simulate_combat_from_setup_with_scratch,
    CombatTrialScratch,
};
use crate::combat::{SimulationConfig, TraceMode};
use serde::{Deserialize, Serialize};

use crate::optimizer::loot::ChainLootSummary;
use crate::optimizer::monte_carlo::scenario::{CombatSimulationInput, SharedScenarioData};

/// How to break ties among crews with similar chain-primary success (conditional on primary hit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChainSecondaryObjective {
    /// Maximize mean attacker hull fraction after the Nth kill (minimize hull damage over the chain).
    MinHullDamage,
    /// Maximize expected primary farming resource payout over the chain, accounting for hostile drops,
    /// applicable canonical officer loot bonuses, and ship cargo capacity (including CargoCapacity
    /// officers). Ties break by ending hull fraction.
    MaxExpectedLoot,
    /// Legacy placeholder: `kills / (1 - hull_frac + eps)` before real loot/cargo modeling.
    MaxLootPerHullProxy,
}

#[derive(Debug, Clone)]
pub struct ChainGrindParams {
    pub kills_target: u32,
    pub secondary: ChainSecondaryObjective,
}

/// Aggregated chain-grind Monte Carlo (attached to optimizer [`super::monte_carlo::SimulationResult`] when enabled).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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
    /// Mean ending attacker hull fraction `[0, 1]` conditional on completing all `kills_target` kills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ending_hull_mean_given_primary: Option<f64>,
    /// Expected farming loot metrics when `max_expected_loot` is active and the hostile carries drops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loot: Option<ChainLootSummary>,
}

/// Unified monotone scalar ranking proxy for chain-grind rows (used by `ranking`, `racing`,
/// `qd_archive`, and `genetic` so every optimizer strategy agrees on chain priority).
///
/// Primary completion rate (`win_rate * 1e4`) always dominates; the secondary term stays strictly
/// inside `[0, 1)` so even a `1e-4` primary gap outranks any secondary tie-breaker.
#[inline]
pub(crate) fn chain_ranking_scalar(
    win_rate: f64,
    hull_frac: f64,
    secondary: ChainSecondaryObjective,
    secondary_val: f64,
) -> f64 {
    let sec = match secondary {
        ChainSecondaryObjective::MinHullDamage => secondary_val.clamp(0.0, 1.0),
        ChainSecondaryObjective::MaxLootPerHullProxy => {
            let v = secondary_val.max(0.0);
            if v.is_finite() {
                v / (1.0 + v)
            } else {
                1.0
            }
        }
        ChainSecondaryObjective::MaxExpectedLoot => {
            let loot_term = if secondary_val.is_finite() {
                (secondary_val.max(0.0).ln_1p() * 0.03).min(0.95)
            } else {
                0.95
            };
            let hull_term = hull_frac.clamp(0.0, 1.0) * 1e-5;
            loot_term + hull_term
        }
    };
    win_rate * 1e4 + sec
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ChainTrialOutcome {
    pub primary_success: bool,
    /// Number of clean hostile kills completed in this trial (`0..=kills_target`).
    pub kills_completed: u32,
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
    let def_max = input.defender.hull_health.max(1.0);
    let mut initial_hull_damage = 0.0_f64;
    let mut first_link_r1_clean_win = false;
    let mut last_def_frac = 1.0_f64;

    let defender_faction = shared.defender_faction_for_combat();
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
            defender_profile_weapon_damage_fraction: input.defender_profile_weapon_damage_fraction,
            defender_hull_faction_id: shared
                .hostile_rec
                .as_ref()
                .and_then(|h| h.faction.as_ref().map(|f| f.id))
                .unwrap_or(0),
            defender_hostile_tag_mask: shared.defender_hostile_tag_mask_for_combat(),
            attacker_owner_faction: input.attacker_owner_faction,
            engagement_enemy_types: input.engagement_enemy_types.clone(),
            defender_level: input.defender_level,
            attacker_roster_officer_ids: input.attacker_roster_officer_ids.clone(),
            incoming_shield_mitigation_bonus: input.incoming_shield_mitigation_bonus,
            incoming_shield_mitigation_bonus_rounds: input.incoming_shield_mitigation_bonus_rounds,
            attacker_hyperthermic_decay_fraction: 0.0,
            emit_state_snapshots: false,
            combat_battle_type_id: input.combat_battle_type_id,
            combat_game_context: input.combat_game_context,
            attacker_is_defending: input.attacker_is_defending,
            attacker_at_station: input.attacker_at_station,
        };

        let setup = build_combat_setup_with_officer_stat(
            &input.attacker,
            &input.defender,
            &combat_config,
            &input.crew,
            defender_faction,
            defender_ship_type,
            attacker_ship_type,
            shared.defender_opponent.defender_is_npc_hostile(),
            shared.defender_opponent.defender_is_player_ship(),
            &input.defender_crew,
            input.officer_stat_round.clone(),
        );
        let mut trial_scratch = CombatTrialScratch::for_setup(&setup);
        let result =
            simulate_combat_from_setup_with_scratch(&setup, fight_seed, &mut trial_scratch);
        let effective_max_hull = trial_scratch.attacker_hull_health(&setup).max(1.0);

        last_def_frac = (result.defender_hull_remaining / def_max).clamp(0.0, 1.0);

        if result.winner_by_round_limit {
            return ChainTrialOutcome {
                primary_success: false,
                kills_completed: link,
                hull_fraction_end: None,
                failed_on_stall: true,
                first_link_r1_clean_win,
                last_defender_hull_frac: last_def_frac,
            };
        }
        if !result.attacker_won {
            return ChainTrialOutcome {
                primary_success: false,
                kills_completed: link,
                hull_fraction_end: None,
                failed_on_stall: false,
                first_link_r1_clean_win,
                last_defender_hull_frac: last_def_frac,
            };
        }

        if link == 0 && result.rounds_simulated == 1 {
            first_link_r1_clean_win = true;
        }

        let hull_frac = (result.attacker_hull_remaining / effective_max_hull).clamp(0.0, 1.0);

        if link + 1 == n {
            return ChainTrialOutcome {
                primary_success: true,
                kills_completed: n,
                hull_fraction_end: Some(hull_frac),
                failed_on_stall: false,
                first_link_r1_clean_win,
                last_defender_hull_frac: last_def_frac,
            };
        }

        initial_hull_damage =
            (effective_max_hull - result.attacker_hull_remaining).clamp(0.0, effective_max_hull);
    }

    ChainTrialOutcome {
        primary_success: false,
        kills_completed: 0,
        hull_fraction_end: None,
        failed_on_stall: false,
        first_link_r1_clean_win,
        last_defender_hull_frac: last_def_frac,
    }
}

pub(crate) fn secondary_draw_with_loot(
    secondary: ChainSecondaryObjective,
    kills_target: u32,
    hull_fraction_end: f64,
    loot_on_primary: Option<f64>,
) -> f64 {
    match secondary {
        ChainSecondaryObjective::MinHullDamage => hull_fraction_end,
        ChainSecondaryObjective::MaxExpectedLoot => loot_on_primary.unwrap_or(hull_fraction_end),
        ChainSecondaryObjective::MaxLootPerHullProxy => {
            let k = kills_target.max(1) as f64;
            k / (1.0 - hull_fraction_end + LOOT_PROXY_EPS)
        }
    }
}

#[cfg(test)]
pub(crate) fn secondary_draw(
    secondary: ChainSecondaryObjective,
    kills_target: u32,
    hull_fraction_end: f64,
) -> f64 {
    secondary_draw_with_loot(secondary, kills_target, hull_fraction_end, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{
        Ability, AbilityClass, AbilityCondition, AbilityEffect, Combatant, CrewConfiguration,
        CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType, TimingWindow,
    };
    use crate::optimizer::monte_carlo::DefenderOpponent;

    fn test_shared() -> SharedScenarioData {
        SharedScenarioData {
            ship: "test".into(),
            hostile: "test".into(),
            officer_index: Default::default(),
            profile: Default::default(),
            lcars_data: None,
            resolve_options: Default::default(),
            ship_rec: None,
            hostile_rec: None,
            cached_defender: None,
            cached_rounds: None,
            cached_defender_hull: None,
            cached_pierce: None,
            cached_defender_mitigation: None,
            using_placeholder_combatants: false,
            resolved_support_buffs: vec![],
            applied_support_buffs: vec![],
            support_static_buffs: Default::default(),
            support_defender_static_buffs: Default::default(),
            support_attacker_debuff_static_buffs: Default::default(),
            unknown_support_buff_ids: vec![],
            resolved_defender_support_buffs: vec![],
            resolved_defender_alliance_debuffs: vec![],
            unknown_defender_support_buff_ids: vec![],
            unknown_defender_alliance_debuffs_ids: vec![],
            applied_defender_support_buffs: vec![],
            applied_defender_alliance_debuffs: vec![],
            research_derived_seats: vec![],
            native_defense_derived_seats: vec![],
            native_defense_rounding: false,
            forbidden_tech_derived_seats: vec![],
            borg_alcove_hull_hp_bonus: None,
            class_gated_torpedo_family_hull_hp_bonus: None,
            class_gated_torpedo_family_hostile_shield_mitigation_sum: None,
            defender_opponent: DefenderOpponent::Hostile,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            dual_gate_research_hull_hp: 0.0,
            dual_gate_research_shield_hp: 0.0,
            engagement_enemy_types: Default::default(),
            combat_context: crate::data::officer_eligibility::resolve_combat_context(
                None, false, false, None, None,
            ),
            defender_level: None,
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            player_defender_officer_seats: vec![],
            player_defender_static_buffs: Default::default(),
            defender_officer_stat_totals: Default::default(),
            defender_bridge_officer_stat_totals: Default::default(),
            defender_officer_stat_rows: Vec::new(),
            defender_pending_officer_stat_contributions: vec![],
            pvp: None,
            defender_ship_rec: None,
            defender_profile: None,
            defender_incoming_shield_mitigation_bonus: 0.0,
            defender_incoming_shield_mitigation_bonus_rounds: 0,
            ship_cargo_capacity: None,
            resource_names: std::sync::Arc::new(std::collections::HashMap::new()),
        }
    }

    fn test_input() -> CombatSimulationInput {
        CombatSimulationInput {
            attacker: Combatant {
                id: "a".into(),
                attack: 100.0,
                hull_health: 1000.0,
                ..Combatant::default()
            },
            defender: Combatant {
                id: "d".into(),
                attack: 50.0,
                hull_health: 150.0,
                ..Combatant::default()
            },
            defender_crew: CrewConfiguration::default(),
            crew: CrewConfiguration::default(),
            rounds: 4,
            defender_hull: 150.0,
            base_seed: 0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_profile_weapon_damage_fraction: 0.0,
            engagement_enemy_types: Default::default(),
            defender_level: None,
            attacker_roster_officer_ids: vec![],
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            officer_stat_round: None,
            combat_battle_type_id: None,
            combat_game_context: crate::combat::CombatGameContext::OpenSpace,
            attacker_is_defending: false,
            attacker_at_station: false,
        }
    }

    #[test]
    fn conditional_max_hull_chain_carries_damage_between_fights() {
        let shared = test_shared();
        let mut input = test_input();
        let run = |input: &CombatSimulationInput, kills_target| {
            run_chain_trial(
                &shared,
                input,
                &ChainGrindParams {
                    kills_target,
                    secondary: ChainSecondaryObjective::MinHullDamage,
                },
                42,
            )
        };
        let baseline_one = run(&input, 1).hull_fraction_end.unwrap();
        let baseline_two = run(&input, 2).hull_fraction_end.unwrap();
        assert!(
            baseline_two < baseline_one,
            "unbuffed damage must carry forward"
        );
        let old_fight = |seed, damage| {
            let config = SimulationConfig {
                rounds: input.rounds,
                seed,
                initial_attacker_hull_damage: damage,
                ..SimulationConfig::default()
            };
            crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
                &input.attacker,
                &input.defender,
                &config,
                &input.crew,
                OpponentFactionTag::Unknown,
                ShipType::Battleship,
                ShipType::Battleship,
                true,
                false,
                &input.defender_crew,
            )
        };
        let first = old_fight(42, 0.0);
        let second = old_fight(
            42 + FIGHT_SEED_STRIDE,
            1000.0 - first.attacker_hull_remaining,
        );
        assert_eq!(
            baseline_two,
            second.attacker_hull_remaining / 1000.0,
            "unbuffed chain must match the original fight path"
        );
        input.crew.seats.push(CrewSeatContext::legacy(
            CrewSeat::Bridge,
            Ability {
                weapon_scope: Default::default(),
                name: "max hull".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HullHpMultiplier(0.5),
                condition: Some(AbilityCondition::AttackerShipTypeIs(ShipType::Battleship)),
            },
            false,
        ));
        let one = run(&input, 1).hull_fraction_end.unwrap();
        let two = run(&input, 2).hull_fraction_end.unwrap();
        assert!(
            two < one,
            "conditional max hull must not erase carried damage: {one} {two}"
        );
        assert!(
            two > baseline_two,
            "larger pool should retain more hull fraction"
        );
        assert_eq!(
            two,
            run(&input, 2).hull_fraction_end.unwrap(),
            "seeded chain must be deterministic"
        );
        input.crew.seats[0].ability.condition =
            Some(AbilityCondition::AttackerShipTypeIs(ShipType::Explorer));
        assert_eq!(
            baseline_two,
            run(&input, 2).hull_fraction_end.unwrap(),
            "nonmatching gate must preserve baseline"
        );
        input.crew.seats[0].ability.condition =
            Some(AbilityCondition::RoundRange { min: 1, max: 1 });
        assert_eq!(
            baseline_two,
            run(&input, 2).hull_fraction_end.unwrap(),
            "an expired max-HP effect must not heal carried damage"
        );
    }

    #[test]
    fn weapon_schedules_reset_between_chain_fights() {
        use crate::combat::{WeaponFiringSchedule, WeaponStats};

        let shared = test_shared();
        let mut input = test_input();
        input.rounds = 4;
        input.attacker.hull_health = 1000.0;
        input.attacker.mitigation = 0.0;
        input.attacker.crit_chance = 0.0;
        // Attacker slot 0 fires every round (50 dmg); slot 1 warms up on R2 with cooldown 2 (150 dmg).
        // Against a 150 HP defender, every fresh fight takes 2 rounds (R1: 50 dmg, R2: 50+150=200 -> kill).
        input.attacker.weapons = vec![
            WeaponStats {
                attack: 50.0,
                shots: Some(1),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 1,
                }),
                ..WeaponStats::default()
            },
            WeaponStats {
                attack: 150.0,
                shots: Some(1),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 2,
                    interval_rounds: 2,
                }),
                ..WeaponStats::default()
            },
        ];
        input.defender.hull_health = 150.0;
        input.defender.mitigation = 0.0;
        input.defender.crit_chance = 0.0;
        // Defender slot 0 fires ONCE on round 1 with cool_down=100 (deals 100 dmg).
        // If schedules reset per fight, each 2-round link deals exactly 100 dmg (not 200 if cooldown ignored,
        // and not 0 on fight 2 if cooldown persisted across links).
        input.defender.weapons = vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            firing_schedule: Some(WeaponFiringSchedule {
                first_round: 1,
                interval_rounds: 100,
            }),
            ..WeaponStats::default()
        }];

        let one = run_chain_trial(
            &shared,
            &input,
            &ChainGrindParams {
                kills_target: 1,
                secondary: ChainSecondaryObjective::MinHullDamage,
            },
            7,
        );
        let two = run_chain_trial(
            &shared,
            &input,
            &ChainGrindParams {
                kills_target: 2,
                secondary: ChainSecondaryObjective::MinHullDamage,
            },
            7,
        );
        assert!(one.primary_success);
        assert!(two.primary_success);
        // Fight 1: defender hits once on R1 (-100 hull -> 900/1000 = 0.9), silent on R2 when attacker kills.
        assert!(
            (one.hull_fraction_end.unwrap() - 0.9).abs() < 1e-12,
            "fight 1 should take exactly one R1 defender hit (0.9 hull), got {:?}",
            one.hull_fraction_end
        );
        // Fight 2: defender schedule resets to R1 (-100 hull again -> 800/1000 = 0.8),
        // and attacker slot 1 warms up again (silent R1, fires R2 -> 2 rounds, not R1 kill).
        assert!(
            (two.hull_fraction_end.unwrap() - 0.8).abs() < 1e-12,
            "fight 2 must reset both attacker warm-up and defender cooldown (0.8 hull), got {:?}",
            two.hull_fraction_end
        );
    }

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
