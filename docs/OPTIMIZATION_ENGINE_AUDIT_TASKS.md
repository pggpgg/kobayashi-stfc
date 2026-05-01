# Optimization Engine Audit — Development Tasks

Tasks identified from a full audit of `src/optimizer/`, `src/combat/`, `src/parallel/`, and
`src/data/`. These focus on **discovering good crews faster**, **avoiding predictably bad
crew combinations**, **simulating more combat per second**, and **making better use of
compute resources** to increase optimization throughput.

Each task includes: **what is wrong**, **what mechanic is affected**, **what code to
change**, and **uncertainty remaining**.

---

## Phase 1 — Foundation (measurement and caching infrastructure)

- [x] **1. Add per-run simulation result cache keyed by crew hash**

  **What is wrong:** Identical crew compositions are re-simulated from scratch every time
  they appear. The `preconfirmed` map in `run_exhaustive_scout_then_full_mc` and
  `run_tiered_with_registry_with_progress` provides a per-call cache mechanism, but there
  is no cross-run or cross-scenario persistence. A crew that was simulated in a prior
  optimization run, or appears in multiple hostile matchups within the same run, gets a
  fresh batch of Monte Carlo trials each time.

  **What code to change:**
  - Add a `HashMap<u64, SimulationResult>` to the optimization session (keyed by
    `crew_candidate_stable_hash`) that persists across all hostiles in a multi-hostile
    run.
  - Hook into `run_monte_carlo_with_shared` and `run_tiered_with_registry_with_progress`
    to check the cache before launching trials.
  - Optionally: serialize the cache to disk (e.g., `~/.kobayashi/sim_cache/`) keyed by
    `{ship}_{hostile_faction}_{crew_hash}` so cold-start runs benefit from prior
    optimizations.
  - Ensure the cache is invalidated when officer data, combat formulas, or ship stats
    change (hash of relevant data files as cache key prefix).

  **Uncertainty:** Low for the in-process cache (proven pattern from `preconfirmed`).
  Medium for on-disk persistence — cache invalidation correctness is critical.

- [x] **2. Add crew enumeration cache for officer pools**

  **What is wrong:** `CrewGenerator::generate_candidates_from_pools` rebuilds officer
  pools from scratch on every call, including loading officers from disk, filtering by
  roster, and applying constraints. The pools are rebuilt even for repeated calls with the
  same ship/hostile/seed combination. The `DataRegistry` path (`generate_candidates_from_registry`)
  avoids disk reload but still re-filters pools on every call.

  **What code to change:**
  - Add a `lru::LruCache<(String, String, u64), OfficerPools>` to `CrewGenerator`
    (key: `(ship, hostile, seed)`).
  - Cache the narrowed pools (post-`narrow_officer_pools_for_constraints`) since
    constraints are part of the strategy.
  - Clear cache entries when `CandidateStrategy` or roster data changes.
  - Consider a two-level cache: pool narrowing is cheap, candidate enumeration is
    expensive — cache candidate lists for small pools.

  **Uncertainty:** Low. LruCache is straightforward. Watch memory for very large officer
  rosters (hundreds of officers × multiple hostiles).

---

## Phase 2 — Smarter Pruning (drop bad crews before expensive simulation)

- [x] **3. Wire analytical expected-damage prefilter into the main optimization path**

  **What is wrong:** `src/optimizer/analytical.rs` contains a closed-form `expected_damage()`
  function that estimates total hull damage from static combatant stats (ignoring per-round
  abilities, crit variance, defender return fire). This function is **not used** in the
  main optimization pipeline — every generated crew proceeds directly to Monte Carlo
  simulation, even if it deals zero damage against a high-mitigation hostile.

  **What code to change:**
  - In `run_tiered_with_registry_with_progress` (and `run_exhaustive_scout_then_full_mc`),
    compute `expected_damage()` for each candidate before launching scout trials.
  - Drop candidates whose `expected_damage()` falls below a configurable fraction
    of the defender's hull (e.g., `< 0.05 × defender.hull_health`). These crews can't
    kill the hostile in any realistic number of rounds.
  - Add a heuristic: if `expected_damage < current_best_crew_expected_damage * 0.5`,
    deprioritize (simulate last).
  - Expose a `prune_analytical_threshold` option in the optimize API so callers can
    control aggressiveness.

  **Uncertainty:** Low. The analytical formula intentionally underestimates (ignores
  crit/proc variance, burning, morale) so it produces false negatives (good crews
  scored low) rather than false positives. A conservative threshold (5% of defender hull)
  should eliminate only truly hopeless crews.

- [x] **4. Add static-gate prefilter: drop crews whose abilities are conditional on the wrong enemy type**

  **What is wrong:** Crews whose officer abilities are gated on conditions that do not
  match the current hostile (e.g., `DefenderFactionIs(Romulan)` against a Swarm hostile)
  waste simulation budget. `src/optimizer/matchup_priors.rs` already evaluates static
  gates (`DefenderShipTypeIs`, `DefenderFactionIs`, `DefenderIsNpcHostile`, etc.) but
  is only used for tie-breaking, not for dropping candidates.

  **What code to change:**
  - In the analytical prefilter path (alongside task 3), call the static-gate evaluator
    for each officer ability on each candidate.
  - Compute a "gated ability fraction" — what portion of the crew's total ability
    contribution is locked behind mismatched conditions.
  - Drop candidates where ≥75% of ability effects (by count or by value) are gated
    behind conditions that statically evaluate to `Fail`.
  - This is especially effective for faction-locked officers (Borg, Cardassian, etc.).

  **Uncertainty:** Low for the gate evaluation (proven in `matchup_priors.rs`). Medium
  for the threshold — 75% might be too aggressive in edge cases where one ungated
  ability is strong enough to carry the crew. Start conservative (95%) and tighten.

- [x] **5. Implement progressive abandonment in Monte Carlo: dynamically cut crews that can't catch the leader**

  **What is wrong:** The current scout early-stop mechanism (`ScoutEarlyStopCfg`) only
  eliminates crews whose win-rate Wilson upper bound falls below a fixed `0.055` threshold.
  It does not consider the **best crew's performance so far** — a crew stuck at 45% win
  rate could still be simulated for hundreds of trials even though the current leader is
  at 98% with tight confidence.

  **What code to change:**
  - In `run_candidate_monte_carlo`, after each checkpoint, compare the candidate's Wilson
    upper bound against the **best crew's Wilson lower bound**.
  - If `candidate.win_rate_upper_95 < best.win_rate_lower_95` and the gap exceeds a
    configurable margin (e.g., 5-10%), terminate this candidate early.
  - This requires the Monte Carlo runner to have visibility into other candidates'
    results. Refactor to pass a shared `&AtomicRefCell<BestSoFar>` or use a
    coordinator pattern.
  - Apply this in both scout and confirmation phases.

  **Uncertainty:** Medium. The coordinator pattern adds complexity to the parallel
  execution model. The trade-off: more coordination overhead vs fewer wasted trials.
  Worth measuring — on a 64-core machine with 5000 candidates, even a 10% reduction
  in scout trials saves minutes.

---

## Phase 3 — Throughput (more simulations per second)

- [x] **6. Batch-process multiple combat trials per simulation call (shared setup, amortized dispatch)**

  **What is wrong:** Each Monte Carlo trial calls `simulate_combat_with_defender_faction_and_defender_crew`
  individually. This means combatant setup, `EffectAccumulator` construction, and crew
  resolution happen N times per candidate. For 500 scout trials × 128 candidates, that's
  64,000 separate calls to `simulate_combat`.

  **What code to change:**
  - Add a `simulate_combat_batch()` function that takes one `Combatant` pair + crew and
    runs M trials with different seeds, returning M results in one call.
  - Pre-compute the immutable parts of the combat setup (crew configuration, effect
    pre-filtering by timing window) once, then run the round loop M times with per-trial
    RNG state.
  - Refactor `run_candidate_monte_carlo` to use batched trials (e.g., batches of 32 or 64).
  - This also reduces Rayon task overhead: fewer parallel units with bigger internal loops
    amortize thread synchronization.

  **Uncertainty:** Low for correctness (same engine, just looped). Medium for throughput
  gain — the round loop is the dominant cost, so batch savings come from setup amortization
  and reduced Rayon overhead. Estimate 5-20% improvement on large workloads.

- [x] **7. Implement SIMD/vectorized damage kernel for the per-shot hot path**

  **What is wrong:** The round loop in `engine.rs` processes each weapon shot individually,
  computing mitigation, crit, damage-through, apex, and isolytic damage per shot. For
  ships with multiple weapons and high shot counts, this is the hottest loop in the
  entire system. `src/combat/simd_damage_kernel.rs` already exists with AVX2 support
  (`avx2_supported()`, `compute_damage_after_apex_batch()`) but the main engine does not
  use it — the batch kernel is gated behind an experimental flag.

  **What code to change:**
  - Wire `compute_damage_after_apex_batch()` into the per-weapon shot loop when AVX2
    is available and the experimental flag is on.
  - Process shots in chunks of 8 (or 4 for SSE) where mitigation/crit/proc values are
    uniform across the chunk.
  - Add a portable scalar fallback that groups shots with identical parameters (same
    mitigation, same crit state, same shield state).
  - Measure with `cargo bench` to confirm improvement before enabling by default.

  **Uncertainty:** Medium. The existing SIMD kernel handles only the damage-after-apex
  step. Full SIMD-ization of the shot loop requires refactoring mitigation-through,
  crit resolution, and shield/hull split into batch-friendly forms. Start with
  shots-where-shield-is-zero (common late-fight case) for highest impact.

- [ ] **8. Add a priority-queue Monte Carlo scheduler: promising crews get more trials sooner**

  **What is wrong:** In both tiered and exhaustive modes, all crews receive the same
  scout simulation budget (e.g., 500 trials) regardless of how they perform. The top-K
  selection happens AFTER all crews have been scouted. This wastes trials on obviously
  bad crews while the best crews wait for confirmation.

  **What code to change:**
  - Implement a `PriorityMonteCarloRunner` that maintains a min-heap of candidates
    ordered by "expected rank" (current Wilson upper bound).
  - Allocate trials in rounds: each round, pop the top N candidates from the heap,
    run B more trials on each, push back.
  - Candidates whose Wilson upper bound falls below the current K-th best's lower
    bound are dropped from the heap (never get more trials).
  - The confirmation phase then runs only on crews that survived the priority queue.
  - This generalizes both scout early-stop and progressive abandonment.

  **Uncertainty:** Medium. The heap-based scheduling adds coordination overhead and
  makes parallel execution trickier (need to batch heap operations). Worth implementing
  after tasks 1, 3, and 5 since it builds on the same infrastructure.

---

## Phase 4 — Genetic Algorithm Efficiency

- [ ] **9. Add incremental fitness evaluation in the genetic algorithm**

  **What is wrong:** `run_genetic_monte_carlo` evaluates every individual in every
  generation with fresh Monte Carlo trials, even when the individual survived from the
  previous generation unchanged (elitism). A crew that was the elite individual in
  generation N gets re-evaluated in generation N+1 with the same sim budget, wasting
  trials.

  **What code to change:**
  - Track which individuals are carried over via elitism (identified by crew hash).
  - For carried-over individuals, skip re-evaluation and reuse the previous
    `SimulationResult`.
  - For mutated individuals, run a reduced sim budget initially (e.g., 25% of
    `sims_per_eval`) to get a rough fitness estimate, then commit the full budget
    only if the rough estimate exceeds the elite's fitness.
  - For crossover individuals, use an analytical proxy score to decide sim budget
    (good parents' offspring likely benefits from more trials).

  **Uncertainty:** Low. The hash tracking is straightforward. The reduced-budget
  heuristic for offspring may introduce noise if the 25% estimate is too coarse —
  measure false-negative rate (good offspring prematurely rejected) before enabling.

- [ ] **10. Add learning-based warm start: weight officer selection by historical performance**

  **What is wrong:** The `sampled_candidates` function in `crew_generator.rs` samples
  below-decks officers with a fixed stride, independent of which officers have historically
  performed well in similar matchups. An officer that has never appeared in a top-10 crew
  gets the same sampling probability as one that appears in 90% of top crews.

  **What code to change:**
  - Maintain a per-officer performance score derived from optimization history (e.g.,
    how many top-K results include this officer, weighted by recency and hostile type).
  - In `sampled_candidates`, use weighted sampling (probability proportional to score)
    instead of stride-based sampling for below-decks officers.
  - Apply a Thompson sampling or epsilon-greedy approach: 80% of the time sample from
    the top-scoring officers, 20% explore uniformly.
  - Store scores per `(officer_id, hostile_faction, ship_type)` tuple so the system
    learns which officers work against which enemy types.
  - This is especially impactful for large rosters (50+ below-decks officers) where
    stride-based sampling can miss entire officer clusters.

  **Uncertainty:** High. This changes the fundamental crew generation strategy from
  uniform to biased, which could hurt discovery of novel crew combinations. The
  exploration rate (20% uniform) should be tunable. Recommend A/B testing against the
  current uniform sampler before enabling by default.
