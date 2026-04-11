//! Chain grind: N=1 primary rate matches single-fight win rate; lexicographic ranking with chain summaries.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::chain::{
    ChainGrindParams, ChainSecondaryObjective, ChainSimulationSummary,
};
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    run_monte_carlo_with_registry, DefenderOpponent, SimulationResult,
};
use kobayashi::optimizer::ranking::rank_results;

#[test]
fn chain_n1_primary_rate_matches_single_fight_win_rate() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let candidate = CrewCandidate {
        captain: "ent-e-picard-556227".to_string(),
        bridge: vec![
            "ent-e-data-871245".to_string(),
            "five-of-eleven-d9aa11".to_string(),
        ],
        below_decks: vec!["harry-kim-a79fdf (T5)".to_string()],
    };
    let iters = 320;
    let seed = 77_001_u64;

    let (single, ph1) = run_monte_carlo_with_registry(
        &registry,
        "uss_enterprise_d",
        "kobayashi_theoretical_damage_sponge",
        Some(5),
        Some(7),
        &[candidate.clone()],
        iters,
        seed,
        Some(DEMO_PROFILE_ID),
        None,
        None,
        DefenderOpponent::Hostile,
    );
    let (chain, ph2) = run_monte_carlo_with_registry(
        &registry,
        "uss_enterprise_d",
        "kobayashi_theoretical_damage_sponge",
        Some(5),
        Some(7),
        &[candidate],
        iters,
        seed,
        Some(DEMO_PROFILE_ID),
        None,
        Some(ChainGrindParams {
            kills_target: 1,
            secondary: ChainSecondaryObjective::MinHullDamage,
        }),
        DefenderOpponent::Hostile,
    );

    assert!(
        !ph1 && !ph2,
        "expected resolved ship/hostile (not placeholder combatants)"
    );
    assert_eq!(single.len(), 1);
    assert_eq!(chain.len(), 1);
    let a = single[0].win_rate;
    let b = chain[0].win_rate;
    assert!(
        (a - b).abs() < 1e-12,
        "N=1 chain primary rate should match single-fight win_rate for the same seed stream; single={a} chain={b}"
    );
    assert!(
        chain[0].chain.is_some(),
        "chain MC should attach ChainSimulationSummary"
    );
}

fn simulation_result_chain(
    captain: &str,
    win_rate: f64,
    avg_hull_remaining: f64,
) -> SimulationResult {
    SimulationResult {
        candidate: CrewCandidate {
            captain: captain.to_string(),
            bridge: vec![],
            below_decks: vec![],
        },
        win_rate,
        win_rate_ci_low: 0.0,
        win_rate_ci_high: 1.0,
        stall_rate: 0.0,
        stall_rate_ci_low: 0.0,
        stall_rate_ci_high: 0.0,
        loss_rate: 0.0,
        loss_rate_ci_low: 0.0,
        loss_rate_ci_high: 0.0,
        r1_kill_rate: 0.0,
        r1_kill_rate_ci_low: 0.0,
        r1_kill_rate_ci_high: 0.0,
        avg_hull_remaining,
        avg_hull_remaining_ci_low: 0.0,
        avg_hull_remaining_ci_high: 1.0,
        avg_defender_hull_remaining: 0.0,
        avg_defender_hull_remaining_ci_low: 0.0,
        avg_defender_hull_remaining_ci_high: 0.0,
        chain: Some(ChainSimulationSummary {
            kills_target: 3,
            secondary_objective: ChainSecondaryObjective::MinHullDamage,
            primary_success_rate: win_rate,
            primary_ci_low: 0.0,
            primary_ci_high: 1.0,
            secondary_mean_given_primary: avg_hull_remaining,
            secondary_ci_low: 0.0,
            secondary_ci_high: 1.0,
            n_primary_successes: 1,
        }),
    }
}

#[test]
fn rank_results_chain_is_lexicographic_primary_then_secondary() {
    let ranked = rank_results(vec![
        simulation_result_chain("low_primary", 0.5, 0.9),
        simulation_result_chain("best", 0.6, 0.1),
        simulation_result_chain("tie_break", 0.5, 0.95),
    ]);
    assert_eq!(ranked[0].captain, "best");
    assert_eq!(ranked[1].captain, "tie_break");
    assert_eq!(ranked[2].captain, "low_primary");
}
