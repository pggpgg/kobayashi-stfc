//! Integration tests for the sensitivity engine.
//!
//! These run with a low sim count for speed; they don't exercise statistical power, just
//! end-to-end plumbing, monotonicity, and determinism.

use std::collections::HashMap;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::sensitivity::{
    default_deltas, run_sensitivity, OutcomeMetric, SensitivityRequest,
};

fn known_scenario(num_sims: u32, metric: OutcomeMetric) -> SensitivityRequest {
    SensitivityRequest {
        ship: "uss_enterprise_d".into(),
        hostile: "kobayashi_theoretical_damage_sponge".into(),
        ship_tier: Some(5),
        ship_level: Some(7),
        captain: Some("ent-e-picard-556227".into()),
        bridge: vec!["ent-e-data-871245".into(), "five-of-eleven-d9aa11".into()],
        below_decks: vec!["harry-kim-a79fdf (T5)".into()],
        support_buffs: None,
        profile_id: Some(DEMO_PROFILE_ID.into()),
        num_sims: Some(num_sims),
        seed: Some(77_001),
        rounds: Some(5),
        metric: Some(metric),
        deltas: None,
    }
}

#[test]
fn defaults_catalog_lists_every_stat_with_finite_positive_delta() {
    let defaults = default_deltas();
    assert!(!defaults.is_empty());
    for (stat, delta) in defaults {
        assert!(delta.is_finite(), "{} delta not finite", stat.as_str());
        assert!(delta > 0.0, "{} delta not positive", stat.as_str());
    }
}

#[test]
fn run_returns_one_row_per_stat_when_no_overrides() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(40, OutcomeMetric::HullRemaining);
    let resp = run_sensitivity(&registry, &req).expect("run_sensitivity");
    assert_eq!(resp.rows.len(), default_deltas().len());
    assert_eq!(resp.metric, "hull_remaining");
    assert_eq!(resp.num_sims, 40);
}

#[test]
fn zero_delta_override_skips_that_stat() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let mut req = known_scenario(20, OutcomeMetric::HullRemaining);
    let mut overrides: HashMap<String, f64> = HashMap::new();
    overrides.insert("apex_shred".into(), 0.0);
    overrides.insert("apex_barrier".into(), 0.0);
    req.deltas = Some(overrides);
    let resp = run_sensitivity(&registry, &req).expect("run_sensitivity");
    assert!(!resp.rows.iter().any(|r| r.stat == "apex_shred"));
    assert!(!resp.rows.iter().any(|r| r.stat == "apex_barrier"));
    assert_eq!(resp.rows.len(), default_deltas().len() - 2);
}

#[test]
fn weapon_damage_perturbation_improves_outcome() {
    // Sanity sniff: bumping weapon_damage by +5% should not *worsen* hull remaining.
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(120, OutcomeMetric::DefenderHullRemaining);
    let resp = run_sensitivity(&registry, &req).expect("run_sensitivity");
    let row = resp
        .rows
        .iter()
        .find(|r| r.stat == "weapon_damage")
        .expect("weapon_damage row present");
    assert!(
        row.mean_diff >= -1e-6,
        "weapon_damage perturbation should not yield negative mean_diff on DefenderHullRemaining (got {})",
        row.mean_diff
    );
}

#[test]
fn determinism_same_seed_produces_same_means() {
    // CRN guarantee: two identical runs with the same base_seed yield the same per-stat
    // mean_diff (modulo Rayon non-determinism on summation order, which is bounded by
    // ULP-level noise — assert with a small tolerance).
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(40, OutcomeMetric::HullRemaining);
    let a = run_sensitivity(&registry, &req).expect("first run");
    let b = run_sensitivity(&registry, &req).expect("second run");
    assert_eq!(a.rows.len(), b.rows.len());
    for (ra, rb) in a.rows.iter().zip(b.rows.iter()) {
        assert_eq!(ra.stat, rb.stat);
        assert!(
            (ra.mean_diff - rb.mean_diff).abs() < 1e-9,
            "{}: mean_diff diverged between runs: {} vs {}",
            ra.stat,
            ra.mean_diff,
            rb.mean_diff
        );
    }
}
