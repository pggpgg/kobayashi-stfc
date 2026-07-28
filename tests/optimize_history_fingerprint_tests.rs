//! Fingerprint-safe reuse of persisted optimizer confirmations (roadmap §1.3).
//!
//! `optimize_history.json` hands back stored Monte Carlo aggregates *instead of simulating*, so those
//! numbers reach the user labelled as confirmed. These tests pin the guard that keeps that sound:
//! reuse only when the run's engine, catalogs, profile, and resolved matchup all still match.
//!
//! Each test uses its own throwaway profile directory under `profiles/` (gitignored) and removes it
//! afterwards — never `profiles/demo`, whose whole subtree is un-ignored, so a stray
//! `optimize_history.json` there would show up in `git status`.

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Method, Request};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::optimize_history;
use kobayashi::data::profile_index::profile_data_dir;
use kobayashi::optimizer::officer_learning::OfficerPerformanceScores;
use kobayashi::server::routes::build_router;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Small tiered run: enough candidates for a real scout→confirm pass, small enough to stay quick.
/// The body must be byte-identical between runs so `entry_matches_run` metadata still lines up and
/// only the fingerprint decides.
///
/// Deliberately does **not** pin `tiered_confirm_budget_cap_mult`, so these tests exercise the
/// default path a user actually takes — including the learning-signal auto-tuner, which derives a cap
/// from the stored entry on the second run. Cache identity uses the *requested* cap, so that derived
/// value no longer invalidates the entry it came from.
const TIERED_BODY: &str = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":60,"seed":5,"max_candidates":16,"strategy":"tiered","tiered_scout_sims":8,"tiered_top_k":2,"optimize_cache_key":"fingerprint-reuse-test-key"}"#;
/// Same run, but asking for a specific confirm cap — a *requested* cap is still part of cache identity.
const TIERED_BODY_WITH_PINNED_CAP: &str = r#"{"ship":"uss_saladin","hostile":"2918121098","sims":60,"seed":5,"max_candidates":16,"strategy":"tiered","tiered_scout_sims":8,"tiered_top_k":2,"tiered_confirm_budget_cap_mult":2.0,"optimize_cache_key":"fingerprint-reuse-test-key"}"#;
const CACHE_KEY: &str = "fingerprint-reuse-test-key";

/// Profile directory that exists but ships no `roster.imported.json`, so crew legality sees the full
/// canonical catalog (same reason `server_api_tests` uses a no-roster id).
struct ScratchProfile {
    id: String,
}

impl ScratchProfile {
    fn new(suffix: &str) -> Self {
        let id = format!("__kobayashi_test_fingerprint_{suffix}__");
        let dir = profile_data_dir(&id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch profile dir");
        Self { id }
    }

    fn dir(&self) -> PathBuf {
        profile_data_dir(&self.id)
    }

    fn write_profile_json(&self, contents: &str) {
        std::fs::write(self.dir().join("profile.json"), contents).expect("write profile.json");
    }
}

impl Drop for ScratchProfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.dir());
    }
}

async fn optimize(profile_id: &str, body: &str) -> serde_json::Value {
    let registry = DataRegistry::load().expect("data registry required for these tests");
    let app = build_router(registry);
    let addr: SocketAddr = "127.0.0.1:12345".parse().expect("loopback");
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/api/optimize")
        .header("content-type", "application/json")
        .header("x-profile-id", profile_id)
        .body(Body::from(body.to_string()))
        .expect("request");
    req.extensions_mut().insert(ConnectInfo(addr));
    let response = app.oneshot_request(req).await;
    serde_json::from_str(&response).expect("optimize response json")
}

/// Thin wrapper so the `tower::ServiceExt::oneshot` import stays local to one place.
trait OneshotToString {
    async fn oneshot_request(self, req: Request<Body>) -> String;
}

impl OneshotToString for axum::Router {
    async fn oneshot_request(self, req: Request<Body>) -> String {
        use tower::ServiceExt;
        let resp = self.oneshot(req).await.expect("router response");
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("response body");
        let body = String::from_utf8_lossy(&bytes).into_owned();
        assert!(status.is_success(), "optimize failed ({status}): {body}");
        body
    }
}

fn stored_entry(profile_id: &str) -> optimize_history::OptimizeHistoryEntry {
    optimize_history::load_history_file(profile_id)
        .entries
        .remove(CACHE_KEY)
        .expect("history entry for the cache key")
}

#[serial_test::serial]
#[tokio::test]
async fn identical_rerun_reuses_stored_confirmations() {
    let profile = ScratchProfile::new("reuse");

    let first = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(
        first["scenario"]["optimize_history_wrote"], true,
        "first run should persist an entry: {}",
        first["scenario"]
    );
    let fingerprint = first["scenario"]["optimize_reuse_fingerprint"]
        .as_str()
        .expect("response reports the computed fingerprint")
        .to_string();
    assert_eq!(
        fingerprint.split(':').count(),
        5,
        "fingerprint should carry schema + four segments, got {fingerprint}"
    );
    assert!(
        first["scenario"]["optimize_history_reuse_refused"].is_null(),
        "nothing was stored before the first run, so nothing can be refused"
    );

    let entry = stored_entry(&profile.id);
    assert_eq!(
        entry.reuse_fingerprint.as_deref(),
        Some(fingerprint.as_str()),
        "the stored entry must carry the fingerprint the run reported"
    );

    let second = optimize(&profile.id, TIERED_BODY).await;
    assert!(
        second["scenario"]["optimize_history_confirm_hits"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "an identical rerun should reuse stored confirmations.\nsecond run scenario: {}\nstored entry: sims={} seed={} scout={} top_k={} n={} cap={:?} crews={}",
        second["scenario"],
        entry.sims,
        entry.seed,
        entry.tiered_scout_sims,
        entry.tiered_top_k,
        entry.n_candidates,
        entry.tiered_confirm_cap_mult,
        entry.crews.len(),
    );
    assert!(
        second["scenario"]["optimize_history_reuse_refused"].is_null(),
        "matching fingerprint must not be reported as refused"
    );
    assert!(
        entry.tiered_confirm_cap_mult.is_none(),
        "the entry must record the requested cap (absent here), not one the auto-tuner derived"
    );
}

/// Regression: the learning-signal auto-tuner derives `tiered_confirm_budget_cap_mult` *from* the
/// stored entry, so folding that derived value into cache identity made every entry reject itself on
/// the very next run — the cache never hit on the default path and the SPA's cached-warm-start badge
/// never lit up. Identity keys on the requested cap instead, so the second run reuses.
///
/// The companion assertion below matters as much: a cap the caller *asked for* must still change
/// identity, or reuse would start serving rows measured under a budget the caller explicitly rejected.
#[serial_test::serial]
#[tokio::test]
async fn an_auto_derived_confirm_cap_does_not_invalidate_the_entry_it_came_from() {
    let profile = ScratchProfile::new("autocap");

    let first = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(first["scenario"]["optimize_history_wrote"], true);

    // No pinned cap anywhere in this test: whatever the auto-tuner derives on the rerun must not
    // stand between the run and the entry it just read.
    let second = optimize(&profile.id, TIERED_BODY).await;
    assert!(
        second["scenario"]["optimize_history_confirm_hits"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "an unpinned identical rerun must reuse stored confirmations: {}",
        second["scenario"]
    );

    // A cap the request carries is still significant: the stored entry was written under no
    // requested cap, so it cannot answer a run that asks for 2.0.
    let third = optimize(&profile.id, TIERED_BODY_WITH_PINNED_CAP).await;
    assert_eq!(
        third["scenario"]["optimize_history_confirm_hits"]
            .as_u64()
            .unwrap_or(0),
        0,
        "a user-specified confirm cap must still invalidate an entry written without one: {}",
        third["scenario"]
    );
    assert!(
        third["scenario"]["optimize_history_reuse_refused"].is_null(),
        "this is a run-metadata mismatch, not a fingerprint refusal"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn tampered_fingerprint_refuses_reuse() {
    let profile = ScratchProfile::new("tampered");
    let first = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(first["scenario"]["optimize_history_wrote"], true);

    // Rewrite the stored fingerprint: same crews, same run metadata, different engine segment —
    // exactly the shape of a combat-engine change landing under an unchanged request.
    let mut file = optimize_history::load_history_file(&profile.id);
    let entry = file.entries.get_mut(CACHE_KEY).expect("entry");
    entry.reuse_fingerprint = Some("1:dead0000dead0000:beef:cafe:f00d".to_string());
    let entry = entry.clone();
    optimize_history::upsert_entry(&profile.id, CACHE_KEY, entry).expect("upsert");

    let second = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(
        second["scenario"]["optimize_history_confirm_hits"]
            .as_u64()
            .unwrap_or(0),
        0,
        "stale metrics must not be served as confirmations: {}",
        second["scenario"]
    );
    assert_eq!(
        second["scenario"]["optimize_history_reuse_refused"], true,
        "the refusal should be visible in the response"
    );
    assert_eq!(
        second["scenario"]["optimize_history_reuse_refused_component"], "engine",
        "the response should name which segment changed"
    );
    let notes = second["approximate_notes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        notes
            .iter()
            .any(|n| n.as_str().unwrap_or("").contains("re-simulated")),
        "a human-readable note should say the crews were re-simulated: {notes:?}"
    );
}

/// The failure this whole change exists to prevent: editing the profile (research, buildings, ops
/// level, equipped tech) changes resolved stats, so previously confirmed win rates no longer describe
/// the fight — yet every request parameter is identical.
#[serial_test::serial]
#[tokio::test]
async fn editing_the_profile_refuses_reuse() {
    let profile = ScratchProfile::new("profileedit");
    profile.write_profile_json(r#"{"bonuses":{}}"#);

    let first = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(first["scenario"]["optimize_history_wrote"], true);

    profile.write_profile_json(r#"{"bonuses":{"weapon_damage":0.25}}"#);

    let second = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(
        second["scenario"]["optimize_history_confirm_hits"]
            .as_u64()
            .unwrap_or(0),
        0,
        "a profile edit must invalidate stored confirmations: {}",
        second["scenario"]
    );
    assert_eq!(
        second["scenario"]["optimize_history_reuse_refused_component"], "profile",
        "the changed segment should be attributed to the profile"
    );
}

/// Entries written before fingerprinting existed carry no fingerprint. Their metrics are refused,
/// but their crew identities stay available to matchup priors — a good crew survives an engine fix.
#[serial_test::serial]
#[tokio::test]
async fn unfingerprinted_entries_lose_metrics_but_keep_identities() {
    let profile = ScratchProfile::new("legacy");
    let first = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(first["scenario"]["optimize_history_wrote"], true);

    let mut file = optimize_history::load_history_file(&profile.id);
    let entry = file.entries.get_mut(CACHE_KEY).expect("entry");
    entry.reuse_fingerprint = None;
    let entry = entry.clone();
    let stored_crews = entry.crews.len();
    assert!(stored_crews > 0, "the first run should have stored crews");
    optimize_history::upsert_entry(&profile.id, CACHE_KEY, entry).expect("upsert");

    let second = optimize(&profile.id, TIERED_BODY).await;
    assert_eq!(
        second["scenario"]["optimize_history_confirm_hits"]
            .as_u64()
            .unwrap_or(0),
        0,
        "an entry with no fingerprint must not supply confirmations"
    );
    assert_eq!(
        second["scenario"]["optimize_history_reuse_refused_component"],
        "unfingerprinted"
    );

    // Identity reuse is unaffected: priors still read the stored crews.
    let entry = stored_entry(&profile.id);
    let mut legacy = entry.clone();
    legacy.reuse_fingerprint = None;
    assert_eq!(
        optimize_history::prior_reference_crews_from_entry(&legacy, &None).len(),
        legacy
            .crews
            .len()
            .min(optimize_history::MAX_PRIOR_REFERENCE_CREWS_FROM_HISTORY),
        "crew identities must stay reusable without a fingerprint"
    );
}

#[serial_test::serial]
#[tokio::test]
async fn officer_learning_scores_reset_across_a_catalog_change() {
    let profile = ScratchProfile::new("learning");
    let mut scores = OfficerPerformanceScores::new();
    scores.set_data_fingerprint("catalog-from-another-build");
    optimize_history::save_officer_scores(&profile.id, &scores).expect("save scores");

    assert!(
        optimize_history::load_officer_scores(&profile.id, Some("catalog-from-another-build"))
            .data_fingerprint()
            .is_some(),
        "a matching catalog fingerprint keeps the file"
    );
    assert!(
        optimize_history::load_officer_scores(&profile.id, Some("current-catalog"))
            .data_fingerprint()
            .is_none(),
        "a catalog change must reset name-keyed scores"
    );
}

/// Two scenarios that differ only in an id neither of which resolves must still fingerprint
/// differently.
///
/// The unresolved marker used to be a constant, so every unknown id collapsed to one segment —
/// while the engine's synthetic fallback derives a *different* fight from each id string. Two
/// different typos therefore looked like the same scenario and could reuse each other's stored
/// results. Ingress rejects unresolvable ids now, but the fingerprint has to hold on its own.
#[test]
fn distinct_unresolvable_ids_do_not_share_a_scenario_fingerprint() {
    use kobayashi::data::optimize_fingerprint::{scenario_fingerprint, ReuseScenarioInputs};

    let registry = DataRegistry::load().expect("data registry");
    let inputs = |ship: &'static str, hostile: &'static str| ReuseScenarioInputs {
        ship,
        hostile,
        ship_tier: None,
        ship_level: None,
        below_decks_slots: 3,
        enemy_type: None,
        defender_opponent: "hostile",
        support_buffs: None,
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        defender_ship: None,
        defender_ship_tier: None,
        defender_ship_level: None,
        defender_profile_id: None,
    };

    let unknown_hostile_a =
        scenario_fingerprint(&registry, &inputs("uss_saladin", "__no_such_hostile_a__"));
    let unknown_hostile_b =
        scenario_fingerprint(&registry, &inputs("uss_saladin", "__no_such_hostile_b__"));
    assert_ne!(
        unknown_hostile_a, unknown_hostile_b,
        "two different unresolvable hostiles must not share a fingerprint"
    );

    let unknown_ship_a =
        scenario_fingerprint(&registry, &inputs("__no_such_ship_a__", "2918121098"));
    let unknown_ship_b =
        scenario_fingerprint(&registry, &inputs("__no_such_ship_b__", "2918121098"));
    assert_ne!(
        unknown_ship_a, unknown_ship_b,
        "two different unresolvable ships must not share a fingerprint"
    );

    // And an unresolvable scenario must never collide with a real one.
    let resolved = scenario_fingerprint(&registry, &inputs("uss_saladin", "2918121098"));
    assert_ne!(resolved, unknown_hostile_a);
    assert_ne!(resolved, unknown_ship_a);
}
