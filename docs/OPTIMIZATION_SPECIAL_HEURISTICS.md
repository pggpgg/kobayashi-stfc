# Optimization Special Heuristics

This document describes **heuristics that Kobayashi actually applies today** during crew optimization — hard filters, pool shaping, ranking priors, and strategy routing. It complements the aspirational catalog in [`OPTIMIZATION_HEURISTICS.md`](OPTIMIZATION_HEURISTICS.md), which lists ideas not yet implemented.

For player-provided seed crews (`data/heuristics/*.txt`), see also the **Heuristics seeds** section in [`CLAUDE.md`](../CLAUDE.md).

---

## Captain ban list

**Purpose:** Until a winning crew is found, simulation budget is better spent on captains whose abilities affect combat, not economy or logistics officers like Quark.

**Behavior:** Officers on the ban list are removed from the **captain pool only**. They may still appear on bridge or below-decks when seat rules allow.

**Data:** [`data/optimizer/captain_ban_list.json`](../data/optimizer/captain_ban_list.json)

```json
{
  "canonical_ids": ["quark-2fd57b", "airiam-9265fc"],
  "names": []
}
```

- **`canonical_ids`** — ids from `data/officers/officers.canonical.json` (preferred).
- **`names`** — optional display names resolved via the canonical catalog (same normalization as roster import).

**Code:** loaded in [`src/data/captain_ban.rs`](../src/data/captain_ban.rs); applied in [`is_captain_eligible`](../src/optimizer/crew_generator.rs) when building officer pools.

**Extending:** add a canonical id or name to the JSON and restart the server/process (list is cached for the process lifetime).

---

## Officer pool construction

Before any strategy runs, [`build_officer_pools*`](../src/optimizer/crew_generator.rs) builds captain, bridge, and below-decks name pools.

| Filter | Scope | Source |
| --- | --- | --- |
| Empty names dropped | all pools | `build_officer_pools_inner` |
| Roster import | all pools | `profiles/{id}/roster.imported.json` unlocked ids only |
| Captain ability required | captain | `is_captain_eligible` (captain-slot ability + not banned) |
| Seat compatibility | bridge / BD | `can_fill_position` from officer `slot` field |
| Below-decks pool mode | below-decks | [`BelowDecksPoolMode`](../src/data/heuristics.rs): **strict/scored** = must have below-decks ability; **relaxed** = any legal BD seat |
| Below-decks ordering | below-decks | combat relevance rank + LCARS power tiebreak ([`sort_below_decks_by_rank_and_power`](../src/optimizer/crew_generator.rs)) |
| Search constraints | pools narrowed | [`narrow_officer_pools_for_constraints`](../src/optimizer/crew_generator.rs) — `captain_must_be`, exclude lists, officer groups |

Default API below-decks mode is **strict** ([`BelowDecksPoolMode::default`](../src/data/heuristics.rs)).

---

## Candidate generation (exhaustive / sampled)

[`CrewGenerator`](../src/optimizer/crew_generator.rs) enumerates or samples crews from the pools.

| Heuristic | When | Detail |
| --- | --- | --- |
| Exhaustive vs sampled | pool size vs `exhaustive_pool_threshold` (default 12) | Small pools: full cartesian product; large pools: capped captain/bridge slices + stride sampling |
| Large-pool caps | sampled path | `large_pool_captain_limit` (10), `large_pool_bridge_limit` (12) |
| Seeded shuffle | sampled path | deterministic shuffle of pool order from seed |
| Learned BD sampling | sampled path + `learned_officer_scores` | epsilon-greedy weighted below-decks picks ([`officer_learning.rs`](../src/optimizer/officer_learning.rs)) |
| Warm-start prepend | all non-genetic paths | [`prepend_warm_start_dedupe`](../src/optimizer/mod.rs) — client / history crews first, deduped by stable hash |
| Crew constraints | post-generation | [`filter_candidates`](../src/optimizer/constraints.rs) |

---

## Heuristics seeds (`data/heuristics/*.txt`)

Parsed in [`src/data/heuristics.rs`](../src/data/heuristics.rs). Expanded before or merged into the main optimize path depending on flags.

| Filter | Applies to | Rule |
| --- | --- | --- |
| Name resolution | all seed officers | aliases → exact → unique substring |
| Bridge synergy filter | bridge officers in seeds | drop bridge unless [`bridge_synergy_strength`](../src/data/heuristics.rs) > `Neither` (same group and/or bridge-slot ability) |
| Below-decks combat filter | BD candidates in seeds | when strict (server default): drop economy modifiers ([`NON_COMBAT_BELOW_DECKS_MODIFIERS`](../src/data/heuristics.rs)); relaxed when `below_decks_pool_mode = relaxed` |
| Scenario ability predicates | *not yet applied to BD pool* | Officers whose below-decks (or bridge) abilities require **PvP**, **armada**, **Borg Cube**, or **loot-only** contexts still enter pools for standard ship-vs-hostile fights. Until scenario-conditioned effectiveness gates ship, treat these as **effectiveness = 0** when hand-picking crews (e.g. Borg Queen BD vs NPC hostiles, Mara Dalen / Zefram Cochrane BD vs non-armada, Phlox BD outside Borg Cube fights, Trip Tucker loot vs non–Species-8472/Hirogen). Roadmap: evaluate LCARS `condition` against ship/hostile/defender_opponent before counting a seat toward optimize rank. |
| Below-decks expansion | BD list longer than ship slots | **ordered** (first k) or **exploration** (all C(n,k)) via `below_decks_strategy` |

**Server flags** ([`src/server/api/execution.rs`](../src/server/api/execution.rs)):

- **`heuristics_only`** — simulate seed crews only; skip main search.
- **`fast_discovery`** — merge expanded seeds into warm-start (cap 480); skip standalone all-seed MC when main strategy runs.
- **`heuristics_seeds`** — which `.txt` stems to load.

---

## Analytical prefilter

**Purpose:** Rank crews by a closed-form damage proxy before expensive Monte Carlo; optionally truncate to top `keep`.

**Code:** [`sort_and_analytical_prefilter`](../src/optimizer/mod.rs), scoring in [`matchup_priors::analytical_prefilter_rank_score`](../src/optimizer/matchup_priors.rs) and [`analytical::expected_damage`](../src/optimizer/analytical.rs).

| Component | Effect |
| --- | --- |
| Expected damage | primary sort key |
| Static gate prior | bump crews whose LCARS conditions match ship/hostile/PvP context |
| Bridge synergy score | [`bridge_synergy_prefilter_score`](../src/data/heuristics.rs) |
| Warm-start / history overlap | crews overlapping reference crews sort higher |
| Learned pair prior | optional (`enable_learned_pair_prior`, default true) — rewards captain–bridge pairs seen in warm-start/history ([`learned_pair_prior_score`](../src/optimizer/matchup_priors.rs)) |

**Auto keep** when client omits `analytical_prefilter_keep`: [`analytical_prefilter_keep_auto`](../src/optimizer/mod.rs) — scales with candidate count, `max_candidates`, tiered top-K, and per-crew sim workload. Skipped for chain grind and genetic strategy.

**Chain grind:** prefilter disabled when `chain_grind` is set ([`analytical_prefilter_unless_chain`](../src/optimizer/mod.rs)).

---

## Strategy auto-routing

When the client omits `strategy`, the server picks **tiered** vs **exhaustive** from effective candidate count ([`resolve_effective_optimize_strategy`](../src/server/api/execution.rs)):

- **Tiered** if `count_effective_optimize_candidates` ≥ 400 (`TIERED_AUTO_THRESHOLD`)
- **Exhaustive** otherwise

Genetic and explicit strategy requests bypass auto-routing. Response field `strategy_auto` indicates auto selection.

---

## Tiered scout → confirm

[`src/optimizer/tiered.rs`](../src/optimizer/tiered.rs)

1. **Scout** — low sims per crew (default 500; workload-scaled down for huge pools).
2. **Adaptive scout** (default) — coarse pass then Wilson-interval refine on crews near the top-K cut.
3. **Confirm** — full `simulation_count` on top `tiered_top_k` (default 20; workload-scaled up).

Resolved defaults: [`tiered_scout_sims_for_workload`](../src/optimizer/tiered.rs), [`tiered_top_k_for_workload`](../src/optimizer/tiered.rs).

**Optimize history:** matching crews reuse stored scout/confirm rows ([`optimize_history::preconfirmed_for_candidates`](../src/data/optimize_history.rs)).

---

## Exhaustive two-phase (adaptive)

When exhaustive path sets `exhaustive_scout_sims` + `exhaustive_scout_top_keep`, [`run_exhaustive_scout_then_full_mc`](../src/optimizer/exhaustive_adaptive.rs) scouts all candidates then confirms top keep with variable full sims (ranking-aligned widths, same confirm allocator ideas as tiered).

---

## Genetic optimizer

[`src/optimizer/genetic.rs`](../src/optimizer/genetic.rs) — population evolution with optional warm-start seeding, adaptive mutation, constraint repair, offspring reduced scout budget, incremental elite fitness. Uses the same officer pools (including captain ban). Analytical prefilter is not applied on the GA path unless explicitly configured via `analytical_prefilter_keep`.

---

## Persisted learning and cache

| Mechanism | File / API | Use |
| --- | --- | --- |
| **Optimize history** | `profiles/{id}/optimize_history.json` | Skip tiered/exhaustive MC for matching crews; inject prior reference crews into analytical ranking |
| **Officer learning scores** | `profiles/{id}/officer_learning.json` | Bias below-decks sampling in large-pool generation |
| **Warm-start crews** | API `warm_start_crews` | Prepended deduped; legality enforced via [`enforce_candidate_legality_with_registry`](../src/optimizer/mod.rs) |
| **Novelty MMR** | API `novelty_lambda` + history anchors | [`apply_novelty_mmr_if_configured`](../src/optimizer/ranking.rs) — diversify final ranking |

Prior reference crews from history (not prepended as candidates): [`prior_reference_crews_for_matchup_priors`](../src/data/optimize_history.rs).

---

## Ranking

[`src/optimizer/ranking.rs`](../src/optimizer/ranking.rs) — default composite: win rate, hull remaining, round-1 kill rate (see module for weights). Chain grind can add secondary objectives ([`chain.rs`](../src/optimizer/chain.rs)).

---

## Related docs

- [`OPTIMIZATION_HEURISTICS.md`](OPTIMIZATION_HEURISTICS.md) — research backlog of additional search-space reductions and vulnerability heuristics **not** listed above.
- [`docs/DESIGN.md`](DESIGN.md) §6–7 — analytical prefilter and synergy design notes.
