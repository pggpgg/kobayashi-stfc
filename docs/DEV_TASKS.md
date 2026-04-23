# Kobayashi Development Tasks

A prioritized list of 20 development tasks to improve Kobayashi, ordered roughly by logical development flow: foundations → data quality → engine/accuracy → optimizer → API/UX → observability → docs/release.

Use the **checkbox on each numbered task** to track progress (`[x]` = done). Nested bullets are scope notes, not separate tasks.

---

## Phase 1 — Foundations & hygiene

- [x] **1. Baseline CI hardening and pre-commit hooks**
  - Ensure `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `npm run test`, and `npm run build` all run in CI on every PR.
  - Add a pre-commit hook (or `cargo xtask`) that runs `fmt` + `clippy` on changed files.
  - **Touchpoints:** `.github/workflows/`, `frontend/package.json`, repo-root `run.sh`.
  - **Done when:** a failing lint or test blocks merge and local pre-commit catches the same issues.
- [x] **2. Introduce a `cargo xtask` (or `just`) task runner** *(shipped: `xtask/` workspace member, `.cargo/config.toml` alias, `cargo xtask --help`; see `CLAUDE.md` § Maintenance.)*
  - Consolidate the many helper scripts (`scripts/*.mjs`, `scripts/*.py`, `cargo run --bin ...`) behind a single entry point with discoverable subcommands (`xtask refresh-ships`, `xtask refresh-hostiles`, `xtask validate`, `xtask regen-lcars`).
  - **Touchpoints:** new `xtask/` crate in `Cargo.toml` workspace; update `CLAUDE.md` and `README.md`.
  - **Done when:** `cargo xtask --help` lists every common maintenance workflow and the docs link to it.
- [x] **3. Upstream data-refresh CI job (scheduled)** *(shipped: [`.github/workflows/data-refresh.yml`](../.github/workflows/data-refresh.yml) — weekly + `workflow_dispatch`; see [scripts/README.md](../scripts/README.md) § Automated refresh.)*
  - Nightly (or weekly) GitHub Action that runs ship/hostile/research fetch scripts, regenerates normalized data, runs `cargo test`, and opens a PR with the diff.
  - **Touchpoints:** `.github/workflows/data-refresh.yml`, `scripts/fetch_stfcspace_*.mjs`, `normalize_`* binaries.
  - **Done when:** a scheduled run produces a draft PR with updated `data/` artifacts and green tests.

## Phase 2 — Data quality & validation

- [x] **4. Strict data validator with actionable error report** *(shipped: `validate_data` JSON/Markdown + `mapping_gap_report`; CI uploads `data-validation-report` from [`.github/workflows/ci.yml`](../.github/workflows/ci.yml); see `cargo run --bin validate_data -- --help`.)*
  - Extend `cargo run --bin validate_data` to emit a structured JSON/Markdown report listing: missing `fid`s, unmapped canonical conditions, ships with no tiers/levels, hostiles missing `ship_type`, officers failing LCARS schema.
  - **Touchpoints:** `src/bin/validate_data.rs`, `src/data/`*, `docs/CANONICAL_CONDITIONS.md`.
  - **Done when:** the report is generated in CI as an artifact and warnings trend to zero.
- [ ] **5. Expand canonical condition token coverage** *(in progress: ship-vs-hostile **scenario literals** — `TargetNotASB`, `SelfAttacking`, `TargetNotPlayerStation` → LCARS `literal_true` / `AbilityCondition::LiteralBool(true)`; `SelfDefending` → `literal_false`; stfc.cc adapter + parity tests; triage in [`CANONICAL_CONDITIONS.md`](CANONICAL_CONDITIONS.md); `report_unknown_mappings` **21** still-unmapped.)*
  - Work through `docs/CANONICAL_CONDITIONS.md` triage; map high-frequency "skipping unmapped" tokens (e.g. `CombatBattleType`, hull-line tokens) to engine-understood conditions with fixtures.
  - **Touchpoints:** `src/lcars/resolver.rs`, `scripts/normalize_officer_id_strings.py`, `data/officers/officers.canonical.json`.
  - **Done when:** `generate_lcars` logs zero warnings on the top 20 most-frequent tokens and tests cover each.
- [x] **6. Backfill `fid` coverage for forbidden/chaos tech catalog** *(catalog + CSV already full `fid`; `forbidden_chaos_unresolved_import_fids` + `sync_readiness_tests` assert demo sync resolves 100% and every CSV row has `fid`.)*
  - Use `scripts/build_chaos_tech_csv_rows.mjs` + manual curation to ensure every committed catalog row has a unique `fid`; enforce in `forbidden_chaos::sync_readiness_tests`.
  - **Touchpoints:** `data/forbidden_chaos_tech.json`, `data/import/forbidden_chaos_tech.csv`, related tests.
  - **Done when:** every catalog row has a `fid` and synced profiles resolve 100% of bonuses.

## Phase 3 — Engine correctness & calibration

- [ ] **7. Grow recorded-fight fixture library**
  - Add ≥ 20 new calibration fixtures from HiggsBozo's `profiles/higgsbozo` battlelogs across PvE/PvP/Armada scenarios; wire into `tests/fixtures/recorded_fights/`.
  - **Touchpoints:** `tests/combat_calibration_`*, `docs/combat_log_format.md`, `profiles/higgsbozo/battlelogs.imported.json`.
  - **Done when:** calibration suite covers each major ship class and `cargo test` stays green.
- [ ] **8. Wire synced battlelogs into the calibration pipeline**
  - Build a tool that converts `profiles/{id}/battlelogs.imported.json` entries into recorded-fight fixtures automatically; include CLI command and docs.
  - **Touchpoints:** `src/sync.rs`, new `src/bin/import_battlelogs.rs`, `docs/SYNC.md`.
  - **Done when:** `cargo xtask battlelogs-to-fixtures --profile higgsbozo` produces valid fixtures the calibration tests can consume.
- [x] **9. Progressive LCARS → `CombatEffectSpec` resolver migration** *(shipped: `resolve_effect` → adapter + `compile_officer_combat_spec`; parity in `tests/lcars_captain_spec_parity_tests.rs` + `tests/lcars_combat_effect_spec_parity_tests.rs`; see `docs/COMBAT_EFFECT_SPEC.md` § Task 9.)*
  - Per `docs/COMBAT_EFFECT_SPEC.md`, migrate officer families one at a time (captains first, then bridge, then BD) with parity tests; keep LCARS as the authoring surface.
  - **Touchpoints:** `src/lcars/effect_spec_adapter.rs`, `src/combat/effect_spec_compile.rs`, `src/lcars/resolver.rs`, `tests/lcars_combat_effect_spec_parity_tests.rs`, `tests/lcars_captain_spec_parity_tests.rs`.
  - **Done when:** at least one officer family runs exclusively through the spec path in production with parity fixtures.
- [x] **10. Hostile `upstream_ship_type` enumeration + validation report** *(shipped: `KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES` + `DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES` in [`src/data/upstream_hostile_ship_type.rs`](../src/data/upstream_hostile_ship_type.rs); `validate_hostiles_dataset` in [`src/data/validate.rs`](../src/data/validate.rs); doc [`UPSTREAM_HOSTILE_SHIP_TYPES.md`](UPSTREAM_HOSTILE_SHIP_TYPES.md); triage subsection in [`mapping_gap_report.rs`](../src/data/mapping_gap_report.rs) / `report_unknown_mappings`.)*
  - Finish the map in `docs/UPSTREAM_HOSTILE_SHIP_TYPES.md`; strict check via `cargo run --bin validate_data` (lists unmapped ids with counts and samples); defer-list warns with a maintainer reason until documented.
  - **Touchpoints:** `src/data/hostile.rs`, `UpstreamHostileShipTypeProfile`, `validate_data`, `report_unknown_mappings`.
  - **Done when:** unmapped count is zero or explicitly allow-listed with a reason.

## Phase 4 — Optimizer intelligence

- [ ] **11. Adaptive simulation budgets in tiered optimizer**
  - Replace fixed `tiered_scout_sims` with a variance/confidence-driven allocator: give more sims only to crews whose win-rate CI overlaps the current top-K cut.
  - **Touchpoints:** `src/optimizer/tiered.rs`, `src/optimizer/ranking.rs`, `src/bin/tiered_scout_budget_compare.rs` (uniform vs adaptive scout totals).
  - **Done when:** total sim budget drops ≥ 30% on representative workloads at equal final top-K accuracy (benchmarked).
  - **Baseline (recorded 2026-04-22, `8312387a`):** from repo root, `cargo run --release --bin tiered_scout_budget_compare`. Uses `profile_id=demo`, `tiered_scout_uniform` on/off; table columns are `OptimizeRunOutcome::tiered_scout_budget.scout_trials_final` (sum of per-crew `trials_run`, so Wilson **scout** early-stop can make totals below `n × tiered_scout_sims`). Wall time is uniform+adaptive for that row (same machine, release build).
    | scenario | `n` | uniform `scout_trials_final` | adaptive `scout_trials_final` | reduction |
    | --- | ---: | ---: | ---: | ---: |
    | `saladin` vs `2918121098` (seed 11, max_c 120, scout 500, top_k 20, full_sims 2000) | 120 | 60_000 | 60_000 | 0% |
    | `uss_enterprise_d` vs `2918121098` (seed 42, max_c 128, scout 500, top_k 20, full_sims 2000) | 128 | 12_800 | 12_800 | 0% |
    | `amalgam` vs `2918121098` (seed 7, max_c 100, scout 400, top_k 16, full_sims 1800) | 100 | 10_000 | 10_000 | 0% |
  - On these three demo-roster scenarios, adaptive scout cost matched the uniform baseline (0% reduction); use this table as the comparison anchor while tuning overlap/refine rules toward the ≥30% “done when” bar.
- [x] **12. Cross-session learning cache (per ship × hostile)** *(shipped: [`src/data/optimize_history.rs`](../src/data/optimize_history.rs) + `OPTIMIZE_HISTORY_JSON` in [`profile_index.rs`](../src/data/profile_index.rs); optimize API `optimize_cache_key` + scenario `optimize_history_confirm_hits` / `optimize_history_wrote`; tiered scout/confirm reuse via [`tiered.rs`](../src/optimizer/tiered.rs) + [`mod.rs`](../src/optimizer/mod.rs); SPA [`workspaceRequests.ts`](../frontend/src/lib/workspaceRequests.ts) / [`useWorkspace.ts`](../frontend/src/lib/useWorkspace.ts) sends same fingerprint as [`optimizeWarmStart.ts`](../frontend/src/lib/optimizeWarmStart.ts); “Cached warm start” in [`OptimizePanel.tsx`](../frontend/src/components/OptimizePanel.tsx).)*
  - Persist winners + warm-start seeds to `profiles/{id}/optimize_history.json`; auto-seed future optimize runs for the same (ship, hostile, constraints) key.
  - **Touchpoints:** `src/data/optimize_history.rs`, `src/optimizer/mod.rs`, `src/optimizer/tiered.rs`, `src/server/api/execution.rs`, `src/server/api/requests.rs`, `frontend/src/lib/optimizeWarmStart.ts`, `frontend/src/lib/workspaceRequests.ts`, `frontend/src/lib/useWorkspace.ts`, `frontend/src/components/OptimizePanel.tsx`.
  - **Done when:** re-running an identical optimize call short-circuits scout on already-confirmed winners and shows a "cached warm start" badge.
- [x] **13. Novelty-aware crew ranking** *(shipped: MMR + Jaccard on officer sets in [`src/optimizer/ranking.rs`](../src/optimizer/ranking.rs); optimize API `novelty_lambda` / `novelty_diverse_top` / `novelty_pool`, default off = pure strength order; workspace Optimize panel sends optional fields.)*
  - Add a diversity penalty (e.g. Jaccard distance over officer ids) so the ranked top-K is not dominated by near-duplicates of one lineage.
  - **Touchpoints:** `src/optimizer/ranking.rs`, new unit tests with synthetic crews.
  - **Done when:** a configurable `novelty_weight` parameter exposes the behavior and defaults preserve current output.
  - **Note:** the JSON field is **`novelty_lambda`** (MMR tradeoff, \((0,1]\)); higher values stay closer to win-rate ordering. The original “novelty weight” wording maps to this control.

## Phase 5 — API, frontend & UX

- [x] **14. OpenAPI spec + typed client for the frontend** *(shipped: [`docs/openapi/kobayashi-openapi.yaml`](../docs/openapi/kobayashi-openapi.yaml) OAS 3.1 + [`src/server/openapi.rs`](../src/server/openapi.rs); `npm run gen:api` → [`frontend/src/lib/api/generated.d.ts`](../frontend/src/lib/api/generated.d.ts) + [`frontend/src/lib/api/schema.ts`](../frontend/src/lib/api/schema.ts); [`tests/openapi_response_contract_test.rs`](../tests/openapi_response_contract_test.rs) validates read-only JSON vs spec.)*
  - Complete `docs/openapi/` into a full OpenAPI 3.1 document covering every `/api/`* route; generate TS types consumed by the SPA to replace ad-hoc fetch wrappers.
  - **Touchpoints:** `docs/openapi/`, `src/server/routes.rs`, `frontend/src/lib/api/` (new).
  - **Done when:** SPA builds using generated types and a contract test asserts server responses match the spec.
- [x] **15. Frontend E2E smoke tests via Playwright** *(shipped: [e2e/workspace-flow.spec.ts](../e2e/workspace-flow.spec.ts) full UI flow + [e2e/smoke.spec.ts](../e2e/smoke.spec.ts); [playwright.config.ts](../playwright.config.ts) `workers: 1`; CI job `e2e_smoke` in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) with failure artifacts; preset POST JSON omits null crew slots in [`savePreset`](../frontend/src/lib/api.ts).)*
  - Use existing `e2e/` + `playwright.config.ts` to cover: load workspace, run sim, run optimize (small), save preset, open results library. Wire into CI.
  - **Touchpoints:** `e2e/`, `.github/workflows/`.
  - **Done when:** CI runs Playwright headless against a locally-served release build and blocks merges on failures.
- [ ] **16. Editable buildings UI + profile management**
  - Follow up on the `GET /api/profile/buildings-summary` hook: add edit controls for building levels when sync is unavailable.
  - **Touchpoints:** `frontend/src/components/` (Roster & Profile page), `src/server/api.rs`.
  - **Done when:** users can edit building levels in the UI and values flow into the optimizer.

## Phase 6 — Observability & performance

- [x] **17. Structured logging + request tracing in the server** *(shipped: JSON `tracing` bootstrap in [`src/logging.rs`](../src/logging.rs) with `KOBAYASHI_LOG`/`RUST_LOG`; per-request spans via `TraceLayer` in [`src/server/routes.rs`](../src/server/routes.rs); optimize/job/sim-batch events in [`src/server/api/execution.rs`](../src/server/api/execution.rs), [`src/optimizer/mod.rs`](../src/optimizer/mod.rs), and [`src/optimizer/tiered.rs`](../src/optimizer/tiered.rs); `jq` recipes in [`docs/DEPLOYMENT_SECURITY.md`](DEPLOYMENT_SECURITY.md).)*
  - Adopt `tracing` + `tracing-subscriber` with JSON output; add span per request, per optimize job, per sim batch; include seed, strategy, candidate count.
  - **Touchpoints:** `src/server/`, `Cargo.toml`, `docs/DEPLOYMENT_SECURITY.md`.
  - **Done when:** `KOBAYASHI_LOG=info` emits structured events and an example dashboard (or `jq` recipe) is documented.
- [ ] **18. Criterion benchmark regression gate**
  - Extend `benches/` with fixed-seed scenarios; store baseline in `benchmark_results.log`; CI fails if p50 regresses > 10% on the reference machine profile.
  - **Touchpoints:** `benches/`, `.github/workflows/`, `docs/PERFORMANCE.md`.
  - **Done when:** CI surfaces a regression summary comment on PRs and the baseline is refreshed on release tags.

## Phase 7 — Release, docs & community

- [x] **19. Prebuilt release artifacts (macOS, Linux, Windows)** *(shipped: [`.github/workflows/release.yml`](../.github/workflows/release.yml) on `push` tags `v*` — matrix Linux / macOS aarch64 / Windows; `SHA256SUMS`; `softprops/action-gh-release` + `generate_release_notes`; bundle readme [`packaging/RELEASE-BUNDLE-README.txt`](../packaging/RELEASE-BUNDLE-README.txt); Quick Start + [`DEPLOYMENT_SECURITY.md`](DEPLOYMENT_SECURITY.md) § Release binaries.)*
  - GitHub Release workflow that builds `kobayashi` + `frontend/dist` and attaches zipped artifacts per OS; include a checksum file and signed tag.
  - **Touchpoints:** `.github/workflows/release.yml`, `README.md` Quick Start, `docs/DEPLOYMENT_SECURITY.md`.
  - **Done when:** tagging `vX.Y.Z` publishes three downloadable archives + release notes.
- [x] **20. Contributor onboarding pass (README + CONTRIBUTING + architecture diagram)**
  - Refresh `CONTRIBUTING.md`, refresh `README.md`'s Quick Start with a first-time contributor path, add a high-level architecture diagram (Mermaid) showing data flow: sync → profile merge → scenario build → optimizer → SPA.
  - **Touchpoints:** `README.md`, `CONTRIBUTING.md`, `docs/DESIGN.md`.
  - **Done when:** a new contributor can go from clone → running server → first passing test using only the docs.

---

## Progress

Count checked tasks above, or use:

- **Completed:** 14 / 20 (tasks 1–4, 6, 9–10, 12–15, 17, 19–20)
- **In progress:** 5 (canonical condition coverage — scenario literals landed; top-frequency tokens e.g. `EnemySentinel` still open)
- **Blocked:** 0

*Keep this file updated as tasks land; link PRs inline in the task bullets when useful.*
