//! Integration tests for the Morris sensitivity engine.
//!
//! Low sim/trajectory counts: end-to-end plumbing + determinism, not statistical power.

use std::collections::HashMap;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::sensitivity::OutcomeMetric;
use kobayashi::optimizer::sensitivity_morris::{run_morris, MorrisRequest};

fn known_scenario(
    num_sims: Option<u32>,
    r_trajectories: Option<u32>,
    metric: OutcomeMetric,
) -> MorrisRequest {
    MorrisRequest {
        ship: "uss_enterprise_d".into(),
        hostile: "kobayashi_theoretical_damage_sponge".into(),
        ship_tier: Some(5),
        ship_level: Some(7),
        captain: Some("ent-e-picard-556227".into()),
        bridge: vec!["ent-e-data-871245".into(), "five-of-eleven-d9aa11".into()],
        below_decks: vec!["harry-kim-a79fdf (T5)".into()],
        support_buffs: None,
        profile_id: Some(DEMO_PROFILE_ID.into()),
        num_sims,
        r_trajectories,
        seed: Some(77_001),
        rounds: Some(5),
        metric: Some(metric),
        deltas: None,
    }
}

#[test]
fn run_returns_one_row_per_stat_when_no_overrides() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(Some(20), Some(4), OutcomeMetric::HullRemaining);
    let resp = run_morris(&registry, &req).expect("run_morris");
    assert_eq!(resp.metric, "hull_remaining");
    assert_eq!(resp.r_trajectories, 4);
    assert_eq!(resp.num_sims_per_point, 20);
    assert_eq!(resp.k_stats as usize, resp.rows.len());
    // Total sims = r × (k+1) × num_sims.
    let expected_total =
        (resp.r_trajectories as u64) * (resp.k_stats as u64 + 1) * (resp.num_sims_per_point as u64);
    assert_eq!(resp.total_sims, expected_total);
    for row in &resp.rows {
        assert_eq!(row.n_samples, resp.r_trajectories);
        assert!(row.mu_star >= 0.0, "{} mu_star negative", row.stat);
        assert!(row.sigma >= 0.0, "{} sigma negative", row.stat);
        assert!(
            row.mu_star >= row.mu.abs() - 1e-9,
            "{} mu_star ({}) < |mu| ({})",
            row.stat,
            row.mu_star,
            row.mu
        );
    }
}

#[test]
fn zero_delta_override_skips_that_stat_from_trajectory() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let mut req = known_scenario(Some(10), Some(3), OutcomeMetric::HullRemaining);
    let mut overrides: HashMap<String, f64> = HashMap::new();
    overrides.insert("apex_shred".into(), 0.0);
    overrides.insert("apex_barrier".into(), 0.0);
    req.deltas = Some(overrides);
    let resp = run_morris(&registry, &req).expect("run_morris");
    assert!(!resp.rows.iter().any(|r| r.stat == "apex_shred"));
    assert!(!resp.rows.iter().any(|r| r.stat == "apex_barrier"));
    // k_stats / row count match what's left after the two skipped stats.
    let expected_k = kobayashi::combat::perturb::StatKey::ALL.len() - 2;
    assert_eq!(resp.rows.len(), expected_k);
    assert_eq!(resp.k_stats as usize, expected_k);
}

#[test]
fn determinism_same_seed_produces_same_mu_star_and_sigma() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(Some(20), Some(4), OutcomeMetric::HullRemaining);
    let a = run_morris(&registry, &req).expect("first run");
    let b = run_morris(&registry, &req).expect("second run");
    assert_eq!(a.rows.len(), b.rows.len());
    for (ra, rb) in a.rows.iter().zip(b.rows.iter()) {
        assert_eq!(ra.stat, rb.stat);
        assert!(
            (ra.mu_star - rb.mu_star).abs() < 1e-9,
            "{}: mu_star diverged: {} vs {}",
            ra.stat,
            ra.mu_star,
            rb.mu_star
        );
        assert!(
            (ra.sigma - rb.sigma).abs() < 1e-9,
            "{}: sigma diverged: {} vs {}",
            ra.stat,
            ra.sigma,
            rb.sigma
        );
    }
}

#[test]
fn server_defaults_apply_when_none_passed() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(None, None, OutcomeMetric::HullRemaining);
    let resp = run_morris(&registry, &req).expect("run_morris");
    use kobayashi::optimizer::sensitivity_morris::{
        DEFAULT_NUM_SIMS_PER_POINT, DEFAULT_R_TRAJECTORIES,
    };
    assert_eq!(resp.num_sims_per_point, DEFAULT_NUM_SIMS_PER_POINT);
    assert_eq!(resp.r_trajectories, DEFAULT_R_TRAJECTORIES);
}
