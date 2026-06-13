# Roadmap

What's shipped and what's planned. Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md).

_Planned 2026-06-09 from a full repo audit (fidelity docs, test inventory, code health). Check items off as they ship; keep durable design detail in the linked docs, not here._

**Baseline at planning time:** clippy clean under `-D warnings`, ~382 backend tests, OpenAPI contract spot-checks, a bench-regression gate with auto-refreshed baselines, bounded job registries, no TODO/FIXME debt in `src/`. The leverage is simulation fidelity and assurance coverage, not cleanup.

**Execution order (2026-06-13, items 1, 8 & 12 shipped):**

1. **Main lane (serialized):** ~~**12** (engine decomposition)~~ **done** → **4** (Phase 4d, the largest open fidelity gap — now has its clean landing zone) → **6** (hostile-ability audit report can start anytime; *closing* its gaps belongs after 4) and **5** (dual-gate research mapping), with 5-vs-6 priority decided by what 6's audit surfaces. **4 is the next pickup.**
2. **Pre-freeze prep lane (early, any order — the freeze window is the scarcest resource, so tooling must be ready before a window opens):** **3** (import faction resolution), the composite-score harness, and the profile-snapshot completeness audit (see protocol below).
3. **Side-quests (small/independent):** **11** (coverage — do first, it calibrates the rest), **7** (SSE test), **9** (OpenAPI gate), **13** (profile.rs split — land before 4 to cut churn in a file 4 will touch), **14** (upstream drift cron — prevents the data/catalog drift that caused PR #214's merge conflicts).
4. **Anytime, independent:** **10** (frontend tests), **15** (display-names investigation, timeboxed).
5. **Post-freeze (maintainer-scheduled):** **2** (scoreboard + snapshot-bound corpus), then the iterate loop from the protocol.

## Tier 1 — Simulation fidelity

The product *is* its accuracy; these are the largest known gaps between the sim and the real game.

- [x] **1. Fix the Explorer defense channel: source Shield Deflection from upstream `Shield.absorption`** (M) — *closed 2026-06-10*
  Investigation found the gap already closed: [normalize_data_stfc_space](../src/bin/normalize_data_stfc_space.rs) sources `shield_deflection` (the in-game Shield Deflection stat) from upstream's legacy `Shield.absorption` field, the §2c routing is live in [profile.rs](../src/data/profile.rs), and all 113 `ships_extended` files carry correct per-tier values (Cerritos T12 = 13,338, in-game verified). The "7 stale ships" in the planning audit was a substring-grep artifact (`: 120` matches `12076.0`). Shipped in this pass: un-staled [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md) §2c; stale-120 Error + explorer zero-deflection Warning in `validate_ships_extended_dataset`; Cerritos T12 anchor test in [data_provenance_tests.rs](../tests/data_provenance_tests.rs); and a normalizer re-run that aligned 4 Track D2 ability rows (B'Rel, Quv'Sompek, Sanctus, Intrepid) from `combat_noop` to their implemented effect types.

- [ ] **2. Calibration scoreboard + recorded-fight corpus growth** (M, ongoing) — **deferred: gated on the snapshot-calibration protocol below**
  Only 20 `drift_*.json` fixtures (mostly synthetic) plus a handful of recorded fights exist, and no accuracy number is documented anywhere. Promote the per-fixture sigma report (already a CI artifact from the mitigation-feedback calibration test) to a committed, regenerated scoreboard doc with explicit band-width targets; document a 10-minute "add a real fight log" workflow around [calibration_drift_tests.rs](../tests/calibration_drift_tests.rs). Includes filling the three `_TBD_` in-game damage anchors in [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md). *Do not grow the corpus from mixed-vintage fights — wait for a snapshot-frozen suite.*

- [ ] **3. Resolve defender faction for recorded-fight imports** (S/M) — **deferred with item 2; good pre-freeze prep**
  TSV fight imports default to `OpponentFactionTag::Unknown`, so faction-gated hull abilities never fire on imported fights — quietly poisoning calibration data. CLI and drift fixtures are already wired; implement enemy-name → hostile-id → faction lookup with an explicit import-flag override (the open decision in [HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md)). The payoff only arrives once fights are recorded under the protocol, so land it as part of pre-freeze tooling prep.

### The snapshot-calibration protocol (gates items 2–3)

**Why items 2–3 keep getting parked.** The live game state evolves continuously — research, buildings, artifacts/forbidden tech, officer and ship levels all improve steadily — while recorded fights are frozen snapshots of whatever the state was when each one was captured. Calibrating against a mixed-vintage corpus risks **overfitting the engine to assumptions that are no longer true** (or were never true simultaneously): a "fix" that improves agreement with stale fights can encode a wrong model of the current game. Calibration data is only trustworthy when the profile the sim consumes matches the exact game state that produced the fights.

**The protocol** — one sitting, in a deliberately chosen window (the freeze blocks STFC event participation, which is why it can't happen casually):

1. **Snapshot** — capture the full game state into a Kobayashi profile: research, buildings, artifacts/forbidden tech, officer levels/tiers, ship tiers/levels — everything the engine consumes as input.
2. **Freeze** — no tiering, leveling, research, building, or any other state-changing progression for the duration of the window.
3. **Record** — run a varied, curated set of in-game fights (different hostiles, levels, crews, ship classes) and export each one. That set is the **fight test suite for that snapshot**, permanently bound to the frozen profile.
4. **Score** — a composite accuracy score over the suite (per-fight deviation → aggregate) measuring how faithfully the engine replicates the frozen reality.
5. **Iterate** — the composite score becomes the objective function for an auto-research self-improvement loop: engine and model changes are evaluated against the frozen suite, accepted when the composite improves without regressing individual fights.

**Prep that can land before the freeze** (none of it depends on the suite existing, and all of it shortens the sitting): item 3's import faction resolution; a profile-snapshot completeness audit (verify every sim input — including artifacts/syndicate/exocomp-class bonuses — is capturable in one pass); and the composite-score harness itself, developable today against the existing synthetic drift fixtures.

- [ ] **4. Phase 4d: dynamic officer-stat gates for Defense/Health axes** (L)
  Conditionally-gated officer stats (morale-gated Kirk, etc.) model only the attack axis today, as a `weapon_damage` multiplier proxy that diverges near breakpoint tier boundaries ([OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md) §Phase 4d). Implement the deferred per-round 3-axis breakpoint path — the biggest known modeling approximation for meta officers; also unblocks the documented PvP limitation in [PVP.md](PVP.md). Sequence after item 12 so it lands in a decomposed engine.

- [ ] **5. Map dual-gate research rows + triage remaining unmapped buffs** (M)
  The engine path for hull/shield HP buffs that *also* gate on morale/burning/hull-breach exists, but zero catalog mappings use it — an estimated 10–20 flagship (NS/KSG tree) projects route incompletely ([research_conditional_routing.md](research_conditional_routing.md)). Also review the 240 `other_unmapped` research buff ids from [research_unmapped_triage.md](research_unmapped_triage.md); the other ~980 are correctly excluded (economy/scope).

- [ ] **6. Hostile-ability coverage audit** (M)
  A large share of hostile records carry upstream ability data; the engine has plumbing (`hostile_ability_id` flows through [engine.rs](../src/combat/engine.rs)) and a few modeled cases (Conqueror Borg suppression), but no report says which hostile abilities are modeled vs silently ignored — unlike ships, which got [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md). Produce the equivalent audit, then close the top gaps it surfaces.

## Tier 2 — Assurance gaps

Places where a bug would ship undetected today.

- [ ] **7. Test the SSE streaming path** (S)
  `handle_optimize_job_stream` in [routes.rs](../src/server/routes.rs) has zero tests: nothing verifies a client receives progress events, the final payload, or clean termination. It's the UI's live-progress backbone.

- [x] **8. Unit-test `mitigation.rs`; add property-based engine invariants** (M) — *closed 2026-06-11*
  Shipped: 21 dedicated tests in [mitigation.rs](../src/combat/mitigation.rs) (constants pinned, curve known-points, EPSILON/negative/NaN guard semantics, class-channel routing, floor/ceiling clamps, morale piercing, isolytic formula) plus pure-formula proptest properties (bounds, monotonicity); [engine_property_tests.rs](../tests/engine_property_tests.rs) with random-valid-combatant strategies asserting damage ≥ 0, hull/shield ∈ [0, max], round caps, and same-seed determinism (no violations found); [lcars_yaml_robustness.rs](../tests/lcars_yaml_robustness.rs) + parser malformed-input tests. Also fixed a real hole the work surfaced: `load_lcars_dir` silently skipped corrupt `*.lcars.yaml` (now warns on stderr) and `validate_lcars_dir` passed on them (now a hard Error).

- [ ] **9. OpenAPI ↔ handler completeness gate** (S/M)
  Contract tests spot-check shapes but don't diff Rust request/response structs against [kobayashi-openapi.yaml](openapi/kobayashi-openapi.yaml) — and frontend types are generated from the YAML, so a new handler field silently breaks the SPA. Add a field-by-field schema comparison test.

- [ ] **10. Frontend: test `RosterProfile.tsx` and sensitivity result components** (M)
  The largest page ([RosterProfile.tsx](../frontend/src/pages/RosterProfile.tsx), ~1,450 lines: profile sync, roster mutations, error paths) has zero unit tests; 6 of 7 pages and the Morris/Sobol result components are untested. Test the critical flows, or split the page into testable pieces first.

- [ ] **11. Coverage measurement in CI** (S)
  `cargo llvm-cov` + a frontend coverage report, even without a hard gate initially. Today nobody can answer "what fraction of the engine is covered"; the Tier 2 priorities above are educated guesses.

## Tier 3 — Maintainability & operations

- [x] **12. Incrementally decompose `simulate_combat_from_setup`** (L, bench-gated) — *closed 2026-06-13*
  Decomposed **3,164 → 835 lines** across PRs [#215](https://github.com/pggpgg/kobayashi-stfc/pull/215) / [#217](https://github.com/pggpgg/kobayashi-stfc/pull/217) / [#218](https://github.com/pggpgg/kobayashi-stfc/pull/218) into a flat sequence of named phase helpers (`apply_combat_begin_phase`, `apply_defender_round_start`, `roll_attacker_round_start_procs`, `fire_attacker_weapon`, `process_defender_shield_break`, `fire_defender_counter`, `apply_round_end_phase`, `resolve_defender_kill`, `apply_combat_end_phase`). Running state bundled into `CombatRunState`; the two weapon blocks take params-structs. Step 0 built a golden-master harness ([engine_golden_master_tests.rs](../tests/engine_golden_master_tests.rs), 15 scenarios × 4 seeds incl. a trace-events scenario) since drift fixtures only assert bands and the determinism property test compares a binary to itself — neither catches a bit-identical-violating refactor. Every extraction was gated on exact golden-master match + the full engine suite; the bench gate vetoed nothing (`#[inline]` kept the hot path intact). Phase 4d (item 4) now has its clean landing zone.

- [ ] **13. Split `profile.rs` special-tech handlers** (S)
  Extract the Borg-alcove / quantum-slipstream FID special cases from the ~5,000-line [profile.rs](../src/data/profile.rs) into submodules (~30% size reduction, purely mechanical). Bundle the one debug leftover: the stray `eprintln!` in [canonical_conditions.rs](../src/lcars/canonical_conditions.rs) → `tracing`.

- [ ] **14. Automate upstream data-drift detection** (M)
  Refreshes from data.stfc.space are fully manual ([STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md)). Clone the cron'd bench-baseline pattern: a weekly upstream diff that opens a PR when ships/hostiles/research change. Include the provenance parity fix (the ship normalizer doesn't update `registry.json`).

- [ ] **15. Hostile display names** (S dev, blocked on data)
  4,930 hostiles render as "Hostile {id}". Pure UX; blocked on finding a verified `loca_id` → string source upstream — worth a timeboxed investigation, not engine work.

## Assessed, no action planned

From the 2026-06-09 audit, these came back clean or justified — don't re-litigate without new evidence:

- `scenario.rs` (~3,800 lines) is large but well-factored internally; no split needed.
- The 39 `clippy::too_many_arguments` allows are justified by the registry/DTO architecture.
- Error handling: panics are confined to defensive asserts, malformed API input 400s cleanly, job registries are bounded with prune-on-insert.
- Dependencies: minimal, stable, no git deps, no duplicates.
- Station-defense conditions remain a non-goal — see [NOT_ROADMAP.md](NOT_ROADMAP.md).
