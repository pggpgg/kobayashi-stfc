# Sim performance (after efficiency improvements)

Benchmarks were run after implementing the sim efficiency plan (lazy trace, pre-compute effects, EffectAccumulator reuse, Monte Carlo shared cache, SplitMix64 RNG).

## Regression gate (Criterion)

CI (`[.github/workflows/benchmark-regression.yml](../.github/workflows/benchmark-regression.yml)`) runs Criterion on `**ubuntu-24.04**` with `**CI=true**` and `**KOBAYASHI_RAYON_THREADS=2**`, then `[cargo xtask bench-check](../xtask/src/main.rs)` compares medians to the committed `[benchmark_results.log](../benchmark_results.log)`.

- **Rule:** for every benchmark id listed in the log, the current run fails if `median_ns > baseline_median × 1.10` (strictly more than 10% slower). The set of ids in the log must match the set under `target/criterion/**/new/estimates.json`.
- **Fixed seeds:** simulator benches use `SimulationConfig::seed = 7` (`[benches/simulator_bench.rs](../benches/simulator_bench.rs)`). Monte Carlo benches use `seed = 42` with ship `saladin` / hostile `2918121098` (`[benches/monte_carlo_parallel_bench.rs](../benches/monte_carlo_parallel_bench.rs)`).
- **Refreshing the baseline:** after a `**v*`** tag push, the Release workflow’s Linux job uploads `benchmark_results.fresh.log` as artifact `**benchmark-results-log**` and publishes it on the GitHub Release as `**benchmark_results.log**`. Copy that file to the repo root (replace `[benchmark_results.log](../benchmark_results.log)`), adjust the `#` header comments if needed, and open a PR to `main`.
- **Local refresh (same env as CI):** from repo root:

```bash
export CI=true
export KOBAYASHI_RAYON_THREADS=2
cargo bench --bench simulator --bench monte_carlo_parallel --bench simd_damage_kernel -- --noplot
cargo xtask bench-check --write-baseline
```

- **Noise:** shared GitHub runners vary; if the gate flakes, re-run the workflow or refresh the log from a fresh Linux release artifact. Prefer `**ubuntu-24.04`** for the committed numbers so they match the gate runner.

## benchmark_parallel_speedup (64 candidates × 1000 iterations)


| Mode       | Time      | Throughput     |
| ---------- | --------- | -------------- |
| Sequential | 532.99 ms | 120,077 sims/s |
| Parallel   | 124.74 ms | 513,081 sims/s |
| Speedup    | 4.27×     | —              |


## cargo bench (criterion)

- **simulator/combat_3_rounds**: ~~1.39 µs per combat (~~720k combats/s)
- **simulator/combat_20_rounds**: ~~1.37 µs per combat (~~730k combats/s)
- **simulator/combat_100_rounds**: ~~1.24 µs per combat (~~809k combats/s)
- **monte_carlo/sequential**: 745–761 ms (criterion reported **~68% faster** vs previous run)
- **monte_carlo/parallel**: 143–148 ms (criterion reported **~44% faster** vs previous run)

## benchmark_simulator --log (100 rounds/combat, 2 s run)

- **Combats/s**: 829,736
- **Rounds/s**: 82,973,621

## benchmark_log.csv trend

Throughput from `cargo run --release --bin benchmark_simulator` (100 rounds/combat, ~2 s wall clock after `MIN_DURATION_MS` / `MIN_COMBATS` — see `[src/bin/benchmark_simulator.rs](../src/bin/benchmark_simulator.rs)`). Use `benchmark_simulator --log` to append a row to `benchmark_log.csv` (file is gitignored).


| Date       | Combats/s   |
| ---------- | ----------- |
| 2026-02-22 | 4,091       |
| 2026-02-23 | 326,119     |
| 2026-02-25 | 354,581     |
| 2026-02-26 | **829,736** |
| 2026-04-24 | **443,131** |


Latest logged row (**2026-04-24**): **443,131** combats/s on the machine used for this doc update. Earlier rows mix different hardware and code generations, so treat the table as a dated journal rather than a strict regression chart.

## Conclusion

Yes, the sim runs faster with these fixes. Criterion reported significant improvement (44–68% faster), and the raw throughput (120k seq / 513k parallel sims/s in the parallel benchmark, ~830k combats/s in the single-combat benchmark) reflects the reduced allocations and shared scenario caching.

## Runtime tuning (optimizer / Rayon)

- `**KOBAYASHI_RAYON_THREADS`**: positive integer → use a Rayon pool with that many worker threads for code paths that use `WorkerPool::install` (`src/parallel/pool.rs`; default remains “all cores” when unset or `0`).
- `**KOBAYASHI_PERF_LOG=1`**: logs wall-clock for crew generation and full Monte Carlo batches with shared scenario data (stderr); zero overhead when unset.
- `**KOBAYASHI_EXPERIMENTAL_SIMD_DAMAGE_KERNEL=1**`: when AVX2 is available and combat trace is off, batches per-hit `damage_after_apex` math (4-wide) for outbound shots and defender counter-fire in `src/combat/engine.rs` via `simd_damage_kernel` (experimental; keep off by default while measuring).

Tiered optimization reuses one `SharedScenarioData` build per phase (`src/optimizer/monte_carlo/scenario.rs`), uses adaptive batch counts via `monte_carlo_batch_count_for_candidates` (`src/parallel/batch.rs`), and runs the scout pass with Wilson-bound early stopping where safe (confirmation pass unchanged).