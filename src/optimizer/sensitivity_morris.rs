//! Morris-method screening sensitivity analysis (elementary effects).
//!
//! Where v1 OAT ([`crate::optimizer::sensitivity`]) perturbs each stat from a fixed baseline,
//! Morris draws `r` random trajectories through stat-space. Each trajectory visits `k+1`
//! points: a starting baseline, then `k` cumulative perturbations applied in a random
//! permutation of the stat list. Per trajectory, paired-CRN Monte Carlo gives a clean
//! elementary effect (EE) per stat; aggregating across trajectories yields:
//!
//! - **μ\*** (mean of `|EE|`): importance, robust to sign cancellation.
//! - **μ** (mean signed EE): direction (positive = increasing the stat helps the attacker).
//! - **σ** (std EE across trajectories): non-linearity / interaction with previously-perturbed
//!   stats. A high σ relative to μ\* indicates the stat's effect depends on the values of
//!   other stats — i.e. the stat **interacts**.
//!
//! Morris is a **screening** method. σ flags interactive stats but does not identify the
//! specific pairs that interact — Sobol pairwise indices are tracked separately in
//! [docs/ROADMAP.md](../../docs/ROADMAP.md).
//!
//! Compute budget: `r × (k+1) × num_sims` engine calls. Defaults
//! (`r=10`, `k=15`, `num_sims=200`) ≈ 32k sims, comparable to the v1 default of
//! `(k+1) × 2000` = 32k.

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
use crate::server::sensitivity_jobs::SensitivityJobProgress;

/// Default number of Morris trajectories. r=10 yields a conservative μ\* / σ estimate;
/// users can override to 20–50 for tighter CIs at proportional cost.
pub const DEFAULT_R_TRAJECTORIES: u32 = 10;
/// Default sims per trajectory point. Smaller than v1 (2000) because each Morris trajectory
/// itself averages out a chunk of variance via CRN pairing.
pub const DEFAULT_NUM_SIMS_PER_POINT: u32 = 200;
/// Server-side cap on `r` to keep a single request from monopolising CPU.
pub const MAX_R_TRAJECTORIES: u32 = 200;
/// Server-side cap on sims per trajectory point.
pub const MAX_NUM_SIMS_PER_POINT: u32 = 10_000;

/// Crew + scenario input for a Morris run. Mirrors [`crate::optimizer::sensitivity::SensitivityRequest`]
/// plus `r_trajectories`. JSON shape matches the OAT request so the frontend can share scenario inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorrisRequest {
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
    /// Paired sims per trajectory point. Default [`DEFAULT_NUM_SIMS_PER_POINT`].
    #[serde(default)]
    pub num_sims: Option<u32>,
    /// Number of Morris trajectories. Default [`DEFAULT_R_TRAJECTORIES`].
    #[serde(default)]
    pub r_trajectories: Option<u32>,
    /// Base RNG seed. Default 0. Both the trajectory permutations and the per-point CRN seeds
    /// derive from this — same seed reproduces the same μ\* / σ exactly.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Combat rounds per sim. Default 3 (engine default).
    #[serde(default)]
    pub rounds: Option<u32>,
    /// Outcome metric to measure against. Default [`OutcomeMetric::HullRemaining`].
    #[serde(default)]
    pub metric: Option<OutcomeMetric>,
    /// Per-stat δ overrides. Stats not listed use [`StatKey::default_delta`]. Stats whose
    /// override is `0.0` are dropped from the perturbation set (no trajectory step for them).
    #[serde(default)]
    pub deltas: Option<HashMap<String, f64>>,
}

/// One stat's aggregated elementary-effect statistics across the `r` trajectories.
#[derive(Debug, Clone, Serialize)]
pub struct MorrisRow {
    /// Stat key (snake_case, matches [`StatKey::as_str`]).
    pub stat: String,
    /// Δ applied to that stat at every trajectory step.
    pub delta_applied: f64,
    /// μ\* — mean of `|EE|` across trajectories. Importance.
    pub mu_star: f64,
    /// μ — mean signed EE. Direction.
    pub mu: f64,
    /// σ — sample std of EE across trajectories (denominator `r - 1`). Interaction signal.
    pub sigma: f64,
    /// Number of EE samples (= number of trajectories the stat was perturbed in).
    pub n_samples: u32,
    /// 95% large-N normal CI on μ\* via the std of `|EE|`.
    pub mu_star_ci95_low: f64,
    pub mu_star_ci95_high: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MorrisResponse {
    pub metric: &'static str,
    pub num_sims_per_point: u32,
    pub r_trajectories: u32,
    pub k_stats: u32,
    pub base_seed: u64,
    /// Total engine calls performed (`r * (k+1) * num_sims`).
    pub total_sims: u64,
    /// One row per stat, **unsorted** — clients sort by μ\* (importance) or σ (interaction).
    pub rows: Vec<MorrisRow>,
}

/// End-to-end Morris run. Builds shared scenario data once, then for each trajectory walks
/// k+1 points (baseline → ... → all perturbed) with the same CRN seed sequence; per stat the
/// step-to-step difference divided by δ is one EE sample.
pub fn run_morris(
    registry: &DataRegistry,
    request: &MorrisRequest,
) -> Result<MorrisResponse, String> {
    run_morris_with_progress(registry, request, &SensitivityJobProgress::no_op())
}

/// Morris run that reports progress + checks cancellation through a sink. The sync
/// [`run_morris`] entry above wraps this with a no-op sink.
pub fn run_morris_with_progress(
    registry: &DataRegistry,
    request: &MorrisRequest,
    progress: &SensitivityJobProgress,
) -> Result<MorrisResponse, String> {
    let num_sims = request
        .num_sims
        .unwrap_or(DEFAULT_NUM_SIMS_PER_POINT)
        .clamp(2, MAX_NUM_SIMS_PER_POINT);
    let r_traj = request
        .r_trajectories
        .unwrap_or(DEFAULT_R_TRAJECTORIES)
        .clamp(2, MAX_R_TRAJECTORIES);
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

    // Resolve the stat list + δs. Stats with explicit override = 0 are dropped (matches v1).
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
    let k_stats = stat_deltas.len() as u32;
    let k = stat_deltas.len();

    if k == 0 {
        return Ok(MorrisResponse {
            metric: metric.as_str(),
            num_sims_per_point: num_sims,
            r_trajectories: r_traj,
            k_stats: 0,
            base_seed,
            total_sims: 0,
            rows: Vec::new(),
        });
    }

    progress.set_total_sims((r_traj as u64) * ((k as u64) + 1) * (num_sims as u64));
    progress.set_phase("trajectories");

    // Per trajectory, collect EE samples keyed by StatKey index in `stat_deltas`.
    // `ee_samples[stat_idx]` accumulates the r elementary-effect values.
    let trajectory_ee: Vec<Vec<(usize, f64)>> = (0..r_traj)
        .into_par_iter()
        .map(|r_idx| {
            let traj_seed =
                base_seed.wrapping_add((r_idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let permutation = random_permutation(traj_seed, k);
            // CRN seed range distinct per trajectory so trajectories are independent samples.
            let crn_base = base_seed.wrapping_add((r_idx as u64).wrapping_mul(num_sims as u64));

            // Walk k+1 points; record metric mean at each step.
            // Step 0 = baseline (no perturbations); step j = first `j` perturbations from the
            // permutation applied cumulatively.
            let mut step_metric: Vec<f64> = Vec::with_capacity(k + 1);
            let mut cumulative: Vec<(StatKey, f64)> = Vec::with_capacity(k);
            for step in 0..=k {
                if step > 0 {
                    let stat_idx = permutation[step - 1];
                    cumulative.push(stat_deltas[stat_idx]);
                }
                let mean = (0..num_sims)
                    .into_par_iter()
                    .map(|i| {
                        let sim_seed = crn_base.wrapping_add(i as u64);
                        let result =
                            run_one_sim_with_perturbations(&shared, &input, sim_seed, &cumulative);
                        let v = metric.extract(&result, attacker_max_hull, defender_max_hull);
                        progress.record_sims(1);
                        v
                    })
                    .sum::<f64>()
                    / num_sims as f64;
                step_metric.push(mean);
            }

            // EE for the stat introduced at step j (j=1..=k) = (y_j - y_{j-1}) / δ.
            let mut ees = Vec::with_capacity(k);
            for j in 1..=k {
                let stat_idx = permutation[j - 1];
                let delta = stat_deltas[stat_idx].1;
                let ee = if delta.abs() > 0.0 {
                    (step_metric[j] - step_metric[j - 1]) / delta
                } else {
                    0.0
                };
                ees.push((stat_idx, ee));
            }
            ees
        })
        .collect();

    if progress.cancelled() {
        return Err("Cancelled".to_string());
    }

    // Aggregate per-stat EE samples.
    let mut per_stat_samples: Vec<Vec<f64>> = vec![Vec::with_capacity(r_traj as usize); k];
    for ees in trajectory_ee {
        for (stat_idx, ee) in ees {
            per_stat_samples[stat_idx].push(ee);
        }
    }

    let rows: Vec<MorrisRow> = stat_deltas
        .iter()
        .enumerate()
        .map(|(idx, (stat, delta))| row_from_ee_samples(*stat, *delta, &per_stat_samples[idx]))
        .collect();

    let total_sims = (r_traj as u64) * ((k as u64) + 1) * (num_sims as u64);

    Ok(MorrisResponse {
        metric: metric.as_str(),
        num_sims_per_point: num_sims,
        r_trajectories: r_traj,
        k_stats,
        base_seed,
        total_sims,
        rows,
    })
}

/// Deterministic Fisher-Yates shuffle of `0..k` seeded by SplitMix64.
fn random_permutation(seed: u64, k: usize) -> Vec<usize> {
    let mut rng = Rng::new(seed);
    let mut perm: Vec<usize> = (0..k).collect();
    // Standard Fisher-Yates: i from k-1 down to 1, swap perm[i] with perm[j], j ∈ [0, i].
    for i in (1..k).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        perm.swap(i, j);
    }
    perm
}

fn row_from_ee_samples(stat: StatKey, delta: f64, samples: &[f64]) -> MorrisRow {
    let n = samples.len();
    if n == 0 {
        return MorrisRow {
            stat: stat.as_str().to_string(),
            delta_applied: delta,
            mu_star: 0.0,
            mu: 0.0,
            sigma: 0.0,
            n_samples: 0,
            mu_star_ci95_low: 0.0,
            mu_star_ci95_high: 0.0,
        };
    }
    let mu: f64 = samples.iter().sum::<f64>() / n as f64;
    let abs_samples: Vec<f64> = samples.iter().map(|x| x.abs()).collect();
    let mu_star: f64 = abs_samples.iter().sum::<f64>() / n as f64;

    let sigma = if n < 2 {
        0.0
    } else {
        let var: f64 = samples.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / (n as f64 - 1.0);
        var.sqrt()
    };
    let abs_sigma = if n < 2 {
        0.0
    } else {
        let var: f64 = abs_samples
            .iter()
            .map(|x| (x - mu_star).powi(2))
            .sum::<f64>()
            / (n as f64 - 1.0);
        var.sqrt()
    };
    // Large-N normal approximation on μ\* (consistent with v1 sensitivity CI). For r < 30
    // this is mildly anti-conservative; the response surfaces `n_samples` so callers know.
    const Z_95: f64 = 1.959_963_984_540_054;
    let stderr = abs_sigma / (n as f64).sqrt();
    let half = Z_95 * stderr;
    MorrisRow {
        stat: stat.as_str().to_string(),
        delta_applied: delta,
        mu_star,
        mu,
        sigma,
        n_samples: n as u32,
        mu_star_ci95_low: mu_star - half,
        mu_star_ci95_high: mu_star + half,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permutation_is_deterministic_and_covers_all_indices() {
        let p1 = random_permutation(42, 15);
        let p2 = random_permutation(42, 15);
        assert_eq!(p1, p2);
        assert_eq!(p1.len(), 15);
        let mut sorted = p1.clone();
        sorted.sort();
        let expected: Vec<usize> = (0..15).collect();
        assert_eq!(sorted, expected);
    }

    #[test]
    fn permutation_different_seeds_yield_different_orderings() {
        let p1 = random_permutation(1, 15);
        let p2 = random_permutation(2, 15);
        // Vanishingly unlikely 15! = 1.3T permutations would collide for two adjacent seeds.
        assert_ne!(p1, p2);
    }

    #[test]
    fn row_from_constant_positive_ee_has_mu_equals_mu_star() {
        let samples = vec![0.5_f64; 20];
        let row = row_from_ee_samples(StatKey::WeaponDamage, 0.05, &samples);
        assert!((row.mu - 0.5).abs() < 1e-12);
        assert!((row.mu_star - 0.5).abs() < 1e-12);
        assert!(row.sigma < 1e-12);
        assert_eq!(row.n_samples, 20);
        assert!(row.mu_star_ci95_low < 0.5 + 1e-12);
        assert!(row.mu_star_ci95_high > 0.5 - 1e-12);
    }

    #[test]
    fn row_from_alternating_sign_ee_separates_mu_from_mu_star() {
        // ±0.5 alternating → μ ≈ 0 (sign cancels), μ* = 0.5 (magnitudes), σ ≈ 0.5.
        let samples: Vec<f64> = (0..20)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let row = row_from_ee_samples(StatKey::CritChance, 0.01, &samples);
        assert!(row.mu.abs() < 1e-9, "mu should be near 0, got {}", row.mu);
        assert!(
            (row.mu_star - 0.5).abs() < 1e-12,
            "mu_star should be 0.5, got {}",
            row.mu_star
        );
        // Variance of ±0.5 around mean=0 is 0.25; sample (Bessel) variance with n=20 is
        // 0.25 × 20/19, so σ = √(0.25 × 20/19) ≈ 0.5129.
        let expected_sigma = (0.25_f64 * 20.0 / 19.0).sqrt();
        assert!(
            (row.sigma - expected_sigma).abs() < 1e-9,
            "sigma should be {}, got {}",
            expected_sigma,
            row.sigma
        );
    }

    #[test]
    fn row_from_empty_samples_returns_zeros() {
        let row = row_from_ee_samples(StatKey::WeaponDamage, 0.05, &[]);
        assert_eq!(row.n_samples, 0);
        assert_eq!(row.mu, 0.0);
        assert_eq!(row.mu_star, 0.0);
        assert_eq!(row.sigma, 0.0);
    }
}
