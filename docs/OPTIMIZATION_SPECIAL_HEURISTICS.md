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
| Scenario eligibility (matrix) | all seats | [`is_eligible_for_optimization`](../src/data/officer_eligibility.rs) — drop officers whose seat ability is `does_not_work` for the scenario; legacy fallback for coverage gaps (see below) |
| Below-decks ordering | below-decks | combat relevance rank + LCARS power tiebreak ([`sort_below_decks_by_rank_and_power`](../src/optimizer/crew_generator.rs)) |
| Search constraints | pools narrowed | [`narrow_officer_pools_for_constraints`](../src/optimizer/crew_generator.rs) — `captain_must_be`, exclude lists, officer groups |

Default API below-decks mode is **strict** ([`BelowDecksPoolMode::default`](../src/data/heuristics.rs)).

---

## Officer eligibility matrix (scenario filter)

The primary scenario-eligibility filter is **data-driven**, not hand-coded. It answers one question: *does this officer's seat ability actually function against the target?* — and prunes officers whose ability does nothing, across **all seats** (captain / bridge / below-decks).

**Source & generation.** The community cheat-sheet (`data/upstream/cheat-sheet/raw-officers-*.csv`, one row per ability) is normalized into [`data/officers/eligibility_matrix.json`](../data/officers/eligibility_matrix.json) by [`cargo run --bin import_officer_eligibility`](../src/bin/import_officer_eligibility.rs). The matrix is keyed by **`ability_id`** (CSV `AbilityID` == [`OfficerAbility.ability_id`](../src/data/officer.rs)); the officer is joined via `source_officer_id` → `data/officers/id_registry.json`. Output is deterministic (sorted), so re-running after a cheat-sheet refresh produces a clean diff.

**Verdicts.** Each ability carries a per-scenario verdict + reason for the 12 combat scenarios ([`EnemyType`](../src/combat/types.rs)):

| glyph | verdict | optimizer effect |
| --- | --- | --- |
| ✅ | `works` | eligible |
| ✴️ | `conditional` — works only if in-combat conditions are met (morale up, target burning, …) | **eligible** (the engine resolves the condition dynamically) |
| ➖ | `does_not_work` — non-combat/loot, wrong target class, PvP-only, … | **excluded** (this is the hard filter) |

**Scenario resolution.** The scenario comes from the optional `enemy_type` request field (snake_case `EnemyType`); when unset it is inferred from the target (PvP → `pvp_space`; group-armada hostile → `group_armadas`; outpost → `outpost_armadas`; else `red_moving_space`). See [`resolve_enemy_type`](../src/data/officer_eligibility.rs).

**Where applied.** [`is_eligible_for_optimization`](../src/data/officer_eligibility.rs) is the shared predicate. Pool construction prunes ineligible officers per seat before generation ([`build_officer_pools_from_registry`](../src/optimizer/crew_generator.rs)); [`apply_crew_constraints`](../src/optimizer/mod.rs) and [`enforce_candidate_legality_inner`](../src/optimizer/mod.rs) re-apply it to generated, seed, warm-start, and history crews so the rule is hard across exhaustive, sampled, tiered, and genetic strategies. In simulation it is interpretability-only: [`simulate_payload`](../src/server/api.rs) emits `eligibility_notes` + warnings and never rejects the player's crew.

**Coverage & fallback.** Officers/abilities **absent from the cheat-sheet** fall back to the legacy heuristics in [`src/data/heuristics.rs`](../src/data/heuristics.rs) (`EnemyPlayer` below-decks exclusion outside PvP, `NON_COMBAT_BELOW_DECKS_MODIFIERS`, and the ban list below). As of cheat-sheet `m90-17rc` vs catalog `m86`, the matrix covers **570/574** catalog abilities; the only gaps are 2 officers the community sheet does not yet rate — **Chancellor Ake** and **Deidamia** (officer + below-decks) — which use the heuristic fallback until the sheet adds them. Separately, **V'Ger Ilia** is present in the sheet but not yet in the `m86` catalog (it appears as an orphan in the importer's coverage report); it joins once the canonical catalog is refreshed to `m90`. The importer prints the current coverage report to stderr.

**Refreshing.** When the upstream cheat-sheet updates, re-export its `RawOfficers` / `MasterOfficers` / `Officers Compact` sheets to `data/upstream/cheat-sheet/*-<version>.csv` (see that directory's `README.md`), point `DEFAULT_CSV_REL` in [`import_officer_eligibility.rs`](../src/bin/import_officer_eligibility.rs) at the new `raw-officers-*.csv`, and re-run `cargo run --bin import_officer_eligibility`. Output is sorted/deterministic, so the matrix diff is clean.

---

## Ban lists (curation opt-out)

The ban lists are **not** functional-eligibility filters — they are a curation layer for officers whose abilities *technically* function but are too weak to be worth spending simulation budget on. They are intentionally kept alongside the matrix.

- **Captain ban list** — [`data/optimizer/captain_ban_list.json`](../data/optimizer/captain_ban_list.json), removed from the **captain pool only** ([`captain_ban.rs`](../src/data/captain_ban.rs)); see the first section above.
- **`PVP_BELOW_DECKS_BANNED_SOURCE_IDS`** ([`src/data/heuristics.rs`](../src/data/heuristics.rs)) — curated below-decks opt-outs for PvP. Keyed by upstream `source_officer_id`.

> **Always-on (overrides the matrix):** the PvP below-decks ban is applied **on top of** the matrix in [`is_eligible_for_optimization`](../src/data/officer_eligibility.rs) — a banned officer is excluded from PvP below-decks regardless of its matrix verdict, *even `works`/`conditional`*. That is precisely what makes it a curation opt-out (suppress weak-but-functional officers) rather than a functional filter. It is **optimization-only**: simulation still reports the true matrix verdict as an eligibility note, so the player always sees ground truth.

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
| Scenario effectiveness gates | all seats (matrix-covered officers) | Now provided by the **officer eligibility matrix** above — officers whose ability is `does_not_work` for the scenario (armada vs non-armada, loot vs combat, PvP-only, Borg-Cube-only, etc.) are pruned across all seats. Officers **absent from the cheat-sheet** (see *Coverage & fallback*, e.g. Mara Dalen vs non-armada) still rely on the heuristic fallback — treat non-matching abilities as **effectiveness = 0** when hand-picking those. |
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

Explicit `strategy` values: `tiered`, `exhaustive`, `genetic`, and `linear_eval` — a single analytical ranking pass with no Monte Carlo and no prefilter ([`parse_strategy`](../src/server/api/requests.rs); see [`DESIGN.md`](DESIGN.md) §6.2.1).

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
