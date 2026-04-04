# Kobayashi — Top 20 improvement tasks (preferred order)

This is an **ordered** list of 20 concrete tasks to improve Kobayashi, optimized for execution flow: foundations → correctness → major combat work → profile/sync → UX/API → long-tail content. Source context: `docs/ROADMAP.md`, `docs/IMPLEMENTATION_PLAN_COMBAT_ENGINE.md`, and the audit list in `docs/KOBAYASHI_IMPROVEMENT_TASKS.md`.

## The 20 tasks (execute top to bottom)

- [x] 1. **Keep CI/local verification aligned and fast**
   - Ensure `npm run verify` stays a faithful superset of CI checks (Rust + frontend + Python parity) and supports pragmatic skip flags for local iteration.

- [x] 2. **Make “warnings are errors” the default for Rust**
   - Enforce `cargo clippy --all-targets -- -D warnings` consistently; document/limit any `#[allow]` to narrow cases.

- [x] 3. **Run CI on feature branches (not just main)**
   - Expand workflow triggers so PRs/branch pushes receive the full signal without manual steps.

- [x] 4. **Stabilize research-catalog operational expectations**
   - `data/research_catalog.json` is required in CI (enforced by `tests/scenario_research_integration_tests.rs`); docs in `README.md`, `scripts/README.md`, and `data/README.md` describe regeneration via `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0`. Locally, you can opt into strictness with `KOBAYASHI_REQUIRE_RESEARCH_CATALOG=1`.

- [x] 5. **Validate research stat semantics that affect combat math**
   - Confirm `accuracy` handling and other “easy to misinterpret” stats; tighten mapping docs and add targeted tests for merge + application.

- [x] 6. **Complete/maintain forbidden/chaos tech sync matching**
   - Ensure every catalog row has a stable `fid` for sync application; improve the importer workflow for new upstream tech.

- [x] 7. **Improve hostile faction resolution and document unknowns**
   - Expand mappings for common hostiles and keep intentional `Unknown` cases explicit (avoid silently wrong categorization).

- [x] 8. **Increase ship-ability catalog coverage (reduce `combat_noop`)**
   - Added stable catalog overrides for additional combat-relevant rows and validated import→scenario wiring with a unit test; regen workflow remains `python3 scripts/generate_full_ship_ability_catalog.py` + overrides.

- [x] 9. **Keep a structured audit trail for `combat_noop` decisions**
   - Updated `docs/SHIP_ABILITY_COMBAT_NOOP_AUDIT.md` to match the current catalog counts and regen-safe noop id inventory.

- [x] 10. **Clarify and harden “profile merge order” invariants**
   - Merge order is now explicitly documented and locked by a unit test in scenario build logic: forbidden/chaos tech → buildings → research (then optional support-buff merge).

- [ ] 11. **(Removed) Persist one high-value additional sync payload**
   - **Deprioritized:** This is not a combat-accuracy priority. Sync expansion beyond the current persisted payloads should only be revisited if it directly improves combat fidelity or core UX.

- [x] 12. **Add minimal browser E2E smoke coverage**
   - Add Playwright smoke tests that prove `serve` can load the SPA, hit a read-only API, and render core flows (keep it small but real).

- [ ] 13. **Expand OpenAPI contract coverage for high-traffic routes**
   - Add/maintain schema assertions for endpoints whose payloads tend to drift (simulate/optimize/profile/sync).

- [x] 14. **Accessibility pass on the core UI flows**
   - **Table usability:** improved the Optimize results table (sticky header, tooltips for truncated cells, row click selection, selection limit messaging). Remaining a11y items (keyboard order, focus trapping/return, ARIA labeling) intentionally deferred.

- [ ] 15. **Optional LAN/internet hardening for CPU-heavy endpoints**
   - For non-loopback binds, document/implement safe defaults (rate limits/concurrency caps) while preserving local-first usage.

- [ ] 16. **Maverick faction support track**
   - Add/refresh Maverick hostiles/research/buildings as upstream data stabilizes; keep the scope explicit per `docs/MAVERICK.md`.

- [ ] 17. **Recorded-fight parity expansion (more fixtures)**
   - Add representative fight families to `tests/fixtures/recorded_fights/` and build regression tests around round timing + key mechanics.

- [x] 18. **Combat trace explainability: stack “why” decomposition**
   - Documented how to read `stack_resolution.stacks` and `effect_contributions` in `docs/COMBAT_TRACE.md` so traces explain base/modifier/flat composition and per-effect deltas.

- [ ] 19. **Station defense “building mode” modeling**
   - Introduce `BuildingMode::StationDefense`, condition gating, and optimizer/context wiring when that scenario is in scope.

- [x] 20. **Maintain Python `tools/combat_engine` parity with Rust hot math**
   - Re-ran parity tests and kept the Python suite green against the current Rust formulas (mitigation/pierce/apex/isolytic).

