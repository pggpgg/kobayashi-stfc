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
- `**KOBAYASHI_FULLMC_EARLY_STOP=0**`: disable top-K progressive abandonment on the exhaustive full Monte Carlo pass (**on by default** — see below; `=0` restores the plain full pass with byte-identical deep-tail fidelity).
- `**KOBAYASHI_GENETIC_EARLY_STOP=1**`: **opt-in** — enable it on the genetic per-generation full-budget population eval (default off; regresses on tested matchups due to shared-leader lock contention).
- `**KOBAYASHI_EARLYSTOP_MARGIN**` (default `0.01`) / `**KOBAYASHI_EARLYSTOP_MIN_TRIALS_DIV**` (default `8`): tune the abandonment cut margin (blended-score scale) and the per-crew minimum-trials floor (`iterations / div`).

Tiered optimization reuses one `SharedScenarioData` build per phase (`src/optimizer/monte_carlo/scenario.rs`), uses adaptive batch counts via `monte_carlo_batch_count_for_candidates` (`src/parallel/batch.rs`), and runs the scout pass with Wilson-bound early stopping where safe (confirmation pass unchanged).

## Top-K progressive abandonment (exhaustive + genetic) — opt-in, score-aware

The exhaustive full Monte Carlo and the genetic per-generation eval previously simulated **every** candidate to full depth, even crews clearly losing. The scout pass's progressive-abandonment machinery was generalized to a **top-K leaderboard** (`TopKLeader`, `src/optimizer/monte_carlo/simulation.rs`) keyed on the **ranking score**, not win rate:

- The leaderboard sorts finished crews by the same score [`rank_results`] uses — `0.8·win + 0.2·hull` for single fights, primary success rate for chain grinds (`blended_score`). The cut line is the **K-th-best crew's score 95% lower bound**; a running crew whose score upper bound (`0.8·win-Wilson-upper + 0.2·hull-normal-upper`) cannot reach `cut − margin` is abandoned at the next checkpoint. No cut exists until `K` crews finish, and each crew runs ≥ `min_trials` first, so survivors keep full-fidelity stats. The top-1 winner is preserved by construction.
- Exhaustive uses `K = max(tiered_top_k_or_default, 64)`; the genetic path shares **one** leaderboard across all chunks within a generation (`K = max(pop/2, elitism+8, 16)`) so the cut keeps tightening. Correctness: `confirm_topk_preserves_ranking_vs_full_pass`, `chunked_topk_shared_leader_preserves_survivors`, and `TopKLeader` unit tests (un-abandoned crews byte-identical to the no-abandonment baseline; winner unchanged).

### The margin must be scaled to the hull weight

Because hull contributes only **0.2** of the blended score, a score margin of `m` is a **hull gap of `m / 0.2 = 5m`**. The original `0.05` margin therefore demanded a 0.25-hull gap that almost never exists, so on real hull-spread matchups it pruned **nothing**. The default is now **`0.01`** (≈ a 0.05 hull gap, still ~4× the score CI half-width at the first checkpoint → never over-prunes; winner held across the sweep down to 0.002).

### Benchmark (`src/bin/early_stop_bench.rs`, M-series mac, release, 3-rep median)

Where it bites: matchups where a properly-statted ship **wins reliably but takes crew-dependent hull damage** (multi-round fights vs hard hostiles), e.g. maxed Enterprise-D (demo roster, T12 L60) vs hostile `10264305` (win=1.0, hull spread 0.85–1.00). Margin sweep on that scenario:

| margin | pruned | speedup | winner |
|---|---|---|---|
| 0.05 (old default) | 0% | 0.93× | ✅ |
| 0.02 | 6% | 1.02× | ✅ |
| **0.01 (default)** | **10%** | **1.06×** | ✅ |
| 0.005 | 10% | 1.10× | ✅ |

Other scenarios (margin 0.01): expensive borderline `21007889` (44s) → 9% pruned, **1.06×**; Enterprise-D vs `3931453197` (hull 0.92–0.93, *tight*) → 0% pruned, neutral (crews are genuinely near-equal — nothing to prune); saladin one-shot saturated-win → 0% pruned, neutral. Winner identical in every case.

**Where it does *not* help:** dominant one-shot matchups (`win=1, hull=1` for every crew — `avg_hull_remaining` is an overkill ratio clamped to 1.0) and hopeless matchups (`win=0`). There is no spread to exploit, so abandonment correctly prunes nothing and is ~neutral. The win is real but **modest (~5–10%) and confined to the "wins-but-bleeds" regime**, because only the clearly-worst ~10% of crews sit far enough below the cut to prune safely; survivors still run ≥ `min_trials`.

**Determinism caveat (when enabled):** under parallel execution the number of trials an *abandoned* (losing) crew runs depends on completion order, so its low-fidelity tail stats are not bit-reproducible; the reported top-K ranking is. The GA's `ga_run_returns_stable_shape_for_same_seed` guarantee (structural shape only) is unaffected.

**Bottom line:** the score-aware leader at margin 0.01 is the correct, regression-free version and delivers ~5–10% on hard "wins-but-bleeds" PvE while preserving the top-K ranking, so it is **on by default for the exhaustive path** (`KOBAYASHI_FULLMC_EARLY_STOP=0` to opt out for byte-identical deep-tail fidelity). The **genetic** path stays opt-in — it still regresses from shared-leader lock contention with little pruning. For broad optimizer-speed wins on this workload the bigger lever remains **candidate-space reduction** (analytical prefilter; `docs/PVE_CREW_SEARCH_SPACE_REDUCTION.md`). Not yet wired (same primitive applies): the tiered **confirmation** pass and the **batched** exhaustive/multi-hostile paths.

## Tier 5: resolve-cache fix for the analytical prefilter (2026-06)

Profiling a real `optimize --ship saladin --hostile 2918121098` run (macOS `sample` on a `[profile.profiling]` build) showed **~85% of samples in JSON file loading + serde deserialization**, not the combat loop. Root cause: the **analytical prefilter** sorts ~10^5 candidates, and the fallback scenario-build path (`scenario_to_combat_input_from_shared` → `computed_defender_mitigation`) called the free `resolve_hostile` / `resolve_ship`, each of which **re-read and re-parsed the ~1 MB `data/hostiles/index.json` from disk on every call** (the per-record LRU in `data_registry.rs` does not cover these free functions).

Fix: process-wide memoization of `resolve_hostile` and `resolve_ship_with_tier_level` keyed by lookup args (`src/data/loader.rs`, `RwLock<HashMap<…>>`, same static-data assumption as the record LRU). The first call for a key parses the index once; subsequent calls are an O(1) lookup + a small record clone.

- **Profile:** data-load/serde self-time **85.4% → 4.2%** of samples.
- **End-to-end wall clock** (same `optimize`, `--sims 4000`, profiling build): **~723 s → ~15.7 s** (~46× on this workload, which is prefilter-dominated). Steady-state per-combat throughput (the Criterion `simulator/*` and `monte_carlo/*` gate) is unaffected — this was wasted setup work, not inner-loop cost.