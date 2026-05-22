//! Integration tests for the Sobol sensitivity engine.
//!
//! Low sample counts: end-to-end plumbing + determinism, not statistical convergence.
//! Unit tests for the Saltelli/Jansen math live in `src/optimizer/sensitivity_sobol.rs`.

use std::collections::HashMap;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::sensitivity::OutcomeMetric;
use kobayashi::optimizer::sensitivity_sobol::{run_sobol, SobolRequest};

fn known_scenario(n_samples: Option<u32>, metric: OutcomeMetric) -> SobolRequest {
    SobolRequest {
        ship: "uss_enterprise_d".into(),
        hostile: "kobayashi_theoretical_damage_sponge".into(),
        ship_tier: Some(5),
        ship_level: Some(7),
        captain: Some("ent-e-picard-556227".into()),
        bridge: vec!["ent-e-data-871245".into(), "five-of-eleven-d9aa11".into()],
        below_decks: vec!["harry-kim-a79fdf (T5)".into()],
        support_buffs: None,
        profile_id: Some(DEMO_PROFILE_ID.into()),
        n_samples,
        seed: Some(77_001),
        rounds: Some(5),
        metric: Some(metric),
        deltas: None,
    }
}

#[test]
fn run_returns_one_row_per_stat_when_no_overrides() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(Some(16), OutcomeMetric::HullRemaining);
    let resp = run_sobol(&registry, &req).expect("run_sobol");
    assert_eq!(resp.metric, "hull_remaining");
    assert_eq!(resp.n_samples, 16);
    assert_eq!(resp.k_stats as usize, resp.rows.len());
    // Total sims = N × (k + 2).
    let expected_total = (resp.n_samples as u64) * (resp.k_stats as u64 + 2);
    assert_eq!(resp.total_sims, expected_total);
    for row in &resp.rows {
        assert!(row.s1 >= 0.0, "{} S_1 negative: {}", row.stat, row.s1);
        assert!(row.st >= 0.0, "{} S_T negative: {}", row.stat, row.st);
        assert!(
            row.interaction >= 0.0,
            "{} interaction negative: {}",
            row.stat,
            row.interaction
        );
        assert!(
            row.s1_ci95_low <= row.s1_ci95_high,
            "{} CI inverted: [{}, {}]",
            row.stat,
            row.s1_ci95_low,
            row.s1_ci95_high
        );
    }
}

#[test]
fn zero_delta_override_drops_stat_from_analysis() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let mut req = known_scenario(Some(16), OutcomeMetric::HullRemaining);
    let mut overrides: HashMap<String, f64> = HashMap::new();
    overrides.insert("apex_shred".into(), 0.0);
    overrides.insert("apex_barrier".into(), 0.0);
    req.deltas = Some(overrides);
    let resp = run_sobol(&registry, &req).expect("run_sobol");
    assert!(!resp.rows.iter().any(|r| r.stat == "apex_shred"));
    assert!(!resp.rows.iter().any(|r| r.stat == "apex_barrier"));
    let expected_k = kobayashi::combat::perturb::StatKey::ALL.len() - 2;
    assert_eq!(resp.rows.len(), expected_k);
    assert_eq!(resp.k_stats as usize, expected_k);
    // Total-sims accounting must match the reduced k.
    let expected_total = (resp.n_samples as u64) * (expected_k as u64 + 2);
    assert_eq!(resp.total_sims, expected_total);
}

#[test]
fn determinism_same_seed_produces_same_s1_and_st() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(Some(16), OutcomeMetric::HullRemaining);
    let a = run_sobol(&registry, &req).expect("first run");
    let b = run_sobol(&registry, &req).expect("second run");
    assert_eq!(a.rows.len(), b.rows.len());
    for (ra, rb) in a.rows.iter().zip(b.rows.iter()) {
        assert_eq!(ra.stat, rb.stat);
        assert!(
            (ra.s1 - rb.s1).abs() < 1e-9,
            "{}: S_1 diverged: {} vs {}",
            ra.stat,
            ra.s1,
            rb.s1
        );
        assert!(
            (ra.st - rb.st).abs() < 1e-9,
            "{}: S_T diverged: {} vs {}",
            ra.stat,
            ra.st,
            rb.st
        );
    }
}

#[test]
fn server_defaults_apply_when_none_passed() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let req = known_scenario(None, OutcomeMetric::HullRemaining);
    let resp = run_sobol(&registry, &req).expect("run_sobol");
    use kobayashi::optimizer::sensitivity_sobol::DEFAULT_N_SAMPLES;
    assert_eq!(resp.n_samples, DEFAULT_N_SAMPLES);
}
