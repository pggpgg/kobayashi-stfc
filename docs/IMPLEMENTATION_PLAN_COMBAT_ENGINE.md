# Combat Engine Implementation Plan (Logical Order)

This plan turns `COMBAT_FEATURES_FROM_STFC_TOOLBOX.md` into execution phases with concrete deliverables.

---

## Completed

### Phase 1 — Deterministic combat math foundation (done)
- **1. Mitigation model:** `component_mitigation`, `mitigation(defender, attacker, ship_type)`, ship-type coefficient table (Survey/Armada/Battleship/Explorer/Interceptor), golden tests and edge-case tests in `tests/combat_tests.rs`. CLI `simulate` uses the engine (attacker/defender stats).
- **2. Effect stacking:** `StackCategory` (Base/Modifier/Flat), `total = A * (1 + B) + C` in `src/combat/stacking.rs`, tests for additive/modifier/mixed and ordering independence.

### Phase 2 — Ability semantics and timing (done)
- **3. Ability activation:** Ability classes (captain/bridge/below-deck), seat gating, `active_effects_for_timing` in `src/combat/abilities.rs`; engine applies effects by timing window.
- **4. Boostability:** `boostable` on effects, timing windows (combat begin, round start/attack/defense/round end), assimilated scaling.

### Phase 3 (partial) — Monte Carlo runner (done)
- **6. Monte Carlo simulator:** `run_monte_carlo` in `src/optimizer/monte_carlo.rs` takes iterations and scenario payload; aggregates win rate and hull remaining; used by optimizer.

### Sprint 1 (done)
- Mitigation module, ship-type map, unit tests with golden vectors, CLI entrypoint (`simulate`), and tolerance behavior are in place.

---

## Remaining work

### Phase 3 — Raw combat log ingestion (not started)

**5. Raw combat log ingestion**

**Goal:** Parse raw STFC combat logs into an internal event model so simulator output can be compared to real combat (replay/parity checks).

**Plan:**
1. Define the expected raw-log format (e.g. paste from game UI or toolbox export) and document it in this repo (e.g. `docs/combat_log_format.md` or a fixture example).
2. Add a parser in `src/` (e.g. `src/combat/log_ingest.rs` or under `src/data/`) that:
   - Reads raw log text or structured export.
   - Produces an internal event timeline (round index, phase, damage, mitigation, etc.) and/or per-round snapshots (attacker/defender stats, damage dealt).
3. Normalize to a type that can be compared to `SimulationResult` and trace events (e.g. same event types and value names as `CombatEvent` where applicable).
4. Add tests: parse at least one fixture log and assert on event count, round count, and key numeric fields.

**Definition of done**
- Parser exists and is tested against one or more fixture logs.
- Documented format (or sample) so new logs can be added for parity checks in Phase 3 DoD (replay/parity between parsed logs and engine output).

**Future / TODO**
- **Sub-round events:** Add `weapon_index` to `IngestedEvent`, parse it from JSON, and pass it through in `ingested_events_to_combat_events`. Format already documents optional `weapon_index`; parser currently ignores it. Needed for per-weapon parity when logs include sub-round granularity.

---

### Phase 4 — Fidelity and diagnostics (not started)

**7. Compatibility toggles and known quirks**

**Goal:** Support optional game quirks and temporary state so the simulator can match observed behavior when needed.

**Plan:**
1. **Duplicate-officer bug mode:** Add a config flag (e.g. on `SimulationConfig` or optimizer scenario) that, when enabled, allows the same officer to appear in more than one seat (or applies a specific stacking/activation rule that mirrors the in-game bug). Document the behavior and when to enable it.
2. **Temporary combat-only state and rollback:** Identify which state (e.g. morale, assimilated, hull breach, burning) is temporary and must be reset or not carried across combats. Ensure `simulate_combat` does not mutate long-lived state; if any shared state is ever introduced, add end-of-combat rollback when this mode is on.
3. Add tests or scenarios that run with toggles on/off and assert expected differences (e.g. duplicate officer changes outcome when toggle is on).

**Definition of done**
- At least one compatibility toggle (e.g. duplicate-officer) is implemented and gated by config.
- No unintended long-lived combat state; rollback or clear documentation of what is combat-local.

---

**8. Explainability + scenario tooling**

**Goal:** Help users and developers see why a given mitigation or damage number is what it is, and how sensitive it is to inputs.

**Plan:**
1. **Mitigation sensitivity table:** Add a small CLI subcommand or library function that, for a given defender/attacker/ship-type baseline, prints a table of mitigation values for small deltas (e.g. defense +10%, pierce +10%, or ±N to each stat). Output can be text or CSV. Validate a few cells against the existing `mitigation()` function.
2. **“Why” trace for mitigation and stacks:** Extend or document the existing event trace so that:
   - Mitigation: trace already includes `mitigation_calc` and `pierce_calc` with `damage_through_factor`; add a short doc or comment in code that describes how to interpret these for “why did this much damage get through.”
   - Stack decomposition: for key stats (e.g. pre-attack damage, attack-phase damage), ensure trace or a helper can show base vs modifier vs flat contributions where applicable (e.g. in `EffectAccumulator` or in a debug-only trace). Document how to read it.
3. Add fixture traces or test assertions that check sensitivity table outputs against reference values and that “why” trace fields are present for a known scenario.

**Definition of done**
- Sensitivity table generator implemented and at least one output validated against `mitigation()`.
- Trace or docs explain how to interpret mitigation and stack decomposition for a given run.
- Fixtures or tests cover sensitivity and trace shape.

---

## Suggested next execution slice

1. **Phase 3.5 — Raw combat log ingestion:** Define log format (or adopt an existing one), implement parser, add fixture and tests. This unblocks replay/parity work.
2. **Phase 4.8 — Explainability (sensitivity + trace):** Add mitigation sensitivity table and document “why” from existing trace. Low risk, high value for tuning and debugging.
3. **Phase 4.7 — Compatibility toggles:** Add duplicate-officer mode and document combat-local state/rollback once real logs or community reports clarify the exact quirk.
