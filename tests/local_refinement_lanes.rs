//! Local refinement (roadmap §1.1) across the search lanes that support it.
//!
//! These assert at the optimizer level rather than over HTTP because the interesting property is
//! *that the pass ran on this lane* — which the pass's own stats report — not that it happened to
//! find an improvement. Whether headroom exists above a lane's finalists depends entirely on the
//! scenario: against the bundled catalog the genetic lane's top crews reach a perfect score, so a
//! test demanding an accepted improvement there would be asserting a property of the fixture data,
//! not of the wiring, and would go red the moment the officer catalog changed.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;
use kobayashi::optimizer::refinement::LocalRefinementParams;
use kobayashi::optimizer::{
    optimize_scenario_with_progress_with_registry, OptimizationScenario, OptimizeRunOutcome,
    OptimizerStrategy,
};

const SHIP: &str = "uss_saladin";
const HOSTILE: &str = "2918121098";

fn run(
    strategy: OptimizerStrategy,
    refinement: Option<LocalRefinementParams>,
) -> OptimizeRunOutcome {
    let registry = DataRegistry::load().expect("data registry required for optimizer tests");
    let scenario = OptimizationScenario {
        ship: SHIP,
        hostile: HOSTILE,
        simulation_count: 150,
        seed: 11,
        max_candidates: Some(24),
        strategy,
        profile_id: Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        local_refinement: refinement,
        ..Default::default()
    };
    optimize_scenario_with_progress_with_registry(&registry, &scenario, |_| true, || true)
}

#[test]
fn genetic_lane_runs_the_refinement_pass() {
    let outcome = run(
        OptimizerStrategy::Genetic,
        Some(LocalRefinementParams::default()),
    );
    let stats = outcome
        .refinement_stats
        .expect("genetic must run the refinement pass when the scenario asks for it");
    assert!(
        stats.seeds_refined > 0,
        "the pass must hill-climb from at least one genetic finalist: {stats:?}"
    );
    assert!(
        stats.neighbors_generated > 0 && stats.neighbors_scouted > 0,
        "the pass must actually enumerate and scout neighbors: {stats:?}"
    );
    // Accepting nothing is a legitimate outcome (the finalists were local optima at this depth),
    // but every accepted improvement must be explainable.
    assert_eq!(
        outcome.refinement_provenance.len(),
        stats.improvements_accepted,
        "each accepted improvement needs exactly one provenance record"
    );
}

#[test]
fn tiered_lane_runs_the_refinement_pass() {
    let outcome = run(
        OptimizerStrategy::Tiered,
        Some(LocalRefinementParams::default()),
    );
    let stats = outcome
        .refinement_stats
        .expect("tiered must run the refinement pass when the scenario asks for it");
    assert!(stats.seeds_refined > 0, "{stats:?}");
    assert_eq!(
        outcome.refinement_provenance.len(),
        stats.improvements_accepted
    );
}

#[test]
fn lanes_report_no_pass_when_refinement_is_off() {
    for strategy in [OptimizerStrategy::Genetic, OptimizerStrategy::Tiered] {
        let outcome = run(strategy, None);
        assert!(
            outcome.refinement_stats.is_none(),
            "{strategy:?} must not report a refinement pass it never ran"
        );
        assert!(outcome.refinement_provenance.is_empty());
        assert!(
            !outcome.ranked.is_empty(),
            "{strategy:?} still has to return results with refinement off"
        );
    }
}

#[test]
fn refinement_only_adds_rows_and_never_drops_the_originals() {
    // The pass appends to the ranked list and re-sorts; a finalist must never be displaced out of
    // the results by its own refined descendant.
    let plain = run(OptimizerStrategy::Tiered, None);
    let refined = run(
        OptimizerStrategy::Tiered,
        Some(LocalRefinementParams::default()),
    );
    assert!(
        refined.ranked.len() >= plain.ranked.len(),
        "refinement adds rows: {} vs {}",
        refined.ranked.len(),
        plain.ranked.len()
    );
    let crew_key = |r: &kobayashi::optimizer::ranking::RankedCrewResult| {
        let mut bridge = r.bridge.clone();
        let mut below = r.below_decks.clone();
        bridge.sort();
        below.sort();
        (r.captain.clone(), bridge, below)
    };
    let kept: std::collections::HashSet<_> = refined.ranked.iter().map(crew_key).collect();
    for row in &plain.ranked {
        assert!(
            kept.contains(&crew_key(row)),
            "refinement dropped an original finalist: {} / {:?}",
            row.captain,
            row.bridge
        );
    }
}
