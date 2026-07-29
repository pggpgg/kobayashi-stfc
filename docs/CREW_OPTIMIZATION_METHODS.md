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

### Budgeted truncation must be stratified

Almost every Kobayashi search runs the generator with a `max_candidates` budget far below the legal
space, so **which** crews the budget buys matters as much as how many. A nested captain → bridge →
below-decks walk that simply stops at the budget is not a sample of the space; it is a prefix of
one cell. One (captain, bridge pair) cell over a 51-officer below-decks pool offers thousands of
combinations against a default budget of 128, so the walk never reaches a second captain. Measured
on the demo profile against pools of 189 captains, 242 bridge officers, and 51 below-decks
officers, the old walk returned crews for **exactly one captain** at every seed — 0.5% of the
captain pool — and so did the benchmark's own reference sweep.

[`src/optimizer/crew_generator.rs`](../src/optimizer/crew_generator.rs) instead treats a
(captain, bridge pair) as a *cell* and fills the budget round-robin:

- Cells are visited **bridge-pair-major, captain-minor**, so consecutive cells rotate captains and
  a budget of N reaches `min(N, cells)` distinct cells before any cell yields a second crew.
- Bridge pairs are enumerated in **colexicographic** order — (0,1), (0,2), (1,2), (0,3), … — which
  grows both indices together. Lexicographic order would pin the first bridge slot to one officer
  for the first `n - 1` pairs, re-creating the same bias one seat down.
- Each cell's below-decks cursor **starts at a staggered rank** (cell ordinal times a golden-ratio
  constant, modulo the cell's combination count). Without this a one-crew-per-cell pass hands every
  cell its lowest-index tuple, and below-decks variety collapses even as captain variety improves.
- Later passes take one more crew per still-productive cell, so the budget is still filled exactly,
  and a budget past the size of the space still returns the whole space.

The three callers share the ordering and differ only in the below-decks pool a cell is given: the
whole pool (exhaustive), a stride subset (sampled), or the epsilon-greedy selection (learned
sampling, which offers each cell exactly one combination). Uncapped generation keeps the plain
nested walk — with no budget there is nothing to bias.

What this does **not** fix is how wide the pools are to begin with. `large_pool_captain_limit` and
`large_pool_bridge_limit` still cut the pools to 10 captains and 12 bridge officers before any of
this runs, and that ceiling is what keeps tiered behind the random control on the method benchmark
— see §16 and roadmap §1.4.

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

### Kobayashi use

Implemented as the local-refinement pass ([`src/optimizer/refinement.rs`](../src/optimizer/refinement.rs)): opt-in via `local_refinement: true` on the optimize request, supported on the **tiered and genetic** strategies, running after the main search has spent its budget. `local_refinement_seeds` (default 3) sets how many finalists to climb from and `local_refinement_rounds` (default 3) caps accepted moves per seed. The pass polls the run's cancellation check between seeds and rounds, so cancelling a background job stops it mid-climb.

The other strategies ignore the flag by design: exhaustive already evaluated the space a neighborhood would re-propose, `linear_eval` runs no Monte Carlo to confirm an improvement against, and `random_stratified` is a benchmark control that has to stay unassisted to remain a valid baseline.

Three neighborhoods are enumerated deterministically — every legal one-slot substitution (coordinate descent over captain, bridge, and below-decks seats), and destroy-repair neighborhoods that vacate two slots and refill them from the ranked pools. Officers already seated elsewhere are skipped during generation, so neighbors are duplicate-free by construction rather than by later filtering.

The "expensive if every neighbor requires deep simulation" weakness above is handled the same way tiered handles it: each round scouts the neighbors *and the incumbent* at shallow depth, promotes only neighbors that beat the incumbent's scout score, and pays full confirmation sims on that shortlist alone. Re-scouting the incumbent each round matters — comparing a shallow neighbor against a deeply-confirmed incumbent would reject good neighbors on depth alone. A round with no scout-level improvement, or a confirmation that fails to reproduce a scout-level gain, stops the climb rather than drifting on noise. A visited set spans all seeds, so overlapping neighborhoods are never re-simulated.

Result rows carry `method_provenance: "local_swap"`, `"local_captain_swap"`, or `"large_neighborhood_repair"`. The label is derived from the diff against the source finalist rather than from the operator that produced the crew, so a crew that improved over several hops is labeled by what actually changed.

Refined rows also carry a `refinement` object: the source finalist's crew, every changed seat as `{slot, index, from, to}`, and the confirmed `baseline_score` / `refined_score` / `gain`. The SPA renders the gain next to the method label and the seat changes as the cell's tooltip, so a recommendation says *which officer swap* improved it rather than only that refinement ran. The pass's budget/effect counters (seeds refined, neighbors generated, scouted, confirmed, improvements accepted) are logged as `optimize_local_refinement_completed` and returned on `OptimizeRunOutcome::refinement_stats` — a pass that generated neighbors and accepted none is a meaningful result (the finalists were local optima at that depth) and is otherwise indistinguishable from a pass that never ran.

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

### Kobayashi use

Implemented as a tagging pass over the finished result set ([`src/optimizer/pareto.rs`](../src/optimizer/pareto.rs)), applied in `build_optimize_response` and always on — it reads metrics the simulation already produced, so it costs no extra trials. It annotates only: the scalar ranking score still sorts the table, and the pass can neither reorder nor drop a crew.

Rows carry `pareto_tags` (stable wire labels) and a `recommendation_reason` in plain language:

| Tag | Meaning | Rows |
| --- | --- | ---: |
| `pareto_optimal` | Nothing else considered is at least as good on every objective | many |
| `safest` | Lowest loss rate, tie-broken by hull left | ≤ 1 |
| `fastest_farming` | Highest round-1 kill rate | ≤ 1 |
| `best_chain` | Highest chain-grind success rate (chain runs only) | ≤ 1 |
| `most_different` | Competitive crew sharing the fewest officers with the top row | ≤ 1 |

Objectives are win rate, hull remaining, round-1 kill rate, damage dealt (`1 − defender hull remaining`), and `1 − loss rate`; chain runs swap the first two for chain success and its secondary. All are fractions oriented so higher is better, so no normalization step is needed.

Three decisions shape what the pass will and will not say:

- **Confidence-interval width is not an objective.** A row scouted on 20 trials has wider intervals than one confirmed on 2,000; folding that into dominance would let under-measured rows crowd the front. Depth is surfaced instead as a caveat sentence inside the reason ("Backed by 40 of the 2000 trials the deepest row got").
- **Ties resolve to the better-ranked row.** Differences within `PARETO_EPSILON` (0.5 percentage points) count as equal, so Monte Carlo jitter cannot manufacture front members; and a row statistically indistinguishable from one above it stays unbadged, because it offers a reader nothing the stronger row does not. Without this the front swallowed 8 of 20 rows on a lopsided matchup.
- **Named views need spread.** A view is skipped when its metric is flat across the considered rows, and `most_different` is skipped when nothing wins at all — a badge that every crew could equally wear is noise.

Whole runs are skipped when there is nothing to trade off: `linear_eval` (no simulated rates) and single-result sets. Tagging is bounded to the first `PARETO_MAX_ROWS_CONSIDERED` (200) rows because dominance is O(n²) and optimize returns every simulated crew; rows past that stay untagged rather than mis-tagged.

Not built here: accessibility, rarity, and substitute views. Ranking a crew by what a player owns needs roster and rarity data these rows do not carry, and belongs with the substitute planner.

The SPA renders a **Why** column of badges with the reason as the cell tooltip, shown only when some row is tagged. A named view already implies front membership, so the generic `Pareto` badge appears only on rows that have nothing more specific to say.

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

## 16. Cross-method benchmarking (roadmap §1.4)

`optimizer_method_bench` (`src/bin/optimizer_method_bench.rs`) runs the lanes; the scoring that
makes them comparable lives in [`src/optimizer/method_bench.rs`](../src/optimizer/method_bench.rs).

### Equal-budget modes

`--budget-mode` decides whether two lanes are comparable at all.

| mode | what it does |
|---|---|
| `native` (default) | Each lane keeps its own CLI knobs. Honest for "how does the product behave today", useless for ranking methods. |
| `equal-trials` | Every lane is sized to `--trial-budget` Monte Carlo trials. Depth is held fixed and breadth is solved for, because depth is what makes a result trustworthy. |
| `equal-wall-clock` | Every lane is sized to `--wall-clock-ms`, from its own measured trial rate. |

Wall-clock mode runs each lane twice at different probe sizes and fits `ms = fixed + trials / rate`.
A single probe charges the lane's fixed setup to every trial, underestimates the rate, and lands
50–90% under target; the two-point fit removes that term.

Every record carries `budget.projected_trials`, `realized_trials`, and `budget_utilization`. A
utilization well under 1.0 means the lane could not spend its budget — normally because the
candidate space is smaller than the budget's breadth, which quietly turns an equal-budget
comparison into "every lane searched everything". **Pick a trial budget whose breadth is below the
case's candidate count**, or the comparison measures nothing. `linear_eval` runs no trials at all,
so both equal-budget modes report `budget.applied: false` with a reason instead of pretending.

### Reference sweep, recall, and regret

`--reference-sweep` evaluates a bounded candidate set deeply with no prefilter and no search
heuristic, giving recall and regret a ground truth. Two properties of it matter:

- `covers_generator_space` says only that `--reference-max-crews` did not bind. It is **not**
  exhaustiveness over the legal space: `CrewGenerator` narrows officer pools before enumerating, so
  lanes that sample or evolve crews straight from the pools routinely propose crews the reference
  never enumerated. `lane_best_crew_in_reference_set` reports that per lane.
- Regret is measured on an **independent confirmation seed** (`confirmation_seed`). Every winner —
  the reference's and each lane's — was chosen by maximizing a noisy score over many crews, so
  scoring them on the seed that selected them flatters whichever search looked at the most crews.
  The confirmation pass re-evaluates the reference's top-K and every lane's winner together on one
  neutral seed; `regret_confirmed_on_independent_seed` records that it ran.

Prefer **`score_regret_vs_reference`** over the win-rate variant. PvE win rate saturates: in most
matchups every legal crew wins or every legal crew loses, and `win_rate_discriminates: false` marks
those cases. The ranking score still separates crews by hull remaining and round-1 kills.

### Prefilter false negatives

`--prefilter-keep 64,128` runs the production analytical prefilter over the reference's evaluated
crews and counts how many reference top-K crews it deleted. The prefilter is a soft filter, so
every top-K crew it drops before Monte Carlo is a crew the search can no longer find.
`win_rate_loss_at_best` separates "dropped several good crews" from "dropped the winner".

### Stability and the CI gate

With a `--seed-panel`, the run emits one `stability` record per (case, method): win-rate spread,
`distinct_best_crews`, `modal_best_crew_share`, and mean recall/regret. A lane that wins on average
but answers differently on every seed is not the same product as one that answers consistently.

`cargo xtask optimizer-bench-check` runs the configuration stored in
`optimizer_method_bench_baseline.json` and enforces three rules: a recall floor, a score-regret
ceiling, and a control-ordering rule requiring every lane to stay within `control_margin` of the
stratified random control. The bench is bit-deterministic per seed — identical output across thread
counts — so the tolerances absorb intentional changes rather than noise. `.github/workflows/
optimizer-method-bench.yml` runs it weekly and on dispatch, uploading the JSONL and Markdown.

Refresh the baseline with `--write-baseline` and explain the change in the PR.

### Reading recall against regret

The two headline metrics answer different questions and can move in opposite directions. Regret
asks "how good is the crew this lane picked?" and is measured on an independent confirmation seed.
Recall@K asks "how many of the reference's best K crews did this lane return?" — a set-overlap
score that a lane can lose by finding an equally good crew that simply is not one of those K.

Stratified truncation is the worked example: tiered's regret improved (0.00627 → 0.00532) while its
recall fell (0.36 → 0.22) against a reference set that did not change. On seed 11 recall fell from
0.8 to 0.1 with regret unchanged at ~3e-5. **When the two disagree, regret is the outcome metric
and recall is the diagnostic.** A recall move on its own is not a regression until you have checked
whether the lane's own pick got worse.

## Suggested next implementation milestones

1. Add per-search telemetry: candidate counts after each filter, scout count, sim count, cache hit rate, and finalist count.
2. Make search lanes explicit: exhaustive, beam, genetic, local-refine, random baseline, and confirmation.
3. Add a small random/stratified baseline to every optimizer benchmark.
4. Add successive-halving confirmation for stochastic objectives.
5. Store simulation observations so future surrogate ranking can be trained and evaluated.
6. Display Pareto reasons in the UI: why this crew is fast, safe, cheap, or matchup-specific.
