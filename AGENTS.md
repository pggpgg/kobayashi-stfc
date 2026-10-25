# KOBAYASHI project instructions

This is the shared instruction source for coding agents working in this repository.
`CLAUDE.md` imports this file. Cursor uses this file and its small project entry point.
Maintain shared policy here instead of copying it into tool-specific files.

## Instruction maintenance

- Follow the user's current request and existing authorization within the host's instruction and permission rules.
- Read applicable directory instructions before editing. Scoped instructions add local requirements; they must not silently contradict this file.
- Use source code, manifests and workflows to verify project facts. Correct stale guidance when behavior changes.
- Keep commands, architecture and policy in one maintained location. Link to detailed documentation instead of duplicating catalogs.
- Keep temporary plans, private evidence and machine-specific settings out of shared instructions. Do not encode a session's test counts or task status as permanent policy.
- Update an existing task checklist and its progress counts as items finish. Do not invent a checklist when none is needed.

## Project map

KOBAYASHI is a Rust Monte Carlo combat simulator and crew optimizer for Star Trek
Fleet Command, with a React, TypeScript and Vite frontend.

| Area | Responsibility |
| --- | --- |
| `src/combat/` | Fight loop, phases, effects, stacking, mitigation, damage and deterministic RNG. |
| `src/lcars/` | Canonical officer conversion, LCARS parsing and ability resolution. |
| `src/data/` | Catalogs, imports, player buffs, profiles, provenance and result-cache fingerprints. |
| `src/optimizer/` | Candidate generation, Monte Carlo, ranking and search strategies; Monte Carlo implementation is under `monte_carlo/`. |
| `src/server/` | Axum/Tokio API, CPU admission, background jobs, SSE, profile storage and native sync. |
| `src/cli.rs`, `src/main.rs` | CLI dispatch and application startup. |
| `frontend/src/` | SPA; pages under `pages/`, components under `components/`, requests and state under `lib/`. |
| `tests/`, `e2e/` | Rust integration tests and Playwright browser tests. |
| `scripts/`, `xtask/` | Data pipelines, maintenance and verification. |
| `tools/combat_engine/` | Python reference implementation and its tests. |

Keep importing, combat resolution, optimization, evaluation and UI concerns separate.
Prefer typed structures, explicit operation order and focused changes over broad refactors.

The server uses REST and SSE, not WebSockets. CPU-heavy requests use blocking
workers with process-wide admission control. Background optimize jobs are not a
durable job queue. The SPA submits work to the Rust API; it does not simulate fights.

Runtime discovery uses `KOBAYASHI_HOME`, the working directory, the executable
directory, then the compile-time checkout fallback. A valid root includes the
core data catalogs. See `src/runtime_paths.rs`; do not assume setting an empty
`KOBAYASHI_HOME` isolates data or profile access.

## Protect player state and existing work

- Inspect the working tree before changing it. Preserve unrelated modifications, branches and staged work.
- Use an isolated checkout or worktree when existing work cannot safely be combined. Do not reset or clean a dirty checkout to simplify a task.
- Real profiles, `profiles/index.json`, sync tokens, backups and raw captures are private. Only the bundled demo profile is intended for publication; see [profiles/README.md](profiles/README.md).
- Do not replace the bundled demo roster with a tiny test import. Tests own their synthetic files and cleanup.
- Do not restart the live capture server, install a mod DLL or change the game setup as an incidental part of a test or documentation task.
- Honor pauses on human-assisted testing. Use existing source evidence and frozen captures before requesting another controlled fight.
- For authorized GitHub work, reproduce reported behavior before fixing it. Distinguish a genuine regression from a setup error, compile failure or empty test filter.
- When authorized to close a non-reproducing issue, document the tested revision, setup, command and observed result first. A failed reproduction caused by missing prerequisites is not evidence to close it.
- Distinguish local changes, committed changes, pushed changes, passing CI and released behavior. Publish authorized work as a reviewable PR; do not describe local fixes as shipped.

## Combat correctness and evidence

- Model mechanics, not individual outcomes. Do not invent constants, silently fit a formula to one fight or add benchmark-specific exceptions.
- Keep base stats, flat additions, additive percentages, multipliers, caps and final values distinct. Preserve each source's units and critical-damage convention.
- Make timing explicit: combat begin, round start, attack, defense, hit, critical hit, shield break, round end and death can have different effects.
- Preserve deterministic behavior for the same complete inputs and seed. Changes to RNG draws, ordering, batching or cache identity need parity checks.
- Separate observed values, source-code evidence, derived values and assumptions. Record build/version identifiers and source hashes when available.
- Unknown conditions and timings must remain unsupported with diagnostics; do not broaden them into unconditional effects or substitute a different event.
- Keep raw game columns, overkill-inclusive simulator counters and actual HP removal separate. A matching kill summary does not establish raw damage or complete player-buff accuracy.
- Validate a mechanic across relevant fight families and counterexamples. Preserve training, validation and holdout separation where those sets exist.
- Do not widen calibration bands or rewrite observed fixtures merely to make a test pass. An intentional correction to a measurement or golden snapshot needs an explicit rationale and a reviewed diff.
- When a golden snapshot changes only for instrumentation, prove that existing outcomes and values are unchanged. Label any remaining limits honestly.
- Combat behavior changes invalidate persisted optimizer metrics. Add coverage to the canary fights in `src/data/optimize_fingerprint.rs`; bump `COMBAT_ENGINE_BEHAVIOR_VERSION` when existing canaries cannot cover the change reliably.
- Do not assume an officer permutation is interchangeable in numerical caches when evaluation order can affect RNG or state. Keep search deduplication distinct from simulation identity.

For a meaningful simulator change, report the mismatch, evidence, modeled mechanic,
code changed, affected fight families, expected test or benchmark slices, validation
results and remaining uncertainty.

## Data and officer definitions

- `data/officers/officers.canonical.json` is the maintained officer ability catalog.
- The default runtime builds the LCARS model in-process through `src/lcars/officer_model.rs`. A generated `officers.lcars.yaml` is not required for a clean checkout to run.
- Generated LCARS YAML is an export/debugging artifact. Do not hand-edit it for routine ability fixes or make tests depend on an ignored generated file.
- Model abilities in canonical data first. Add Rust support when a verified, reusable effect family is missing; do not patch one officer's fight outcome in the engine.
- Preserve officer IDs, name aliases and `loca_id` translation linkage. Modeling fidelity is maintained in `data/officers/officer_modeling_fidelity.yaml`.
- Preserve raw upstream values separately from normalized and derived values. Keep data version, source notes, scope, rank selection, conditions and unit evidence with mappings.
- Missing or ambiguous data is a modeling gap. Quarantine unsupported buffs with reasons instead of guessing their units or dropping their gates.
- Import failures must preserve valid saved state. Use checked input, atomic writes and bounded restore/export behavior; retain the profile-store transaction discipline.
- Use maintained generators and importers. Inspection or subset modes must not replace a full production catalog. Review semantic diffs and generated-output idempotence.

Detailed references: [data layout](data/README.md), [maintenance pipelines](scripts/README.md),
[LCARS authoring](docs/LCARS_CONTRIBUTING.md),
[officer translations](docs/OFFICER_TRANSLATIONS_MAPPING.md), and
[effect specification](docs/COMBAT_EFFECT_SPEC.md).

## Rust commands and test discipline

Run commands from the repository root unless stated otherwise. Use the toolchain
and dependency policy in [.github/workflows/ci.yml](.github/workflows/ci.yml).

| Task | Command |
| --- | --- |
| Fast compilation check | `cargo check --locked` |
| Development build | `cargo build --locked` |
| Release build | `cargo build --release --locked` |
| Formatting check | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --all-targets --locked -- -D warnings` |
| One integration crate | `cargo test --locked --test officer_reload_speed` |
| One integration function | `cargo test --locked --test officer_reload_speed kuron_captain` |
| Library unit-test filter | `cargo test --locked --lib resolve_crew` |
| Full Rust suite | `cargo test --locked` |
| CI test runner | `cargo nextest run --profile ci`, then `cargo test --doc` |
| Performance benchmarks | `cargo bench` (release profile) |

Integration file stems are not function filters. Do not use bare
`cargo test <name>` when targeting one test; choose `--lib` or `--test <stem>`.
Confirm the intended target ran and at least one intended test passed. Compilation
success and `0 passed; ... filtered out` are not validation.

Keep full logs and exit codes. Avoid pipelines that hide compilation output or
mask failures. Long compilation is not evidence of a hang; use resumable command
execution instead of fixed timeout guesses. Serialize Cargo builds that share a
target directory. Do not change `CARGO_TARGET_DIR` casually to bypass its build lock.

### Filesystem test isolation

Broad Rust, API, import and profile tests can write relative `profiles/` paths.
Do not run them from a checkout containing real player data.

- Use a disposable checkout with public/synthetic profiles, or build with `--no-run` and execute the test artifacts from a fresh fixture runtime.
- The runtime CWD and `KOBAYASHI_HOME` must point to the same isolated root containing the required `data/` tree and synthetic `profiles/`.
- Preserve required fixture paths and test environment, including runtime `CARGO_MANIFEST_DIR` for harnesses that read it. Do not direct it at private player data.
- If launching artifacts manually, retain each test artifact's kind and executable path; a library and binary can share a target name.
- Use the configured nextest test groups for shared sync files. Check `.config/nextest.toml` rather than assuming those tests are skipped.
- Keep temporary directories inside an approved workspace location. On Windows, quote paths with spaces and verify paths before cleanup.
- A filesystem permission failure is a setup/permission issue, not a passing test. Record it and use a permitted isolated verification path.

## Frontend and API contracts

From `frontend/`: use `npm ci`, `npm run dev`, `npm run lint`,
`npm run typecheck`, `npm run test` and `npm run build` as appropriate.
The build emits `frontend/dist`; the Rust server serves it when present.
`VITE_API_BASE` selects the build-time API base.

- Reuse existing workspace, profile, request and job-tracking patterns. Co-locate Vitest tests with their components or helpers.
- Maintain scenario, ship tier/level and defender inputs through request construction. Cancel departed jobs and reject stale responses.
- Keep generated wire types aligned with the backend. Update `docs/openapi/kobayashi-openapi.yaml`, then run `npm run gen:api` in `frontend/`; do not hand-edit `src/lib/api/generated.d.ts`.
- Optimizer DTO changes also use `export_optimizer_schemas` and `scripts/refresh_optimizer_openapi.mjs`; follow [scripts/README.md](scripts/README.md).
- Preserve browser-boundary, API-key, profile and native-ingress contracts when changing endpoints. Do not bypass these checks to fix a UI test.
- Treat Rust routes, DTOs and the OpenAPI document as API references. Do not maintain another incomplete route list here.

## Verification and maintenance

Run checks appropriate to the change; documentation-only edits need link and
instruction consistency checks, not a combat rebuild. For behavior changes, run
focused regressions first, then the relevant broader suites.

The **CI** workflow is authoritative for its current jobs: Rust tests and doctests,
Clippy/fmt, scripts, calibration and generated-data checks, release build/audit,
frontend audit/lint/typecheck/coverage/build, Python tests and Playwright smoke.
Coverage and live upstream-drift enforcement have event-specific conditions.
Benchmark workflows provide separate performance and search-quality gates.

- `scripts/local-ci.sh` runs the main local Rust/frontend checks, not every CI job. It can skip audit when the tool is absent; report that as skipped.
- `cargo xtask --help` lists maintenance wrappers. Important checks include `cargo xtask check-generated`, `cargo xtask calibration-scoreboard` and `cargo xtask validate`.
- Use `cargo xtask refresh-ships`, `refresh-hostiles`, `data-refresh` and `check-upstream-drift` for their documented data flows. Node and Python are required by relevant wrappers; `PYTHON` selects the Python executable.
- `cargo xtask regen-lcars` exports officer YAML. Run `scripts/normalize_officer_id_strings.py` after an upstream officer-ID refresh when required by that pipeline.
- Regenerate generated reports with their documented commands; do not hand-edit computed numbers. Review all changed fields, including provenance dates.
- Use release builds for timing comparisons. Retain benchmark budgets, controls and configured thresholds.
- Record actual remote CI results when reporting a published fix. A local pass does not prove GitHub CI passed.

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and CI,
[docs/DESIGN.md](docs/DESIGN.md) for the model,
[docs/COMBAT_TRACE.md](docs/COMBAT_TRACE.md) for trace semantics, and
[docs/README.md](docs/README.md) for the documentation index.
Read [docs/STYLE_STE100.md](docs/STYLE_STE100.md) before changing its listed user-facing documents.

## Runtime and CPU controls

Start a built server with `./target/release/kobayashi serve` (use `.exe` on Windows).
The default bind is `127.0.0.1:3000`; `KOBAYASHI_BIND` overrides it.
Use the CLI help and `src/cli.rs` for current command syntax.

| Environment variable | Role |
| --- | --- |
| `KOBAYASHI_HOME` | Runtime assets and profiles root; requires valid catalogs. |
| `KOBAYASHI_RAYON_THREADS` | Global Rayon cap; unset or `0` uses logical CPUs. |
| `KOBAYASHI_LOW_PRIORITY=1` | Windows process priority below normal. |
| `KOBAYASHI_MAX_CONCURRENT_CPU_JOBS` | CPU-heavy API admission limit; default `1`. |
| `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS` | Positive queue timeout returns HTTP 503 `cpu_busy`; unset or `0` waits. |
| `KOBAYASHI_FT_LEVEL_TIER_SCALING=1` | Optional forbidden-tech scaling; see `src/data/profile/mod.rs`. |

Restart the server to apply process-wide CPU settings. Concurrent jobs share the
same Rayon pool. Tests or benches that initialize Rayon first cannot retroactively
change its size. Use an isolated server and port for browser tests when a live
capture server is present; see `playwright.config.ts`.
