//! SIMD feasibility prototype for per-hit combat damage math.
//!
//! This module intentionally keeps a scalar baseline and an optional AVX2 path
//! so we can benchmark speedup and verify numerical parity before considering
//! engine integration.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelExecutionPath {
    Scalar,
    Avx2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageKernelBatchError {
    LengthMismatch,
    UnsupportedSimd,
}

pub struct DamageKernelBatchInputs<'a> {
    pub effective_attack: &'a [f64],
    pub mitigation_multiplier: &'a [f64],
    pub effective_pierce: &'a [f64],
    pub defense_mitigation_bonus: &'a [f64],
    pub crit_multiplier: &'a [f64],
    pub proc_multiplier: &'a [f64],
    pub isolytic_taken: &'a [f64],
    pub apex_damage_factor: &'a [f64],
    pub shield_mitigation: &'a [f64],
    pub defender_shield_remaining: &'a [f64],
}

impl<'a> DamageKernelBatchInputs<'a> {
    pub fn len(&self) -> usize {
        self.effective_attack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effective_attack.is_empty()
    }
}

pub struct DamageKernelBatchOutputs<'a> {
    pub damage_after_apex: &'a mut [f64],
    pub shield_damage: &'a mut [f64],
    pub hull_damage: &'a mut [f64],
}

pub fn experimental_engine_kernel_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(
        || match std::env::var("KOBAYASHI_EXPERIMENTAL_SIMD_DAMAGE_KERNEL") {
            Ok(value) => {
                let lowered = value.trim().to_ascii_lowercase();
                matches!(lowered.as_str(), "1" | "true" | "yes" | "on")
            }
            Err(_) => false,
        },
    )
}

fn validate_lengths(
    inputs: &DamageKernelBatchInputs<'_>,
    outputs: &DamageKernelBatchOutputs<'_>,
) -> Result<(), DamageKernelBatchError> {
    let len = inputs.len();
    let input_lengths = [
        inputs.mitigation_multiplier.len(),
        inputs.effective_pierce.len(),
        inputs.defense_mitigation_bonus.len(),
        inputs.crit_multiplier.len(),
        inputs.proc_multiplier.len(),
        inputs.isolytic_taken.len(),
        inputs.apex_damage_factor.len(),
        inputs.shield_mitigation.len(),
        inputs.defender_shield_remaining.len(),
    ];
    let output_lengths = [
        outputs.damage_after_apex.len(),
        outputs.shield_damage.len(),
        outputs.hull_damage.len(),
    ];

    if input_lengths.iter().all(|n| *n == len) && output_lengths.iter().all(|n| *n == len) {
        Ok(())
    } else {
        Err(DamageKernelBatchError::LengthMismatch)
    }
}

#[inline]
fn scalar_kernel_at(
    idx: usize,
    inputs: &DamageKernelBatchInputs<'_>,
    outputs: &mut DamageKernelBatchOutputs<'_>,
) {
    let damage_through = (inputs.mitigation_multiplier[idx]
        + inputs.effective_pierce[idx]
        + inputs.defense_mitigation_bonus[idx])
        .max(0.0);
    let pre_attack_damage = inputs.effective_attack[idx]
        * damage_through
        * inputs.crit_multiplier[idx]
        * inputs.proc_multiplier[idx];
    let damage_before_apex = pre_attack_damage + inputs.isolytic_taken[idx];
    let damage_after_apex = damage_before_apex * inputs.apex_damage_factor[idx];
    let shield_portion = damage_after_apex * inputs.shield_mitigation[idx];
    let hull_portion = damage_after_apex * (1.0 - inputs.shield_mitigation[idx]);
    let actual_shield_damage = shield_portion.min(inputs.defender_shield_remaining[idx]);
    let shield_overflow = shield_portion - actual_shield_damage;
    let hull_damage = hull_portion + shield_overflow;

    outputs.damage_after_apex[idx] = damage_after_apex;
    outputs.shield_damage[idx] = actual_shield_damage;
    outputs.hull_damage[idx] = hull_damage;
}

pub fn compute_damage_kernel_batch_scalar(
    inputs: &DamageKernelBatchInputs<'_>,
    outputs: &mut DamageKernelBatchOutputs<'_>,
) -> Result<(), DamageKernelBatchError> {
    validate_lengths(inputs, outputs)?;
    for idx in 0..inputs.len() {
        scalar_kernel_at(idx, inputs, outputs);
    }
    Ok(())
}

pub fn avx2_supported() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

pub fn compute_damage_kernel_batch(
    inputs: &DamageKernelBatchInputs<'_>,
    outputs: &mut DamageKernelBatchOutputs<'_>,
) -> Result<KernelExecutionPath, DamageKernelBatchError> {
    validate_lengths(inputs, outputs)?;
    if avx2_supported() {
        compute_damage_kernel_batch_avx2(inputs, outputs)?;
        Ok(KernelExecutionPath::Avx2)
    } else {
        compute_damage_kernel_batch_scalar(inputs, outputs)?;
        Ok(KernelExecutionPath::Scalar)
    }
}

pub fn compute_damage_kernel_batch_avx2(
    inputs: &DamageKernelBatchInputs<'_>,
    outputs: &mut DamageKernelBatchOutputs<'_>,
) -> Result<(), DamageKernelBatchError> {
    validate_lengths(inputs, outputs)?;

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if !avx2_supported() {
            return Err(DamageKernelBatchError::UnsupportedSimd);
        }
        // SAFETY:
        // - AVX2 availability is checked at runtime above.
        // - All loads/stores are within bounds due to validated equal lengths and loop guards.
        unsafe { compute_damage_kernel_batch_avx2_impl(inputs, outputs) };
        Ok(())
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = inputs;
        let _ = outputs;
        Err(DamageKernelBatchError::UnsupportedSimd)
    }
}

/// Apply post-attack damage resolution using the SIMD kernel contract for a single hit.
///
/// `damage_after_attack_phase` is the already-resolved attack-phase damage (after crit/proc/effect
/// composition). This helper maps that scalar into the batch kernel for engine integration parity.
pub fn resolve_damage_application_single_hit(
    damage_after_attack_phase: f64,
    isolytic_taken: f64,
    apex_damage_factor: f64,
    shield_mitigation: f64,
    defender_shield_remaining: f64,
) -> (f64, f64, f64, KernelExecutionPath) {
    let effective_attack = [damage_after_attack_phase];
    let mitigation_multiplier = [1.0];
    let effective_pierce = [0.0];
    let defense_mitigation_bonus = [0.0];
    let crit_multiplier = [1.0];
    let proc_multiplier = [1.0];
    let isolytic_taken_arr = [isolytic_taken];
    let apex_damage_factor_arr = [apex_damage_factor];
    let shield_mitigation_arr = [shield_mitigation];
    let defender_shield_remaining_arr = [defender_shield_remaining];

    let inputs = DamageKernelBatchInputs {
        effective_attack: &effective_attack,
        mitigation_multiplier: &mitigation_multiplier,
        effective_pierce: &effective_pierce,
        defense_mitigation_bonus: &defense_mitigation_bonus,
        crit_multiplier: &crit_multiplier,
        proc_multiplier: &proc_multiplier,
        isolytic_taken: &isolytic_taken_arr,
        apex_damage_factor: &apex_damage_factor_arr,
        shield_mitigation: &shield_mitigation_arr,
        defender_shield_remaining: &defender_shield_remaining_arr,
    };

    let mut damage_after_apex = [0.0];
    let mut shield_damage = [0.0];
    let mut hull_damage = [0.0];
    let mut outputs = DamageKernelBatchOutputs {
        damage_after_apex: &mut damage_after_apex,
        shield_damage: &mut shield_damage,
        hull_damage: &mut hull_damage,
    };

    match compute_damage_kernel_batch(&inputs, &mut outputs) {
        Ok(path) => (
            outputs.damage_after_apex[0],
            outputs.shield_damage[0],
            outputs.hull_damage[0],
            path,
        ),
        Err(_) => {
            let _ = compute_damage_kernel_batch_scalar(&inputs, &mut outputs);
            (
                outputs.damage_after_apex[0],
                outputs.shield_damage[0],
                outputs.hull_damage[0],
                KernelExecutionPath::Scalar,
            )
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn compute_damage_kernel_batch_avx2_impl(
    inputs: &DamageKernelBatchInputs<'_>,
    outputs: &mut DamageKernelBatchOutputs<'_>,
) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let len = inputs.len();
    let mut idx = 0usize;

    let zeros = _mm256_set1_pd(0.0);
    let ones = _mm256_set1_pd(1.0);

    while idx + 4 <= len {
        let effective_attack = _mm256_loadu_pd(inputs.effective_attack.as_ptr().add(idx));
        let mitigation_multiplier = _mm256_loadu_pd(inputs.mitigation_multiplier.as_ptr().add(idx));
        let effective_pierce = _mm256_loadu_pd(inputs.effective_pierce.as_ptr().add(idx));
        let defense_mitigation_bonus =
            _mm256_loadu_pd(inputs.defense_mitigation_bonus.as_ptr().add(idx));
        let crit_multiplier = _mm256_loadu_pd(inputs.crit_multiplier.as_ptr().add(idx));
        let proc_multiplier = _mm256_loadu_pd(inputs.proc_multiplier.as_ptr().add(idx));
        let isolytic_taken = _mm256_loadu_pd(inputs.isolytic_taken.as_ptr().add(idx));
        let apex_damage_factor = _mm256_loadu_pd(inputs.apex_damage_factor.as_ptr().add(idx));
        let shield_mitigation = _mm256_loadu_pd(inputs.shield_mitigation.as_ptr().add(idx));
        let defender_shield_remaining =
            _mm256_loadu_pd(inputs.defender_shield_remaining.as_ptr().add(idx));

        let damage_through = _mm256_max_pd(
            _mm256_add_pd(
                _mm256_add_pd(mitigation_multiplier, effective_pierce),
                defense_mitigation_bonus,
            ),
            zeros,
        );
        let pre_attack_damage = _mm256_mul_pd(
            _mm256_mul_pd(
                _mm256_mul_pd(effective_attack, damage_through),
                crit_multiplier,
            ),
            proc_multiplier,
        );
        let damage_before_apex = _mm256_add_pd(pre_attack_damage, isolytic_taken);
        let damage_after_apex = _mm256_mul_pd(damage_before_apex, apex_damage_factor);
        let shield_portion = _mm256_mul_pd(damage_after_apex, shield_mitigation);
        let hull_portion = _mm256_mul_pd(damage_after_apex, _mm256_sub_pd(ones, shield_mitigation));
        let actual_shield_damage = _mm256_min_pd(shield_portion, defender_shield_remaining);
        let shield_overflow = _mm256_sub_pd(shield_portion, actual_shield_damage);
        let hull_damage = _mm256_add_pd(hull_portion, shield_overflow);

        _mm256_storeu_pd(
            outputs.damage_after_apex.as_mut_ptr().add(idx),
            damage_after_apex,
        );
        _mm256_storeu_pd(
            outputs.shield_damage.as_mut_ptr().add(idx),
            actual_shield_damage,
        );
        _mm256_storeu_pd(outputs.hull_damage.as_mut_ptr().add(idx), hull_damage);

        idx += 4;
    }

    for tail_idx in idx..len {
        scalar_kernel_at(tail_idx, inputs, outputs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_inputs(len: usize) -> [Vec<f64>; 10] {
        let mut effective_attack = Vec::with_capacity(len);
        let mut mitigation_multiplier = Vec::with_capacity(len);
        let mut effective_pierce = Vec::with_capacity(len);
        let mut defense_mitigation_bonus = Vec::with_capacity(len);
        let mut crit_multiplier = Vec::with_capacity(len);
        let mut proc_multiplier = Vec::with_capacity(len);
        let mut isolytic_taken = Vec::with_capacity(len);
        let mut apex_damage_factor = Vec::with_capacity(len);
        let mut shield_mitigation = Vec::with_capacity(len);
        let mut defender_shield_remaining = Vec::with_capacity(len);

        for i in 0..len {
            let t = i as f64;
            effective_attack.push(500.0 + t % 113.0);
            mitigation_multiplier.push(0.15 + (t % 11.0) * 0.03);
            effective_pierce.push((t % 7.0) * 0.02);
            defense_mitigation_bonus.push((t % 5.0) * 0.01);
            crit_multiplier.push(1.0 + (t % 4.0) * 0.2);
            proc_multiplier.push(1.0 + (t % 3.0) * 0.15);
            isolytic_taken.push((t % 17.0) * 2.0);
            apex_damage_factor.push(0.6 + (t % 9.0) * 0.03);
            shield_mitigation.push(((t % 10.0) * 0.08).clamp(0.0, 0.95));
            defender_shield_remaining.push(50.0 + (t % 101.0) * 3.0);
        }

        [
            effective_attack,
            mitigation_multiplier,
            effective_pierce,
            defense_mitigation_bonus,
            crit_multiplier,
            proc_multiplier,
            isolytic_taken,
            apex_damage_factor,
            shield_mitigation,
            defender_shield_remaining,
        ]
    }

    fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn single_hit_wrapper_matches_scalar_formula() {
        let damage_after_attack_phase = 1400.5_f64;
        let isolytic_taken = 42.2_f64;
        let apex_damage_factor = 0.77_f64;
        let shield_mitigation = 0.83_f64;
        let defender_shield_remaining = 300.0_f64;

        let expected_damage_after_apex =
            (damage_after_attack_phase + isolytic_taken) * apex_damage_factor;
        let expected_shield_damage =
            (expected_damage_after_apex * shield_mitigation).min(defender_shield_remaining);
        let expected_hull_damage = expected_damage_after_apex * (1.0 - shield_mitigation)
            + (expected_damage_after_apex * shield_mitigation - expected_shield_damage);

        let (damage_after_apex, shield_damage, hull_damage, _) =
            resolve_damage_application_single_hit(
                damage_after_attack_phase,
                isolytic_taken,
                apex_damage_factor,
                shield_mitigation,
                defender_shield_remaining,
            );

        assert!(approx_eq(
            damage_after_apex,
            expected_damage_after_apex,
            1e-12
        ));
        assert!(approx_eq(shield_damage, expected_shield_damage, 1e-12));
        assert!(approx_eq(hull_damage, expected_hull_damage, 1e-12));
    }

    #[test]
    fn scalar_and_auto_paths_match() {
        let data = sample_inputs(257);
        let inputs = DamageKernelBatchInputs {
            effective_attack: &data[0],
            mitigation_multiplier: &data[1],
            effective_pierce: &data[2],
            defense_mitigation_bonus: &data[3],
            crit_multiplier: &data[4],
            proc_multiplier: &data[5],
            isolytic_taken: &data[6],
            apex_damage_factor: &data[7],
            shield_mitigation: &data[8],
            defender_shield_remaining: &data[9],
        };

        let mut scalar_damage_after_apex = vec![0.0; inputs.len()];
        let mut scalar_shield_damage = vec![0.0; inputs.len()];
        let mut scalar_hull_damage = vec![0.0; inputs.len()];
        let mut scalar_outputs = DamageKernelBatchOutputs {
            damage_after_apex: &mut scalar_damage_after_apex,
            shield_damage: &mut scalar_shield_damage,
            hull_damage: &mut scalar_hull_damage,
        };
        compute_damage_kernel_batch_scalar(&inputs, &mut scalar_outputs).unwrap();

        let mut auto_damage_after_apex = vec![0.0; inputs.len()];
        let mut auto_shield_damage = vec![0.0; inputs.len()];
        let mut auto_hull_damage = vec![0.0; inputs.len()];
        let mut auto_outputs = DamageKernelBatchOutputs {
            damage_after_apex: &mut auto_damage_after_apex,
            shield_damage: &mut auto_shield_damage,
            hull_damage: &mut auto_hull_damage,
        };
        let _ = compute_damage_kernel_batch(&inputs, &mut auto_outputs).unwrap();

        for i in 0..inputs.len() {
            assert!(approx_eq(
                scalar_outputs.damage_after_apex[i],
                auto_outputs.damage_after_apex[i],
                1e-12
            ));
            assert!(approx_eq(
                scalar_outputs.shield_damage[i],
                auto_outputs.shield_damage[i],
                1e-12
            ));
            assert!(approx_eq(
                scalar_outputs.hull_damage[i],
                auto_outputs.hull_damage[i],
                1e-12
            ));
        }
    }

    #[test]
    fn avx2_matches_scalar_when_available() {
        if !avx2_supported() {
            return;
        }

        let data = sample_inputs(513);
        let inputs = DamageKernelBatchInputs {
            effective_attack: &data[0],
            mitigation_multiplier: &data[1],
            effective_pierce: &data[2],
            defense_mitigation_bonus: &data[3],
            crit_multiplier: &data[4],
            proc_multiplier: &data[5],
            isolytic_taken: &data[6],
            apex_damage_factor: &data[7],
            shield_mitigation: &data[8],
            defender_shield_remaining: &data[9],
        };

        let mut scalar_damage_after_apex = vec![0.0; inputs.len()];
        let mut scalar_shield_damage = vec![0.0; inputs.len()];
        let mut scalar_hull_damage = vec![0.0; inputs.len()];
        let mut scalar_outputs = DamageKernelBatchOutputs {
            damage_after_apex: &mut scalar_damage_after_apex,
            shield_damage: &mut scalar_shield_damage,
            hull_damage: &mut scalar_hull_damage,
        };
        compute_damage_kernel_batch_scalar(&inputs, &mut scalar_outputs).unwrap();

        let mut simd_damage_after_apex = vec![0.0; inputs.len()];
        let mut simd_shield_damage = vec![0.0; inputs.len()];
        let mut simd_hull_damage = vec![0.0; inputs.len()];
        let mut simd_outputs = DamageKernelBatchOutputs {
            damage_after_apex: &mut simd_damage_after_apex,
            shield_damage: &mut simd_shield_damage,
            hull_damage: &mut simd_hull_damage,
        };
        compute_damage_kernel_batch_avx2(&inputs, &mut simd_outputs).unwrap();

        for i in 0..inputs.len() {
            assert!(approx_eq(
                scalar_outputs.damage_after_apex[i],
                simd_outputs.damage_after_apex[i],
                1e-12
            ));
            assert!(approx_eq(
                scalar_outputs.shield_damage[i],
                simd_outputs.shield_damage[i],
                1e-12
            ));
            assert!(approx_eq(
                scalar_outputs.hull_damage[i],
                simd_outputs.hull_damage[i],
                1e-12
            ));
        }
    }
}
