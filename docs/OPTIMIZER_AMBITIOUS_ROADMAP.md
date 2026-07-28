# Ambitious Optimizer Roadmap

Future work for the next generation of Kobayashi's crew optimizer. This roadmap focuses on capabilities that have not been built yet: better finalist refinement, richer recommendations, explicit discovery lanes, adaptive simulation budgets, and eventually learned search that remains subordinate to the simulator.

Current optimizer behavior belongs in [DESIGN.md](DESIGN.md), [PERFORMANCE.md](PERFORMANCE.md), and [CREW_OPTIMIZATION_METHODS.md](CREW_OPTIMIZATION_METHODS.md). Completed implementation details should not accumulate here.

_Last updated 2026-07-28._

## North Star

Kobayashi should become a multi-stage, self-auditing search engine:

```text
profile + ship + hostile
  -> exact role pools and legality funnel
  -> proxy ranking and independent search lanes
  -> diverse discovery: random, genetic, beam, archive, local repair
  -> adaptive simulation budget: racing, confidence bounds, seed panels
  -> high-depth confirmation
  -> Pareto recommendations, explanations, and substitute crews
  -> durable observations for future learned search
```

The product experience should remain simple: ask for a crew and get several trustworthy choices, each with a reason. Under the hood, Kobayashi should know which method found each crew, how much evidence supports it, and whether a cheaper method would have found the same answer.

## Principles

1. **Exact filters may delete. Soft heuristics may only prioritize.**
   Eligibility, duplicate prevention, seat legality, owned roster, and impossible conditions are hard gates. Proxy scores, archetypes, learned models, and meta assumptions stay as ordering, sampling, or budget-allocation signals unless proven exact.

2. **Every search method needs a simple control.**
   New lanes must be compared with stratified random, tiered, genetic, analytical, and warm-start baselines under equal budgets.

3. **Simulation remains the judge.**
   Surrogates, beam scores, and Bayesian proposals may nominate crews. They do not crown winners.

4. **Diversity is product quality.**
   Users need fast, safe, low-variance, accessible, and matchup-specific crews. Returning near-identical variants of one winner is usually worse than returning a real menu of strong alternatives.

5. **Telemetry comes before tuning.**
   Do not tune beam widths, prune thresholds, mutation rates, or learned models without enough stage-level evidence to measure the effect.

## Phase 1: Better Recommendations

These are the highest-confidence improvements because they reuse existing finalists, role pools, simulation outputs, and legality machinery.

### 1.1 Local Refinement and Large-Neighborhood Repair

Search around tiered and genetic finalists using:

- one-slot bridge swaps
- one-slot below-decks swaps
- captain swaps that preserve compatible support officers
- trigger-producer or trigger-consumer replacement
- destroy-repair neighborhoods that remove two or three officers and rebuild from high-fit compatible pools

Use scout depth first, then confirm only improvements and diverse near-ties. Preserve the source crew and changed slots so the UI can explain how refinement improved the recommendation.

Method labels:

- `local_swap`
- `local_captain_swap`
- `large_neighborhood_repair`

**Status: landed** — [`src/optimizer/refinement.rs`](../src/optimizer/refinement.rs), opt-in via `local_refinement` on the optimize request, wired into the **tiered and genetic** paths. Covers one-slot bridge/below-decks swaps, captain swaps preserving support officers, and two-slot destroy-repair, with scout-then-confirm budgeting, cooperative cancellation, and the three method labels above. Refined rows expose a `refinement` object (source crew, changed seats, baseline/refined score, measured gain) that the results table renders; the pass reports its own budget/effect counters so a pass that accepted nothing is distinguishable from one that never ran. See [CREW_OPTIMIZATION_METHODS.md §10](CREW_OPTIMIZATION_METHODS.md) for behavior.

Still open on this item: trigger-producer/consumer-targeted replacement as its own neighborhood, and three-slot destroy-repair.

### 1.2 Pareto Frontier Recommendations

Compute Pareto fronts over metrics already produced by simulation:

- win, loss, and stall rates
- round-1 kill rate
- average hull remaining
- average defender hull remaining
- confidence interval width
- chain-grind utility
- roster accessibility or rarity cost

Keep the scalar score as the default sort while adding `pareto_tags` and `recommendation_reason` to result rows. Initial user-facing views should cover safest, fastest farming, best chain crew, best substitute, and most different competitive crew.

**Status: landed** — [`src/optimizer/pareto.rs`](../src/optimizer/pareto.rs), applied to every optimize response (no request flag: the pass reads metrics simulation already produced and costs no trials). Rows carry `pareto_tags` — `pareto_optimal`, plus at most one row each of `safest`, `fastest_farming`, `best_chain`, `most_different` — and a `recommendation_reason` that includes a confirmation-depth caveat when the row was simulated less deeply than the table's deepest row. Ordering is untouched. The SPA renders a **Why** badge column with the reason as tooltip. See [CREW_OPTIMIZATION_METHODS.md §11](CREW_OPTIMIZATION_METHODS.md) for the objective set and the three tie/spread rules that keep the front small.

Still open on this item: the **best-substitute** and roster-accessibility views, which need officer rarity and ownership these rows do not carry (see §4.2); consistency/variance as an objective; and per-view filter chips in the SPA rather than badges alone.

### 1.3 Evidence and Provenance Completion

Make each recommendation auditable from input through confirmation:

- preserve every proposing method, not only the final method label
- distinguish discovery, promotion, and confirmation stages
- fingerprint the simulator, officer catalog, profile, ship components, hostile, support buffs, and seed panel
- expose evidence level, uncertainty reason, and confirmation depth in API results
- retain enough local-refinement context to explain changed officers and measured gain

Harden durable observation storage with retention, compaction, schema migration, and offline inspection tools. Observation reuse must be fingerprint-safe and must never cross incompatible simulator or data versions.

**Status: fingerprint-safe reuse landed** — [`src/data/optimize_fingerprint.rs`](../src/data/optimize_fingerprint.rs). A `ReuseFingerprint` carries four independent segments — `engine` (a synthetic canary-fight digest plus combat-affecting env values and `COMBAT_ENGINE_BEHAVIOR_VERSION`), `data` (LCARS officer model, eligibility matrix, research/support-buff/forbidden-tech catalogs, ship/hostile index versions, hostile ability catalog), `profile` (contents of every combat-relevant file under `profiles/{id}/`), and `scenario` (the **resolved** `ShipRecord` and `HostileRecord`, buff selections, PvP defender) — so a mismatch is attributable rather than just fatal.

Stored **metrics** are refused unless all four match: `preconfirmed_for_candidates`, `preconfirmed_for_exhaustive_two_phase`, and the learning-signal auto-tuner (which derived confirm-budget policy from stored Wilson intervals). Absent fingerprints fail closed, so pre-fingerprint entries and non-server callers never reuse. Crew **identities** stay ungated for matchup priors and novelty anchors — a good crew composition survives an engine fix, and those crews are already re-validated against the live catalog and roster. Officer-learning scores are name-keyed, so they reset on a `data` change only.

Refusals are visible: `scenario.optimize_history_reuse_refused{,_component}` plus `optimize_reuse_fingerprint` on the optimize response, an `approximate_notes` line, and a "Saved results ignored" badge in the SPA. Observation and budget-telemetry JSONL logs rotate at a size cap (two generations), rows carry the fingerprint and a real simulator identity, and `kobayashi observations [--stale-only] [--summary] [--json]` inspects a log read-only, flagging rows whose fingerprint no longer matches the current build. The migration is non-destructive: `OPTIMIZE_HISTORY_SCHEMA` stays at 2 and old entries age out through the existing LRU.

Still open on this item: preserving **every** proposing method rather than the single collapsed label, the discovery/promotion/confirmation stage model, explicit `evidence_level` / `uncertainty_reason` / `confirmation_depth` fields (`trials_run` is today's de-facto depth signal), seed-panel identity (there are no shared seed panels yet — see §5.2), and per-record schema migration for observation rows (the reader tolerates both schemas instead).

### 1.4 Controlled Benchmark Expansion

Extend optimizer benchmarks with:

- equal wall-clock and equal-trial budget modes
- deeper reference sweeps on tractable search spaces
- top-K recall and prefilter false-negative scoring
- diversity and stability metrics across seed panels
- CI artifacts and regression thresholds
- method-specific subphase timings for every new lane

Benchmarks should answer both "did this method find a stronger crew?" and "did it add a useful recommendation family?"

**Status: landed** — [`src/optimizer/method_bench.rs`](../src/optimizer/method_bench.rs) holds the scoring; `optimizer_method_bench` runs the lanes; `cargo xtask optimizer-bench-check` gates against [`optimizer_method_bench_baseline.json`](../optimizer_method_bench_baseline.json); `.github/workflows/optimizer-method-bench.yml` runs it weekly and uploads the JSONL + Markdown. Covers `--budget-mode native|equal-trials|equal-wall-clock` (wall-clock fits `ms = fixed + trials/rate` from two probe sizes, because a single probe charges setup to every trial and lands 50–90% under target), a deep reference sweep with top-K recall, prefilter false-negative scoring at chosen keep values, and per-(case, method) seed-panel stability. Regret is measured on an independent confirmation seed over the reference top-K plus every lane's winner, so the winner's curse does not reward whichever lane looked at the most crews. The gate enforces a recall floor, a regret ceiling, and a control-ordering rule against stratified random. Output is bit-deterministic per seed across thread counts. See [CREW_OPTIMIZATION_METHODS.md §16](CREW_OPTIMIZATION_METHODS.md).

Three findings came out of building it, each an open item rather than a solved one:

1. **The committed `saladin_numeric` case was measuring nothing.** `saladin` is not a ship id (`uss_saladin` is), and an unresolved ship or hostile id does not fail — it produces a fight the crew wins in round 1, every time, at any hostile level. The case is replaced by `saladin_corvus` (`uss_saladin` vs `1140710508`, a matchup where the ranking score actually separates crews) and the harness now refuses a case whose ids do not resolve. **The silent fallback itself is not fixed** and is not confined to the bench: any API or CLI caller that typos a ship or hostile id gets a 100%-win-rate answer instead of an error.
2. **Stratified random beats tiered on this case at equal budget** — 0.0 mean score regret against tiered's ~0.0063 — because the crews random samples lie outside the narrowed proposal space `CrewGenerator` enumerates (`lane_best_crew_in_reference_set` is false for every random winner). Principle 2 says a lane that cannot beat random is not earning its complexity; the baseline's `control_margin` currently tolerates the gap so the gate still catches further regressions, and closing it means widening what the generator proposes, not tuning tiered.
3. **PvE win rate rarely discriminates.** Across hostile levels the demo profile either wins every fight or loses every fight, with the cliff between levels 52 and 60; only the composite ranking score separates crews. Any future metric built on win rate alone will measure tie-break noise.

Still open on this item: method-specific subphase timings (lane records carry total elapsed plus tiered scout/confirm trial counts, not a per-phase breakdown); the diversity metrics are computed per run but not yet aggregated across the seed panel; and the harness ships two cases on one profile, so it cannot yet show that a change generalizes across matchups.

### 1.5 Richer Random Exploration Strata

Add ability and archetype tags to random exploration strata, then evaluate whether a small default-on exploration share improves recall without materially increasing latency. Keep this lane independent enough to detect blind spots shared by analytical, tiered, and genetic search.

## Phase 2: Explicit Discovery Lanes

Turn search strategy into a portfolio of independent candidate proposers that feed a shared simulation and confirmation pipeline.

### 2.1 Beam Search With Diversity Lanes

Build crews slot by slot and retain partial candidates in separate beams:

- damage
- survivability
- round-1 kill
- long-fight sustain
- anti-dodge and high-accuracy
- shield-bypass and isolytic
- low-rarity or owned-roster substitutes
- per-captain family

Score partial crews conservatively using analytical expected damage, static gate matches, bridge synergy, profile headroom, and a diversity penalty. Beam output should feed scout, genetic seeds, local refinement, and Pareto views; it must not displace the random control.

### 2.2 Quality-Diversity Archive

Add a bounded MAP-Elites-style archive that keeps the best crew in each behavioral cell instead of retaining only the global top score.

Candidate dimensions include:

- fight-length bucket
- round-1 kill probability
- hull remaining
- damage archetype
- trigger package
- captain family or faction
- rarity and accessibility
- scenario family

Start in benchmarks, prove that the archive produces competitive and materially different crews, then expose a bounded production lane.

### 2.3 Cross-Entropy Sampler

Learn probability weights over captains, bridge slots, below-decks officers, officer packages, and archetype tags:

1. Sample legal crews from the current distribution.
2. Scout them on a shared seed panel.
3. Select elites using confidence-aware scores.
4. Update material weights while preserving a minimum entropy floor.
5. Repeat for a bounded number of rounds.

Compare this lane with genetic and stratified-random search on wide categorical spaces.

### 2.4 Hyperband-Style Racing

Generalize scout and confirmation into explicit multi-fidelity brackets:

```text
bracket A: many crews, few trials
bracket B: fewer crews, moderate trials
bracket C: finalists, deep trials
```

Promote crews whose confidence intervals overlap the top-K boundary. Keep captain and archetype strata alive long enough to avoid eliminating specialists from noisy early samples. Use common seed panels for comparisons and independent seeds for final confirmation.

## Phase 3: Learned Search

Learned search begins only after observations are versioned, inspectable, and benchmarked. The first model should be modest, explainable, and unable to bypass simulator confirmation.

### 3.1 Feature Store

Derive stable candidate and scenario features:

- officer ids, roles, tags, and ability tags
- static condition matches
- faction, synergy, and trigger-package counts
- resolved ship and profile stat buckets
- hostile type, faction, level, mitigation, shield, dodge, and damage profile
- fight-archetype estimates
- compatible historical observations

Implement feature extraction in Rust and version its schema independently from any trained model.

### 3.2 Surrogate Ranker

Evaluate models that handle categorical features and missingness well, starting with tree-based rankers and TPE- or SMAC-style proposers. Use uncertainty or an explicit exploration term to prevent collapse onto stale meta crews.

Promotion criteria:

- beats stratified random under equal scout budget
- improves top-K recall over analytical prefilter alone
- remains useful after officer-catalog changes
- never supplies an unconfirmed final recommendation

### 3.3 Active Learning

Use disagreement and uncertainty to propose the next simulation batch:

- underexplored captain families
- near-frontier crews that need more trials
- analytical and surrogate disagreements
- sparse local neighborhoods around winners

The output is always a candidate batch for simulation, not a recommendation.

### 3.4 Counterfactual Explanations

Generate recommendation explanations from measured ablations and replacements:

- replace each officer with the best legal alternative
- estimate marginal value with uncertainty
- identify trigger dependencies
- report scenario or profile sensitivity that changes the preferred crew

Prefer these measured explanations over prose inferred from tags alone.

## Phase 4: Multi-Scenario and Fleet-Aware Optimization

### 4.1 Robust Search Across Target Sets

Optimize over target collections such as hostile level bands, armada variants, or PvP attacker classes. Support average-case, worst-case, and small-cover-set objectives without weakening scenario legality.

### 4.2 Substitute Crew Planner

Given a recommendation and unavailable officers, find replacements by trigger role, stat contribution, archetype cell, and local refinement. Report the expected performance loss and confidence interval for each substitute crew.

### 4.3 Campaign and Chain Policies

Optimize policy-level outcomes:

- fights before repair
- expected repair cost
- median and lower-bound chain length
- low-variance survival
- speed subject to a safety constraint

Share Pareto and racing machinery while keeping chain-specific objectives explicit.

## Phase 5: Compute Engine Upgrades

Pursue compute work after algorithmic budget reductions have measurable benchmarks.

### 5.1 Candidate Representation

Move hot optimizer paths toward compact officer ids, slot bitsets, canonical tuple hashes, precomputed eligibility and package scores, and structure-of-arrays simulation batches.

### 5.2 Common Random Numbers and Seed Panels

Compare scout candidates and local neighbors on identical seed panels, then reserve independent seeds for final confirmation. Measure variance reduction and guard deterministic replay across every search lane.

### 5.3 SIMD and Batch Simulation

Batch candidates with similar ability structure, separate static preprocessing from the round loop, and vectorize uniform numeric kernels while retaining scalar trace and debug parity. Treat GPU simulation as a later experiment only if profiling shows it can beat better search and CPU batching.

## Phase 6: Meta-Optimizer

Once several lanes are proven, add a scheduler that allocates a fixed budget using scenario shape, roster size, historical method performance, candidate-funnel telemetry, and user intent.

It should choose:

- which lanes run
- budget per lane
- scout and confirmation depth
- exploration quota
- diversity target

The scheduler itself must be benchmarked against a fixed allocation policy.

## Suggested Implementation Order

1. Local refinement around finalists. **(landed, §1.1)**
2. Pareto tags and recommendation reasons. **(landed, §1.2)**
3. Observation fingerprints, retention, and benchmark gates. **(landed, §1.3 and §1.4)**
4. Hyperband-style racing with protected strata.
5. Beam search as a discovery lane.
6. Quality-diversity archive in benchmarks, then bounded production use.
7. Cross-entropy sampling.
8. Feature extraction and a surrogate proposer.
9. Robust multi-scenario and substitute optimization.
10. Meta-scheduling across proven lanes.

## Validation Gates

Every roadmap item must include:

- deterministic seed reproducibility
- candidate legality tests
- hard-filter false-negative tests
- comparison with stratified random and existing production methods
- small-space recall against deeper exhaustive or near-exhaustive search
- method and evidence provenance in results
- no model-only final recommendations
- bounded telemetry and observation growth

## Likely Code Ownership

- `src/optimizer/mod.rs`: portfolio orchestration, provenance, and funnel telemetry
- `src/optimizer/crew_generator.rs`: role pools, canonicalization, bitsets, random and beam generation
- `src/optimizer/tiered.rs`: racing, seed panels, and confidence scheduling
- `src/optimizer/genetic.rs`: quality-diversity emitters, cross-entropy seeds, and local repair integration
- `src/optimizer/ranking.rs`: Pareto tags, recommendation reasons, and frontier helpers
- `src/data/optimize_history.rs`: migration toward fingerprinted observation reuse
- `src/data/budget_telemetry.rs`: method and subphase measurements
- `frontend/src/components/SimResults.tsx`: Pareto and reason display
- `frontend/src/components/OptimizePanel.tsx`: portfolio strategy, exploration budget, and objective presets

## Research Influences

- **MAP-Elites / quality diversity:** retain strong solutions across behavior cells.
- **Hyperband / successive halving:** increase simulation depth only for promising candidates.
- **BOHB:** combine model proposals with multi-fidelity evaluation.
- **SMAC and TPE:** propose candidates in noisy categorical spaces.
- **Cross-entropy methods:** learn sampling distributions over strong officer packages.
- **Large-neighborhood search:** improve crews by destroying and repairing meaningful subsets.
- **Racing algorithms:** eliminate statistically weak candidates while respecting noisy objectives.

Primary references:

- Jean-Baptiste Mouret and Jeff Clune, [Illuminating search spaces by mapping elites](https://arxiv.org/abs/1504.04909)
- Lisha Li et al., [Hyperband: A Novel Bandit-Based Approach to Hyperparameter Optimization](https://jmlr.org/papers/v18/16-558.html)
- Stefan Falkner, Aaron Klein, and Frank Hutter, [BOHB: Robust and Efficient Hyperparameter Optimization at Scale](https://arxiv.org/abs/1807.01774)
- Frank Hutter, Holger Hoos, and Kevin Leyton-Brown, [Sequential Model-Based Optimization for General Algorithm Configuration](https://ml.informatik.uni-freiburg.de/wp-content/uploads/papers/11-LION5-SMAC.pdf)
- James Bergstra et al., [Algorithms for Hyper-Parameter Optimization](https://papers.neurips.cc/paper/4443-algorithms-for-hyper-parameter-optimization.pdf)
- Reuven Rubinstein and Dirk Kroese, [The Cross-Entropy Method for Combinatorial and Continuous Optimization](https://people.smp.uq.edu.au/DirkKroese/ps/CEopt.pdf)

## Explicit Deferrals

- Do not build a learned model before observation data is versioned and benchmarkable.
- Do not let surrogate output bypass simulator confirmation.
- Do not add broad functional-officer bans to reduce candidate counts.
- Do not begin with GPU simulation.
- Do not expose every optimizer control in the default UI.
