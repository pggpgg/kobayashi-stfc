//! HTTP integration tests for Track B endpoints:
//! - `POST /api/crew/counterfactual` (CRN seat ablations + substitute crew planner)
//! - `POST /api/fleet/allocate` (multi-ship disjoint-officer fleet allocator)

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::server::routes::build_router;
use serde_json::{json, Value};
use tower::ServiceExt;

fn test_registry() -> &'static Arc<DataRegistry> {
    static REGISTRY: OnceLock<Arc<DataRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| DataRegistry::load().expect("data registry"))
}

async fn post_json(path: &str, body: Value) -> (StatusCode, Value) {
    let app = build_router(Arc::clone(test_registry()));
    let mut req = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json")
        .header("x-profile-id", DEMO_PROFILE_ID)
        .body(Body::from(body.to_string()))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 45123))));

    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let parsed: Value = serde_json::from_slice(&bytes).expect("JSON response body");
    (status, parsed)
}

#[tokio::test]
async fn crew_counterfactual_endpoint_returns_ablations_and_legal_substitutes() {
    let payload = json!({
        "ship": "uss_saladin",
        "hostile": "1140710508",
        "below_decks_slots": 2,
        "num_sims": 100,
        "seed": 17,
        "unavailable_officers": ["Cadet Uhura"],
        "max_substitutes_per_slot": 3,
        "max_substitute_crews": 4,
        "crew": {
            "captain": "Cadet Uhura",
            "bridge": ["Badgey", "Nesmith"],
            "below_deck": ["Odo", "Harry Kim"]
        }
    });

    let (status, body) = post_json("/api/crew/counterfactual", payload).await;
    assert_eq!(status, StatusCode::OK, "response: {body}");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["trials_per_variant"], 100);

    let ablations = body["seat_ablations"].as_array().expect("seat_ablations");
    assert_eq!(ablations.len(), 5);
    assert_eq!(ablations[0]["slot_group"], "captain");
    assert_eq!(ablations[0]["officer"], "Cadet Uhura");
    assert_eq!(ablations[0]["unavailable"], true);
    assert!(
        ablations[0]["marginal_contribution"].as_f64().unwrap() > 0.0,
        "Cadet Uhura should positively contribute on Saladin vs 1140710508"
    );

    let subs = body["substitutes"].as_array().expect("substitutes");
    assert!(
        !subs.is_empty(),
        "should propose at least one legal substitute"
    );
    for sub in subs {
        assert_ne!(sub["captain"], "Cadet Uhura");
        let changed = sub["changed_slots"].as_array().unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0]["slot_group"], "captain");
        assert!(sub["metrics"]["fidelity"]["grade"].is_string());
    }
}

#[tokio::test]
async fn crew_counterfactual_rejects_unknown_ship() {
    let payload = json!({
        "ship": "not_a_real_ship",
        "hostile": "1140710508",
        "crew": {
            "captain": "Cadet Uhura",
            "bridge": ["Badgey", "Nesmith"],
            "below_deck": ["Odo", "Harry Kim"]
        }
    });
    let (status, body) = post_json("/api/crew/counterfactual", payload).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {body}");
}

#[tokio::test]
async fn fleet_allocate_endpoint_solves_disjoint_two_dock_fleet() {
    let payload = json!({
        "objective": "min_regret",
        "sims_per_crew": 80,
        "candidates_per_slot": 32,
        "seed": 23,
        "slots": [
            {
                "label": "Dock 1 · Saladin",
                "ship": "uss_saladin",
                "hostile": "1140710508",
                "below_decks_slots": 2,
                "priority_weight": 1.5
            },
            {
                "label": "Dock 2 · NX-01",
                "ship": "enterprise_nx_01",
                "hostile": "1120713907",
                "below_decks_slots": 2,
                "priority_weight": 1.0
            }
        ]
    });

    let (status, body) = post_json("/api/fleet/allocate", payload).await;
    assert_eq!(status, StatusCode::OK, "response: {body}");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["objective"], "min_regret");
    assert_eq!(body["conflict_free"], true);

    let slots = body["slots"].as_array().expect("slots");
    assert_eq!(slots.len(), 2);

    let mut used = HashSet::new();
    for slot in slots {
        let crew = &slot["assigned_crew"];
        assert!(crew.is_object(), "each dock must receive an assigned crew");
        let cap = crew["captain"].as_str().unwrap().trim().to_lowercase();
        assert!(
            used.insert(cap.clone()),
            "duplicate officer across fleet: {cap}"
        );
        for br in crew["bridge"].as_array().unwrap() {
            let n = br.as_str().unwrap().trim().to_lowercase();
            assert!(
                used.insert(n.clone()),
                "duplicate officer across fleet: {n}"
            );
        }
        for bd in crew["below_decks"].as_array().unwrap() {
            let n = bd.as_str().unwrap().trim().to_lowercase();
            assert!(
                used.insert(n.clone()),
                "duplicate officer across fleet: {n}"
            );
        }
    }
    assert!(body["fleet_efficiency_pct"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn fleet_allocate_rejects_fewer_than_two_slots() {
    let payload = json!({
        "slots": [
            {
                "ship": "uss_saladin",
                "hostile": "1140710508"
            }
        ]
    });
    let (status, body) = post_json("/api/fleet/allocate", payload).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {body}");
}
