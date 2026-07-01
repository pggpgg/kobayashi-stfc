# Kobayashi Moonshot Roadmap

This is the "dream big" roadmap. It assumes unusually high development capacity, frontier-model agents, and the willingness to build Kobayashi into something larger than an optimizer.

The practical roadmap asks:

> How can Kobayashi find better crews faster?

This moonshot asks:

> What if Kobayashi became an autonomous STFC combat scientist?

This document is intentionally aspirational. It is not a near-term commitment and it may require revisiting items currently parked in [`NOT_ROADMAP.md`](NOT_ROADMAP.md), especially snapshot-bound calibration. Its job is to name the north star clearly enough that the practical roadmap can take steps in the right direction.

## North Star

Kobayashi should become an autonomous combat research lab:

- it ingests game data, combat logs, screenshots, profiles, and community observations
- it builds a causal knowledge graph of officers, ships, hostiles, buffs, conditions, and outcomes
- it proposes hypotheses about hidden combat mechanics
- it designs the next fights needed to disambiguate those hypotheses
- it updates the simulator, tests itself, and prepares evidence-backed patches
- it optimizes crews, chains, events, and progression plans for a real player profile
- it explains recommendations in causal terms, with uncertainty
- it knows when it does not know enough yet

In short:

```text
simulator + optimizer + knowledge graph + experiment designer + research agents + strategic planner
```

The end-state product is not "here is the best crew." It is:

> "Given your account, your goal, and what we currently know about the game, here is the best plan, why it works, what could falsify it, and what to test next."

## Guiding Beliefs

1. **Truth beats confidence.**
   Frontier models can help reason, search, summarize, and code. They must not become the source of truth. The simulator, recorded evidence, reproducible tests, and falsifiable hypotheses remain the authority.

2. **Mechanics are discoverable.**
   If enough controlled fights are observed, hidden formulas can be inferred, narrowed, or at least bounded. Kobayashi should treat unknown mechanics as research targets.

3. **Uncertainty is a product feature.**
   A recommendation should say whether it is backed by deep simulation, a calibrated model, sparse observations, or a speculative hypothesis.

4. **The system should ask better questions.**
   The most valuable AI behavior may be: "record exactly these six fights; they will distinguish between the two remaining formulas."

5. **Autonomy needs audit trails.**
   Every model update, code patch, data import, and recommendation must carry evidence, versioning, and rollback paths.

## Moonshot Architecture

### 1. Evidence Lake

A unified store for every observation Kobayashi can learn from:

- imported player profiles and profile snapshots
- ship component states, officer tiers, forbidden tech, research, buildings, artifacts, exocomps, and support buffs
- raw combat logs
- normalized combat traces
- screenshots or screen recordings when structured logs are absent
- stfc.space and upstream data snapshots
- community cheat sheets and curated mechanic notes
- optimizer simulation observations
- failed predictions and calibration residuals

Every row should include provenance:

- source
- capture time
- profile hash
- game data version
- parser version
- simulator version
- confidence level
- privacy policy and sharing scope

### 2. Combat Knowledge Graph

Represent combat as connected, queryable facts:

- officers, seats, ranks, abilities, tags, predicates, trigger packages
- ships, tiers, levels, components, abilities, weapons, firing patterns
- hostiles, factions, levels, ship classes, resistances, special mechanics
- buffs, stacking buckets, duration, trigger, target, scope
- profile sources: research, building, reputation, artifacts, forbidden tech, exocomps
- causal relationships: "burning enables X", "accuracy counters dodge", "shield bypass changes objective"
- evidence links: which logs, tests, or docs support each edge

This graph becomes the substrate for optimizer features, explanations, hypothesis generation, substitute search, and experiment design.

### 3. Simulator as Executable Theory

Treat the Rust simulator as the current best formal theory of STFC combat.

The simulator should expose:

- trace-level introspection for every damage and buff decision
- symbolic names for formula stages
- component-level residuals against recorded fights
- switchable hypotheses for uncertain mechanics
- deterministic replay with stable seeds
- golden traces for regressions

Instead of a single hard-coded truth, uncertain areas become named hypotheses:

```text
mitigation_ordering = hypothesis_a | hypothesis_b | learned_formula_v3
shield_overflow_rule = current | alternate_client_log_interpretation
hostile_special_ability = noop | inferred_proc | explicit_script
```

### 4. Agent Research Layer

Frontier-model agents operate above the evidence and simulator layers:

- read new logs and identify anomalies
- propose mechanic hypotheses
- generate test cases
- run simulations and compare traces
- inspect code paths
- draft patches
- write calibration notes
- update docs
- open PRs with evidence bundles

Agents should never directly bless changes. They prepare candidates for deterministic validation and human review.

### 5. Strategy and Planning Layer

Above combat science sits player planning:

- "What should I grind for 30 minutes?"
- "Which ship should I upgrade next?"
- "Which officer tier gives the best marginal event performance?"
- "What is my safest chain-grind policy?"
- "Which crew works if I lack this rare officer?"
- "How should I allocate repair speedups, exocomps, and support buffs?"

This layer optimizes across time, resources, risk, and player intent.

## Horizon 0: Epistemic Foundation

Before the grand machine can learn, it needs a spine.

### Goals

- immutable run records
- evidence-linked recommendations
- full optimizer and simulator provenance
- profile snapshots with complete input hashes
- calibration residual reports by mechanic family
- structured uncertainty fields in API responses

### Deliverables

- `evidence/` or profile-scoped evidence store
- immutable `RunManifest` for optimize, simulate, calibration, import, and benchmark jobs
- trace diff viewer: expected vs observed by round, weapon, damage stage, and buff event
- "known, uncertain, unsupported" status for every major mechanic
- result rows with `evidence_level`, `method_provenance`, and `uncertainty_reason`

### Why It Matters

The moonshot fails without epistemic hygiene. A model that cannot cite its evidence is just vibes with a nice API.

## Horizon 1: Universal Combat Data Ingestion

Kobayashi should aggressively reduce the friction of adding evidence.

### Inputs

- structured client combat logs
- TSV/CSV fight exports
- screenshots of combat reports
- screen recordings of fights
- player profile exports
- live sync snapshots
- upstream game data snapshots
- manual mechanic notes

### Capabilities

- OCR and visual parsing for screenshots when logs are unavailable
- LLM-assisted extraction into a strict schema
- confidence scoring per extracted field
- duplicate detection
- profile snapshot binding
- "this fight is unusable because..." diagnostics
- privacy scrubber for user names, alliance names, and coordinates

### Moonshot Feature: Fight Capture Wizard

The app tells the player:

1. set this exact crew
2. fight this target
3. upload the screenshot or log
4. Kobayashi checks whether the evidence is useful
5. if not, it says what is missing

This turns calibration from a maintainer chore into a guided scientific workflow.

## Horizon 2: Mechanics Discovery Engine

This is the heart of "combat scientist."

### Hypothesis Generation

For unexplained residuals, generate candidate hypotheses:

- formula ordering alternatives
- missing buff bucket
- wrong target scope
- wrong trigger timing
- hidden cap or floor
- hostile special ability
- ship ability activation condition
- rank or level scaling mismatch
- client log interpretation error

### Search Methods

Use multiple discovery techniques:

- symbolic regression for numeric formulas
- program synthesis for small effect rules
- Bayesian model comparison across formula variants
- active learning for experiment selection
- causal inference over intervention data
- property-based testing to reject impossible formulas
- constraint solving for hidden caps and thresholds
- ensemble disagreement to find useful fights

### Deliverables

- `MechanicHypothesis` objects with evidence, priors, and falsification criteria
- automated "hypothesis tournament" runner
- residual clustering by likely mechanic
- generated patches behind feature flags
- generated tests for each accepted hypothesis

### Example

Kobayashi observes that high-dodge hostiles consistently deviate from prediction. It proposes:

- accuracy is applied before a hidden hostile dodge multiplier
- officer accuracy buffs stack in a different bucket
- the hostile has an unmodeled round-start evasion state

It then designs the minimal fight set needed to tell those apart.

## Horizon 3: Active Experiment Designer

The system should not passively wait for data. It should ask for the most informative data.

### Core Idea

Given competing hypotheses, choose fights that maximize expected information gain.

### Inputs

- current profile snapshot
- available ships and officers
- reachable hostiles
- hypothesis set
- player time constraints
- event participation constraints

### Outputs

- ranked experiment list
- exact crew and ship setup
- target hostile
- expected outcomes under each hypothesis
- how many repeats are needed
- what evidence to capture
- how the result will update confidence

### Moonshot Feature: Science Queue

The app maintains a queue:

- "High value: one Saladin fight vs hostile X with crew Y will distinguish two mitigation formulas."
- "Medium value: three repeats needed because proc variance is high."
- "Low value: this fight is redundant with existing evidence."

The player can contribute evidence without needing to understand the underlying model dispute.

## Horizon 4: World Model of Combat

Build learned models that understand combat outcomes while respecting the symbolic simulator.

### Model Types

- surrogate outcome rankers for optimizer proposals
- residual predictors: "where will the simulator be wrong?"
- uncertainty models for recommendation confidence
- trace-level sequence models for round-by-round dynamics
- embedding models for officers, abilities, hostiles, and trigger packages
- hybrid neuro-symbolic models that propose formula corrections

### Guardrails

- learned models propose, simulator confirms
- model-only recommendations are marked experimental
- model drift is tracked across game patches
- out-of-distribution detection is mandatory
- all production claims cite simulation or recorded evidence

### Moonshot Feature: Disagreement Radar

Kobayashi highlights scenarios where:

- simulator confidence is high and learned model agrees
- simulator confidence is high but learned model predicts failure
- simulator confidence is low and evidence is sparse
- observed fights repeatedly contradict both

This tells maintainers where research matters.

## Horizon 5: Autonomous Optimizer Portfolio

Move beyond one optimizer strategy into a self-scheduling portfolio.

### Lanes

- exact exhaustive where feasible
- analytical proxy
- stratified random
- beam search
- genetic search
- cross-entropy sampler
- MAP-Elites quality-diversity archive
- local and large-neighborhood refinement
- surrogate-guided proposals
- robust multi-scenario search
- chain-policy optimization

### Meta-Scheduler

Given an objective and budget, choose:

- which lanes to run
- how much budget each lane gets
- when to stop a lane
- when to transfer discoveries between lanes
- when confidence is high enough
- when to ask for more real fight evidence

### Objective Modes

- best single fight win rate
- fastest farming
- safest chain
- fewest repairs
- low-rarity substitute
- PvP burst
- armada endurance
- robust crew across hostile set
- event points per minute
- event points per repair resource

### Moonshot Feature: Crew Atlas

Instead of a table, expose a map of the search space:

- islands of crew archetypes
- best crew in each behavior cell
- substitute paths
- why each cluster works
- where uncertainty is high

The optimizer becomes exploratory, not just prescriptive.

## Horizon 6: Causal Explanation Engine

Recommendations should be explanations you can trust.

### Explanation Sources

- counterfactual swaps
- ablation tests
- trace deltas
- causal graph paths
- profile headroom analysis
- confidence intervals
- observed fight support

### Output Examples

- "This crew is fastest because it crosses the round-1 kill threshold. Replacing officer A drops expected opening hull damage by 18%."
- "This crew is safer because it delays shield collapse by two rounds, which prevents the hostile's highest damage window from reaching hull."
- "Your profile already saturates this mitigation bucket, so officer B's listed bonus has low marginal value."
- "The recommendation is uncertain because this hostile's special ability is inferred from only two fights."

### Moonshot Feature: Why Not This Crew?

Let users compare:

- recommended crew vs current crew
- two top crews
- a known meta crew vs Kobayashi's suggestion
- a missing-officer substitute vs ideal crew

The answer should be causal, quantitative, and tied to traces.

## Horizon 7: Strategic Player Planner

Kobayashi should optimize player intent across time, not just one battle.

### Planning Domains

- daily hostile grinding
- event route selection
- repair budget management
- officer tier priority
- ship upgrade priority
- forbidden tech selection
- exocomp and support buff timing
- armada team composition
- PvP scouting and counter-crew selection
- multi-ship assignment

### Planning Methods

- constrained optimization
- multi-objective planning
- robust optimization under uncertainty
- bandit learning over repeated player outcomes
- Markov decision processes for chain grind and repair cycles
- what-if simulators for progression choices

### Moonshot Feature: Intent Console

The user types:

> "I have 45 minutes, these ships, this repair budget, and I want event points. What should I do?"

Kobayashi returns:

- target list
- ship and crew sequence
- expected points
- expected repairs
- risk range
- when to swap crew
- what evidence would improve the plan

## Horizon 8: Autonomous Engineering Lab

Frontier models should help maintain Kobayashi itself.

### Agent Roles

- **Triage Agent:** watches failures, drift reports, CI, issue reports, and calibration residuals
- **Mechanic Agent:** proposes formula hypotheses and tests
- **Data Agent:** refreshes upstream data, detects schema drift, and opens reviewable diffs
- **Optimizer Agent:** benchmarks search methods and proposes tuning changes
- **Frontend Agent:** improves evidence display and user workflows
- **Release Agent:** validates bundles, changelogs, and migration notes
- **Reviewer Agent:** audits patches for regressions, missing tests, and unsupported claims

### Required Discipline

- every agent action produces a run manifest
- every PR includes evidence and rollback notes
- no autonomous merge to protected branches
- generated code must pass ordinary human-readable review
- experiments stay behind flags until validated

### Moonshot Feature: Evidence-Backed PRs

A model-generated PR should say:

- what observation triggered the change
- which hypotheses were considered
- why this patch won
- which fights improved
- which fights regressed
- which tests were added
- what uncertainty remains

That is a maintainable autonomy loop.

## Horizon 9: Community Science Network

If users opt in, Kobayashi can become a distributed observatory.

### Capabilities

- privacy-preserving upload of normalized fight evidence
- profile feature bucketing without exposing account identity
- signed evidence bundles
- public mechanic confidence dashboard
- patch-day anomaly detection
- community challenge suites
- reproducible benchmark leaderboards for optimizer methods

### Safeguards

- opt-in only
- local-first by default
- visible data preview before upload
- user and alliance identifiers scrubbed
- no monetized telemetry
- ability to delete local and remote contributions where feasible

### Moonshot Feature: Patch-Day Observatory

When STFC changes:

- detect upstream data diffs
- identify mechanics likely affected
- ask volunteers for targeted fights
- compare observed outcomes to old formulas
- generate a patch-day drift report
- propose simulator updates

## Horizon 10: Open Combat Research Platform

At the far end, Kobayashi becomes a general framework for games with opaque combat systems:

- declarative ability language
- evidence ingestion
- simulator hypothesis engine
- optimizer portfolio
- active experiment design
- causal explanations
- autonomous code/test/doc agents

STFC remains the first and primary domain, but the architecture becomes reusable.

## Concrete Moonshot Milestones

### Milestone A: Evidence-Linked Simulator

- every simulation has a manifest
- every recommendation has provenance
- trace diff tooling exists
- mechanics can be marked known, uncertain, or inferred

### Milestone B: Fight Capture Wizard

- upload screenshots/logs
- bind to profile snapshot
- normalize into combat trace
- grade evidence quality
- store in evidence lake

### Milestone C: Hypothesis Tournament

- define competing formula variants
- run them against evidence
- rank by likelihood and residuals
- generate falsifying tests

### Milestone D: Active Experiment Queue

- choose next best fights by information gain
- guide player through capture
- update hypothesis confidence after ingest

### Milestone E: Crew Atlas

- quality-diversity archive
- behavior map
- substitute paths
- causal explanations per cluster

### Milestone F: Strategic Intent Planner

- optimize across time, repairs, events, and resources
- produce actionable plans with confidence ranges
- learn from player-confirmed outcomes

### Milestone G: Evidence-Backed Autonomous PRs

- agents propose simulator/data/optimizer patches
- CI runs calibration and benchmark gates
- PR includes evidence packet and uncertainty notes

### Milestone H: Patch-Day Observatory

- upstream drift detection
- targeted evidence requests
- community opt-in fight collection
- patch impact report
- simulator update proposals

## Technical Bets Worth Making

- **Neuro-symbolic mechanics discovery:** use learned models to identify residual patterns, then synthesize symbolic simulator patches.
- **Bayesian experimental design:** choose fights by expected information gain.
- **Causal graphs:** represent triggers, buffs, and outcomes as intervention-ready structures.
- **Program synthesis:** generate small candidate formulas or effect handlers from traces.
- **Quality-diversity search:** map viable crew families instead of chasing one optimum.
- **World-model ensembles:** predict where the simulator is likely wrong.
- **Agentic software maintenance:** let frontier models generate evidence-backed patches, but keep deterministic validation and human merge gates.
- **Privacy-preserving community learning:** aggregate evidence without making local-first users pay a trust tax.

## What This Changes About Today's Roadmap

The practical optimizer roadmap is still the right next step. But moonshot alignment changes the why:

- telemetry is not just performance logging; it is the evidence layer
- optimize history is not just cache; it is the seed of an observation corpus
- Pareto recommendations are not just UI polish; they are the first Crew Atlas
- local refinement is not just search; it is counterfactual explanation infrastructure
- benchmark baselines are not just engineering hygiene; they are scientific controls
- profile snapshots are not just sync artifacts; they are experimental conditions

The moonshot does not invalidate practical work. It gives it a larger shape.

## Risks

- **False authority:** polished explanations can make weak evidence look strong.
- **Data contamination:** mixed profile snapshots can corrupt mechanics inference.
- **Patch drift:** the game changes faster than evidence can stabilize.
- **Overfitting:** formula changes may fit stale fights and regress current reality.
- **Privacy:** community science must not quietly become surveillance.
- **Complexity:** a brilliant lab that nobody can use is a failure.
- **Agent autonomy:** code-writing agents need hard review and validation boundaries.

## Non-Negotiables

- local-first remains the default
- no hidden telemetry
- no model-only claims presented as truth
- no unreviewed autonomous merges
- every accepted mechanic change gets tests
- every recommendation can expose its evidence level
- uncertainty must be visible, not buried

## The Dream

Kobayashi starts as a simulator. Then it becomes an optimizer. Then it becomes a research assistant.

The moonshot is the next identity:

> Kobayashi is an autonomous combat science lab for STFC.

It helps players win fights, but the deeper promise is stranger and better: it can learn the game.
