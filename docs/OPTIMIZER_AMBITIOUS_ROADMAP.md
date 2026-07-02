# Ambitious Optimizer Roadmap

This roadmap is for the next generation of Kobayashi's crew optimizer: not just a faster exhaustive search, but a search system that learns where good crews live, explains tradeoffs, and spends simulation budget where it changes decisions.

It builds on the current foundation:

- production hard filters: roster, seat legality, bans, scenario eligibility, duplicate prevention, and constraints
- closed-form analytical ranking and prefiltering
- tiered scout to confirm Monte Carlo
- genetic search with warm starts, repair, adaptive mutation, and final confirmation
- optimize history, learned below-decks sampling, novelty-aware ranking, and confidence intervals

The goal is not to replace these pieces. The goal is to make them into an explicit optimizer portfolio.

## North Star

Kobayashi should become a multi-stage, self-auditing search engine:

```text
profile + ship + hostile
  -> exact role pools and legality funnel
  -> proxy ranking and search lanes
  -> diverse discovery: beam, random, genetic, MAP-Elites, local repair
  -> adaptive simulation budget: racing, successive halving, confidence bounds
  -> high-depth confirmation with common seeds
  -> Pareto recommendations with explanations and substitute crews
  -> durable observations for future learned search
```

The product experience should feel simple: ask for a crew and get several trustworthy choices, each with a reason. Under the hood, Kobayashi should know which method found each crew, how much evidence supports it, and whether a cheaper method would have found the same answer.

## Principles

1. **Exact filters may delete. Soft heuristics may only prioritize.**
   Eligibility, duplicate prevention, seat legality, owned roster, and impossible conditions are hard gates. Proxy scores, archetypes, learned models, and meta assumptions must stay as ordering, sampling, or budget allocation signals unless proven exact.

2. **Every clever method needs a boring baseline.**
   A stratified random search should run in benchmarks and, for wide searches, optionally in production as a small exploration lane. If a new method cannot beat that baseline, delete or demote it.

3. **Simulation remains the judge.**
   Surrogates, beam scores, and Bayesian proposals can nominate crews. They do not crown winners.

4. **Diversity is product quality, not just algorithmic decoration.**
   Users need fast, safe, low-variance, accessible, and matchup-specific crews. Returning ten near-identical variants of one top crew is usually worse than returning a real menu of strong alternatives.

5. **Telemetry before tuning.**
   Do not tune beam widths, prune thresholds, mutation rates, or learned models without recording enough stage-level data to know what changed.

## Phase 0: Measurement Spine

Make the optimizer observable enough that future changes can be judged scientifically.

### 0.1 Candidate Funnel Telemetry

Record per optimize run:

- raw role pool sizes by seat
- counts after roster, bans, eligibility, constraints, and duplicate/canonicalization
- generated candidate count
- warm-start count and dedupe count
- analytical prefilter from/kept counts
- scout candidate count, scout trial count, confirm candidate count, confirm trial count
- optimize history hits
- final result count
- elapsed time per phase
- cancellation point, if any

This extends the current budget telemetry into a full optimizer funnel. It should be emitted in logs, optionally appended to profile telemetry, and summarized in API responses for advanced mode.

**Status:** implemented for the current optimizer paths. Kobayashi now emits API, log, and budget-telemetry counts for raw role pools, ban-list filtering, eligibility filtering, roster narrowing, final constrained pools, generated candidates, warm-start/dedupe, constraints, analytical prefilter, scout/confirm candidates, history hits, final results, coarse phase durations, and structured async cancellation points. Result rows now carry `method_provenance`. Remaining Phase 0 measurement work is 0.2 durable simulation observations and 0.3 controlled optimizer benchmarks; future lanes should add their own method-specific subphase timings as they land.

### 0.2 Simulation Observation Log

Create a durable `optimize_observations` store keyed by:

- profile hash and profile id
- ship id, tier, level, component hash
- hostile/scenario hash
- support buff and PvP defender fingerprint
- officer catalog version
- simulator version
- candidate crew hash
- method that proposed the crew
- trials, seed range or seed panel id, objective metrics, confidence intervals

This should not replace `optimize_history` immediately. Start as append-only telemetry, then graduate into a reusable dataset for surrogate evaluation and cache reuse.

**Status:** first slice implemented. When `KOBAYASHI_OPTIMIZE_OBSERVATIONS=1`, Kobayashi appends final ranked crew observations to `profiles/{id}/optimize_observations.jsonl` or `KOBAYASHI_OPTIMIZE_OBSERVATIONS_PATH`. Rows include profile id/hash, scenario/request context, method provenance, crew hash/material, trial count, score, objective metrics, and confidence intervals. Remaining 0.2 work: catalog/simulator version fingerprints beyond crate version, richer hostile/support fingerprints, offline dataset tooling, retention/compaction, and reuse by surrogate search.

### 0.3 Benchmark Harness With Controls

Add an optimizer benchmark suite that compares:

- current tiered
- current genetic
- analytical-only
- stratified random baseline
- known warm-start crews
- any new lane added later

Score each method by:

- best confirmed crew found under equal wall-clock and equal trial budgets
- top-K recall against deeper confirmation on small search spaces
- diversity of finalists
- stability across seeds
- regret versus the best discovered crew
- false-negative rate of any prefilter

**Status:** first slice implemented. `cargo run --release --bin optimizer_method_bench` runs built-in optimizer method comparisons over fixed scenarios and optional `--seed-panel` values, then emits JSONL (or `--pretty`) rows for `tiered`, `genetic`, `linear_eval`, `warm_start_tiered`, and a benchmark-only `random_stratified` control. Rows include budgets, elapsed time, candidate funnel/trial accounting where available, top crew identity, top-K material diversity metrics, best discovered win-rate, and per-method regret. Remaining 0.3 work: equalized wall-clock/trial budget modes, deeper reference sweeps for top-K recall/false-negative scoring, CI artifact wiring, and baseline thresholds.

## Phase 1: Better Recommendations Without New Magic

These are high confidence improvements because they reuse Kobayashi's existing simulator and ranking outputs.

### 1.1 Stratified Random Baseline

Add a small random exploration lane:

- sample valid crews after the exact filters
- stratify by captain, faction, rarity, archetype tags, and below-decks families
- reserve a configurable percentage of scout budget for unusual combinations
- write method label `random_stratified`

This is both a benchmark control and an antidote to overconfident proxy rankings.

**Status:** implemented. [`src/optimizer/random_stratified.rs`](../src/optimizer/random_stratified.rs) samples legal crews from the eligibility-filtered pools, round-robin across captain (faction, rarity) cells with below-decks group-family rotation, deterministic per seed. Two production surfaces: `strategy: "random_stratified"` runs a standalone control lane (pure random candidate set → tiered scout → confirm; ignores warm-start, skips the analytical prefilter and optimize-history preconfirm reuse), and `tiered_random_exploration_pct` (0, 0.5] swaps that fraction of the tiered scout candidate list — budget-neutral, post-prefilter — for random crews. Both label result rows `method_provenance: "random_stratified"` and report `random_exploration_candidates` in the funnel telemetry; the SPA exposes the strategy option and a "Random exploration %" tiered control. `optimizer_method_bench`'s `random_stratified` control now calls the same production sampler. Remaining 1.1 work: archetype/ability-tag strata (beyond faction/rarity/group) once tags exist, and default-on evaluation for the tiered slice after benchmark comparisons justify it.

### 1.2 Local Refinement and Large-Neighborhood Repair

After tiered/genetic finalists, search nearby:

- one-slot bridge swaps
- one-slot below-decks swaps
- captain swap while preserving support officers
- swap one trigger producer or consumer
- destroy-repair neighborhoods: remove 2-3 officers and rebuild from high-fit compatible pools

Use scout depth first, then confirm only improvements and diverse near-ties. This is the first "ambitious but practical" quality jump: Kobayashi already has finalists, role pools, legality repair, and shared scenario data.

Method labels:

- `local_swap`
- `local_captain_swap`
- `large_neighborhood_repair`

User-facing payoff: "This recommendation came from improving the genetic winner by replacing X with Y."

### 1.3 Pareto Frontier Recommendations

Compute Pareto fronts over metrics Kobayashi already has:

- win rate
- loss rate
- stall rate
- round-1 kill rate
- average hull remaining
- average defender hull remaining
- confidence interval width
- chain-grind efficiency
- roster accessibility or rarity score

Expose views:

- safest
- fastest farming
- highest damage
- lowest variance
- best chain crew
- best substitute
- most different from current crew

Keep the current scalar score as the default sort, but let the API return `pareto_tags` and `recommendation_reason`.

### 1.4 Method Provenance

Every result row should know how it was discovered:

- warm start
- history
- analytical prefilter
- tiered scout
- genetic
- random baseline
- local refine
- future beam
- future quality-diversity archive
- future surrogate proposal

This makes optimizer behavior debuggable and gives the UI honest explanations.

## Phase 2: Explicit Discovery Lanes

Make search strategy a portfolio rather than a single path.

### 2.1 Beam Search With Diversity Lanes

Build crews slot by slot and retain partial candidates. Use separate beams instead of one global beam:

- damage beam
- survivability beam
- round-1 kill beam
- long-fight sustain beam
- anti-dodge/high-accuracy beam
- shield-bypass/isolytic beam
- low-rarity or owned-roster substitute beam
- per-captain beam for top captain families

The hard part is partial scoring. Start conservative:

- use existing analytical expected damage for partials
- add bridge synergy score
- add static gate match score
- add profile headroom score
- add diversity penalty

Beam search should feed tiered scout, genetic seeds, local refinement, and Pareto views. It should not delete the random lane.

### 2.2 Quality-Diversity Archive

Add a MAP-Elites-inspired archive: keep the best crew per behavioral cell instead of only the global top score.

Possible behavior dimensions:

- fight length bucket
- round-1 kill probability bucket
- hull remaining bucket
- damage archetype: direct, crit, isolytic, apex, shield bypass
- trigger package: morale, burning, hull breach, on-kill, none
- captain family or faction
- rarity/accessibility bucket
- PvE/PvP/armada scenario family

This is a better fit for Kobayashi than pure "find one optimum" search because players often need substitutes, safe crews, fast crews, and matchup-specific crews. It also gives the UI a search-space atlas: "here are the best crews of each kind."

Start offline or benchmark-only, then move into production as a bounded archive lane.

### 2.3 Cross-Entropy / Estimation-of-Distribution Sampler

Implement an adaptive sampler that learns probability weights over:

- captains
- bridge officers by slot
- below-decks officers
- pair and triple packages
- archetype tags

Loop:

1. sample valid crews from the current distribution
2. scout them
3. choose elites by confirmed or scout-adjusted score
4. update sampling weights toward elite material while preserving entropy
5. repeat for a few rounds

This is simpler than a full learned surrogate and can reuse the same repair/legality machinery as genetic search. It may outperform GA on wide categorical spaces because it learns population-level officer/package weights directly.

### 2.4 Hyperband-Style Racing

Generalize tiered scout into explicit brackets:

```text
bracket A: many crews, very few trials
bracket B: fewer crews, moderate trials
bracket C: finalists, high trials
```

Use confidence-aware elimination:

- promote crews whose confidence intervals overlap the top-K boundary
- keep archetype/captain strata alive long enough to avoid early specialist elimination
- use common random numbers or fixed seed panels for fair comparisons

This evolves Kobayashi's current adaptive scout into a principled multi-fidelity evaluator.

## Phase 3: Learned Search

Only do this after Phase 0 creates the observation store. The first learned system should be modest, inspectable, and subordinate to the simulator.

### 3.1 Feature Store

For each candidate, derive features:

- officer ids by role
- officer tags and ability tags
- captain/bridge/below-decks role indicators
- static condition match flags
- faction and synergy counts
- stat buckets after profile and ship resolution
- hostile type, faction, level, mitigation, shield, dodge, and damage profile
- fight archetype estimates
- prior observations for similar candidate/material

Write this as a Rust feature extraction module first. Model training can happen out of process.

### 3.2 Surrogate Ranker

Start with models that handle categorical features and missingness well:

- random forest or gradient-boosted trees
- TPE-style proposal model
- SMAC-style random-forest surrogate
- BOHB-style combination of model proposals plus multi-fidelity racing

Use the model to propose candidates and allocate budget. Do not use it as final ranking.

Success criteria:

- beats stratified random under equal scout budget
- improves top-K recall over analytical prefilter alone
- uncertainty or exploration term prevents collapse onto stale meta crews
- degrades gracefully after officer catalog updates

### 3.3 Active Learning Loop

Let the optimizer ask useful questions:

- which untested captain family has high uncertainty?
- which near-frontier crew needs more trials?
- which surrogate-predicted crew disagrees most with analytical scoring?
- which local neighborhood around a winner is underexplored?

The output is a candidate batch for scout, not a final answer.

### 3.4 Recommendation Explanations From Counterfactuals

Generate explanations by cheap ablation and replacement tests:

- remove each officer and replace with the best legal alternative
- report marginal value estimates with uncertainty
- flag trigger dependencies: "Y matters because X supplies burning"
- flag profile sensitivity: "accuracy is valuable here because this hostile has high dodge"

This is more trustworthy than natural-language explanations invented from tags alone.

## Phase 4: Multi-Scenario and Fleet-Aware Optimization

Kobayashi should eventually optimize not just one fight, but a player's intent.

### 4.1 Robust Crew Search Across Hostile Sets

Optimize against a set of targets:

- common grind hostiles in a level band
- all relevant armada variants
- PvP attacker classes
- station/outpost scenario families when in scope

Objectives:

- max average performance
- max worst-case performance
- choose one crew that is "good enough" across a range
- choose a small set of crews that covers the range

### 4.2 Substitute Crew Planner

Given a recommended crew and missing officer(s), find substitutes by:

- same trigger role
- same stat bucket
- same archetype cell
- local refinement around the unavailable crew
- Pareto-preserving replacement

This should become a first-class player feature.

### 4.3 Campaign / Chain Policies

For chain grind, optimize policy-level outcomes:

- fights before repair
- expected repair cost
- median and lower-bound chain length
- low-variance chain survival
- speed with minimum safety constraint

This should share the Pareto and racing machinery, but use chain-specific objectives.

## Phase 5: Compute Engine Upgrades

Do these after search algorithms are better. Algorithmic budget reduction should come before hardware heroics.

### 5.1 Candidate Representation

Move hot optimizer paths toward:

- compact officer ids
- slot bitsets
- canonical tuple hashes
- precomputed role eligibility bitsets
- precomputed pair and package scores
- structure-of-arrays batches for numeric simulation inputs

This improves exhaustive, tiered, random, local, and learned lanes together.

### 5.2 Common Random Numbers and Seed Panels

Use paired comparisons where possible:

- scout candidates on identical seed panels
- compare local neighbors using identical seeds
- reserve independent seeds for final confirmation

This can reduce ranking noise without changing combat math.

### 5.3 SIMD and Batch Simulation

Continue CPU-first:

- batch candidates by similar ability structure
- separate static preprocessing from round loop
- vectorize uniform numeric kernels where branch behavior is low
- keep scalar trace/debug parity

GPU work is a later experiment. Kobayashi's ability system is branchy enough that better search will likely beat GPU acceleration for a long while.

## Phase 6: Meta-Optimizer

Once multiple lanes exist, add a scheduler that decides how to spend a fixed budget.

Inputs:

- candidate funnel telemetry
- historical method performance by scenario
- current roster size
- hostile archetype
- user objective: fast answer, exact answer, diverse answers, chain survival

Outputs:

- which lanes run
- budget per lane
- scout depth
- confirm depth
- exploration quota

This is the point where Kobayashi starts optimizing the optimizer.

## Suggested Implementation Order

1. [x] Extend optimizer telemetry and result provenance.
2. [x] Add stratified random baseline and benchmark it against current tiered/genetic.
3. Add local refinement around finalists.
4. Add Pareto tags and recommendation reasons to API/UI.
5. Generalize tiered scout into Hyperband-style racing with strata.
6. Add beam search as a discovery lane.
7. Add observation logging and feature extraction.
8. Add quality-diversity archive offline, then production-bounded.
9. Add cross-entropy sampler.
10. Add first surrogate proposer and compare it to random, beam, GA, and analytical.
11. Add robust multi-scenario optimization.
12. Add meta-scheduler.

## Validation Gates

Every roadmap item should include:

- deterministic seed reproducibility
- candidate legality tests
- hard-filter false-negative tests
- benchmark comparison against stratified random
- small-space recall against deeper exhaustive or near-exhaustive search
- method provenance in results
- no model-only final recommendations
- no unbounded telemetry growth

## Where This Fits In The Codebase

Likely modules:

- `src/optimizer/mod.rs`: orchestration, strategy portfolio, provenance, funnel telemetry
- `src/optimizer/crew_generator.rs`: role pools, canonicalization, bitsets, random/beam candidate generation
- `src/optimizer/tiered.rs`: racing, successive halving, seed panels, confidence scheduling
- `src/optimizer/genetic.rs`: quality-diversity emitters, cross-entropy seeds, local repair integration
- `src/optimizer/ranking.rs`: Pareto tags, recommendation reasons, frontier helpers
- `src/data/optimize_history.rs`: migration path toward observation storage
- `src/data/budget_telemetry.rs`: wider optimizer telemetry
- `frontend/src/components/SimResults.tsx`: Pareto/reason display
- `frontend/src/components/OptimizePanel.tsx`: advanced controls for portfolio strategy, exploration budget, and objective presets

## Research Influences Worth Borrowing

- **MAP-Elites / quality diversity**: keep high-performing solutions across behavior cells, not only the single best solution. This maps naturally to Kobayashi recommendation families.
- **Hyperband / successive halving**: allocate increasing simulation resources only to promising candidates.
- **BOHB**: combine model-guided proposals with Hyperband-style multi-fidelity evaluation.
- **SMAC / random-forest Bayesian optimization**: good fit for categorical search spaces and noisy simulation results.
- **TPE**: a pragmatic proposal model for high-dimensional conditional/categorical configurations.
- **Cross-Entropy / estimation-of-distribution methods**: learn sampling distributions over strong officer/material combinations.
- **Large-neighborhood search**: improve strong crews by destroying and repairing meaningful chunks instead of only one-slot swaps.
- **Racing algorithms**: eliminate statistically weak candidates while respecting noisy objectives.

Primary references:

- Jean-Baptiste Mouret and Jeff Clune, [Illuminating search spaces by mapping elites](https://arxiv.org/abs/1504.04909)
- Lisha Li et al., [Hyperband: A Novel Bandit-Based Approach to Hyperparameter Optimization](https://jmlr.org/papers/v18/16-558.html)
- Stefan Falkner, Aaron Klein, and Frank Hutter, [BOHB: Robust and Efficient Hyperparameter Optimization at Scale](https://arxiv.org/abs/1807.01774)
- Frank Hutter, Holger Hoos, and Kevin Leyton-Brown, [Sequential Model-Based Optimization for General Algorithm Configuration](https://ml.informatik.uni-freiburg.de/wp-content/uploads/papers/11-LION5-SMAC.pdf)
- James Bergstra et al., [Algorithms for Hyper-Parameter Optimization](https://papers.neurips.cc/paper/4443-algorithms-for-hyper-parameter-optimization.pdf)
- Reuven Rubinstein and Dirk Kroese, [The Cross-Entropy Method for Combinatorial and Continuous Optimization](https://people.smp.uq.edu.au/DirkKroese/ps/CEopt.pdf)

## Explicit Deferrals

- Do not build a learned model before observation data exists.
- Do not let surrogate output bypass full simulator confirmation.
- Do not add broad functional-officer bans to make counts look better.
- Do not start with GPU simulation.
- Do not make the default UI expose every optimizer knob. Advanced mode can have controls; the normal path should ask for intent.

## The Ambitious Bet

Kobayashi can become more than a simulator plus optimizer. It can become an experimental search laboratory for STFC combat:

- it discovers strong crews,
- maps why they work,
- offers substitutes,
- measures its own uncertainty,
- learns from past simulations,
- and can say when it does not know enough yet.

That is the right kind of ambition: not mysticism, just a very good machine with a memory and a conscience.
