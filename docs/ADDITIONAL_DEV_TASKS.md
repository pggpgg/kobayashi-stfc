# Additional development tasks (supplementary)

Twenty improvement ideas for Kobayashi that are **not** listed in [`DEV_TASKS.md`](DEV_TASKS.md) and **not** framed as the active backlog items described in [`ROADMAP.md`](ROADMAP.md). Use checkboxes locally to track progress.

- [ ] Add explicit **HTTP health and readiness** routes (e.g. separate liveness vs “data registry loaded”) and document expected status codes for reverse proxies and container orchestrators.
- [ ] Ship a **`cargo xtask doctor`** (or equivalent) that prints a single report: Rust/toolchain versions, required env vars, presence of `data/` artifacts, profile path validity, and optional mod-sync reachability hints.
- [ ] Introduce **Content-Security-Policy** (and related security headers) for the static SPA + API static file path, with a short runbook for adjusting nonces/hashes when the bundle changes.
- [ ] Wire **end-to-end correlation IDs**: accept `X-Request-Id` (or generate one), attach it to all `tracing` spans for that HTTP session, and return it in error JSON for support workflows.
- [ ] Add a **`cargo fuzz`** target (or `libfuzzer-sys` harness) for the **LCARS parser** and golden-parse fixtures to catch panics and undefined behavior on hostile inputs.
- [ ] Publish a **`NOTICES` / third-party attribution** file covering vendored or generated data sources, npm dependency summaries, and any upstream scraper/mod credits required for redistribution.
- [ ] Add **frontend bundle size budgets** (e.g. `rollup-plugin-visualizer` + a small CI script) so accidental dependency regressions fail PRs with a diff-friendly artifact.
- [ ] Stand up **component isolation** (Storybook, Ladle, or similar) for the heaviest React surfaces (`CrewBuilder`, `OptimizePanel`, roster tables) to speed UI iteration without a full backend.
- [ ] Implement **graceful server shutdown**: stop accepting new optimize/sim jobs, surface in-flight job status to clients where feasible, and document timeout expectations for deploys.
- [ ] Define a small **API stability/versioning policy** (e.g. `/api/v1` prefix or version field in responses) and enforce it in the OpenAPI document before the next breaking JSON change.
- [ ] Expand **mechanics coverage reporting** (`src/mechanics/coverage.rs`) into a **CI-uploaded artifact** or a dedicated “Mechanics coverage” section in an existing internal doc, with trend tracking over time.
- [ ] Build a **profile diff utility** (CLI or SPA page) that compares two saved profiles for officers, research levels, buildings, and forbidden tech—aimed at auditing sync drift or alt-account comparisons.
- [ ] Add a **keyboard shortcuts cheat sheet** modal (common actions in workspace: run sim, run optimize, save preset) with sensible defaults and screen-reader friendly labels.
- [ ] Harden **PR review ergonomics for `data/`**: CODEOWNERS, required labels, or a GitHub Action comment summarizing touched hostile/officer files and suggested reviewer checklist items.
- [ ] Create a **load-test harness** (e.g. `k6` or `oha`) for concurrent read-heavy endpoints and small optimize jobs, with documented baseline numbers on a reference machine—not a regression gate yet, just reproducible stress scripts.
- [ ] Add **PGO (profile-guided optimization)** as an optional release build path for the Rust binary, documenting capture methodology and expected win bounds on the Monte Carlo hot path.
- [ ] Add **structured export** of optimize result tables (CSV or Parquet) from the SPA or a small CLI wrapper around existing JSON responses for spreadsheet-centric workflows beyond current combat CSV export.
- [ ] Introduce **mutation testing** (e.g. `cargo-mutants`) scoped to a few high-risk modules (`src/combat/damage.rs`, mitigation, proc resolution) with a weekly scheduled job rather than every PR.
- [ ] Improve **syndicate reputation UX**: surface `syndicate_combat` / reputation-derived bonuses where they affect scenarios, with clear “assumption” callouts when game evidence is thin.
- [ ] Add **offline-friendly static hosting** polish: `manifest.json`/PWA shell, cache busting strategy, and a documented “air-gapped install” path that does not assume live mod sync.
