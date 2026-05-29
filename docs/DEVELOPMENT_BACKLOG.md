# Development backlog

Five engineering tasks for the next chunk of work on kobayashi, ordered by logical
dependency rather than priority. Item numbering is referenced by PR titles and commit
messages.

Sibling docs:

- [`ROADMAP.md`](ROADMAP.md) — shipped + planned **product** capabilities (PvP, sensitivity
  methods, etc.). This backlog is more operational: each item is roughly a PR or two,
  and each has a defined endpoint.
- [`NOT_ROADMAP.md`](NOT_ROADMAP.md) — explicit non-goals.

Item 1 is the calibration foundation; item 2 is blocked on it. Items 3–4 are largely
independent and can be reordered. Item 5 needs a design doc before code.

Recently shipped (removed from this queue; see [`ROADMAP.md`](ROADMAP.md)): shared
async job registry (#194), monthly benchmark baseline refresh (#195), sensitivity async
+ SSE (#192), defender/alliance debuff scenario inputs (340c5b0c), sensitivity
`crit_damage_reduction` removal (#196), buildings sync → combat observability
(`GET /api/profile/buildings-summary`, Roster & Profile UI; f53ddd17).

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

## Engine accuracy / model fidelity

### 2. Per-sub-round vs profile-only timing for forbidden-tech effects — **calibration-blocked**

Calibration uncertainty flagged in [`data/README.md` § Forbidden tech](../data/README.md):
some forbidden-tech bonuses *may* apply per-sub-round in-game while the engine treats
them as profile-flat.

**Architecture finding (investigated, no game data yet).** The flat-vs-timed split
already exists and is deliberate:

- **Timed seats** ([`src/data/profile.rs`](../src/data/profile.rs) ~658–937) already
  handle every effect that genuinely needs per-round or conditional timing — Borg Alcove
  conditional crit (attack-phase, Voyager-only), Quantum Slipstream's *cumulative*
  shield-mitigation debuff (round-start), Borg Operating Table (Conqueror-Borg-gated),
  and the S31 / Control Seeker / Dual Photon torpedo family (ship-class-gated, with
  per-stat windows).
- **Profile-flat** handles the rest: passive always-on percentage multipliers (Romulan
  Mining Laser, Ablative Armor, Transphasic Torpedoes, …). The engine applies the profile
  bonus on every shot, so flat application already *is* per-shot for these — "per-sub-round"
  only changes outcomes for effects that accumulate or turn on conditionally, and those are
  already seats.

So the architecture is likely already correct, and the README note is an un-validated
hypothesis rather than a known defect.

**Why the literal "add a `timing` field" endpoint is the wrong shape:**

1. **No confirmed consumer.** No currently-flat tech can be shown to need `per_sub_round`
   without a real game trace — every row would default to `profile`, so the field would
   be a mechanism with no user.
2. **A flat enum can't express the model.** The existing seat routing encodes *conditions*
   (Voyager-only, Conqueror-Borg-only, ship-class match) and *per-stat timing splits*
   within one tech. A `timing` field on a `BonusEntry` can't represent any of that; it
   would be a weaker parallel mechanism, not a replacement for the hardcoded logic.

**Blocked on:** item #1 (recorded-fight calibration harness), which is itself blocked on
real game traces. The only valid resolution path is: capture a trace with a suspect tech
equipped, compare engine output round-by-round, and *then* decide whether any currently-flat
tech needs reclassification as a timed seat (not a data-field toggle).

**Doable-now alternative (not yet built):** a timing-coverage diagnostic that enumerates
every catalog tech's current treatment (flat profile key vs seat + window + gating
condition), turning the implicit hardcoded routing into a reviewable artifact for the #1
calibration step.

---

## New features

### 3. Strict validation report for opaque `buff_*` stats

[`ROADMAP.md` § Buildings](ROADMAP.md). The data-normalization pipeline silently skips
`buff_*` keys it doesn't know how to map. Extend `report_unknown_mappings` (already
wired into `cargo xtask` and the data-refresh workflow) to also enumerate unmapped
building buff IDs and forbidden-tech bonus keys. Forces the data import to either map
each, explicitly opt-out via an allowlist, or fail loudly.

**Endpoint:** existing `report_unknown_mappings` binary gains two new sections.
`docs/CANONICAL_CONDITIONS.md`-style "Still unmapped: 0" goal applies to both.

**Why now:** the Roster & Profile buildings panel already surfaces unmapped game `bid`
values (e.g. bids with no catalog entry); this task closes the loop on opaque
`buff_*` keys inside mapped buildings — see [`building_gaps.md`](building_gaps.md).

---

### 4. Synergy learning from simulation results

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

### 5. Armada mode (multi-ship combat)

[`ROADMAP.md` "Planned"](ROADMAP.md). Major feature — multiple ships per side, target
selection, fire-distribution rules, hull-breach propagation. Not a single PR; needs a
design pass first to decide which in-game armada mechanics are in scope and which are
deferred (e.g. armada commendation timers, multiple armada types).

**Note:** this is the only item on the backlog that isn't well-scoped today.
Everything else has a defined endpoint; #5 needs a design doc before code.

---

## Excluded from this backlog

- **Full LCARS coverage of 280+ officers** — on `ROADMAP.md` but is incremental data
  work, not a discrete PR. Tracked via the fidelity score in
  [`OFFICER_MODELING_SCORECARD.md`](OFFICER_MODELING_SCORECARD.md).
- **Buildings drill-down (optional)** — per-module combat-stat contribution breakdown
  and/or a global catalog browse API. Core observability (synced levels, aggregate
  combat bonuses, unmapped `bid` callouts) already ships via
  [`GET /api/profile/buildings-summary`](../src/server/api.rs) and the Roster & Profile
  panel. Revisit only if users need row-level attribution or offline catalog inspection.
- **Station-defense mode in the optimizer** — currently in
  [`NOT_ROADMAP.md`](NOT_ROADMAP.md) pending broader station-defense scope.

---

## Maintenance notes

- This document should be updated when items ship (remove them; link the PR in
  [`ROADMAP.md`](ROADMAP.md)) or when new items emerge that fit the "well-scoped
  engineering task" profile.
- Items shipped should be removed (not struck out) — the corresponding ROADMAP.md
  bullets are the durable record of "what shipped." This file is the working queue.
