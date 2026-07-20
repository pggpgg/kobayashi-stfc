# Roadmap

Future work for KOBAYASHI. This document is intentionally forward-looking: it should say what is worth doing next, what order roughly makes sense, and where deeper planning lives. Shipped work belongs in PRs, release notes, or the specialized design docs that describe the current system.

Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md). The next-generation optimizer plan lives in [OPTIMIZER_AMBITIOUS_ROADMAP.md](OPTIMIZER_AMBITIOUS_ROADMAP.md), and the longer-range research-lab vision lives in [KOBAYASHI_MOONSHOT_ROADMAP.md](KOBAYASHI_MOONSHOT_ROADMAP.md).

_Last updated 2026-07-20._

## Near-Term Priorities

### Product Polish and Speed

Keep refining the React SPA as a working tool, not a marketing surface. The main product risk is not missing a giant redesign; it is friction in repeated day-to-day use.

Planned focus:

- Make dense optimizer controls easier to scan without hiding important state.
- Improve result-table readability for wide recommendation sets and narrow screens.
- Surface method provenance, confidence, and caveats in compact result-row affordances.
- Keep profile, component, sync, and roster state visible enough that users can trust which inputs drove a recommendation.
- Continue extracting shared UI pieces only where it reduces real duplication or render churn.

### Recommendation Quality

Improve the crews Kobayashi finds without making the optimizer harder to reason about. Exact legality filters stay authoritative; heuristics should prioritize, explore, or allocate budget, not silently delete valid crews.

Planned focus:

- Add local refinement around tiered and genetic finalists: one-slot bridge swaps, below-decks swaps, captain-preserving variants, and small repair neighborhoods.
- Add Pareto tags and recommendation reasons over existing metrics such as win rate, loss rate, round-1 kills, hull remaining, defender hull remaining, confidence width, and chain-grind utility.
- Expand benchmark controls so new lanes are judged against current tiered, genetic, analytical, warm-start, and stratified-random baselines.
- Use observation logs and method provenance to explain which search path found each recommendation and whether it was confirmed deeply enough.

See [OPTIMIZER_AMBITIOUS_ROADMAP.md](OPTIMIZER_AMBITIOUS_ROADMAP.md) for the staged implementation plan.

### Evidence and Provenance

Make recommendations and simulations easier to audit. Kobayashi should be able to answer "what inputs, simulator version, data snapshot, method, seed budget, and uncertainty produced this result?"

Planned focus:

- Broaden run manifests for optimize, simulate, import, calibration, and benchmark jobs.
- Add stronger simulator and catalog fingerprints to durable optimizer observations.
- Improve retention, compaction, and offline tooling for observation logs.
- Expose uncertainty and evidence level in API responses where the UI can use them.
- Keep trace diffs and calibration residuals structured enough to support future hypothesis testing.

### Combat and Data Fidelity

Keep the simulator aligned with known STFC mechanics while avoiding speculative overreach. New mechanics should be grounded in upstream ability text, existing engine hooks, community-documented behavior, or controlled fixtures.

Planned focus:

- Continue retiring high-impact `combat_noop` ability families from the ship and hostile audits.
- Keep upstream drift detection actionable after data refreshes.
- Extend hostile faction and ship-type mapping when new upstream ids appear.
- Improve validation warnings where malformed or unsupported data would otherwise compile to no effect.
- Preserve deterministic seeded behavior when adding new mechanics or RNG draws.

See [HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md](HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md) and [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) for detailed inventories.

## Later Horizons

### Optimizer Portfolio

After the practical recommendation-quality work, turn the optimizer into an explicit portfolio of search lanes:

- beam search with diversity lanes
- quality-diversity archives
- adaptive simulation budgets and racing
- surrogate-assisted proposals that nominate, never crown, crews
- richer substitute search for missing officers or low-rarity alternatives

### Combat Research Loop

Build toward a simulator that can compare hypotheses instead of treating every uncertain mechanic as a single hard-coded truth:

- named mechanic hypotheses behind deterministic switches
- residual clustering by mechanic family
- experiment suggestions for the smallest fight set that would distinguish hypotheses
- evidence-linked patches and regression traces

### Strategic Planning

Extend beyond single-fight crew selection once combat recommendations are auditable enough:

- chain-grind planning with repair and risk constraints
- officer and ship upgrade marginal-value estimates
- event and progression planning for a real profile
- support-buff, exocomp, and resource-allocation tradeoffs

## Out of Scope

Do not add completed-work summaries here. If a shipped item needs durable documentation, update the design or subsystem doc that explains the current behavior. If an idea is intentionally deferred or declined, put it in [NOT_ROADMAP.md](NOT_ROADMAP.md).
