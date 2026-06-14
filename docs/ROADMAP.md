# Roadmap

What's shipped and what's planned. Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md).

_Planned 2026-06-09 from a full repo audit (fidelity docs, test inventory, code health). Check items off as they ship; keep durable design detail in the linked docs, not here._

**Baseline at planning time:** clippy clean under `-D warnings`, ~382 backend tests, OpenAPI contract spot-checks, a bench-regression gate with auto-refreshed baselines, bounded job registries, no TODO/FIXME debt in `src/`. The leverage is simulation fidelity and assurance coverage, not cleanup.

**Execution order (2026-06-13, items 1, 4, 8 & 12 shipped):**

1. **Main lane (serialized):** ~~**12** (engine decomposition)~~ **done** → ~~**4** (Phase 4d)~~ **done** → ~~**5** (dual-gate research mapping)~~ **done** → ~~**6** (hostile-ability audit)~~ **done**.
2. **Pre-freeze prep lane (early, any order — the freeze window is the scarcest resource, so tooling must be ready before a window opens):** **3** (import faction resolution), the composite-score harness, and the profile-snapshot completeness audit (see protocol below).
3. **Side-quests (small/independent, slot between 12's bench-gated PRs):** ~~**11** (coverage)~~ *shipped*; ~~**7** (SSE test)~~ *shipped*; ~~**9** (OpenAPI gate)~~ *shipped*; ~~**13** (profile.rs split)~~ *shipped*; ~~**14** (upstream drift cron)~~ *shipped*; ~~**16–17** (June 2026 patch ship + officers)~~ *shipped*.
4. **Anytime, independent:** ~~**10** (frontend tests)~~ *shipped*; ~~**15** (hostile display names)~~ *shipped — UI already resolved*.
5. **Post-freeze (maintainer-scheduled):** **2** (scoreboard + snapshot-bound corpus), then the iterate loop from the protocol.

## Tier 1 — Simulation fidelity

The product *is* its accuracy; these are the largest known gaps between the sim and the real game.

- [x] **1. Fix the Explorer defense channel: source Shield Deflection from upstream `Shield.absorption`** (M) — *closed 2026-06-10*
  Investigation found the gap already closed: [normalize_data_stfc_space](../src/bin/normalize_data_stfc_space.rs) sources `shield_deflection` (the in-game Shield Deflection stat) from upstream's legacy `Shield.absorption` field, the §2c routing is live in [profile.rs](../src/data/profile.rs), and all 113 `ships_extended` files carry correct per-tier values (Cerritos T12 = 13,338, in-game verified). The "7 stale ships" in the planning audit was a substring-grep artifact (`: 120` matches `12076.0`). Shipped in this pass: un-staled [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md) §2c; stale-120 Error + explorer zero-deflection Warning in `validate_ships_extended_dataset`; Cerritos T12 anchor test in [data_provenance_tests.rs](../tests/data_provenance_tests.rs); and a normalizer re-run that aligned 4 Track D2 ability rows (B'Rel, Quv'Sompek, Sanctus, Intrepid) from `combat_noop` to their implemented effect types.

- [ ] **2. Calibration scoreboard + recorded-fight corpus growth** (M, ongoing) — **prep shipped 2026-06-14; corpus growth deferred on snapshot freeze**
  Only 20 `drift_*.json` fixtures (mostly synthetic) plus a handful of recorded fights exist. **Shipped:** composite-score harness ([`src/calibration/scoreboard.rs`](../src/calibration/scoreboard.rs)), `cargo xtask calibration-scoreboard`, committed [CALIBRATION_SCOREBOARD.md](CALIBRATION_SCOREBOARD.md), CI artifact + stale-doc gate, suite manifest ([`recorded_fight_suite.json`](../tests/fixtures/recorded_fights/recorded_fight_suite.json)), profile-bound recorded runner ([`src/calibration/recorded.rs`](../src/calibration/recorded.rs)), [CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md). **Still pending:** populate ~40 snapshot-bound fights; fill three `_TBD_` in-game damage anchors in [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md). *Do not grow the corpus from mixed-vintage fights — wait for a snapshot-frozen suite.* Fight selection: [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md).

- [x] **3. Resolve defender faction for recorded-fight imports** (S/M) — *closed 2026-06-14*
  TSV fight exports now capture enemy summary identity (`enemy_player_name`, `enemy_ship_level`) and resolve defender faction via `defender_faction_for_fight_export` / `resolve_hostile_by_display_name` in [`loader.rs`](../src/data/loader.rs) (display-name translations → hostile id → `opponent_faction_tag()`), with explicit slug override matching CLI/drift precedence. Calibration test [`fight_export_realta_vs_takret_militia_10_matches_simulation`](../tests/recorded_fight_calibration_tests.rs) threads faction into `simulate_combat_with_defender_faction`. Takret Militia resolves to a bundled hostile id but remains `Unknown` for ability gating until upstream faction mapping improves.

### The snapshot-calibration protocol (gates items 2–3)

**Why items 2–3 keep getting parked.** The live game state evolves continuously — research, buildings, artifacts/forbidden tech, officer and ship levels all improve steadily — while recorded fights are frozen snapshots of whatever the state was when each one was captured. Calibrating against a mixed-vintage corpus risks **overfitting the engine to assumptions that are no longer true** (or were never true simultaneously): a "fix" that improves agreement with stale fights can encode a wrong model of the current game. Calibration data is only trustworthy when the profile the sim consumes matches the exact game state that produced the fights.

**The protocol** — one sitting, in a deliberately chosen window (the freeze blocks STFC event participation, which is why it can't happen casually):

1. **Snapshot** — capture the full game state into a Kobayashi profile: research, buildings, artifacts/forbidden tech, officer levels/tiers, ship tiers/levels — everything the engine consumes as input.
2. **Freeze** — no tiering, leveling, research, building, or any other state-changing progression for the duration of the window.
3. **Record** — run a varied, curated set of in-game fights (different hostiles, levels, crews, ship classes) and export each one. That set is the **fight test suite for that snapshot**, permanently bound to the frozen profile.
4. **Score** — a composite accuracy score over the suite (per-fight deviation → aggregate) measuring how faithfully the engine replicates the frozen reality.
5. **Iterate** — the composite score becomes the objective function for an auto-research self-improvement loop: engine and model changes are evaluated against the frozen suite, accepted when the composite improves without regressing individual fights.

**Prep that can land before the freeze** (none of it depends on the suite existing, and all of it shortens the sitting): item 3's import faction resolution; a profile-snapshot completeness audit (verify every sim input — including artifacts/syndicate/exocomp-class bonuses — is capturable in one pass); and the composite-score harness itself, developable today against the existing synthetic drift fixtures.

- [x] **4. Phase 4d: dynamic officer-stat gates for Defense/Health axes** (L) — *closed 2026-06-12*
  Conditionally-gated officer stats (morale-gated Kirk, etc.) now use the per-round 3-axis breakpoint path via [`OfficerStatRoundContext`](../src/data/officer_stat_round.rs) instead of the attack-axis-only `weapon_damage` multiplier proxy. Defense deltas apply on inbound counter-fire; attack uses proper breakpoint lookup. Health max scaling is modeled via `health_max_mult` delta (proportional max-HP assumption). PvP defender-side dynamic `target: enemy` debuffs remain deferred (no prod LCARS cases).

- [x] **5. Map dual-gate research rows + triage remaining unmapped buffs** (M) — *closed 2026-06-14*
  Cross-faction `weapon_damage` mappings fixed (Romulan/Federation/Klingon owner+defender lines). Faction patch merged 22 additional gates. Conditional `hull_hp`/`shield_hp` compile to `HullHpMultiplier`/`ShieldHpMultiplier` seats (morale → round-start; burning/HB/faction → attack-phase). Upstream audit: **no** owner+defender `hull_hp`/`shield_hp` projects yet — scenario dual-gate path unit-tested. `other_unmapped` triage refreshed (271 ids; no new hull/shield inference candidates). See [research_conditional_routing.md](research_conditional_routing.md), [research_unmapped_triage.md](research_unmapped_triage.md).

- [x] **6. Hostile-ability coverage audit** (M) — *closed 2026-06-14*
  Regenerated full [`hostile_ability_catalog.json`](../data/upstream/data-stfc-space/hostile_ability_catalog.json) (**976** ids; **246** modeled, **730** `combat_noop`) via [`generate_full_hostile_ability_catalog.py`](../scripts/generate_full_hostile_ability_catalog.py); audit doc [`HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md`](HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md). Resolver delegates isolytic/apex/crit (and shared ship effect types) into `defender_crew` at scenario build. Top PvE buckets (isolytic + apex) modeled; Conqueror Borg beams remain tag-driven (Lane B). Tests: [`hostile_ability_catalog_parity.rs`](../tests/hostile_ability_catalog_parity.rs), [`hostile_ability_catalog_combat.rs`](../tests/hostile_ability_catalog_combat.rs).

- [x] **16. U.S.S. Athena (`uss_athena`) — ship ability + combat readiness** (M) — *closed 2026-06-13*
  "Athena's Fury" (`2357321655`) classified as a faction-gated weapon-damage multiplier vs **Venari Ral [VENRA]** hostiles and added as a durable override row in [ship_ability_catalog_overrides.json](../data/upstream/data-stfc-space/ship_ability_catalog_overrides.json) (`attack_multiplier` + `condition_opponent_faction: venari_ral`, mirroring the shipped Xindi counter ship). New `OpponentFactionTag::VenariRal` variant + slug ([types.rs](../src/combat/types.rs)) and hostile faction mapping for upstream id `331567901` / loca `90001` ([hostile.rs](../src/data/hostile.rs)); catalog + `ships_extended` regenerated so [uss_athena.json](../data/ships_extended/uss_athena.json) now carries the ability (plus its ASA apex-barrier rows). Integration test: [ship_ability_athena_venari_ral.rs](../tests/ship_ability_athena_venari_ral.rs) (effect resolution, faction-gate condition, and Venari Ral hostiles → tag; inert vs other factions).

- [x] **17. June 2026 officer batch — canonical catalog + LCARS** (M) — *closed 2026-06-13*
  All six officers added to [officers.canonical.json](../data/officers/officers.canonical.json) + [id_registry.json](../data/officers/id_registry.json) and regenerated into [officers.lcars.yaml](../data/officers/officers.lcars.yaml) (286 officers): **Academy Doctor**, **Caleb Mir**, **Genesis Lythe**, **Jirali**, **Chancellor Ake**, **Deidamia**. Caleb Mir's per-round shield restore vs non-Armada hostiles is the one combat-modeled bridge row; the rest are intentionally inert on the default ship-vs-hostile path (Wave Defense → `literal_false`, PvP-only `EnemyPlayer`, or `SelfAtStation`-gated Outpost/Retaliation), and Jirali + Genesis Lythe's loot/repair rows map to `:non_combat` tags (new modifier arms in [officer_model.rs](../src/lcars/officer_model.rs)). No new `conditions` tokens (`report_unknown_mappings` still 0 officer-side; only pre-existing `SelfDefending`). Fidelity notes added to [officer_modeling_fidelity.yaml](../data/officers/officer_modeling_fidelity.yaml); [OFFICER_MODELING_SCORECARD.md](OFFICER_MODELING_SCORECARD.md) regenerated.

## Tier 2 — Assurance gaps

Places where a bug would ship undetected today.

- [x] **7. Test the SSE streaming path** (S) — *closed 2026-06-12*
  [optimize_job_sse_tests.rs](../tests/optimize_job_sse_tests.rs) covers unknown-job error, terminal error payload, running→done progress via seeded jobs, and E2E `POST /api/optimize/start` + SSE stream through to `done` with recommendations.

- [x] **8. Unit-test `mitigation.rs`; add property-based engine invariants** (M) — *closed 2026-06-11*
  Shipped: 21 dedicated tests in [mitigation.rs](../src/combat/mitigation.rs) (constants pinned, curve known-points, EPSILON/negative/NaN guard semantics, class-channel routing, floor/ceiling clamps, morale piercing, isolytic formula) plus pure-formula proptest properties (bounds, monotonicity); [engine_property_tests.rs](../tests/engine_property_tests.rs) with random-valid-combatant strategies asserting damage ≥ 0, hull/shield ∈ [0, max], round caps, and same-seed determinism (no violations found); [lcars_yaml_robustness.rs](../tests/lcars_yaml_robustness.rs) + parser malformed-input tests. Also fixed a real hole the work surfaced: `load_lcars_dir` silently skipped corrupt `*.lcars.yaml` (now warns on stderr) and `validate_lcars_dir` passed on them (now a hard Error).

- [x] **9. OpenAPI ↔ handler completeness gate** (S/M) — *closed 2026-06-13*
  [openapi_parity.rs](../src/server/openapi_parity.rs) compares schemars-derived property names on handler DTOs against bundled OpenAPI component schemas (~45 mapped types); integration test in [openapi_schema_parity_test.rs](../tests/openapi_schema_parity_test.rs). Caught and fixed drift: `OptimizeRequest.enable_learned_pair_prior` and `ProfileEntry.is_default` were missing from the YAML.

- [x] **10. Frontend: test `RosterProfile.tsx` and sensitivity result components** (M) — *closed 2026-06-13*
  [RosterProfile.test.tsx](../frontend/src/pages/RosterProfile.test.tsx) covers mod-sync banner, building-summary errors, roster import success/failure, and bonus save/error paths. [SensitivityResults.test.tsx](../frontend/src/components/SensitivityResults.test.tsx), [MorrisResults.test.tsx](../frontend/src/components/MorrisResults.test.tsx), and [SobolResults.test.tsx](../frontend/src/components/SobolResults.test.tsx) cover empty states, default sort order, and interactive sort/pairwise rendering.

- [x] **11. Coverage measurement in CI** (S) — *closed 2026-06-12*
  Parallel **Rust coverage** job runs `cargo llvm-cov --lib --tests` (summary in logs, `rust-lcov.info` artifact). Frontend job runs `npm run test:coverage` (Vitest v8; `frontend/coverage/lcov.info` artifact). Informational only — no threshold gate.

## Tier 3 — Maintainability & operations

- [x] **12. Incrementally decompose `simulate_combat_from_setup`** (L, bench-gated) — *closed 2026-06-13*
  Decomposed **3,164 → 835 lines** across PRs [#215](https://github.com/pggpgg/kobayashi-stfc/pull/215) / [#217](https://github.com/pggpgg/kobayashi-stfc/pull/217) / [#218](https://github.com/pggpgg/kobayashi-stfc/pull/218) into a flat sequence of named phase helpers (`apply_combat_begin_phase`, `apply_defender_round_start`, `roll_attacker_round_start_procs`, `fire_attacker_weapon`, `process_defender_shield_break`, `fire_defender_counter`, `apply_round_end_phase`, `resolve_defender_kill`, `apply_combat_end_phase`). Running state bundled into `CombatRunState`; the weapon blocks take params-structs. Step 0 built the golden-master harness ([engine_golden_master_tests.rs](../tests/engine_golden_master_tests.rs)) — drift fixtures only assert bands and the determinism property test compares a binary to itself, so neither catches a bit-identical-violating refactor. Every extraction was gated on exact golden-master match + the full engine suite; the bench gate vetoed nothing.

- [x] **13. Split `profile.rs` special-tech handlers** (S) — *closed 2026-06-14*
  Extracted Borg Alcove, Borg Operating Table, Quantum Slipstream, and ship-class torpedo-family forbidden-tech routing/seat builders into [profile/forbidden_tech_special.rs](../src/data/profile/forbidden_tech_special.rs) (~1,259 lines); [profile/mod.rs](../src/data/profile/mod.rs) shrank **5,122 → 3,908** (~24%). Public API preserved via re-exports; merge skip hooks use `skips_entire_flat_merge` / `skip_forbidden_tech_profile_bonus_for_fid`. Bundled: `eprintln!` in [canonical_conditions.rs](../src/lcars/canonical_conditions.rs) → `tracing::warn!` with `logging::init()` in [generate_lcars.rs](../src/bin/generate_lcars.rs).

- [x] **14. Automate upstream data-drift detection** (M) — *closed 2026-06-14*
  Weekly [`.github/workflows/data-refresh.yml`](../.github/workflows/data-refresh.yml) refresh (ships `--full` on schedule; hostiles/research missing-only) now runs `validate_data`, embeds a summary drift report in the PR body, and opens a PR when `data/` changes. CI job `upstream_drift` in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) fails when live ship/hostile/research summaries diverge from committed caches (`scripts/check_stfcspace_summary_drift.mjs`; `cargo xtask check-upstream-drift`). Ship provenance parity: `normalize_data_stfc_space` merge-updates `data/registry.json` via shared [`merge_registry_entry`](../src/data/registry.rs) with `STFCSPACE_SHIPS_VERSION` / `STFCSPACE_SHIPS_SOURCE_NOTE` env overrides (mirrors hostiles).

- [x] **15. Hostile display names** (S) — *closed 2026-06-14*
  Reassessed: the hostile picker already shows proper names. [`/api/hostiles`](../src/server/api.rs) resolves `display_name` from each index row's `loca_id` via bundled upstream translations ([`hostile_loca.rs`](../src/data/hostile_loca.rs): `translations-ships.json`, `translations-officer_names.json`, `translations-navigation.json`); [HostilePicker](../frontend/src/components/HostilePicker.tsx) labels rows with `display_name ?? hostile_name`. **5,384 / 5,385** hostiles resolve (only the internal `kobayashi_theoretical_damage_sponge` fixture lacks `loca_id`). On-disk `hostile_name` in `data/hostiles/*.json` still uses `Hostile {id}` placeholders from the normalizer — cosmetic data debt, not a UI gap. Optional follow-up: bake resolved names into `normalize_hostiles_stfc_space` and refresh stale notes in [STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md).

## Assessed, no action planned

From the 2026-06-09 audit, these came back clean or justified — don't re-litigate without new evidence:

- `scenario.rs` (~3,800 lines) is large but well-factored internally; no split needed.
- The 39 `clippy::too_many_arguments` allows are justified by the registry/DTO architecture.
- Error handling: panics are confined to defensive asserts, malformed API input 400s cleanly, job registries are bounded with prune-on-insert.
- Dependencies: minimal, stable, no git deps, no duplicates.
- Station-defense conditions remain a non-goal — see [NOT_ROADMAP.md](NOT_ROADMAP.md).
