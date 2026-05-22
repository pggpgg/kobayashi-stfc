//! Sobol variance-based sensitivity analysis (Saltelli design, Jansen estimators).
//!
//! Where Morris draws random trajectories and reports μ\* / σ as a screening signal, Sobol
//! decomposes the **variance** of the outcome metric into contributions from each input and
//! their interactions:
//!
//! - **S_i** (first-order index): the fraction of Var(Y) explained by stat `i` alone (main
//!   effect). High S_i → "this stat moves the needle by itself."
//! - **S_T_i** (total-order index): the fraction of Var(Y) involving stat `i`, including all
//!   interactions. High S_T_i with low S_i → "this stat matters mostly through interactions."
//! - **Interaction strength** (S_T_i − S_i): the share of variance from interactions
//!   between `i` and any other stat. The sum of these over all stats gives a "total
//!   interaction budget"; individual pairwise indices S_ij (which pair of stats interacts)
//!   are deferred to a follow-up — see [docs/ROADMAP.md](../../docs/ROADMAP.md).
//!
//! ## Method (Saltelli–Jansen)
//!
//! Two independent N×k sample matrices `A` and `B` (rows uniform in `[0, 1]^k`). For each
//! input `i`, build `A_B^(i)` = A with column `i` replaced by B's column `i`. Evaluate the
//! model on every row of A, B, and each A_B^(i). Then:
//!
//! ```text
//! V(Y)   ≈ Var(f(A) ∪ f(B))                           [Saltelli 2010, eq. (b)]
//! V_i    ≈ (1/N) Σ f(B)_j · (f(A_B^(i))_j − f(A)_j)   [Jansen 1999]
//! V_T_i  ≈ (1/2N) Σ (f(A)_j − f(A_B^(i))_j)²         [Jansen 1999]
//! S_i    = V_i   / V(Y)
//! S_T_i  = V_T_i / V(Y)
//! ```
//!
//! The Jansen total-order estimator is the recommended robust choice — it's positive by
//! construction and doesn't blow up when V(Y) is small.
//!
//! ## CRN pairing
//!
//! Row `j` of `A` and row `j` of `A_B^(i)` differ only in column `i`. To reduce variance in
//! the estimator we reuse the same combat RNG seed for both — any noise unrelated to the
//! parameter swap cancels. Rows of `B` use a disjoint seed range.
//!
//! ## Compute budget
//!
//! Total engine calls: `N × (k + 2)`. Defaults (`N = 512`, `k = 18` after the mitigation
//! split) ≈ 10k sims, comparable to or cheaper than Morris/OAT.

use std::collections::HashMap;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::perturb::StatKey;
use crate::combat::rng::Rng;
use crate::data::data_registry::DataRegistry;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, scenario_to_combat_input_from_shared,
    DefenderOpponent,
};
use crate::optimizer::sensitivity::{run_one_sim_with_perturbations, OutcomeMetric};

/// Default number of Saltelli samples per matrix. N=512 gives reasonably stable S_i / S_T_i
/// for the 18-stat catalog; users can push to N=2048 for tighter CIs at proportional cost.
pub const DEFAULT_N_SAMPLES: u32 = 512;
/// Server-side cap on N to keep a single request from monopolising CPU.
pub const MAX_N_SAMPLES: u32 = 8192;
/// Bootstrap resamples for the 95% CI on S_i / S_T_i. Cheap (no extra engine calls — just
/// resamples the cached output vectors).
const BOOTSTRAP_RESAMPLES: usize = 200;

/// Crew + scenario input for a Sobol run. JSON shape mirrors the OAT / Morris requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SobolRequest {
    pub ship: String,
    pub hostile: String,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub captain: Option<String>,
    pub bridge: Vec<String>,
    #[serde(default)]
    pub below_decks: Vec<String>,
    #[serde(default)]
    pub support_buffs: Option<Vec<String>>,
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Samples per Saltelli matrix. Default [`DEFAULT_N_SAMPLES`].
    #[serde(default)]
    pub n_samples: Option<u32>,
    /// Base RNG seed. Both the Saltelli sampling and the per-row CRN seeds derive from this.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Combat rounds per sim. Default 3 (engine default).
    #[serde(default)]
    pub rounds: Option<u32>,
    /// Outcome metric to measure against. Default [`OutcomeMetric::HullRemaining`].
    #[serde(default)]
    pub metric: Option<OutcomeMetric>,
    /// Per-stat δ overrides. The Saltelli sample `u_i ∈ [0, 1]` is mapped to a
    /// perturbation `δ_i ∈ [0, 2 · base_δ_i]` where `base_δ_i` is the override (or the
    /// [`StatKey::default_delta`] when no override given). A `base_δ_i = 0.0` drops the
    /// stat from the analysis entirely.
    #[serde(default)]
    pub deltas: Option<HashMap<String, f64>>,
}

/// One stat's variance-decomposition row.
#[derive(Debug, Clone, Serialize)]
pub struct SobolRow {
    /// Stat key (snake_case, matches [`StatKey::as_str`]).
    pub stat: String,
    /// Base δ used for the input range mapping `u_i ∈ [0, 1] → δ_i ∈ [0, 2·base_δ]`.
    pub base_delta: f64,
    /// First-order Sobol index (main effect). Fraction of Var(Y) explained by this stat
    /// alone. Clamped to [0, 1] for display when the estimator returns small negative
    /// values from finite-sample noise.
    pub s1: f64,
    /// Total-order Sobol index. Fraction of Var(Y) involving this stat in any interaction.
    /// Lower-bounded at 0.
    pub st: f64,
    /// Interaction strength: `S_T_i − S_i`. The share of variance from interactions
    /// between this stat and any other. Per-pair S_ij is not estimated in v1.
    pub interaction: f64,
    /// 95% bootstrap CI on S_i (resampled rows with replacement, no extra sims).
    pub s1_ci95_low: f64,
    pub s1_ci95_high: f64,
    /// 95% bootstrap CI on S_T_i.
    pub st_ci95_low: f64,
    pub st_ci95_high: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SobolResponse {
    pub metric: &'static str,
    pub n_samples: u32,
    pub k_stats: u32,
    pub base_seed: u64,
    /// Total engine calls performed (`N × (k + 2)`).
    pub total_sims: u64,
    /// Estimated output variance V(Y) for the run. Useful as a sanity check (rows with
    /// V_i / V_T_i divided by this).
    pub output_variance: f64,
    /// One row per stat, **unsorted** — clients sort by S_T (importance with interactions),
    /// S_1 (pure main effect), or interaction strength.
    pub rows: Vec<SobolRow>,
}

/// End-to-end Sobol run. Generates Saltelli matrices, evaluates the combat metric across
/// `A`, `B`, and the `k` swapped matrices, then computes Jansen estimators with bootstrap
/// CIs.
pub fn run_sobol(registry: &DataRegistry, request: &SobolRequest) -> Result<SobolResponse, String> {
    let n_samples = request
        .n_samples
        .unwrap_or(DEFAULT_N_SAMPLES)
        .clamp(8, MAX_N_SAMPLES);
    let base_seed = request.seed.unwrap_or(0);
    let metric = request.metric.unwrap_or_default();

    let shared = build_shared_scenario_data_from_registry(
        registry,
        &request.ship,
        &request.hostile,
        request.ship_tier,
        request.ship_level,
        request.profile_id.as_deref(),
        request.support_buffs.as_deref(),
        DefenderOpponent::default(),
        None,
        None,
    );

    let candidate = CrewCandidate {
        captain: request.captain.clone().unwrap_or_default(),
        bridge: request.bridge.clone(),
        below_decks: request.below_decks.clone(),
    };

    let mut input = scenario_to_combat_input_from_shared(&shared, &candidate, base_seed);
    if let Some(r) = request.rounds {
        input.rounds = r;
    }

    let attacker_max_hull = input.attacker.hull_health.max(1.0);
    let defender_max_hull = input.defender.hull_health.max(1.0);

    // Resolve the stat list + per-stat base δ. Stats with explicit override = 0 are dropped.
    let overrides = request.deltas.clone().unwrap_or_default();
    let stat_deltas: Vec<(StatKey, f64)> = StatKey::ALL
        .iter()
        .filter_map(|stat| {
            let delta = match overrides.get(stat.as_str()) {
                Some(v) if *v == 0.0 => return None,
                Some(v) => *v,
                None => stat.default_delta(),
            };
            Some((*stat, delta))
        })
        .collect();
    let k = stat_deltas.len();
    let k_stats = k as u32;

    if k == 0 {
        return Ok(SobolResponse {
            metric: metric.as_str(),
            n_samples,
            k_stats: 0,
            base_seed,
            total_sims: 0,
            output_variance: 0.0,
            rows: Vec::new(),
        });
    }

    // Sample matrices A (N×k) and B (N×k) from independent uniform [0, 1]^k.
    // Sampling RNG seeded distinctly from per-row combat seeds.
    let sample_seed = base_seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xDEADBEEF_CAFEBABE;
    let mut sample_rng = Rng::new(sample_seed);
    let mat_a = sample_matrix(&mut sample_rng, n_samples as usize, k);
    let mat_b = sample_matrix(&mut sample_rng, n_samples as usize, k);

    // Evaluate A and B in parallel. Each row j of A uses combat seed (base_seed + j); B
    // rows use (base_seed + N + j). The k swapped matrices reuse A's seeds for CRN pairing.
    let f_a: Vec<f64> = (0..n_samples as usize)
        .into_par_iter()
        .map(|j| {
            evaluate_row(
                &shared,
                &input,
                metric,
                attacker_max_hull,
                defender_max_hull,
                &stat_deltas,
                &mat_a[j],
                base_seed.wrapping_add(j as u64),
            )
        })
        .collect();
    let f_b: Vec<f64> = (0..n_samples as usize)
        .into_par_iter()
        .map(|j| {
            evaluate_row(
                &shared,
                &input,
                metric,
                attacker_max_hull,
                defender_max_hull,
                &stat_deltas,
                &mat_b[j],
                base_seed
                    .wrapping_add(n_samples as u64)
                    .wrapping_add(j as u64),
            )
        })
        .collect();

    // V(Y) estimated from the combined A∪B sample.
    let v_y = variance(&f_a, &f_b);

    // For each stat i, build A_B^(i) on the fly (only column i differs from A) and evaluate.
    // Then compute V_i and V_T_i via the Jansen estimators.
    let rows: Vec<SobolRow> = (0..k)
        .into_par_iter()
        .map(|i| {
            let f_ab_i: Vec<f64> = (0..n_samples as usize)
                .into_par_iter()
                .map(|j| {
                    let mut row = mat_a[j].clone();
                    row[i] = mat_b[j][i];
                    evaluate_row(
                        &shared,
                        &input,
                        metric,
                        attacker_max_hull,
                        defender_max_hull,
                        &stat_deltas,
                        &row,
                        base_seed.wrapping_add(j as u64), // CRN with A_j
                    )
                })
                .collect();
            row_from_estimators(stat_deltas[i], &f_a, &f_b, &f_ab_i, v_y, base_seed)
        })
        .collect();

    let total_sims = (n_samples as u64) * (k as u64 + 2);

    Ok(SobolResponse {
        metric: metric.as_str(),
        n_samples,
        k_stats,
        base_seed,
        total_sims,
        output_variance: v_y,
        rows,
    })
}

/// Generate an N×k uniform [0, 1] matrix using SplitMix64. The next-u64 output is mapped
/// to a [0, 1) double via the high-52-bit trick (standard).
fn sample_matrix(rng: &mut Rng, n: usize, k: usize) -> Vec<Vec<f64>> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut row = Vec::with_capacity(k);
        for _ in 0..k {
            row.push(u64_to_unit_double(rng.next_u64()));
        }
        out.push(row);
    }
    out
}

#[inline]
fn u64_to_unit_double(x: u64) -> f64 {
    // 53 high bits → [0, 1).
    ((x >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
}

/// Evaluate the combat metric at a single Saltelli row. Maps each u_i ∈ [0, 1] to
/// δ_i ∈ [0, 2 · base_δ_i], applies all perturbations cumulatively via the shared engine
/// hook, and extracts the metric value.
#[allow(clippy::too_many_arguments)]
fn evaluate_row(
    shared: &crate::optimizer::monte_carlo::scenario::SharedScenarioData,
    input: &crate::optimizer::monte_carlo::scenario::CombatSimulationInput,
    metric: OutcomeMetric,
    attacker_max_hull: f64,
    defender_max_hull: f64,
    stat_deltas: &[(StatKey, f64)],
    sample_row: &[f64],
    sim_seed: u64,
) -> f64 {
    let perturbations: Vec<(StatKey, f64)> = stat_deltas
        .iter()
        .zip(sample_row.iter())
        .map(|((stat, base_delta), u)| (*stat, 2.0 * base_delta * *u))
        .collect();
    let result = run_one_sim_with_perturbations(shared, input, sim_seed, &perturbations);
    metric.extract(&result, attacker_max_hull, defender_max_hull)
}

/// Combined-sample variance estimator for V(Y). Uses A∪B (length 2N).
fn variance(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() + b.len();
    if n < 2 {
        return 0.0;
    }
    let sum: f64 = a.iter().sum::<f64>() + b.iter().sum::<f64>();
    let mean = sum / n as f64;
    let sq_sum: f64 = a.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
        + b.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
    sq_sum / (n as f64 - 1.0)
}

fn row_from_estimators(
    stat: (StatKey, f64),
    f_a: &[f64],
    f_b: &[f64],
    f_ab_i: &[f64],
    v_y: f64,
    bootstrap_seed: u64,
) -> SobolRow {
    let (stat_key, base_delta) = stat;
    let n = f_a.len();

    let (v_i, v_t_i) = jansen_estimators(f_a, f_b, f_ab_i);
    let s1 = if v_y > 0.0 { (v_i / v_y).max(0.0) } else { 0.0 };
    let st = if v_y > 0.0 {
        (v_t_i / v_y).max(0.0)
    } else {
        0.0
    };

    // Bootstrap 95% CIs by resampling rows with replacement. No extra engine calls.
    let mut rng = Rng::new(
        bootstrap_seed
            .wrapping_add(stat_key.as_str().len() as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15),
    );
    let mut s1_samples = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    let mut st_samples = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    let mut buf_a = vec![0.0; n];
    let mut buf_b = vec![0.0; n];
    let mut buf_ab = vec![0.0; n];
    for _ in 0..BOOTSTRAP_RESAMPLES {
        for slot in 0..n {
            let idx = (rng.next_u64() as usize) % n;
            buf_a[slot] = f_a[idx];
            buf_b[slot] = f_b[idx];
            buf_ab[slot] = f_ab_i[idx];
        }
        let v_boot = variance(&buf_a, &buf_b);
        if v_boot <= 0.0 {
            continue;
        }
        let (vi_boot, vt_boot) = jansen_estimators(&buf_a, &buf_b, &buf_ab);
        s1_samples.push((vi_boot / v_boot).max(0.0));
        st_samples.push((vt_boot / v_boot).max(0.0));
    }
    let (s1_lo, s1_hi) = ci95(&mut s1_samples);
    let (st_lo, st_hi) = ci95(&mut st_samples);

    SobolRow {
        stat: stat_key.as_str().to_string(),
        base_delta,
        s1,
        st,
        interaction: (st - s1).max(0.0),
        s1_ci95_low: s1_lo,
        s1_ci95_high: s1_hi,
        st_ci95_low: st_lo,
        st_ci95_high: st_hi,
    }
}

/// Jansen 1999 estimators for V_i (first-order) and V_T_i (total-order).
fn jansen_estimators(f_a: &[f64], f_b: &[f64], f_ab_i: &[f64]) -> (f64, f64) {
    let n = f_a.len() as f64;
    let v_i: f64 = f_a
        .iter()
        .zip(f_b.iter())
        .zip(f_ab_i.iter())
        .map(|((a, b), ab)| b * (ab - a))
        .sum::<f64>()
        / n;
    let v_t_i: f64 = f_a
        .iter()
        .zip(f_ab_i.iter())
        .map(|(a, ab)| (a - ab).powi(2))
        .sum::<f64>()
        / (2.0 * n);
    (v_i, v_t_i)
}

/// Lower / upper 2.5 / 97.5 percentile of a sample (modifies the slice in place by sorting).
fn ci95(samples: &mut [f64]) -> (f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo_idx = ((samples.len() as f64) * 0.025).floor() as usize;
    let hi_idx = ((samples.len() as f64) * 0.975)
        .ceil()
        .min(samples.len() as f64 - 1.0) as usize;
    (samples[lo_idx], samples[hi_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u64_to_unit_maps_into_unit_interval() {
        for x in [0u64, 1, 0xFFFF_FFFF_FFFF_FFFF, 0x8000_0000_0000_0000] {
            let v = u64_to_unit_double(x);
            assert!(v >= 0.0, "u64_to_unit produced negative for {x}: {v}");
            assert!(v < 1.0, "u64_to_unit produced >=1 for {x}: {v}");
        }
    }

    #[test]
    fn sample_matrix_shape_and_determinism() {
        let mut r1 = Rng::new(42);
        let mut r2 = Rng::new(42);
        let m1 = sample_matrix(&mut r1, 16, 5);
        let m2 = sample_matrix(&mut r2, 16, 5);
        assert_eq!(m1.len(), 16);
        for row in &m1 {
            assert_eq!(row.len(), 5);
            for &v in row {
                assert!((0.0..1.0).contains(&v));
            }
        }
        assert_eq!(m1, m2, "same seed should produce same matrix");
    }

    #[test]
    fn variance_combined_sample_matches_known_value() {
        // Values {1, 1, 1, 1} and {3, 3, 3, 3} concatenated → mean 2, var (n=8) sample
        // (Bessel) variance = (sum (x_i - 2)^2) / (n - 1) = (4 + 4) / 7 ≈ 1.1428.
        let a = vec![1.0; 4];
        let b = vec![3.0; 4];
        let v = variance(&a, &b);
        assert!((v - (8.0 / 7.0)).abs() < 1e-12, "got {v}");
    }

    #[test]
    fn jansen_recovers_known_decomposition_for_linear_model() {
        // Synthetic linear model Y = 2·X1 + 3·X2 with X_i ~ U(0, 1) independent.
        // Var(Y) = 4·Var(X1) + 9·Var(X2) = (4 + 9) / 12 = 13/12.
        // S_1 = (4/12) / (13/12) = 4/13 ≈ 0.3077.
        // S_2 = (9/12) / (13/12) = 9/13 ≈ 0.6923.
        // No interactions → S_T_i = S_i.
        let n = 4096usize;
        let mut rng = Rng::new(123);
        let mat_a = sample_matrix(&mut rng, n, 2);
        let mat_b = sample_matrix(&mut rng, n, 2);
        let f = |row: &[f64]| 2.0 * row[0] + 3.0 * row[1];
        let f_a: Vec<f64> = mat_a.iter().map(|r| f(r)).collect();
        let f_b: Vec<f64> = mat_b.iter().map(|r| f(r)).collect();
        let v_y = variance(&f_a, &f_b);
        // Index 0 (X1):
        let f_ab_0: Vec<f64> = (0..n)
            .map(|j| {
                let mut r = mat_a[j].clone();
                r[0] = mat_b[j][0];
                f(&r)
            })
            .collect();
        let (v_1, vt_1) = jansen_estimators(&f_a, &f_b, &f_ab_0);
        let s_1 = (v_1 / v_y).max(0.0);
        let st_1 = (vt_1 / v_y).max(0.0);
        assert!(
            (s_1 - 4.0 / 13.0).abs() < 0.03,
            "S_1 estimator off: expected ≈0.3077, got {s_1}"
        );
        // Linear-additive: S_T should equal S_1 within sampling noise.
        assert!(
            (st_1 - s_1).abs() < 0.05,
            "linear model should have S_T ≈ S_1; got S_1={s_1}, S_T={st_1}"
        );
    }

    #[test]
    fn jansen_detects_interaction_for_multiplicative_model() {
        // Y = X1 · X2 with X_i ~ U(0, 1) independent. Pure interaction model: S_1 ≈ S_2 ≈ 0
        // (marginal mean E[X_j · X_i] = E[X_i]·E[X_j] = constant; main effects vanish).
        // S_T_1 and S_T_2 should be substantial (the interaction contributes entirely to both).
        let n = 4096usize;
        let mut rng = Rng::new(456);
        let mat_a = sample_matrix(&mut rng, n, 2);
        let mat_b = sample_matrix(&mut rng, n, 2);
        let f = |row: &[f64]| row[0] * row[1];
        let f_a: Vec<f64> = mat_a.iter().map(|r| f(r)).collect();
        let f_b: Vec<f64> = mat_b.iter().map(|r| f(r)).collect();
        let v_y = variance(&f_a, &f_b);
        let f_ab_0: Vec<f64> = (0..n)
            .map(|j| {
                let mut r = mat_a[j].clone();
                r[0] = mat_b[j][0];
                f(&r)
            })
            .collect();
        let (v_1, vt_1) = jansen_estimators(&f_a, &f_b, &f_ab_0);
        let s_1 = (v_1 / v_y).max(0.0);
        let st_1 = (vt_1 / v_y).max(0.0);
        // S_1 for multiplicative model: actually NOT zero — E[X1·X2 | X1] = X1 · E[X2] = 0.5·X1,
        // so the main effect of X1 IS non-zero. Var(0.5·X1) = 0.25/12, Var(Y) = E[Y²] - E[Y]² =
        // E[X1²]·E[X2²] - (E[X1]·E[X2])² = (1/3)·(1/3) - 0.25 = 1/9 - 1/4 = -5/36. Hmm wait.
        // Actually Var(Y) = 1/9 - 1/16 ≈ 0.0486. S_1 ≈ Var(0.5·X1) / Var(Y) = 0.0208 / 0.0486 ≈ 0.43.
        // So S_1 ≈ 0.43, S_T_1 should be > S_1 (interaction adds on top), so S_T - S_1 > 0.
        assert!(
            st_1 > s_1 + 0.05,
            "multiplicative model: S_T_1 ({st_1}) should exceed S_1 ({s_1}) by interaction contribution"
        );
    }

    #[test]
    fn ci95_quantiles_on_sorted_uniform() {
        let mut samples: Vec<f64> = (0..200).map(|i| i as f64 / 200.0).collect();
        let (lo, hi) = ci95(&mut samples);
        // 2.5% ≈ 0.025, 97.5% ≈ 0.975.
        assert!((lo - 0.025).abs() < 0.05);
        assert!((hi - 0.975).abs() < 0.05);
    }
}
