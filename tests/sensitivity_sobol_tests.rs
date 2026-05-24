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
        include_pairwise: None,
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

/// `include_pairwise: false` (default) returns `pairs: None` and the baseline `N × (k+2)`
/// sim budget. `include_pairwise: true` returns `Some(pairs)` with `k(k−1)/2` entries
/// and the extended sim budget.
#[test]
fn pairwise_opt_in_toggles_pairs_payload_and_sim_count() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let mut req = known_scenario(Some(16), OutcomeMetric::HullRemaining);
    req.include_pairwise = Some(false);
    let resp_baseline = run_sobol(&registry, &req).expect("baseline run");
    assert!(resp_baseline.pairs.is_none());
    let k = resp_baseline.k_stats as u64;
    let n = resp_baseline.n_samples as u64;
    assert_eq!(resp_baseline.total_sims, n * (k + 2));

    req.include_pairwise = Some(true);
    let resp_pairs = run_sobol(&registry, &req).expect("pairwise run");
    let pairs = resp_pairs
        .pairs
        .as_ref()
        .expect("pairwise run yields pairs");
    let expected_pair_count = (k as usize) * (k as usize - 1) / 2;
    assert_eq!(
        pairs.len(),
        expected_pair_count,
        "expected k(k-1)/2 = {expected_pair_count} pairs, got {}",
        pairs.len()
    );
    assert_eq!(
        resp_pairs.total_sims,
        n * (k + 2) + n * k * (k - 1) / 2,
        "total_sims should include pairwise cost"
    );
    // All s_ij values must be finite and within the documented [0, 1] display clamp.
    for p in pairs {
        assert!(
            p.s_ij.is_finite() && (0.0..=1.0).contains(&p.s_ij),
            "s_ij out of bounds: {p:?}"
        );
        assert!(p.s_ij_ci95_low <= p.s_ij_ci95_high, "CI low > high: {p:?}");
    }
    // Pairs reported only once per unordered combination (stat_a < stat_b in StatKey::ALL order).
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for p in pairs {
        assert!(
            seen.insert((p.stat_a.clone(), p.stat_b.clone())),
            "duplicate pair {} × {}",
            p.stat_a,
            p.stat_b,
        );
        assert!(
            !seen.contains(&(p.stat_b.clone(), p.stat_a.clone())),
            "both orderings present: {} × {}",
            p.stat_a,
            p.stat_b,
        );
    }
}

/// Determinism: same seed + same include_pairwise=true → bitwise-identical pair s_ij values.
#[test]
fn pairwise_determinism_same_seed() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let mut req = known_scenario(Some(16), OutcomeMetric::HullRemaining);
    req.include_pairwise = Some(true);
    let a = run_sobol(&registry, &req).expect("first run");
    let b = run_sobol(&registry, &req).expect("second run");
    let pa = a.pairs.as_ref().expect("first pairs");
    let pb = b.pairs.as_ref().expect("second pairs");
    assert_eq!(pa.len(), pb.len());
    for (ra, rb) in pa.iter().zip(pb.iter()) {
        assert_eq!(ra.stat_a, rb.stat_a);
        assert_eq!(ra.stat_b, rb.stat_b);
        assert!(
            (ra.s_ij - rb.s_ij).abs() < 1e-9,
            "{} × {}: s_ij diverged: {} vs {}",
            ra.stat_a,
            ra.stat_b,
            ra.s_ij,
            rb.s_ij,
        );
    }
}
