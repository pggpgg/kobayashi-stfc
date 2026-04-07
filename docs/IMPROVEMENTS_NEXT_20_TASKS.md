# Kobayashi: next 20 improvement tasks (fresh list)

This is a **new** prioritized task list intended to avoid duplicating:
- Anything already called out in `docs/ROADMAP.md`
- Ideas already discussed earlier in this Cursor thread

Focus is on simulator correctness, explainability, maintainability, and developer workflow.

---

## Ordered task list (1 = do first)

- [ ] **1) Recorded-fight “parity harness” (trace → assertions)**  
  Add a small harness that runs a fixture fight, captures a deterministic trace, and asserts **invariants** (event ordering, monotonic counters, non-negative clamps, duration expiry, proc cap behavior) rather than trying to match every number.

- [x] **2) Faction-gated effects: make defender faction first-class for CLI/imports**  
  `kobayashi simulate` accepts `--defender-faction <slug>` and/or `--hostile <id|name level>`; slug wins, else faction from resolved hostile, else `Unknown`. Implemented in `data::loader::defender_faction_for_cli_simulate` and wired in `main.rs` + `cli::handle_simulate`. (Roster `import` does not run combat.)

- [ ] **3) Condition engine: centralize “is this condition true?” evaluation**  
  Consolidate condition checks (burning, hull breach, morale, opponent tags, ship class, “first N rounds”, etc.) behind a single typed evaluator so new effects don’t re-implement slightly different logic.

- [ ] **4) Burning/Hull Breach/Morale: write explicit timing tests**  
  Add unit/integration tests that lock down **when** statuses apply, tick, decay, and clear relative to attack/defense windows and end-of-round cleanup.

- [ ] **5) Proc semantics: standardize proc RNG + per-round/proc caps**  
  Define and test proc behavior (roll timing, proc-chance stacking rules, cap enforcement, and duration refresh vs extend) with a minimal synthetic scenario suite.

- [ ] **6) “Shots” / multi-hit semantics: unify hit accounting**  
  Ensure extra-shots and multi-hit effects interact correctly with mitigation, crit, pierce, and on-hit triggers. Add regression tests around hit counting and sub-round ordering.

- [ ] **7) Crit pipeline audit: ensure consistent crit chance/damage application points**  
  Verify where crit chance is read and where crit damage multiplier is applied (including counter-fire). Add a small trace invariant to guarantee crit is applied once per hit.

- [ ] **8) Isolytic damage modeling: isolate formula + add sensitivity tests**  
  Extract isolytic math behind a dedicated module API, add golden-ish tests for edge cases (0%, very high values, negative/overflow protection), and document assumptions.

- [ ] **9) Status effect lifetime model: “duration rounds” semantics**  
  Normalize how “duration_rounds” is interpreted (inclusive/exclusive, decremented at which window, stacking/refresh rules), and add tests for a few representative effects.

- [ ] **10) Trace shape stabilization: versioned event schema**  
  Add a `trace_version` field and an internal schema test that ensures required fields exist for downstream tooling; allow additive fields without breaking older consumers.

- [ ] **11) Determinism audit: one seed → identical results across thread counts**  
  Add a deterministic test that runs the same scenario with different Rayon thread caps (e.g. 1 vs N) and asserts identical aggregated outputs where intended.

- [ ] **12) Optimizer correctness: duplicate/off-limits officer constraints as a typed rule set**  
  Represent roster/seat constraints (no duplicates, locked seats, excluded officers, captain-only) as a typed “constraints” object shared by exhaustive/genetic/tiered strategies.

- [ ] **13) Optimizer explainability: “why this crew won” summary**  
  Produce a small structured explanation per top crew: key contributing buffs/effects, notable proc events, and primary mitigation drivers (from trace aggregation).

- [ ] **14) Scenario caching correctness: cache key audit + tests**  
  Add tests that ensure scenario caches invalidate correctly when any input changes (ship, hostile, profile bonuses, support buffs, strategy config).

- [ ] **15) Data validation tightening: fail-fast for impossible stat names**  
  Add a strict validation mode that errors on unknown stat keys in catalogs/LCARS resolution (behind a flag) to prevent silently skipped effects from creeping in.

- [ ] **16) LCARS authoring UX: “effect lint” tool**  
  Create a small CLI/tool that lints LCARS YAML for common mistakes (unknown stats/operators, impossible triggers, missing scaling fields), with actionable messages.

- [ ] **17) Backend API: explicit simulation contract for “mode” and “context”**  
  Add a typed request field that makes combat context explicit (ship combat vs other modes), and reject/ignore incompatible options deterministically.

- [ ] **18) Frontend: reproducible “share link” for a simulation/optimization run**  
  Add a shareable URL (or saved preset) that encodes scenario inputs and replays the same run configuration, for bug reports and comparisons.

- [ ] **19) Frontend: trace viewer MVP (filter + highlight)**  
  Provide an interactive trace viewer that can filter by round/window/event type and highlight where key multipliers/statuses changed.

- [ ] **20) Developer workflow: “one command” fixture regeneration**  
  Add a scripted flow to regenerate fixtures (catalog-derived JSONs, any derived docs, and validation) with a single command that is safe and repeatable.

