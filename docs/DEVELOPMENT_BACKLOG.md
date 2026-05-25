# Development backlog

Ten engineering tasks for the next chunk of work on kobayashi, ordered by logical
dependency rather than priority. Item numbering is referenced by PR titles and commit
messages.

Sibling docs:

- [`ROADMAP.md`](ROADMAP.md) — shipped + planned **product** capabilities (PvP, sensitivity
  methods, etc.). This backlog is more operational: each item is roughly a PR or two,
  and each has a defined endpoint.
- [`NOT_ROADMAP.md`](NOT_ROADMAP.md) — explicit non-goals.

The ordering has a soft dependency chain in the first half (1 → 2 → 3 are foundation;
4 → 5/6 → 7 has a paving-the-way relationship). Items 8–10 are largely independent and
can be reordered.

---

## Foundation & quality

### 1. Recorded-fight calibration harness for multi-source CDR

PR #188 fixed `hostile_crit_damage_reduction_active_at_round` from `max`-aggregation to
per-round additive stacking. It's justified by analytical unit tests
([`src/combat/abilities.rs`](../src/combat/abilities.rs) `hostile_crit_reduction_*`) but
not against real game data — the prevalence audit for PR #191 found that no recorded
fight in `tests/fixtures/recorded_fights/` covers a crew with multiple overlapping CDR
sources (Crozier hull + `player_crit_damage_reduction` profile bonus + Borg Operating
Table tech vs Conqueror Borg). Capture a handful of such fights and add a calibration
test that exercises them end-to-end.

**Endpoint:** `tests/recorded_fight_calibration_tests.rs` gains a `multi_source_cdr`
test that runs each captured trace through the engine and asserts effective CDR per
round matches in-game observation within tolerance.

**Blocking dependency:** requires real game traces — not something the engine can
produce. The user has to capture them.

---

### 2. Extract shared async-job runner

`src/server/api/execution.rs` (optimize jobs) and `src/server/sensitivity_jobs.rs` (PR
\#192) share ~250 lines of structural plumbing: in-memory `HashMap<String, JobState>`
registry, `Arc<AtomicBool>` cancel flags, oldest-finished eviction, `lock_*` helpers
with poison recovery, deterministic job-id generation. Refactor into a generic
`JobRegistry<S: JobState>` module so the third async route doesn't accumulate a third
copy of the same pattern.

**Endpoint:** new `src/server/job_registry.rs` providing `JobRegistry<S>` + the
`JobState` trait. Both optimize and sensitivity migrate to use it. Existing tests pass
unchanged.

**Why now:** doing it **before** a third async route appears is much cheaper than after.
Two copies are tolerable; three start to drift.

---

### 3. Automate the benchmark baseline refresh

`bench-refresh-baseline.yml` is `workflow_dispatch`-only. Across PRs #181–#188 the
baseline went stale and the regression gate silently became a no-op (every PR reported
"-77%" wins). PR #189 refreshed it manually. Add a monthly cron schedule and an
auto-open-PR step so the baseline can't drift unattended again.

**Endpoint:** workflow runs on `schedule: cron: …` the 1st of each month, runs the
benches, and opens a PR against main with the updated `benchmark_results.log` and a
before-/after comparison in the body. Skip if no diff.

---

## Engine accuracy / model fidelity

### 4. Finish `player_crit_damage_reduction` uniform plumbing

The open bullet in [`ROADMAP.md`](ROADMAP.md) under "Stat modeling improvements". PR
\#187 moved the sensitivity-perturb scalar from `SimulationConfig` to
`Combatant.crit_damage_reduction_bonus` and deferred the rest because the cleanest
unification — adding a new `HostileCritDamageReductionBonus` effect variant the
seat-walk sums on top of `max` — touches the combat engine for a sensitivity-only
concern (see PR #187's "max-vs-additive note").

If a second additive-bonus stat surfaces from items #5 or #6 below, do this refactor
generically — the parallel `*Bonus` effect variant pattern. Otherwise, this stays
deferred.

---

### 5. Defender-side support buffs + alliance debuffs as scenario inputs

[`ROADMAP.md`](ROADMAP.md) "Planned" — currently *partial*: defender-static support buff
keys apply in PvP-shaped scenarios, but **alliance debuffs are not yet scenario
inputs**. Completes the PvP-modeling story so the optimizer can search against an
opponent's full stack.

**Endpoint:** `SimulateRequest` / `OptimizeRequest` gain a `defender_support_buffs`
and a `defender_alliance_debuffs` field; scenario builder applies them symmetrically to
the player's existing `support_buffs` path.

---

### 6. Per-sub-round vs profile-only timing for forbidden-tech effects

Calibration uncertainty flagged in [`data/README.md` § Forbidden tech](../data/README.md).
Some forbidden-tech bonuses may apply per-sub-round in-game but the engine treats them
as profile-flat. The right resolution path goes through the calibration harness from
item #1 — record a trace with the tech equipped, compare engine output, then bind the
correct timing in the data import.

**Endpoint:** `data/forbidden_chaos_tech.json` rows gain an explicit `timing` field
where calibration shows a per-sub-round pattern; engine reads it; integration test in
`tests/recorded_fight_calibration_tests.rs` exercises the divergent rows.

**Soft dependency:** ideally lands after item #1 so it has the calibration harness to
verify against.

---

## New features

### 7. Building catalog API + UI panel

[`ROADMAP.md` § Buildings](ROADMAP.md). The building catalog is consumed silently during
scenario load — there's no way for users to inspect what combat bonuses they're
actually getting from their synced buildings. Surface it.

**Endpoint:** new `GET /api/buildings/catalog` returning per-building stats + the
user's synced levels + the contribution to their effective profile bonuses. New
"Buildings" panel in the React SPA renders the catalog with the synced levels
highlighted. This is also the foundation for the deferred station-defense work in
`NOT_ROADMAP.md`.

---

### 8. Strict validation report for opaque `buff_*` stats

[`ROADMAP.md` § Buildings](ROADMAP.md). The data-normalization pipeline silently skips
`buff_*` keys it doesn't know how to map. Extend `report_unknown_mappings` (already
wired into `cargo xtask` and the data-refresh workflow) to also enumerate unmapped
building buff IDs and forbidden-tech bonus keys. Forces the data import to either map
each, explicitly opt-out via an allowlist, or fail loudly.

**Endpoint:** existing `report_unknown_mappings` binary gains two new sections.
`docs/CANONICAL_CONDITIONS.md`-style "Still unmapped: 0" goal applies to both.

---

### 9. Synergy learning from simulation results

[`ROADMAP.md` "Planned"](ROADMAP.md). Use the accumulated `optimize_history.json` cache
(`MAX_OPTIMIZE_HISTORY_CREWS = 24` per cache key, `MAX_OPTIMIZE_CACHE_KEYS = 200`) to
build a per-officer-pair co-occurrence statistic, then feed it as a prior into the
analytical-prefilter ranking — pairs that historically rank well together get a small
positive bump.

**Endpoint:** new module `src/optimizer/synergy_learning.rs` computing the
co-occurrence statistic offline (or lazily), gated by an opt-in request field
`use_learned_synergies: bool` so users can A/B compare. Choice of statistic (raw
co-occurrence vs lift vs pointwise mutual information) is the open design question —
start with lift since it's the most interpretable.

---

## Long-horizon

### 10. Armada mode (multi-ship combat)

[`ROADMAP.md` "Planned"](ROADMAP.md). Major feature — multiple ships per side, target
selection, fire-distribution rules, hull-breach propagation. Not a single PR; needs a
design pass first to decide which in-game armada mechanics are in scope and which are
deferred (e.g. armada commendation timers, multiple armada types).

**Note:** this is the only item on the backlog that isn't well-scoped today.
Everything else has a defined endpoint; #10 needs a design doc before code.

---

## Excluded from this backlog

- **Full LCARS coverage of 280+ officers** — on `ROADMAP.md` but is incremental data
  work, not a discrete PR. Tracked via the fidelity score in
  [`OFFICER_MODELING_SCORECARD.md`](OFFICER_MODELING_SCORECARD.md).
- **Station-defense mode in the optimizer** — currently in
  [`NOT_ROADMAP.md`](NOT_ROADMAP.md) pending broader station-defense scope. Becomes a
  follow-up to #7 if station defense moves in-scope.

---

## Maintenance notes

- This document should be updated when items ship (mark them shipped + link the PR) or
  when new items emerge that fit the "well-scoped engineering task" profile.
- Items shipped should be removed (not struck out) — the corresponding ROADMAP.md
  bullets are the durable record of "what shipped." This file is the working queue.
