# Kobayashi — Top 20 improvement tasks (preferred order)

This is an **ordered** list of 20 concrete tasks to improve Kobayashi, optimized for execution flow: foundations → correctness → major combat work → profile/sync → UX/API → long-tail content. Source context: `docs/ROADMAP.md`, `docs/IMPLEMENTATION_PLAN_COMBAT_ENGINE.md`, and the audit list in `docs/KOBAYASHI_IMPROVEMENT_TASKS.md`.

## The 20 tasks (execute top to bottom)

- [x] 1. **Keep CI/local verification aligned and fast**
   - Ensure `npm run verify` stays a faithful superset of CI checks (Rust + frontend + Python parity) and supports pragmatic skip flags for local iteration.

- [x] 2. **Make “warnings are errors” the default for Rust**
   - Enforce `cargo clippy --all-targets -- -D warnings` consistently; document/limit any `#[allow]` to narrow cases.

- [x] 3. **Run CI on feature branches (not just main)**
   - Expand workflow triggers so PRs/branch pushes receive the full signal without manual steps.

- [ ] 4. **Stabilize research-catalog operational expectations**
   - Make tests + docs unambiguous about when `data/research_catalog.json` must exist, how to regenerate it, and how to detect drift.

- [x] 5. **Validate research stat semantics that affect combat math**
   - Confirm `accuracy` handling and other “easy to misinterpret” stats; tighten mapping docs and add targeted tests for merge + application.

- [x] 6. **Complete/maintain forbidden/chaos tech sync matching**
   - Ensure every catalog row has a stable `fid` for sync application; improve the importer workflow for new upstream tech.

- [x] 7. **Improve hostile faction resolution and document unknowns**
   - Expand mappings for common hostiles and keep intentional `Unknown` cases explicit (avoid silently wrong categorization).

- [ ] 8. **Increase ship-ability catalog coverage (reduce `combat_noop`)**
   - Iterate on `ship_ability_catalog.json` generation/mapping so more upstream ship abilities are represented in combat with explicit, testable proxies.

- [ ] 9. **Keep a structured audit trail for `combat_noop` decisions**
   - Maintain the noop audit buckets (economy-only, armada-only, proc-chain omissions, unmodeled conditions) so future catalog work is disciplined.

- [ ] 10. **Clarify and harden “profile merge order” invariants**
   - Ensure forbidden tech → buildings → research merge order is explicit, tested, and surfaced in docs/UI where it affects expectations.

- [ ] 11. **Persist one high-value additional sync payload**
   - Pick a payload type stfc-mod already sends (traits/slots/buffs/etc.), persist it, and wire it into profile/scenario with tests.

- [x] 12. **Add minimal browser E2E smoke coverage**
   - Add Playwright smoke tests that prove `serve` can load the SPA, hit a read-only API, and render core flows (keep it small but real).

- [ ] 13. **Expand OpenAPI contract coverage for high-traffic routes**
   - Add/maintain schema assertions for endpoints whose payloads tend to drift (simulate/optimize/profile/sync).

- [ ] 14. **Accessibility pass on the core UI flows**
   - Keyboard order, focus trapping/return, ARIA labeling, and table usability for Workspace + Results Library + Roster/Profile.

- [ ] 15. **Optional LAN/internet hardening for CPU-heavy endpoints**
   - For non-loopback binds, document/implement safe defaults (rate limits/concurrency caps) while preserving local-first usage.

- [ ] 16. **Maverick faction support track**
   - Add/refresh Maverick hostiles/research/buildings as upstream data stabilizes; keep the scope explicit per `docs/MAVERICK.md`.

- [ ] 17. **Recorded-fight parity expansion (more fixtures)**
   - Add representative fight families to `tests/fixtures/recorded_fights/` and build regression tests around round timing + key mechanics.

- [ ] 18. **Combat trace explainability: stack “why” decomposition**
   - Provide a traceable breakdown of base/flat/%/mult contributions for key stats so mismatches are diagnosable (engine + docs).

- [ ] 19. **Station defense “building mode” modeling**
   - Introduce `BuildingMode::StationDefense`, condition gating, and optimizer/context wiring when that scenario is in scope.

- [ ] 20. **Maintain Python `tools/combat_engine` parity with Rust hot math**
   - Keep the Python reference implementation aligned with Rust (goldens + vector tests) so mitigation/pierce/apex/isolytic math remains trustworthy.

