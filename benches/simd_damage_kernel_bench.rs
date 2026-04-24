//! SIMD feasibility benchmark for a hot per-hit combat math kernel.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use kobayashi::combat::simd_damage_kernel::{
    avx2_supported, compute_damage_kernel_batch, compute_damage_kernel_batch_avx2,
    compute_damage_kernel_batch_scalar, DamageKernelBatchInputs, DamageKernelBatchOutputs,
};

#[derive(Clone)]
struct KernelBatchData {
    effective_attack: Vec<f64>,
    mitigation_multiplier: Vec<f64>,
    effective_pierce: Vec<f64>,
    defense_mitigation_bonus: Vec<f64>,
    crit_multiplier: Vec<f64>,
    proc_multiplier: Vec<f64>,
    isolytic_taken: Vec<f64>,
    apex_damage_factor: Vec<f64>,
    shield_mitigation: Vec<f64>,
    defender_shield_remaining: Vec<f64>,
}

impl KernelBatchData {
    fn synthetic(len: usize) -> Self {
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
            let x = i as f64;
            effective_attack.push(420.0 + (x % 97.0));
            mitigation_multiplier.push(0.12 + (x % 13.0) * 0.025);
            effective_pierce.push((x % 9.0) * 0.018);
            defense_mitigation_bonus.push((x % 6.0) * 0.01);
            crit_multiplier.push(1.0 + (x % 5.0) * 0.2);
            proc_multiplier.push(1.0 + (x % 4.0) * 0.12);
            isolytic_taken.push((x % 17.0) * 1.5);
            apex_damage_factor.push(0.65 + (x % 7.0) * 0.04);
            shield_mitigation.push(((x % 10.0) * 0.08).clamp(0.0, 0.95));
            defender_shield_remaining.push(80.0 + (x % 111.0) * 2.5);
        }

        Self {
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
        }
    }

    fn as_inputs(&self) -> DamageKernelBatchInputs<'_> {
        DamageKernelBatchInputs {
            effective_attack: &self.effective_attack,
            mitigation_multiplier: &self.mitigation_multiplier,
            effective_pierce: &self.effective_pierce,
            defense_mitigation_bonus: &self.defense_mitigation_bonus,
            crit_multiplier: &self.crit_multiplier,
            proc_multiplier: &self.proc_multiplier,
            isolytic_taken: &self.isolytic_taken,
            apex_damage_factor: &self.apex_damage_factor,
            shield_mitigation: &self.shield_mitigation,
            defender_shield_remaining: &self.defender_shield_remaining,
        }
    }
}

#[derive(Clone)]
struct KernelOutputs {
    damage_after_apex: Vec<f64>,
    shield_damage: Vec<f64>,
    hull_damage: Vec<f64>,
}

impl KernelOutputs {
    fn zeros(len: usize) -> Self {
        Self {
            damage_after_apex: vec![0.0; len],
            shield_damage: vec![0.0; len],
            hull_damage: vec![0.0; len],
        }
    }

    fn as_mut_outputs(&mut self) -> DamageKernelBatchOutputs<'_> {
        DamageKernelBatchOutputs {
            damage_after_apex: &mut self.damage_after_apex,
            shield_damage: &mut self.shield_damage,
            hull_damage: &mut self.hull_damage,
        }
    }
}

fn bench_simd_damage_kernel(c: &mut Criterion) {
    let len = 16_384usize;
    let data = KernelBatchData::synthetic(len);

    let mut group = c.benchmark_group("simd_damage_kernel");
    if std::env::var_os("CI").is_some() {
        group.sample_size(30);
    } else {
        group.sample_size(80);
    }
    group.throughput(Throughput::Elements(len as u64));

    let mut scalar_outputs = KernelOutputs::zeros(len);
    group.bench_function("scalar", |b| {
        b.iter(|| {
            let inputs = data.as_inputs();
            let mut outputs = scalar_outputs.as_mut_outputs();
            compute_damage_kernel_batch_scalar(&inputs, &mut outputs).unwrap();
            black_box(outputs.hull_damage[0]);
        });
    });

    let mut auto_outputs = KernelOutputs::zeros(len);
    group.bench_function("auto_dispatch", |b| {
        b.iter(|| {
            let inputs = data.as_inputs();
            let mut outputs = auto_outputs.as_mut_outputs();
            let path = compute_damage_kernel_batch(&inputs, &mut outputs).unwrap();
            black_box(path);
            black_box(outputs.shield_damage[len - 1]);
        });
    });

    if avx2_supported() {
        let mut avx2_outputs = KernelOutputs::zeros(len);
        group.bench_function("avx2_direct", |b| {
            b.iter(|| {
                let inputs = data.as_inputs();
                let mut outputs = avx2_outputs.as_mut_outputs();
                compute_damage_kernel_batch_avx2(&inputs, &mut outputs).unwrap();
                black_box(outputs.damage_after_apex[len / 2]);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_simd_damage_kernel);
criterion_main!(benches);
