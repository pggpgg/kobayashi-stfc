use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::heuristics::BelowDecksPoolMode;
use kobayashi::optimizer::crew_generator::{
    CandidateStrategy, CrewGenerator, NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS,
};
use kobayashi::server::api::optimize_estimate_payload;

#[test]
fn omitted_limit_estimate_matches_the_actual_safety_limit() {
    let registry = DataRegistry::load().unwrap();
    let generator = CrewGenerator::with_strategy(CandidateStrategy {
        max_candidates: None,
        below_decks_slots: 2,
        ..CandidateStrategy::default()
    });
    let expected = generator.effective_run_candidate_count_from_registry(
        &registry,
        "realta",
        "2918121098",
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    );
    let response = optimize_estimate_payload(
        &registry,
        "/api/optimize/estimate?ship=realta&hostile=2918121098&below_decks_slots=2",
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .unwrap();
    let body: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(body["estimated_candidates"].as_u64(), Some(expected as u64));
    assert!(expected <= 10_000);
}

#[test]
fn estimate_matches_bounded_generation_without_materializing_every_crew() {
    let registry = DataRegistry::load().unwrap();
    for mode in [
        BelowDecksPoolMode::Strict,
        BelowDecksPoolMode::Scored,
        BelowDecksPoolMode::Relaxed,
    ] {
        for cap in [1, 17] {
            let generator = CrewGenerator::with_strategy(CandidateStrategy {
                max_candidates: Some(cap),
                below_decks_pool_mode: mode,
                below_decks_slots: 2,
                ..CandidateStrategy::default()
            });
            let expected = generator
                .generate_candidates_from_registry(
                    &registry,
                    "realta",
                    "2918121098",
                    0,
                    Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
                )
                .len();
            let path = format!(
                "/api/optimize/estimate?ship=realta&hostile=2918121098&max_candidates={cap}&below_decks_pool_mode={}&below_decks_slots=2",
                mode.as_api_str()
            );
            let response = optimize_estimate_payload(
                &registry,
                &path,
                Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            )
            .unwrap();
            let body: serde_json::Value = serde_json::from_str(&response).unwrap();
            assert_eq!(body["estimated_candidates"].as_u64(), Some(expected as u64));
        }
    }
}
