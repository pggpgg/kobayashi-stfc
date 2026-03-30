# Kobayashi improvement tasks (audit)

This document lists **20** actionable improvements derived from a codebase audit: repository layout (Rust core + Axum server + React/Vite frontend + Python `tools/combat_engine`), existing docs ([ROADMAP.md](ROADMAP.md), [HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md), [DEPLOYMENT_SECURITY.md](DEPLOYMENT_SECURITY.md)), CI (`.github/workflows/ci.yml`), and current gaps (ship abilities, partial research/FT, no browser E2E in CI).

Each task is **independent enough** to track as its own issue; dependencies are noted where sequencing matters.

---

## Suggested execution order

Work in **phases** so foundations and correctness land before large feature work and polish.

| Phase | Focus | Tasks (see below) |
|-------|--------|-------------------|
| **1 — Hygiene & CI** | Keep main green, catch regressions early, align local scripts with CI | 1, 2, 3 |
| **2 — Data & correctness** | Catalogs, mappings, and tests that unlock reliable sims | 4, 5, 6, 7 |
| **3 — Combat engine (major)** | Core differentiators: ship abilities and noop audit | 8, 9 |
| **4 — Profile & sync** | Player-specific bonuses and mod pipeline | 10, 11 |
| **5 — UX & API** | Usability, contracts, and optional hardening | 12, 13, 14, 15 |
| **6 — Content & long tail** | Game evolution and deferred modeling | 16, 17, 18, 19, 20 |

**Rationale:** Phase 1 reduces friction for every other change. Phase 2 fixes “wrong numbers” before building new mechanics on top. Phase 3 is the roadmap’s chosen combat pillar. Phases 4–6 parallelize better once 1–3 are underway.

---

## The 20 tasks

### Phase 1 — Hygiene & CI

**1. Align `npm run verify` with CI**  
`scripts/verify.mjs` runs `cargo test`, `cargo build --release`, `cargo clippy`, and frontend `test`/`build`, but **does not** mirror `cargo fmt --check`, `cargo audit`, `npm audit`, Biome `lint`, or `tsc` typecheck. Extend the script (or document a two-command workflow) so a pre-push local run matches what GitHub enforces.

**2. Treat Clippy warnings as errors in CI (optional flag)** — **done**  
CI and `npm run verify` run `cargo clippy --all-targets -- -D warnings`. Remaining policy exceptions use targeted `#[allow(clippy::…)]` on specific APIs (e.g. many-arg registry entrypoints, complex tuple return types).

**3. Run CI on development branches**  
Workflow triggers are limited to `main`/`master`. If the team uses long-lived branches (e.g. `cursor/stage-batch`), add `workflow_dispatch` and/or branch patterns so PRs and pushes to those branches get the same checks without manual `verify` runs.

---

### Phase 2 — Data & correctness

**4. Stabilize research integration tests operationally**  
`tests/scenario_research_integration_tests.rs` depends on a populated `data/research_catalog.json` and upstream fetch workflow ([ROADMAP.md](ROADMAP.md)). Document the refresh path in contributor onboarding and/or add a CI job that fails with a clear message when the catalog is missing or stale relative to a checksum.

**5. Verify research `accuracy` and conditional scopes**  
`accuracy` and faction-/mode-scoped research lines are called out as partially verified. Add calibration against combat logs or toolbox expectations; tighten `buff_id_to_stat` / scenario wiring where descriptions were mapped as global bonuses incorrectly.

**6. Complete forbidden tech `fid` mapping**  
Synced FT merges match catalog rows by `fid`; gaps are documented in [ROADMAP.md](ROADMAP.md). Build or import a mapping from game/sync ids to catalog entries so “Use synced” applies the right bonuses for typical profiles.

**7. Hostile → faction resolution**  
Extend `HostileRecord::opponent_faction_tag()` and related data so upstream faction ids map to [`OpponentFactionTag`](src/combat/types.rs) where possible; document intentional `Unknown` cases ([HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md)). Unlocks correct faction-gated hull/ability behavior when hostiles are known.

---

### Phase 3 — Combat engine (major)

**8. Implement ship abilities in combat**  
Roadmap “next pillar”: evaluate `ability` arrays from ship data during combat (distinct from officers), including common “on hit / on shield break” style effects. Touches [`ship_ability_resolve.rs`](src/data/ship_ability_resolve.rs), engine round loop, and tests.

**9. Structured audit of `combat_noop` ship abilities**  
Follow the four-step plan in [ROADMAP.md](ROADMAP.md): inventory noop ids, bucket reasons, decide per bucket (keep noop, extend resolver, or document), and make regeneration via `scripts/generate_full_ship_ability_catalog.py` safe for hand-tuned rows.

---

### Phase 4 — Profile & sync

**10. Optional: forbidden tech timing**  
Today FT applies at profile merge (pre-combat). If evidence shows in-game per-sub-round application, design a minimal engine phase; otherwise document the approximation and close the gap intentionally.

**11. Persist high-value sync payloads from stfc-mod**  
[ROADMAP.md](ROADMAP.md) lists traits, slots, buffs, and others as accepted but not stored. Prioritize one or two that improve sim fidelity or UX (e.g. roster completeness), with schema + API + profile merge tests.

---

### Phase 5 — UX, API, security

**12. Browser E2E smoke (Playwright)**  
Root `package.json` includes Playwright; CI does not run end-to-end tests. Add a small suite (health, load Workspace, one read-only API) and an optional CI job so regressions in `serve` + static assets are caught.

**13. OpenAPI contract coverage expansion**  
Heavy payloads are documented in `docs/openapi/kobayashi-heavy-payloads.yaml` with tests in `tests/openapi_contract_test.rs`. Extend coverage for new or high-traffic routes as they evolve; keep `/api/openapi.yaml` the single contract source of truth.

**14. Accessibility pass on core UI**  
Modal focus trapping exists (`useModalFocusTrap`). Audit Workspace, Results Library, and Roster flows for keyboard order, labels, and focus return; fix high-impact issues without a full i18n effort.

**15. Optional LAN/internet hardening**  
[DEPLOYMENT_SECURITY.md](DEPLOYMENT_SECURITY.md) describes API keys and trust boundaries. For non-loopback binds, consider configurable rate limits or stricter concurrency defaults on CPU-heavy routes beyond the existing semaphore—document tradeoffs vs. local-first use.

---

### Phase 6 — Content & long tail

**16. Maverick faction track**  
Follow [MAVERICK.md](MAVERICK.md): research catalog, hostiles, buildings/sync as the game’s Maverick content stabilizes; keep parallel to ship-ability work where possible.

**17. Apex (shred / barrier) from research**  
Roadmap notes apex not merged from research. If required for current meta scenarios, add profile keys and merge rules consistent with [DESIGN.md](DESIGN.md).

**18. Station defense building mode**  
[ROADMAP.md](ROADMAP.md) backlog: `BuildingMode::StationDefense`, conditions on `BonusEntry`, and optimizer context when starbase defense is in scope.

**19. i18n scaffolding**  
If non-English UI is planned, introduce message catalogs or a lightweight extraction pipeline early ([ROADMAP.md](ROADMAP.md)); defer full translation.

**20. Python `tools/combat_engine` parity and docs**  
Keep the Python package tested in CI (`pytest`) aligned with Rust semantics for shared scenarios; expand golden tests when engine rules change, and cross-link [tools/combat_engine/README.md](../tools/combat_engine/README.md) from the main README for contributors choosing Python for experiments.

---

## Maintenance

- Revisit this list after major releases (new officers, new game systems).
- Prefer linking new work to [ROADMAP.md](ROADMAP.md) sections to avoid duplicate “source of truth” drift.
