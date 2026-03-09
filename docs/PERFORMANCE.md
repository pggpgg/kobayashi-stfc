# Sim performance (after efficiency improvements)

Benchmarks were run after implementing the sim efficiency plan (lazy trace, pre-compute effects, EffectAccumulator reuse, Monte Carlo shared cache, SplitMix64 RNG).

## benchmark_parallel_speedup (64 candidates × 1000 iterations)

| Mode       | Time     | Throughput   |
|-----------|----------|--------------|
| Sequential| 532.99 ms| 120,077 sims/s|
| Parallel  | 124.74 ms| 513,081 sims/s|
| Speedup   | 4.27×    | —            |

## cargo bench (criterion)

- **simulator/combat_3_rounds**: ~1.39 µs per combat (~720k combats/s)
- **simulator/combat_20_rounds**: ~1.37 µs per combat (~730k combats/s)
- **simulator/combat_100_rounds**: ~1.24 µs per combat (~809k combats/s)
- **monte_carlo/sequential**: 745–761 ms (criterion reported **~68% faster** vs previous run)
- **monte_carlo/parallel**: 143–148 ms (criterion reported **~44% faster** vs previous run)

## benchmark_simulator --log (100 rounds/combat, 2 s run)

- **Combats/s**: 829,736
- **Rounds/s**: 82,973,621

## benchmark_log.csv trend

| Date       | Combats/s  |
|------------|------------|
| 2026-02-22 | 4,091      |
| 2026-02-23 | 326,119    |
| 2026-02-25 | 354,581    |
| 2026-02-26 | **829,736**|

The latest run (after all fixes) is ~2.3× the previous best in this log and ~200× the earliest entry.

## Conclusion

Yes, the sim runs faster with these fixes. Criterion reported significant improvement (44–68% faster), and the raw throughput (120k seq / 513k parallel sims/s in the parallel benchmark, ~830k combats/s in the single-combat benchmark) reflects the reduced allocations and shared scenario caching.
