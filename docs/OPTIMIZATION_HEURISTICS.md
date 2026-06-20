# Optimization Heuristics for Crew Search

## Category: Exact Search-Space Reduction

1. **Precompile Legal Crew Domains**
   - **Rationale**: Invalid captain, bridge, below-deck, duplicate-officer, ship-class, or faction combinations should never reach combat simulation.
   - **Implementation Rule**: Build role-specific bitsets for every officer and intersect them while constructing crews; reject partial crews immediately when no legal completion remains.
   - **Expected Impact**: Eliminates 10–50% of generated candidates and substantially reduces allocation overhead.
   - **Applicability**: All modes and fights.
   - **Integration**: Use as a mandatory generator/GA repair layer; exact and optimality-preserving.

2. **Canonicalize Interchangeable Positions**
   - **Rationale**: Some bridge or below-deck slots are mechanically symmetric.
   - **Implementation Rule**: If swapping two positions cannot change any ability target, synergy, or stat contribution, require `officer_id(slot_a) < officer_id(slot_b)`.
   - **Expected Impact**: Reduces symmetric permutations by 2× to factorial-scale factors.
   - **Applicability**: Crews with position-independent slots.
   - **Integration**: Apply before exhaustive enumeration, tiered scouting, and GA hashing; exact.

3. **Remove Provably Inert Officers**
   - **Rationale**: Abilities requiring absent mechanics—such as burning, hull breach, morale, player ships, armadas, on-kill events, or a specific faction—have zero value.
   - **Implementation Rule**: Evaluate each officer’s activation predicate against the ship, hostile, and scenario; exclude only when both ability effects and passive stats are provably dominated by a legal replacement.
   - **Expected Impact**: Shrinks relevant officer pools by 15–60%.
   - **Applicability**: Highly scenario-specific fights.
   - **Integration**: Hard-filter when zero-effect dominance is proven; otherwise reduce scout priority.

4. **Collapse Exact Officer Equivalence Classes**
   - **Rationale**: Officers can become mechanically identical after level, rank, profile caps, and scenario predicates are resolved.
   - **Implementation Rule**: Hash normalized captain ability, officer ability, below-deck effect, stats, tags, and activation predicates; simulate one representative per identical hash.
   - **Expected Impact**: Removes 5–25% of candidates on large rosters.
   - **Applicability**: Rosters containing duplicate-effect or inactive-effect officers.
   - **Integration**: Expand equivalent representatives only when reporting tied crews; exact.

5. **Admissible Partial-Crew Upper Bounds**
   - **Rationale**: Partial crews that cannot surpass the current top-K threshold need not be completed.
   - **Implementation Rule**: For each empty slot, add the maximum independently attainable damage, mitigation, healing, and proc contribution; prune only when this optimistic bound is below the incumbent score.
   - **Expected Impact**: Cuts 30–90% of exhaustive-search branches after strong incumbents appear.
   - **Applicability**: Exhaustive and branch-and-bound modes.
   - **Integration**: Seed strong crews first to tighten the bound; exact if the bound never underestimates.

6. **Cap-Saturation Dominance Pruning**
   - **Rationale**: Additional accuracy, mitigation, crit chance, piercing, or protected-cargo bonuses may do nothing after engine caps.
   - **Implementation Rule**: Resolve profile and ship bonuses first; when a candidate already reaches a hard cap, treat further additions to that stat as zero in dominance comparisons.
   - **Expected Impact**: Removes 10–40% of redundant stat-stacking candidates.
   - **Applicability**: High-research or artifact-heavy profiles.
   - **Integration**: Use for safe pairwise dominance and partial-crew bounds; exact only for true engine caps.

## Category: Vulnerability and Profile Analysis

7. **Finite-Difference Hostile Vulnerability Ranking**
   - **Rationale**: The best crew amplifies stats to which the hostile outcome is actually sensitive.
   - **Implementation Rule**: Run a baseline fight, perturb each controllable stat by `+1%`, and compute `Δobjective/Δstat`; rank officers by their projected contribution along positive gradients.
   - **Expected Impact**: Finds strong candidates 2–5× earlier.
   - **Applicability**: All fights.
   - **Integration**: Order exhaustive search, weight tiered scouts, and bias GA initialization; soft heuristic.

8. **Residual Mitigation Targeting**
   - **Rationale**: Armor, shield deflection, and dodge differ by hostile and ship class.
   - **Implementation Rule**: Calculate post-profile residual mitigation for each incoming damage channel; prioritize piercing or accuracy effects against the largest residual term, not the largest raw hostile stat.
   - **Expected Impact**: Improves top-decile hit rate by 15–40%.
   - **Applicability**: High-mitigation hostiles.
   - **Integration**: Add a vulnerability-weighted officer score; do not hard-prune unless damage equations prove dominance.

9. **Isolytic Marginal-Value Gate**
   - **Rationale**: Isolytic damage is valuable only when the scenario permits it to bypass or outperform conventional mitigation.
   - **Implementation Rule**: Compare analytical marginal damage from `+1% isolytic` and `+1% conventional damage`; boost isolytic-tagged crews only when the former exceeds the latter by a configurable ratio, such as `1.15`.
   - **Expected Impact**: Reduces wasted isolytic exploration by 30–70%.
   - **Applicability**: Isolytic-capable ships and hostiles.
   - **Integration**: Use for ordering and GA mutation weights; retain a small exploration quota.

10. **Apex Contribution Gate**
   - **Rationale**: Apex effects can be decisive or nearly inert depending on hostile defenses and activation requirements.
   - **Implementation Rule**: Resolve expected Apex uptime and post-defense multiplier; demote Apex officers when expected Apex contribution is below 5% of baseline objective value.
   - **Expected Impact**: Removes 20–50% of low-value Apex combinations from early tiers.
   - **Applicability**: Apex-capable content.
   - **Integration**: Scout only representative Apex crews when below threshold; confirm more if representatives overperform.

11. **Profile Headroom Scoring**
   - **Rationale**: Research, buildings, artifacts, and exocomps can saturate some stats while leaving others weak.
   - **Implementation Rule**: Define `headroom(stat) = useful_cap - profile_effective_stat`; multiply each officer’s stat contribution by normalized positive headroom.
   - **Expected Impact**: Improves early top-K discovery by 20–50% for mature accounts.
   - **Applicability**: Profile-aware optimization.
   - **Integration**: Recompute seed and mutation weights whenever the player profile changes.

12. **Multiplicative Complement Preference**
   - **Rationale**: Adding a new multiplier often outperforms stacking an already-large additive bucket.
   - **Implementation Rule**: Normalize bonuses into engine stacking buckets; score an officer by exact marginal output after existing profile and crew bonuses rather than raw listed percentage.
   - **Expected Impact**: Rejects 15–35% of deceptively strong paper-stat crews.
   - **Applicability**: Profiles with large research or artifact bonuses.
   - **Integration**: Use in proxy scoring, branch bounds, and GA fitness priors.

## Category: Synergy and Role-Based Pruning

13. **Captain Dependency Closure**
   - **Rationale**: Many captain abilities require specific faction, synergy, state, or trigger support.
   - **Implementation Rule**: For each captain, precompute mandatory and beneficial support tags; reject partial crews once mandatory tags cannot be supplied by remaining slots.
   - **Expected Impact**: Cuts captain-centered search branches by 25–70%.
   - **Applicability**: Captains with explicit activation requirements.
   - **Integration**: Use during crew construction and GA repair; exact for mandatory predicates.

14. **Producer–Consumer Trigger Matching**
   - **Rationale**: Effects consuming burning, hull breach, morale, critical hits, or other states are weak without reliable producers.
   - **Implementation Rule**: Require expected trigger uptime above a configurable minimum, such as 20%, before prioritizing a consumer; calculate uptime from producer proc rate, weapon count, and turn horizon.
   - **Expected Impact**: Removes 20–60% of nonfunctional synergy combinations.
   - **Applicability**: Proc-driven crews.
   - **Integration**: Hard-prune only impossible triggers; otherwise demote proportionally to uptime.

15. **Synergy Graph Beam Expansion**
   - **Rationale**: Useful crews usually contain connected officer interactions rather than three isolated abilities.
   - **Implementation Rule**: Build a weighted graph where edges represent faction synergy, shared tags, trigger flow, or multiplicative interaction; expand partial crews through the top `B` connected neighbors first.
   - **Expected Impact**: Produces strong scouts 3–10× earlier.
   - **Applicability**: Medium and large rosters.
   - **Integration**: Use as tiered beam ordering and GA seed generation; preserve disconnected exploration.

16. **Captain Marginal-Value Screening**
   - **Rationale**: A famous captain may be poor when their captain ability adds little in the specific fight.
   - **Implementation Rule**: Compare each captain’s proxy score against using the same officer in a bridge role; demote captains whose captain-only marginal value is below a threshold such as 2%.
   - **Expected Impact**: Reduces captain branches by 20–50%.
   - **Applicability**: Large captain pools.
   - **Integration**: Scout low-marginal captains sparsely rather than excluding them outright.

17. **Below-Deck Opportunity-Cost Budget**
   - **Rationale**: Below-deck slots compete for limited stat and ability capacity.
   - **Implementation Rule**: Compute marginal objective gain per below-deck slot; discard a below-deck candidate from early tiers when its optimistic gain is below the current slot threshold.
   - **Expected Impact**: Shrinks below-deck combinations by 40–85%.
   - **Applicability**: Ships with many eligible below-deck officers.
   - **Integration**: Optimize bridge first, then search only the top `M` below-deck contributors per bridge core.

18. **Anti-Synergy Conflict Matrix**
   - **Rationale**: Some officers shorten fights that another officer needs to ramp, overwrite the same state, compete for capped stats, or require mutually exclusive conditions.
   - **Implementation Rule**: Maintain condition-based conflict rules; penalize or exclude a pair only when the engine proves their combined marginal gain is nonpositive relative to either replacement.
   - **Expected Impact**: Avoids 10–35% of structurally weak combinations.
   - **Applicability**: Large, mechanics-rich rosters.
   - **Integration**: Apply as GA repair and scout ordering; hard-prune only proven dominance cases.

## Category: Hostile-Specific Crew Templates

19. **Fast-Kill Burst Template**
   - **Rationale**: Short fights undervalue ramping, regeneration, and late-turn effects.
   - **Implementation Rule**: If baseline median duration is at most three rounds, prioritize opening-round damage, starting buffs, crit, and immediate isolytic effects; demote abilities activating after the expected final round.
   - **Expected Impact**: Cuts 30–70% of irrelevant sustain candidates.
   - **Applicability**: Grinding, low-level hostiles, speed farming.
   - **Integration**: Allocate most scout budget to burst templates while retaining a small general pool.

20. **Long-Fight Sustain Template**
   - **Rationale**: Healing, mitigation, stacking, and periodic effects compound in long fights.
   - **Implementation Rule**: If baseline duration exceeds ten rounds or survival probability is below 80%, score effects by expected cumulative activations and prioritize effective-health-per-round.
   - **Expected Impact**: Improves strong-crew discovery by 25–60%.
   - **Applicability**: Bosses, high-level hostiles, endurance content.
   - **Integration**: Seed sustain captains and producer–consumer ramp packages.

21. **Shield-Heavy Bypass Template**
   - **Rationale**: High shield mitigation can make conventional hull-DPS bonuses inefficient.
   - **Implementation Rule**: If expected shield phase exceeds 60% of fight duration, prioritize shield pierce, shield bypass, isolytic damage, or shield-specific debuffs; demote hull-only effects until a proxy shows early shield collapse.
   - **Expected Impact**: Reduces low-value exploration by 30–65%.
   - **Applicability**: Shield-dominant bosses and armadas.
   - **Integration**: Create a dedicated scout stratum and compare it against a conventional control stratum.

22. **High-Dodge Accuracy Template**
   - **Rationale**: Damage bonuses have little value when attacks miss.
   - **Implementation Rule**: Estimate hit probability after profile bonuses; when it falls below 80%, prioritize accuracy and dodge-reduction until their marginal value drops below direct damage.
   - **Expected Impact**: Improves proxy fidelity and removes 20–50% of ineffective DPS stacks.
   - **Applicability**: High-dodge hostiles or unfavorable class matchups.
   - **Integration**: Alter stat weights dynamically after every accuracy addition.

23. **Swarm and On-Kill Template**
   - **Rationale**: On-kill and carry-over effects require multiple enemies or chained encounters.
   - **Implementation Rule**: Set on-kill contribution to zero for single-target isolated simulations; for chained hostiles, model activation frequency from kills per run and remaining encounter count.
   - **Expected Impact**: Removes up to 50% of false-positive crews in single-target fights.
   - **Applicability**: Swarms, grinding chains, multi-wave encounters.
   - **Integration**: Separate single-fight and campaign objectives rather than sharing one ranking.

24. **Scripted-Phase Template**
   - **Rationale**: Hostiles may change defense, damage, or mechanics at known hull, shield, or turn thresholds.
   - **Implementation Rule**: Segment the fight into phases and score abilities only over reachable active intervals; prioritize crews that cross dangerous thresholds before the hostile’s power spike.
   - **Expected Impact**: Reduces misleading average-stat candidates by 20–45%.
   - **Applicability**: Bosses, armadas, scripted encounters.
   - **Integration**: Include phase-transition time as a secondary scout objective.

## Category: Cheap Proxy and Multi-Fidelity Evaluation

25. **Analytical Zero-Round Proxy**
   - **Rationale**: Most obviously weak crews can be rejected without executing combat turns.
   - **Implementation Rule**: Estimate opening DPS, expected hit rate, effective health, trigger feasibility, and capped-stat waste; retain only the top 10–30% per captain or template.
   - **Expected Impact**: Reduces full simulations by 50–90%.
   - **Applicability**: Medium and large spaces.
   - **Integration**: First stage of tiered search; periodically admit random rejects to measure proxy recall.

26. **Single-Seed Deterministic Scout**
   - **Rationale**: One shared RNG seed cheaply identifies gross performance differences.
   - **Implementation Rule**: Simulate every candidate on the same representative seed, then advance only the top fraction plus high-variance or diverse candidates.
   - **Expected Impact**: Cuts initial Monte Carlo cost by 80–99%.
   - **Applicability**: All stochastic fights.
   - **Integration**: Use common random numbers and never treat single-seed rank as final.

27. **Reduced-Horizon Scout**
   - **Rationale**: Early-round state often predicts final performance.
   - **Implementation Rule**: Simulate only `min(3 rounds, 25% of baseline duration)` and record damage, incoming damage, states, and ramp slope; extrapolate with conservative uncertainty.
   - **Expected Impact**: Makes scouting 3–10× cheaper.
   - **Applicability**: Fights without dominant late scripted mechanics.
   - **Integration**: Route crews with strong positive ramp slopes to full simulations even if current score is mediocre.

28. **Representative Seed Panel**
   - **Rationale**: Random seeds can be clustered by crit, proc, and targeting behavior.
   - **Implementation Rule**: Generate a large baseline seed pool, cluster outcome feature vectors, and select 4–16 medoid seeds with cluster weights.
   - **Expected Impact**: Approximates large Monte Carlo batches at 5–20% of their cost.
   - **Applicability**: Stable combat engine and repeated scenario optimization.
   - **Integration**: Use the panel for scouting and the full seed distribution for confirmation.

29. **Learned Outcome Surrogate**
   - **Rationale**: Previously simulated crews provide scenario-specific training data.
   - **Implementation Rule**: Train a lightweight regression or ranking model on officer IDs, tags, resolved stats, trigger uptime, and pairwise interactions; advance candidates with high predicted score or high uncertainty.
   - **Expected Impact**: Reduces full evaluations by 40–80% after warm-up.
   - **Applicability**: Repeated searches or very large spaces.
   - **Integration**: Retrain online; never hard-prune solely from the surrogate when guarantees are required.

30. **Failure-Mode Gate Classifier**
   - **Rationale**: Crews often fail for simple reasons such as misses, early death, no shield break, or unactivated procs.
   - **Implementation Rule**: Run a minimal scout and classify failure mode; reject candidates when no remaining stochastic outcome can overcome the measured deficit under an admissible bound.
   - **Expected Impact**: Saves 20–60% of confirm-stage simulations.
   - **Applicability**: Difficult bosses and survival-constrained objectives.
   - **Integration**: Store failure reasons for mutation guidance and diagnostics.

## Category: Adaptive Monte Carlo and Tiered Racing

31. **Successive Halving**
   - **Rationale**: Simulation budget should concentrate progressively on promising candidates.
   - **Implementation Rule**: Evaluate all candidates with `n₀` seeds, retain the top `1/η`, multiply seeds by `η`, and repeat; typical values are `n₀=4–16`, `η=3–4`.
   - **Expected Impact**: Reduces Monte Carlo work by 60–95%.
   - **Applicability**: Tiered and very large searches.
   - **Integration**: Stratify by captain or template to avoid eliminating entire niches too early.

32. **Confidence-Bound Elimination**
   - **Rationale**: A candidate can stop receiving samples once it cannot reach top-K.
   - **Implementation Rule**: Maintain confidence intervals for mean objective; eliminate candidate `i` when `UCB(i) < LCB(current Kth)` at the chosen family-wise error rate.
   - **Expected Impact**: Cuts confirm simulations by 30–80%.
   - **Applicability**: Stochastic objectives with bounded or estimable variance.
   - **Integration**: Use empirical Bernstein bounds or sequential tests; provides probabilistic guarantees.

33. **Top-K Boundary Sampling**
   - **Rationale**: Samples spent on obvious winners and losers rarely change the result.
   - **Implementation Rule**: Allocate the next batch to candidates whose intervals overlap the Kth-place boundary, weighted by interval width.
   - **Expected Impact**: Reduces final ranking cost by 25–70%.
   - **Applicability**: Top-K searches.
   - **Integration**: Replace uniform confirmation batches with boundary-focused scheduling.

34. **Variance-Adaptive Batch Sizes**
   - **Rationale**: Proc-heavy crews require more seeds than deterministic crews.
   - **Implementation Rule**: Estimate variance after the first batch; choose additional samples proportional to `variance / target_error²`, subject to minimum and maximum budgets.
   - **Expected Impact**: Saves 20–60% versus fixed sample counts.
   - **Applicability**: Mixed deterministic and high-proc candidate sets.
   - **Integration**: Use larger batches only for unstable crews near the selection boundary.

35. **Common Random Numbers**
   - **Rationale**: Comparing crews on identical random events reduces variance in pairwise differences.
   - **Implementation Rule**: Evaluate every candidate in a tier on the same ordered seed set; estimate confidence from paired score differences rather than independent means.
   - **Expected Impact**: Often halves the samples needed for reliable ranking.
   - **Applicability**: Deterministic seeded combat engines.
   - **Integration**: Required for scouts, racing, local neighborhoods, and incumbent comparisons.

36. **Within-Fight Early Termination Bounds**
   - **Rationale**: Some simulated fights become mathematically unwinnable or unable to beat the incumbent before their natural end.
   - **Implementation Rule**: At each round, compute optimistic remaining damage, survival, and objective bounds; terminate when even the optimistic outcome cannot affect candidate ranking.
   - **Expected Impact**: Reduces combat-turn execution by 10–50%.
   - **Applicability**: Long fights and survival failures.
   - **Integration**: Preserve exactness by using only admissible future-outcome bounds.

## Category: Ordering and Anytime Search

37. **Known-Synergy Seed Library**
   - **Rationale**: Community and historical meta crews provide strong early incumbents.
   - **Implementation Rule**: Store parameterized crew templates by hostile type, ship class, faction, and mechanic; validate legality and evaluate them before generated candidates.
   - **Expected Impact**: Tightens pruning bounds early and finds competitive crews almost immediately.
   - **Applicability**: All modes.
   - **Integration**: Feed exhaustive ordering, tiered scouts, and GA initial populations.

38. **One-Swap Neighborhood Expansion**
   - **Rationale**: Optima often lie near strong known crews after profile-specific substitutions.
   - **Implementation Rule**: For each seed crew, enumerate every legal single-officer replacement, then expand improving candidates to two-swap neighborhoods.
   - **Expected Impact**: Finds profile-adjusted improvements with 1–10% of full-space evaluations.
   - **Applicability**: Large rosters with established meta crews.
   - **Integration**: Run before broad exploration and after every new incumbent.

39. **Mechanic-Diversity Quotas**
   - **Rationale**: Pure score ordering can overcommit to one misleading family.
   - **Implementation Rule**: Bucket candidates by primary mechanic—burst, sustain, isolytic, Apex, crit, mitigation, control—and reserve at least 5–10% of scout slots per viable bucket.
   - **Expected Impact**: Improves top-K recall with modest extra cost.
   - **Applicability**: Uncertain or novel hostiles.
   - **Integration**: Use in tiered retention and GA populations.

40. **Captain-Stratified Scouting**
   - **Rationale**: A strong captain may have weak early bridge pairings and be eliminated prematurely.
   - **Implementation Rule**: Evaluate at least `m` best proxy crews per legal captain before applying global ranking; use `m=3–10` based on roster size.
   - **Expected Impact**: Prevents family-level false negatives while reducing total scouts by 30–70%.
   - **Applicability**: Large captain pools.
   - **Integration**: Collapse strata only after captain-specific evidence accumulates.

41. **Sensitivity-Guided Candidate Ordering**
   - **Rationale**: The desired stat direction changes after each selected officer.
   - **Implementation Rule**: Recompute approximate objective gradients after filling each slot; order the next officer by marginal gain against the current partial crew rather than static rating.
   - **Expected Impact**: Produces high-quality incumbents 2–6× earlier.
   - **Applicability**: Nonlinear stacking and capped stats.
   - **Integration**: Use in beam search, branch-and-bound, and greedy GA seeding.

42. **Cancellation-Safe Checkpointing**
   - **Rationale**: Early cancellation is useful only if the best result is already well validated.
   - **Implementation Rule**: Persist every new incumbent, sample count, confidence interval, seed cursor, and search frontier; confirm incumbents with a minimum seed count before exposing them.
   - **Expected Impact**: Preserves useful results under strict time budgets with negligible simulation overhead.
   - **Applicability**: Anytime and interactive searches.
   - **Integration**: Return best confirmed, best provisional, and uncertainty separately.

## Category: Genetic Algorithm Enhancements

43. **Role-Aware Chromosome Encoding**
   - **Rationale**: Generic chromosomes generate many illegal or nonsensical crews.
   - **Implementation Rule**: Encode captain, bridge, and below-deck genes with separate legal domains; repair duplicates and unmet mandatory tags immediately after mutation or crossover.
   - **Expected Impact**: Eliminates 20–70% wasted GA evaluations.
   - **Applicability**: Very large spaces.
   - **Integration**: Make repair deterministic so equivalent offspring share cache entries.

44. **Synergy-Linkage Crossover**
   - **Rationale**: Random crossover breaks producer–consumer and faction synergy packages.
   - **Implementation Rule**: Identify positively interacting officer blocks from simulation data; transfer complete blocks between parents before filling remaining roles.
   - **Expected Impact**: Improves offspring quality by 15–50% and accelerates convergence.
   - **Applicability**: Synergy-heavy rosters.
   - **Integration**: Update linkage weights from observed pair and triple residuals.

45. **Vulnerability-Biased Mutation**
   - **Rationale**: Uniform mutation spends too much effort on irrelevant mechanics.
   - **Implementation Rule**: Sample replacement officers with probability proportional to positive vulnerability-weighted marginal score, while reserving 10–20% probability for uniform mutation.
   - **Expected Impact**: Reaches strong populations 2–4× faster.
   - **Applicability**: Profile- and hostile-aware GA searches.
   - **Integration**: Recompute mutation weights when the population discovers a new mechanic family.

46. **Diversity-Preserving Niches**
   - **Rationale**: Premature convergence can lose isolytic, sustain, or unconventional optima.
   - **Implementation Rule**: Penalize duplicate officer sets and maintain elites within mechanic or captain niches; cap any one niche at 30–50% of the population.
   - **Expected Impact**: Improves top-K recall by 10–30% on multimodal spaces.
   - **Applicability**: Large, nonlinear search spaces.
   - **Integration**: Use behavior descriptors such as damage mix, duration, trigger profile, and survival.

47. **Elite Resampling Before Reproduction**
   - **Rationale**: Lucky low-sample crews can dominate selection and corrupt later generations.
   - **Implementation Rule**: Before granting elite status, evaluate candidates on additional common seeds until their lower confidence bound exceeds the population median.
   - **Expected Impact**: Reduces false-elite propagation by 30–80%.
   - **Applicability**: Proc-heavy stochastic fights.
   - **Integration**: Spend confirmation budget only on prospective elites and niche champions.

48. **Stagnation Restart with Archive Injection**
   - **Rationale**: GA populations can converge around a local optimum.
   - **Implementation Rule**: If best confirmed fitness improves by less than `ε` for `G` generations and diversity falls below threshold, replace 30–70% of the population with archived niche elites, proxy-ranked crews, and random legal crews.
   - **Expected Impact**: Recovers alternative optima without restarting the entire search.
   - **Applicability**: Long-running GA optimization.
   - **Integration**: Stop only when repeated diverse restarts fail or the evaluation budget is exhausted.
