# Crew Optimization Methods

This note catalogs practical methods for searching very large Star Trek Fleet Command crew spaces. It expands the high-level answer: do not simulate every crew equally. Use exact filters to avoid impossible or provably weak candidates, cheap scoring to rank the rest, and expensive combat simulation only where it buys information.

Related project docs:

- [`PVE_CREW_SEARCH_SPACE_REDUCTION.md`](PVE_CREW_SEARCH_SPACE_REDUCTION.md) measures how much the current production eligibility filters shrink the PvE search space.
- [`OPTIMIZATION_HEURISTICS.md`](OPTIMIZATION_HEURISTICS.md) is the research backlog of additional crew-search heuristics.
- [`OPTIMIZATION_SPECIAL_HEURISTICS.md`](OPTIMIZATION_SPECIAL_HEURISTICS.md) documents heuristics already wired into the optimizer.
- [`PERFORMANCE.md`](PERFORMANCE.md) covers implementation-level performance work.

## Core principle

A crew optimizer has two separate jobs:

1. **Discovery:** find strong crews quickly enough to be useful interactively.
2. **Confirmation:** spend deeper simulation budget on the best candidates so recommendations are trustworthy.

The fastest systems separate those phases. They use cheap exact checks and approximate models to reduce the candidate space, then reserve high-fidelity simulation for the final frontier.

A useful mental model:

```text
all legal tuples
  -> legality, duplicate, seat, roster, and scenario filters
  -> cheap analytical/proxy score
  -> scout simulation or learned surrogate ranking
  -> top-K full simulation
  -> local refinement around winners
  -> Pareto-ranked recommendations
```

## 1. Optimized exhaustive search

Exhaustive search evaluates every valid crew. It is still useful when the post-filtered search space is small enough or when exactness matters more than latency.

### Techniques

- Generate only legal crews: valid captain seat, bridge seats, below-decks seats, no duplicate officers, and scenario-compatible pools.
- Canonicalize symmetric slots so the same crew is not simulated under multiple orderings.
- Precompute officer stats, ability predicates, synergy flags, and profile-adjusted values.
- Use compact integer officer IDs, bitsets, and tuple hashes instead of object-heavy data structures.
- Keep a bounded top-K heap rather than storing all scored crews.
- Parallelize by captain, captain-pair shard, hostile scenario, or candidate chunk.
- Batch simulation work where possible so cache and vector units are used efficiently.

### Strengths

- Exact if the generator includes all legal candidates and filters are optimality-preserving.
- Easy to reason about and debug.
- Produces reliable baselines for validating heuristic methods.

### Weaknesses

- Runtime grows combinatorially with officer count and below-decks slots.
- Wasteful when billions of crews are technically legal but obviously uncompetitive.
- Random combat outcomes may require many repeated simulations per crew, multiplying cost.

### Kobayashi use

Use exhaustive search after strong scenario filters, owned-roster narrowing, and eligibility gates have already reduced the space. For broad PvP or catch-all PvE searches, exhaustive search should usually become a final confirmation tool rather than the first pass.

## 2. Exact pruning and constraint filtering

Exact pruning removes candidates that cannot matter before simulation.

### Examples

- Seat legality: officer can actually occupy captain, bridge, or below-decks role.
- Duplicate prevention: the same officer cannot appear twice.
- Scenario eligibility: an ability that only works against armadas should not be explored for ordinary hostiles.
- Roster ownership: do not consider officers the player does not own.
- Ban list or curation gates: remove known non-useful officers when the project has a documented rationale.
- Exact dominance: if one officer is always at least as good as another in a resolved context, keep the dominating officer.
- Equivalence classes: if two officers become mechanically identical after profile, rank, and scenario resolution, simulate one representative.

### Strengths

- Usually the highest return on investment.
- Safe when rules are exact.
- Reduces every downstream cost: enumeration, simulation, memory, ranking, and UI latency.

### Weaknesses

- Exactness depends on predicate quality. A mislabeled eligibility rule can discard a winner.
- Hard filters need tests and auditability.
- Over-aggressive bans can encode current meta assumptions that later become stale.

### Implementation advice

Classify filters as either:

- **Hard exact filters:** impossible, ineligible, duplicate, mechanically inert, or provably dominated. These may remove candidates.
- **Soft priority filters:** likely weak, low synergy, low marginal value, or unproven dominance. These should demote or sample sparsely, not delete.

## 3. Analytical proxy scoring

A proxy score estimates crew quality without running the full combat simulation.

### Inputs

- Profile-adjusted attack, defense, health, mitigation, piercing, accuracy, crit, isolytic, and other stat values.
- Captain ability value in the specific fight.
- Officer ability activation predicates and expected uptime.
- Synergy bonuses and faction tags.
- Fight archetype: fast kill, long sustain, shield-heavy, high-dodge, boss, armada, PvP.
- Hostile vulnerability: which stats actually change the objective.

### Strengths

- Can rank millions of candidates quickly.
- Useful for ordering exhaustive search so good incumbents appear early.
- Can feed beam search, genetic initialization, local search, or tournament brackets.

### Weaknesses

- Proxy scores can be wrong when interactions are nonlinear.
- A proxy trained on one fight archetype can mis-rank another.
- Needs calibration against real simulation output.

### Recommended use

Use proxy scoring as a **ranking and budget allocation tool**, not as final truth. Keep an exploration quota for lower-proxy crews so the optimizer can discover surprising interactions.

## 4. Beam search

Beam search builds crews slot by slot and keeps only the best partial candidates at each depth.

Example:

```text
captain candidates
  -> keep top 100 captains
captain + bridge officer 1
  -> keep top 1,000 partial crews
captain + two bridge officers
  -> keep top 5,000 bridge cores
bridge core + below-decks officers
  -> keep top final candidates for simulation
```

### Strengths

- Finds good crews much faster than full enumeration.
- Natural fit for captain/bridge/below-decks structure.
- Easy to combine with synergy graphs, trigger requirements, and partial upper bounds.

### Weaknesses

- Can discard a weak-looking partial crew that becomes excellent after a later synergy piece.
- Beam width is a quality/runtime knob and requires tuning.
- Needs diversity controls to avoid returning many near-duplicates.

### Implementation advice

Use multiple beams instead of one global beam:

- Top damage beam.
- Top survivability beam.
- Top speed-farming beam.
- Top boss/endurance beam.
- Top low-rarity or owned-roster beam.
- Per-captain or per-archetype beams.

This prevents one obvious archetype from crowding out niche but valuable crews.

## 5. Genetic algorithms

A genetic algorithm treats a crew as a genome, for example:

```text
[captain_id, bridge_1_id, bridge_2_id, below_1_id, below_2_id, below_3_id]
```

It repeatedly scores a population, keeps strong crews, mutates them, crosses them with other strong crews, repairs illegal children, and repeats.

### Useful mutation operators

- Swap one bridge officer.
- Swap one below-decks officer.
- Replace captain while preserving support crew.
- Replace a low-uptime synergy consumer with a matching producer.
- Move from a damage archetype to a sustain archetype.
- Random restart from an underexplored captain or faction.

### Strengths

- Good for huge nonlinear spaces.
- Discovers interactions that are difficult to encode by hand.
- Can run continuously in the background and improve recommendations over time.

### Weaknesses

- No guarantee of global optimum.
- Requires careful repair logic for legal crews.
- Can converge prematurely without diversity pressure.

### Recommended use

Seed the initial population with known good crews, beam-search results, proxy-score leaders, and random valid crews. Keep mutation rates high enough to explore, and preserve elites only after they survive deeper simulation.

## 6. Monte Carlo random search

Random search samples valid crews, simulates them, and keeps the best. It is simple and surprisingly effective as a baseline.

### Improvements

- Weighted sampling toward relevant officers.
- Stratified sampling by captain, faction, rarity, fight archetype, or ability tag.
- Exploration quota for unusual combinations.
- Adaptive sampling that increases probability for officer families that are overperforming.

### Strengths

- Easy to implement.
- Good first benchmark for more complex search methods.
- Embarrassingly parallel.

### Weaknesses

- Inefficient if sampling ignores known constraints and synergies.
- Can miss rare high-performing interactions.
- Needs many samples in large spaces.

### Recommended use

Use random search as a control group. If a complex heuristic cannot beat a well-seeded random baseline, the heuristic is probably not worth its complexity.

### Kobayashi use

Implemented as the `random_stratified` lane ([`src/optimizer/random_stratified.rs`](../src/optimizer/random_stratified.rs)): `strategy: "random_stratified"` runs a standalone benchmark-control lane (stratified sampling over captain faction/rarity cells and below-decks group families, then scout → confirm), and `tiered_random_exploration_pct` reserves a budget-neutral slice of the tiered scout candidate list for the same sampler, bypassing the analytical prefilter. Result rows carry `method_provenance: "random_stratified"`.

## 7. Multi-armed bandits and adaptive sampling

Bandit methods allocate more simulation budget to promising arms. An arm can be a captain, crew archetype, officer family, synergy package, or full candidate crew.

### Candidate algorithms

- UCB1.
- Thompson sampling.
- Successive halving.
- Hyperband-style brackets.
- Racing algorithms that eliminate statistically weak candidates early.

### Strengths

- Excellent when simulations are stochastic and repeated trials are expensive.
- Avoids spending equal budget on bad crews.
- Produces confidence-aware rankings.

### Weaknesses

- Requires careful objective design: win rate, average damage, median hull left, repair cost, or speed.
- Early noisy results can over-promote lucky crews unless confidence bounds are used.
- Arms must be chosen at the right granularity.

### Recommended use

Use successive halving for final confirmation:

```text
simulate 100,000 crews at 10 trials each
keep top 10,000
simulate to 50 trials
keep top 1,000
simulate to 250 trials
keep top 100
simulate to 1,000+ trials
```

## 8. Bayesian optimization and surrogate-guided search

Bayesian optimization chooses the next candidates by modeling the relationship between crew features and observed score.

For categorical crew search, practical surrogate models include:

- Tree-structured Parzen estimators, as used by tools like Optuna.
- Random forests.
- Gradient-boosted trees.
- CatBoost or LightGBM on categorical and numeric features.
- Neural embeddings for officer IDs and ability tags.

### Strengths

- Sample-efficient when full simulation is expensive.
- Learns interactions that simple proxy formulas miss.
- Can balance exploitation and exploration.

### Weaknesses

- More complex than beam or genetic search.
- Needs clean feature encoding and enough initial data.
- Model uncertainty can be hard with high-cardinality categorical officers.

### Recommended use

Use this after collecting a simulation dataset from random, beam, and genetic runs. The surrogate should propose candidates; the combat simulator remains the final judge.

## 9. Learned evaluator / surrogate scoring model

A learned evaluator approximates the expensive combat simulator. The optimizer can score millions of candidates with the model, then fully simulate only the top slice.

### Features

- Officer IDs and role positions.
- Ability tags and activation predicates.
- Captain/bridge/below-decks effects.
- Faction and synergy counts.
- Profile-adjusted stat buckets.
- Opponent type and hostile stats.
- Fight length estimate.
- Prior simulation outputs for similar crews.

### Strengths

- Very fast once trained.
- Improves as the project accumulates simulation data.
- Can detect nonlinear interactions better than hand-written heuristics.

### Weaknesses

- Needs ongoing validation against real sim results.
- Can overfit stale data or old combat formulas.
- May confidently mis-rank edge cases outside the training distribution.

### Recommended use

Use the learned evaluator as a middle tier:

```text
hard filters -> proxy score -> learned evaluator -> full sim confirmation
```

Never let the learned model be the only source of truth for final recommendations.

## 10. Local search and hill climbing

Local search starts from a good crew and tries small changes.

### Variants

- Coordinate descent: replace one slot at a time with every legal alternative.
- Random-restart hill climbing: start from many seeds.
- Simulated annealing: sometimes accept worse moves to escape local optima.
- Tabu search: avoid cycling through recently seen crews.

### Strengths

- Very effective after beam, genetic, or known-meta seeds.
- Easy to explain: "this officer swap improved expected hull by 4%."
- Good at squeezing extra value from strong crews.

### Weaknesses

- Can get trapped in local optima.
- Needs multiple seeds for broad discovery.
- Expensive if every neighbor requires deep simulation.

### Recommended use

Run local search around all finalist crews, not just the single current winner. Keep a visited set so equivalent swaps are not re-tested.

## 11. Pareto frontier search

Many crew decisions are multi-objective. A single scalar score may hide important tradeoffs.

Possible objectives:

- Win rate.
- Average damage dealt.
- Median damage taken.
- Survival probability.
- Fight duration.
- Repair efficiency.
- Consistency / variance.
- Accessibility for a player-owned roster.
- Performance against multiple hostile types.

A crew is Pareto-optimal when no other crew is at least as good on every objective and strictly better on one.

### Strengths

- Produces useful recommendation sets instead of one brittle answer.
- Helps users choose between speed, safety, damage, and accessibility.
- Good fit for UI filters and explainability.

### Weaknesses

- Frontier size can grow large.
- Requires objective normalization and clear display.
- Needs tie-breaking for user-facing recommendations.

### Recommended use

Store the Pareto frontier per scenario and expose preset views:

- best farming speed,
- safest crew,
- best average damage,
- best low-variance crew,
- best owned-roster crew,
- best low-rarity substitute.

## 12. Tournament-style evaluation

Tournament evaluation tests many crews cheaply, then increases rigor only for survivors.

Example:

```text
round 1: all candidates vs representative hostile, 5-10 sims
round 2: top 20% vs broader hostile set, 25-50 sims
round 3: top 5% vs target hostile, 100-250 sims
final: top 100 with full sim depth and confidence intervals
```

### Strengths

- Aligns compute cost with candidate quality.
- Works well with stochastic simulations.
- Easy to parallelize and report progress.

### Weaknesses

- Early representative tests can eliminate specialists.
- Needs stratification so niche archetypes survive long enough.
- Requires careful confidence handling for noisy outcomes.

### Recommended use

Use separate tournament lanes per scenario or archetype, then merge finalists into a common confirmation round.

## 13. Memoization and incremental evaluation

Large crew spaces repeat many components. Cache everything that is independent of the final tuple.

### Cache candidates

- Officer role eligibility.
- Profile-adjusted officer stats.
- Ability activation predicates.
- Pair and triple synergy scores.
- Captain dependency closure.
- Hostile vulnerability vectors.
- Partial-crew proxy scores.
- Combat states for deterministic prefixes if the simulator supports it.

### Strengths

- Speeds every search strategy.
- Reduces repeated work in local search and genetic mutation.
- Improves interactivity for repeated searches with the same profile and hostile.

### Weaknesses

- Cache invalidation can be tricky when profile, research, ship, or formulas change.
- Large caches can consume memory.
- Incorrect cache keys create subtle optimizer bugs.

### Recommended use

Key caches by profile hash, ship hash, hostile/scenario hash, officer version, and simulator version. Invalidate aggressively when formulas change.

## 14. Vectorized, batched, and GPU-friendly simulation

If the simulator can express much of combat as array operations, evaluate candidates in large batches.

### Techniques

- Structure-of-arrays layout for candidate stats.
- NumPy, Numba, JAX, PyTorch, CuPy, Rust SIMD, or C++ SIMD for tight numeric loops.
- Chunked workers that process thousands of crews at a time.
- Avoid branch-heavy per-candidate logic in hot loops.
- Separate static preprocessing from round-by-round simulation.

### Strengths

- Massive throughput improvement for numeric workloads.
- Good fit for exhaustive or tournament stages.
- Can turn "millions of crews" from impossible into routine if formulas are batchable.

### Weaknesses

- Harder to implement when abilities have complex conditional behavior.
- GPU transfer overhead can dominate small batches.
- Debugging vectorized combat logic is harder than scalar logic.

### Recommended use

Start with CPU batching and data-layout improvements before adding GPU complexity. GPU is most worthwhile when the per-candidate simulation is uniform and repeated many times.

## 15. Clustering and archetype discovery

Cluster officers or crews by behavior so the optimizer can explore different families deliberately.

### Examples

- Burst damage crews.
- Long-fight sustain crews.
- Shield bypass crews.
- High accuracy / anti-dodge crews.
- Isolytic crews.
- Morale, burning, hull breach, or other trigger-package crews.
- Armada-specific crews.
- PvP station and PvP space crews.

### Strengths

- Prevents one meta archetype from consuming all search budget.
- Improves explainability.
- Helps produce substitute recommendations when a player lacks a key officer.

### Weaknesses

- Requires maintaining useful tags and archetype definitions.
- Clusters can drift as new officers or mechanics are added.
- Overly rigid archetypes can miss cross-family hybrids.

### Recommended use

Use archetypes for budget partitioning, UI grouping, and recommendation explanation. Do not use them as hard exclusive categories unless mechanics prove exclusivity.

## Recommended hybrid pipeline

For Kobayashi, the practical default should be a layered pipeline:

1. **Build role pools** from owned roster, seat legality, bans, and scenario eligibility.
2. **Canonicalize and deduplicate** symmetric or equivalent crew fragments.
3. **Calculate profile and hostile context** once: stat headroom, hostile vulnerabilities, expected fight length, and mechanic eligibility.
4. **Generate seeds** from known crews, proxy leaders, synergy templates, random samples, and prior search winners.
5. **Run beam search** over captain/bridge/below-decks construction with diversity lanes.
6. **Run genetic or local refinement** around strong seeds and beam finalists.
7. **Scout with low simulation depth** using tournament or successive-halving brackets.
8. **Train or update a surrogate evaluator** when enough simulation observations exist.
9. **Confirm finalists** with high simulation depth and confidence intervals.
10. **Emit Pareto-ranked recommendations** with explanation, tradeoffs, and substitute options.

In short:

```text
exact filters + proxy scoring + diverse search + adaptive simulation + Pareto reporting
```

## Method selection guide

| Situation | Best starting method |
| --- | --- |
| Few million post-filter crews and cheap deterministic sim | Optimized exhaustive search |
| Billions of legal tuples | Exact pruning + beam search |
| Need a good answer quickly | Beam search + proxy scoring |
| Simulation is stochastic | Tournament evaluation + successive halving |
| Simulation is very expensive | Surrogate model + Bayesian/adaptive sampling |
| Need exact top crew | Exhaustive search with exact branch-and-bound pruning |
| Need diverse recommendations | Pareto frontier + archetype lanes |
| Already have strong known crews | Local search / coordinate descent |
| Looking for surprising combinations | Genetic algorithm + random exploration quota |
| Need performance at scale | Memoization + batched/vectorized simulation |

## Quality and safety rules

- Keep exact filters separate from soft heuristics.
- Audit every hard exclusion with testable predicates.
- Preserve an exploration budget so unusual crews can still surface.
- Report confidence or simulation depth with every recommendation.
- Track which method discovered each crew so search quality can be measured.
- Validate heuristic winners against the full combat simulator before presenting them as best.
- Prefer reproducible random seeds for debugging and benchmark comparisons.

## Suggested next implementation milestones

1. Add per-search telemetry: candidate counts after each filter, scout count, sim count, cache hit rate, and finalist count.
2. Make search lanes explicit: exhaustive, beam, genetic, local-refine, random baseline, and confirmation.
3. Add a small random/stratified baseline to every optimizer benchmark.
4. Add successive-halving confirmation for stochastic objectives.
5. Store simulation observations so future surrogate ranking can be trained and evaluated.
6. Display Pareto reasons in the UI: why this crew is fast, safe, cheap, or matchup-specific.
