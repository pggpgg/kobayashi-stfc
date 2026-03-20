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

### Log / parity (JSON path — done)
- **`weapon_index`:** `IngestedEvent` in `src/combat/log_ingest.rs` carries optional `weapon_index`; `ingested_events_to_combat_events` forwards it to `CombatEvent` unchanged. Documented in `docs/combat_log_format.md`; fixture `tests/fixtures/recorded_fights/sample_combat_log.json`; assertions in `tests/log_ingest_tests.rs`.

### Mitigation sensitivity (done)
- **CLI:** `kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]` (see `src/cli.rs` / `src/main.rs`).
- **Library:** `src/combat/mitigation_sensitivity.rs` (`HostileMitigationBaseline`, `default_percent_sensitivity_rows`, `format_sensitivity_tsv`); usage notes in `docs/COMBAT_TRACE.md`.
- **Tests:** unit tests in `mitigation_sensitivity.rs` validate baseline rows against `mitigation_for_hostile` / `pierce_damage_through_bonus` / `compute_damage_through_factor`.

---

## Remaining work

### Phase 3 — Raw combat log ingestion (partial)

**5. Raw combat log ingestion**

**Goal:** Parse raw STFC combat logs into an internal event model so simulator output can be compared to real combat (replay/parity checks).

**Status:** JSON ingestion is in place (`parse_combat_log_json`, `IngestedCombatLog` / `IngestedEvent` in `src/combat/log_ingest.rs`) with fixture tests in `tests/log_ingest_tests.rs`. Sub-round **`weapon_index`** is parsed from JSON and forwarded through `ingested_events_to_combat_events` when present. Game **TSV** export is parsed by `parse_fight_export` in `src/combat/export_csv.rs`; vanilla exports usually omit per-weapon columns — an optional **`Weapon Index`** column is supported when present (see `docs/combat_log_format.md`).

**Plan:**
1. Define the expected raw-log format (e.g. paste from game UI or toolbox export) and document it in this repo (e.g. `docs/combat_log_format.md` or a fixture example).
2. Add a parser in `src/` (e.g. `src/combat/log_ingest.rs` or under `src/data/`) that:
   - Reads raw log text or structured export.
   - Produces an internal event timeline (round index, phase, damage, mitigation, etc.) and/or per-round snapshots (attacker/defender stats, damage dealt).
3. Normalize to a type that can be compared to `SimulationResult` and trace events (e.g. same event types and value names as `CombatEvent` where applicable).
4. Add tests: parse at least one fixture log and assert on event count, round count, and key numeric fields.

**Definition of done**
- Parser exists and is tested against fixture logs for core fields.
- Documented format (or sample) so new logs can be added for parity checks in Phase 3 DoD (replay/parity between parsed logs and engine output).
- `weapon_index` is parsed and threaded through ingested → engine events when present in the JSON format; TSV path documents and optionally parses `Weapon Index` when the export includes it.

**Future / TODO**
- **Richer TSV → trace parity:** Map additional game event columns (if stable) into a timeline comparable to `CombatEvent` (beyond summary + optional `Weapon Index`).
- **More fixtures:** Additional recorded logs for regression (multiple fight families).

---

### Phase 4 — Fidelity and diagnostics (partial)

**7. Compatibility toggles and known quirks**

**Goal:** Support optional game quirks and temporary state so the simulator can match observed behavior when needed.

**Status (duplicate officers):** Implemented. `SimulationConfig::allow_duplicate_officers` (default `false`) gates `apply_duplicate_officer_policy` in `simulate_combat`. LCARS `resolve_crew_to_buff_set` respects `ResolveOptions::allow_duplicate_officers` (default `true` for ad-hoc resolver calls; scenario/optimizer passes through the scenario flag). Tests: `tests/duplicate_officer_compat_tests.rs`.

**Plan:**
1. **Duplicate-officer bug mode:** ~~Add a config flag~~ **Done:** `SimulationConfig.allow_duplicate_officers` + optimizer/API alignment; when `true`, duplicate canonical ids may contribute from every slot; when `false`, first slot wins for static buffs, proc, and crew rows (batched by `contribution_batch` / officer id).
2. **Temporary combat-only state and rollback:** Identify which state (e.g. morale, assimilated, hull breach, burning) is temporary and must be reset or not carried across combats. Ensure `simulate_combat` does not mutate long-lived state; if any shared state is ever introduced, add end-of-combat rollback when this mode is on.
3. Add tests or scenarios that run with toggles on/off and assert expected differences (e.g. duplicate officer changes outcome when toggle is on).

**Definition of done**
- At least one compatibility toggle (e.g. duplicate-officer) is implemented and gated by config.
- No unintended long-lived combat state; rollback or clear documentation of what is combat-local.

---

**8. Explainability + scenario tooling**

**Goal:** Help users and developers see why a given mitigation or damage number is what it is, and how sensitive it is to inputs.

**Plan:**
1. **Mitigation sensitivity table:** ~~Add a small CLI subcommand or library function~~ **Done** — see “Mitigation sensitivity (done)” above and `docs/COMBAT_TRACE.md`.
2. **“Why” trace for mitigation and stacks:** Mitigation side is largely documented in `docs/COMBAT_TRACE.md` (trace fields `mitigation_calc`, `pierce_calc`, `damage_through_factor`). **Remaining:** stack decomposition “why” — for key stats (e.g. pre-attack damage, attack-phase damage), ensure trace or a helper can show base vs modifier vs flat contributions where applicable (e.g. in `EffectAccumulator` or in a debug-only trace). Document how to read it.
3. **Fixtures / tests:** Sensitivity rows are covered in `mitigation_sensitivity.rs` tests. **Remaining:** fixture traces or assertions that specific “why” trace fields appear for a known combat scenario (beyond sensitivity module tests).

**Definition of done**
- ~~Sensitivity table generator implemented and at least one output validated against `mitigation()`.~~ **Met** (hostile path + unit tests).
- Trace or docs explain how to interpret mitigation ~~and stack decomposition~~ for a given run — **mitigation done**; **stack decomposition still open**.
- Fixtures or tests cover sensitivity ~~and trace shape~~ — **sensitivity met**; **trace-shape fixtures optional follow-up**.

---

## Suggested next execution slice

1. **Phase 4.8 (remainder) — Stack “why” + trace fixtures:** Decompose or document stack contributions; add one trace fixture test if useful.
2. **Phase 3 — More recorded fixtures:** Broaden parity regression across fight families (see benchmark rules).
3. **Phase 4.7 — Compatibility toggles:** Add duplicate-officer mode and document combat-local state/rollback once real logs or community reports clarify the exact quirk.
