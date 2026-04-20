# Kobayashi Development Tasks

A prioritized list of 20 development tasks to improve Kobayashi, ordered roughly by logical development flow: foundations → data quality → engine/accuracy → optimizer → API/UX → observability → docs/release.

Check boxes off as tasks are completed. Each task lists a suggested scope, primary touchpoints, and a done-when signal.

---

## Phase 1 — Foundations & hygiene

- **1. Baseline CI hardening and pre-commit hooks**
  - Ensure `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `npm run test`, and `npm run build` all run in CI on every PR.
  - Add a pre-commit hook (or `cargo xtask`) that runs `fmt` + `clippy` on changed files.
  - **Touchpoints:** `.github/workflows/`, `frontend/package.json`, repo-root `run.sh`.
  - **Done when:** a failing lint or test blocks merge and local pre-commit catches the same issues.
- **2. Introduce a `cargo xtask` (or `just`) task runner** *(shipped: `xtask/` workspace member, `.cargo/config.toml` alias, `cargo xtask --help`; see `CLAUDE.md` § Maintenance.)*
  - Consolidate the many helper scripts (`scripts/*.mjs`, `scripts/*.py`, `cargo run --bin ...`) behind a single entry point with discoverable subcommands (`xtask refresh-ships`, `xtask refresh-hostiles`, `xtask validate`, `xtask regen-lcars`).
  - **Touchpoints:** new `xtask/` crate in `Cargo.toml` workspace; update `CLAUDE.md` and `README.md`.
  - **Done when:** `cargo xtask --help` lists every common maintenance workflow and the docs link to it.
- **3. Upstream data-refresh CI job (scheduled)** *(shipped: [`.github/workflows/data-refresh.yml`](../.github/workflows/data-refresh.yml) — weekly + `workflow_dispatch`; see [scripts/README.md](../scripts/README.md) § Automated refresh.)*
  - Nightly (or weekly) GitHub Action that runs ship/hostile/research fetch scripts, regenerates normalized data, runs `cargo test`, and opens a PR with the diff.
  - **Touchpoints:** `.github/workflows/data-refresh.yml`, `scripts/fetch_stfcspace_*.mjs`, `normalize_`* binaries.
  - **Done when:** a scheduled run produces a draft PR with updated `data/` artifacts and green tests.

## Phase 2 — Data quality & validation

- **4. Strict data validator with actionable error report**
  - Extend `cargo run --bin validate_data` to emit a structured JSON/Markdown report listing: missing `fid`s, unmapped canonical conditions, ships with no tiers/levels, hostiles missing `ship_type`, officers failing LCARS schema.
  - **Touchpoints:** `src/bin/validate_data.rs`, `src/data/`*, `docs/CANONICAL_CONDITIONS.md`.
  - **Done when:** the report is generated in CI as an artifact and warnings trend to zero.
- **5. Expand canonical condition token coverage**
  - Work through `docs/CANONICAL_CONDITIONS.md` triage; map high-frequency "skipping unmapped" tokens (e.g. `CombatBattleType`, hull-line tokens) to engine-understood conditions with fixtures.
  - **Touchpoints:** `src/lcars/resolver.rs`, `scripts/normalize_officer_id_strings.py`, `data/officers/officers.canonical.json`.
  - **Done when:** `generate_lcars` logs zero warnings on the top 20 most-frequent tokens and tests cover each.
- **6. Backfill `fid` coverage for forbidden/chaos tech catalog**
  - Use `scripts/build_chaos_tech_csv_rows.mjs` + manual curation to ensure every committed catalog row has a unique `fid`; enforce in `forbidden_chaos::sync_readiness_tests`.
  - **Touchpoints:** `data/forbidden_chaos_tech.json`, `data/import/forbidden_chaos_tech.csv`, related tests.
  - **Done when:** every catalog row has a `fid` and synced profiles resolve 100% of bonuses.

## Phase 3 — Engine correctness & calibration

- **7. Grow recorded-fight fixture library**
  - Add ≥ 20 new calibration fixtures from HiggsBozo's `profiles/higgsbozo` battlelogs across PvE/PvP/Armada scenarios; wire into `tests/fixtures/recorded_fights/`.
  - **Touchpoints:** `tests/combat_calibration_`*, `docs/combat_log_format.md`, `profiles/higgsbozo/battlelogs.imported.json`.
  - **Done when:** calibration suite covers each major ship class and `cargo test` stays green.
- **8. Wire synced battlelogs into the calibration pipeline**
  - Build a tool that converts `profiles/{id}/battlelogs.imported.json` entries into recorded-fight fixtures automatically; include CLI command and docs.
  - **Touchpoints:** `src/sync.rs`, new `src/bin/import_battlelogs.rs`, `docs/SYNC.md`.
  - **Done when:** `cargo xtask battlelogs-to-fixtures --profile higgsbozo` produces valid fixtures the calibration tests can consume.
- **9. Progressive LCARS → `CombatEffectSpec` resolver migration**
  - Per `docs/COMBAT_EFFECT_SPEC.md`, migrate officer families one at a time (captains first, then bridge, then BD) with parity tests; keep LCARS as the authoring surface.
  - **Touchpoints:** `src/lcars/effect_spec_adapter.rs`, `src/combat/effect_spec_compile.rs`, `tests/lcars_combat_effect_spec_parity_tests.rs`.
  - **Done when:** at least one officer family runs exclusively through the spec path in production with parity fixtures.
- **10. Hostile `upstream_ship_type` enumeration + validation report**
  - Finish the map in `docs/UPSTREAM_HOSTILE_SHIP_TYPES.md`; add a validator that lists any normalized hostile whose `upstream_ship_type` is unmapped.
  - **Touchpoints:** `src/data/hostile.rs`, `UpstreamHostileShipTypeProfile`, new validator binary.
  - **Done when:** unmapped count is zero or explicitly allow-listed with a reason.

## Phase 4 — Optimizer intelligence

- **11. Adaptive simulation budgets in tiered optimizer**
  - Replace fixed `tiered_scout_sims` with a variance/confidence-driven allocator: give more sims only to crews whose win-rate CI overlaps the current top-K cut.
  - **Touchpoints:** `src/optimizer/tiered.rs`, `src/optimizer/ranking.rs`.
  - **Done when:** total sim budget drops ≥ 30% on representative workloads at equal final top-K accuracy (benchmarked).
- **12. Cross-session learning cache (per ship × hostile)**
  - Persist winners + warm-start seeds to `profiles/{id}/optimize_history.json`; auto-seed future optimize runs for the same (ship, hostile, constraints) key.
  - **Touchpoints:** `src/optimizer/mod.rs`, `src/server/api/execution.rs`, `frontend/src/lib/optimizeWarmStart.ts`.
  - **Done when:** re-running an identical optimize call short-circuits scout on already-confirmed winners and shows a "cached warm start" badge.
- **13. Novelty-aware crew ranking**
  - Add a diversity penalty (e.g. Jaccard distance over officer ids) so the ranked top-K is not dominated by near-duplicates of one lineage.
  - **Touchpoints:** `src/optimizer/ranking.rs`, new unit tests with synthetic crews.
  - **Done when:** a configurable `novelty_weight` parameter exposes the behavior and defaults preserve current output.

## Phase 5 — API, frontend & UX

- **14. OpenAPI spec + typed client for the frontend**
  - Complete `docs/openapi/` into a full OpenAPI 3.1 document covering every `/api/`* route; generate TS types consumed by the SPA to replace ad-hoc fetch wrappers.
  - **Touchpoints:** `docs/openapi/`, `src/server/routes.rs`, `frontend/src/lib/api/` (new).
  - **Done when:** SPA builds using generated types and a contract test asserts server responses match the spec.
- **15. Frontend E2E smoke tests via Playwright**
  - Use existing `e2e/` + `playwright.config.ts` to cover: load workspace, run sim, run optimize (small), save preset, open results library. Wire into CI.
  - **Touchpoints:** `e2e/`, `.github/workflows/`.
  - **Done when:** CI runs Playwright headless against a locally-served release build and blocks merges on failures.
- **16. Editable buildings UI + profile management**
  - Follow up on the `GET /api/profile/buildings-summary` hook: add edit controls for building levels when sync is unavailable.
  - **Touchpoints:** `frontend/src/components/` (Roster & Profile page), `src/server/api.rs`.
  - **Done when:** users can edit building levels in the UI and values flow into the optimizer.

## Phase 6 — Observability & performance

- **17. Structured logging + request tracing in the server**
  - Adopt `tracing` + `tracing-subscriber` with JSON output; add span per request, per optimize job, per sim batch; include seed, strategy, candidate count.
  - **Touchpoints:** `src/server/`, `Cargo.toml`, `docs/DEPLOYMENT_SECURITY.md`.
  - **Done when:** `KOBAYASHI_LOG=info` emits structured events and an example dashboard (or `jq` recipe) is documented.
- **18. Criterion benchmark regression gate**
  - Extend `benches/` with fixed-seed scenarios; store baseline in `benchmark_results.log`; CI fails if p50 regresses > 10% on the reference machine profile.
  - **Touchpoints:** `benches/`, `.github/workflows/`, `docs/PERFORMANCE.md`.
  - **Done when:** CI surfaces a regression summary comment on PRs and the baseline is refreshed on release tags.

## Phase 7 — Release, docs & community

- **19. Prebuilt release artifacts (macOS, Linux, Windows)**
  - GitHub Release workflow that builds `kobayashi` + `frontend/dist` and attaches zipped artifacts per OS; include a checksum file and signed tag.
  - **Touchpoints:** `.github/workflows/release.yml`, `README.md` Quick Start, `docs/DEPLOYMENT_SECURITY.md`.
  - **Done when:** tagging `vX.Y.Z` publishes three downloadable archives + release notes.
- **20. Contributor onboarding pass (README + CONTRIBUTING + architecture diagram)**
  - Write `CONTRIBUTING.md`, refresh `README.md`'s Quick Start, add a high-level architecture diagram (Mermaid) showing data flow: sync → profile merge → scenario build → optimizer → SPA.
  - **Touchpoints:** `README.md`, new `CONTRIBUTING.md`, `docs/DESIGN.md`.
  - **Done when:** a new contributor can go from clone → running server → first passing test using only the docs.

---

## Progress

- Completed: 0 / 20
- In progress: 0
- Blocked: 0

*Keep this file updated as tasks land; link PRs inline next to the checkbox when useful.*