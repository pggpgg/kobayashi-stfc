//! Integration tests for issue #414 (hostile spawn/warp/loot disambiguation metadata) and
//! issue #415 (chain-grind `max_expected_loot` farming payout vs `min_hull_damage` survival).

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::optimizer::chain::{ChainGrindParams, ChainSecondaryObjective};
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{run_monte_carlo_with_registry, DefenderOpponent};
use kobayashi::optimizer::ranking::rank_results;
use kobayashi::server::api::{chain_grind_params_from_request, ChainGrindRequest};
use kobayashi::server::routes::build_router;
use serde_json::Value;
use std::net::SocketAddr;
use tower::ServiceExt;

#[tokio::test]
async fn hostiles_api_exposes_system_warp_and_loot_for_yoton_vs_ghost() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let app = build_router(registry);

    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/hostiles")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).expect("valid JSON");
    let hostiles = json["hostiles"].as_array().expect("hostiles array");

    let yoton = hostiles
        .iter()
        .find(|h| h["id"].as_str() == Some("3919830203"))
        .expect("Yoton L62 Xindi-Aquatic Cruiser 3919830203 present");
    let ghost = hostiles
        .iter()
        .find(|h| h["id"].as_str() == Some("3294438884"))
        .expect("no-system L62 Xindi-Aquatic Cruiser 3294438884 present");

    assert_eq!(yoton["level"].as_u64(), Some(62));
    assert_eq!(ghost["level"].as_u64(), Some(62));

    let yoton_systems: Vec<&str> = yoton["system_names"]
        .as_array()
        .expect("system_names array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        yoton_systems.contains(&"Yoton"),
        "expected Yoton in system_names, got {yoton_systems:?}"
    );
    assert_eq!(yoton["warp"].as_u64(), Some(1000));
    assert_eq!(yoton["has_loot"].as_bool(), Some(true));
    assert_eq!(yoton["primary_resource_name"].as_str(), Some("Tech Module"));

    assert!(
        ghost["system_names"]
            .as_array()
            .is_none_or(|arr| arr.is_empty()),
        "ghost cruiser 3294438884 should have no spawn systems: {ghost}"
    );
    assert_eq!(ghost["has_loot"].as_bool(), Some(false));
    assert!(
        ghost["primary_resource_name"].is_null(),
        "ghost cruiser 3294438884 should have no primary resource: {ghost}"
    );
}

#[test]
fn chain_request_parser_accepts_max_expected_loot_and_min_hull_damage() {
    let loot = chain_grind_params_from_request(&ChainGrindRequest {
        enabled: true,
        kills_target: Some(4),
        secondary: Some("max_expected_loot".into()),
    })
    .expect("valid max_expected_loot")
    .expect("enabled");
    assert_eq!(loot.kills_target, 4);
    assert_eq!(loot.secondary, ChainSecondaryObjective::MaxExpectedLoot);

    let hull = chain_grind_params_from_request(&ChainGrindRequest {
        enabled: true,
        kills_target: Some(4),
        secondary: Some("min_hull_damage".into()),
    })
    .expect("valid min_hull_damage")
    .expect("enabled");
    assert_eq!(hull.secondary, ChainSecondaryObjective::MinHullDamage);

    let err = chain_grind_params_from_request(&ChainGrindRequest {
        enabled: true,
        kills_target: Some(4),
        secondary: Some("bogus_objective".into()),
    })
    .expect_err("unknown secondary must fail validation");
    assert!(
        err.contains("max_expected_loot"),
        "error should list max_expected_loot: {err}"
    );
}

#[test]
fn xindi_farming_loot_prefers_loot_bonus_until_cargo_hold_is_full() {
    let registry = DataRegistry::load().expect("DataRegistry::load");

    // Verify NX-01 T12 cargo capacity comes from upstream Cargo.max_resources (4,240,000).
    let nx01_t12_cargo = registry
        .resolve_ship_cargo_capacity("enterprise_nx_01", Some(12), Some(60), &[])
        .expect("NX-01 T12 cargo capacity");
    assert!(
        (nx01_t12_cargo - 4_240_000.0).abs() < 1.0,
        "expected NX-01 T12 cargo 4,240,000, got {nx01_t12_cargo}"
    );

    // Two equally survivable crews (both 100% P(N-kill)) against Xindi-Aquatic Cruiser L42
    // (`1284290168`, Tech Module 2,250..2,363 + Xindi Scraps 40..42), where NX-01 T12 easily
    // survives 4 consecutive kills and cargo hold (4.24M) is far from full.
    let archer_crew = CrewCandidate {
        captain: "Jonathan Archer".into(),
        bridge: vec!["Spock".into(), "Bones".into()],
        below_decks: vec!["Uhura".into()],
    };
    let kang_crew = CrewCandidate {
        captain: "Kang".into(),
        bridge: vec!["Spock".into(), "Bones".into()],
        below_decks: vec!["Uhura".into()],
    };

    // 1. Uncapped farming run (`max_expected_loot`, 4 kills vs L42 Xindi-Aquatic Cruiser).
    let (loot_results, placeholder) = run_monte_carlo_with_registry(
        &registry,
        "enterprise_nx_01",
        "1284290168",
        Some(12),
        Some(60),
        &[kang_crew.clone(), archer_crew.clone()],
        40,
        41501,
        Some(DEMO_PROFILE_ID),
        SupportBuffScenarioRequest::default(),
        Some(ChainGrindParams {
            kills_target: 4,
            secondary: ChainSecondaryObjective::MaxExpectedLoot,
        }),
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert!(!placeholder, "expected real ship and hostile records");
    let ranked_loot = rank_results(loot_results);
    assert_eq!(ranked_loot.len(), 2);

    // Both crews survive all 4 kills cleanly, but Jonathan Archer (+300% Xindi loot) wins #1.
    assert!(
        (ranked_loot[0].win_rate - 1.0).abs() < 1e-9
            && (ranked_loot[1].win_rate - 1.0).abs() < 1e-9,
        "both crews should complete the 4-kill chain 100%: {:?}",
        ranked_loot
            .iter()
            .map(|r| (&r.captain, r.win_rate))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ranked_loot[0].captain, "Jonathan Archer",
        "under max_expected_loot with spare cargo hold, Archer (+300% Xindi loot) must rank #1"
    );

    let archer_chain = ranked_loot[0].chain.as_ref().expect("archer chain");
    let kang_chain = ranked_loot[1].chain.as_ref().expect("kang chain");
    let archer_loot = archer_chain.loot.as_ref().expect("archer loot summary");
    let kang_loot = kang_chain.loot.as_ref().expect("kang loot summary");

    assert_eq!(archer_loot.resource_name, "Tech Module");
    assert_eq!(kang_loot.resource_name, "Tech Module");
    assert!(!archer_loot.cargo_capped);
    assert!(!kang_loot.cargo_capped);
    assert!(
        (archer_loot.loot_multiplier - 4.0).abs() < 1e-6,
        "expected Archer 4x loot multiplier, got {}",
        archer_loot.loot_multiplier
    );
    assert!(
        (archer_loot.expected_loot_mean - kang_loot.expected_loot_mean * 4.0).abs() < 1e-3,
        "uncapped Archer payout ({}) should be 4x Kang payout ({})",
        archer_loot.expected_loot_mean,
        kang_loot.expected_loot_mean
    );
    // Ending hull fraction must stay a real [0, 1] fraction, never replaced with raw loot units.
    assert!(
        ranked_loot[0].avg_hull_remaining > 0.0 && ranked_loot[0].avg_hull_remaining <= 1.0,
        "avg_hull_remaining must stay a [0, 1] hull fraction, got {}",
        ranked_loot[0].avg_hull_remaining
    );
    assert!(archer_chain.ending_hull_mean_given_primary.is_some());

    // 2. Plain repeat-kill survival (`min_hull_damage`): no loot object attached, ending hull
    //    is the secondary objective.
    let (hull_results, _) = run_monte_carlo_with_registry(
        &registry,
        "enterprise_nx_01",
        "1284290168",
        Some(12),
        Some(60),
        &[kang_crew.clone(), archer_crew.clone()],
        40,
        41501,
        Some(DEMO_PROFILE_ID),
        SupportBuffScenarioRequest::default(),
        Some(ChainGrindParams {
            kills_target: 4,
            secondary: ChainSecondaryObjective::MinHullDamage,
        }),
        DefenderOpponent::Hostile,
        None,
        None,
    );
    for row in &hull_results {
        let ch = row.chain.as_ref().expect("chain summary");
        assert_eq!(
            ch.secondary_objective,
            ChainSecondaryObjective::MinHullDamage
        );
        assert!(ch.loot.is_none(), "min_hull_damage must not attach loot");
        assert!(
            (ch.secondary_mean_given_primary - row.avg_hull_remaining).abs() < 1e-12,
            "min_hull_damage secondary must equal ending hull fraction"
        );
        assert!(row.avg_hull_remaining > 0.0 && row.avg_hull_remaining <= 1.0);
    }

    // 3. Cargo hold full vs expanded via `CargoCapacity` officer (`Ent-E Data (T5)` = +70% cargo):
    //    Use `uss_enterprise_d` T12 L60 (cargo 235,000) vs `1306807635` (Xindi-Aquatic Scout L34,
    //    hull 59,000, drops 5,000..10,000 -> mean 7,500/kill):
    //    - At kills_target = 35: base drops = 262,500 > 235,000 cargo hold!
    //      So BOTH Kang (1x -> 262.5k uncapped) and Ent-E Picard (+60% CombatPveRewards ->
    //      420k uncapped) survive cleanly, completely fill the 235,000 cargo hold
    //      (`cargo_capped == true`), and tie on capped payout at 235,000.
    //    - Adding `Ent-E Data (T5)` below decks expands cargo hold to 399,500 (+70%), unlocking
    //      Ent-E Picard's loot bonus up to 399,500 (+70% higher payout) and ranking strictly #1!
    let picard_crew = CrewCandidate {
        captain: "Ent-E Picard".into(),
        bridge: vec!["Spock".into(), "Bones".into()],
        below_decks: vec!["Uhura".into()],
    };
    let picard_with_cargo = CrewCandidate {
        captain: "Ent-E Picard".into(),
        bridge: vec!["Spock".into(), "Bones".into()],
        below_decks: vec!["Ent-E Data (T5)".into()],
    };
    let (capped_results, capped_placeholder) = run_monte_carlo_with_registry(
        &registry,
        "uss_enterprise_d",
        "1306807635",
        Some(12),
        Some(60),
        &[kang_crew, picard_crew, picard_with_cargo],
        10,
        41502,
        Some(DEMO_PROFILE_ID),
        SupportBuffScenarioRequest::default(),
        Some(ChainGrindParams {
            kills_target: 35,
            secondary: ChainSecondaryObjective::MaxExpectedLoot,
        }),
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert!(!capped_placeholder);

    let kang_capped = capped_results[0]
        .chain
        .as_ref()
        .and_then(|c| c.loot.as_ref())
        .expect("kang capped loot");
    let picard_capped = capped_results[1]
        .chain
        .as_ref()
        .and_then(|c| c.loot.as_ref())
        .expect("picard capped loot");
    let picard_cargo = capped_results[2]
        .chain
        .as_ref()
        .and_then(|c| c.loot.as_ref())
        .expect("picard+data loot");

    assert!(
        (capped_results[0].win_rate - 1.0).abs() < 1e-9
            && (capped_results[1].win_rate - 1.0).abs() < 1e-9
            && (capped_results[2].win_rate - 1.0).abs() < 1e-9,
        "all three Enterprise-D crews should cleanly survive 35 Scout kills: {:?}",
        capped_results
            .iter()
            .map(|r| r.win_rate)
            .collect::<Vec<_>>()
    );
    assert!(
        kang_capped.cargo_capped,
        "Kang should fill 235,000 hold in 35 kills"
    );
    assert!(
        picard_capped.cargo_capped,
        "Ent-E Picard should fill 235,000 hold in 35 kills"
    );
    assert!(
        (picard_capped.loot_multiplier - 1.6).abs() < 1e-6,
        "expected Ent-E Picard 1.6x CombatPveRewards multiplier, got {}",
        picard_capped.loot_multiplier
    );
    assert!(
        (picard_capped.expected_loot_mean - 235_000.0).abs() < 1.0
            && (kang_capped.expected_loot_mean - 235_000.0).abs() < 1.0,
        "once hold is full for both crews, capped payout must tie at 235,000: picard={}, kang={}",
        picard_capped.expected_loot_mean,
        kang_capped.expected_loot_mean
    );
    assert!(
        (picard_cargo.cargo_capacity.expect("cargo cap") - 235_000.0 * 1.7).abs() < 1.0,
        "Ent-E Data (T5) below decks must expand cargo capacity by +70%: {:?}",
        picard_cargo.cargo_capacity
    );
    assert!(
        (picard_cargo.expected_loot_mean - 235_000.0 * 1.7).abs() < 1.0,
        "cargo capacity officer must lift capped farming payout to 399,500: {} vs {}",
        picard_cargo.expected_loot_mean,
        picard_capped.expected_loot_mean
    );

    let ranked_capped = rank_results(capped_results);
    assert_eq!(
        ranked_capped[0].below_decks,
        vec!["Ent-E Data (T5)".to_string()],
        "crew with cargo expansion must beat cargo-capped crews"
    );

    // When both crews are equally cargo-capped, higher ending hull breaks the tie
    // instead of wasted loot bonus.
    let mut better_hull_low_loot = ranked_capped[1].clone();
    let mut worse_hull_high_loot = ranked_capped[2].clone();
    better_hull_low_loot.win_rate = 1.0;
    worse_hull_high_loot.win_rate = 1.0;
    better_hull_low_loot.avg_hull_remaining = 0.92;
    worse_hull_high_loot.avg_hull_remaining = 0.70;
    if let Some(ref mut c) = better_hull_low_loot.chain {
        c.secondary_mean_given_primary = 235_000.0;
    }
    if let Some(ref mut c) = worse_hull_high_loot.chain {
        c.secondary_mean_given_primary = 235_000.0;
    }
    better_hull_low_loot.captain = "BetterHull".into();
    worse_hull_high_loot.captain = "WastedLoot".into();
    let mut tie_rows = vec![worse_hull_high_loot, better_hull_low_loot];
    kobayashi::optimizer::ranking::sort_ranked_rows(&mut tie_rows);
    assert_eq!(
        tie_rows[0].captain, "BetterHull",
        "when cargo hold is full for both crews, extra wasted loot must not beat higher ending hull"
    );
}
