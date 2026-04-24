# Additional Development Tasks (Post-Roadmap)

New tasks below are intentionally outside the work already tracked in `docs/ROADMAP.md`, and are ordered in a practical build sequence: guardrails -> correctness evidence -> performance execution -> UX -> operations.

## 1) Foundations and guardrails

- [ ] **1. Add a deterministic scenario snapshot format**
  - Persist fully-resolved simulation inputs (ship stats, crew effects, buffs, hostile state, seed) to a single JSON artifact for reproducible debugging.
- [ ] **2. Introduce a scenario snapshot loader CLI**
  - Add a CLI entry point to run a simulation directly from a saved snapshot artifact and verify parity with live pipeline output.
- [ ] **3. Add profile merge precedence golden tests**
  - Lock down merge order and override rules across synced data, manual profile JSON, support buffs, and static bonuses using fixture-based tests.
- [ ] **4. Add property-based tests for combat stacking invariants**
  - Use randomized inputs to validate operator order, cap behavior, and monotonicity constraints in core stacking and mitigation math.

## 2) Correctness evidence and explainability

- [ ] **5. Build a combat trace explain mode**
  - Add an optional trace payload that explains per-round stat deltas and damage contributors without changing core combat outcomes.
- [ ] **6. Add ability-level contribution attribution**
  - Aggregate and surface per-ability contribution totals (damage gained, mitigation gained, survivability impact) for post-fight analysis.
- [ ] **7. Create an assumptions registry for approximate mechanics**
  - Add a machine-readable assumptions file tied to tests so uncertain mechanics are explicit, searchable, and versioned.
- [ ] **8. Add hostile and officer modeling coverage dashboards**
  - Generate docs artifacts that show which entities have high-fidelity modeling, partial approximations, or no combat impact coverage.
- [ ] **9. Add parity tests for optimizer request variants**
  - Assert equivalent outcomes across sync and standalone execution paths for identical logical scenarios and constraints.
- [ ] **10. Add regression fixtures for support-buff interactions**
  - Create targeted fixtures that validate additive/multiplicative interaction boundaries and exclusivity-group conflict resolution.

## 3) Runtime execution and throughput

- [x] **11. Add SIMD feasibility prototype for hot combat math** *(shipped: `src/combat/simd_damage_kernel.rs` scalar + AVX2 paths with parity tests; Criterion bench `benches/simd_damage_kernel_bench.rs`, registered in `Cargo.toml` as `simd_damage_kernel`; experimental engine integration via `KOBAYASHI_EXPERIMENTAL_SIMD_DAMAGE_KERNEL=1` in outbound per-hit damage application.)*
  - Prototype SIMD acceleration in isolated hot-path kernels and compare accuracy/perf against current scalar implementation.
- [ ] **12. Introduce allocator and cache-efficiency profiling harness**
  - Add repeatable profiling scripts and reports focused on allocations, cache misses, and branch misprediction in simulation loops.
- [x] **13. Improve cancellation responsiveness for long optimize jobs** *(shipped: batched heuristic MC with cancel between batches in `gather_optimize_simulation_results`; `eval_should_continue` + exhaustive batch gates and genetic `run_monte_carlo_parallel_deduped_chunked` between unique chunks; GA skips final full-sim MC when aborted; tiered confirm MC split with `on_progress` between slices; tests in `genetic.rs` / `monte_carlo/simulation.rs`.)*
  - Add cooperative cancellation checkpoints so stop requests are honored quickly during large candidate evaluations.
- [ ] **14. Add backpressure-aware SSE progress publishing**
  - Prevent progress stream pressure from degrading optimize throughput by coalescing or sampling progress updates.
- [ ] **15. Add API-level latency budgets and SLO tests**
  - Create route-specific latency targets with automated tests that fail when p95 exceeds guardrail thresholds.

## 4) UX and workflow quality

- [ ] **16. Add a simulation diff view in the frontend**
  - Provide side-by-side outcome deltas for two crews, including key stat swings and round-window impact summaries.
- [ ] **17. Add preset validation and repair UX**
  - Detect stale officer/ship/hostile references in saved presets and offer guided repair actions instead of hard failures.
- [ ] **18. Add first-class reproducibility export/import in UI**
  - Let users export a run as a reproducible snapshot and import it later for replay, sharing, or bug reports.

## 5) Operations and release confidence

- [ ] **19. Add a diagnostics bundle command**
  - Package profile-safe logs, environment metadata, version hashes, and reproducible scenario snapshots into a support artifact.
- [ ] **20. Add release-readiness quality gates for simulation confidence**
  - Require a minimum suite of calibration, parity, and performance checks to pass before tagging a release.
