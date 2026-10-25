# Contributing to Kobayashi

## First-time setup (the minimal path)

Do these steps after you clone the repository:

```bash
# from repo root
cargo test
(cd frontend && npm ci && npm run build)
cargo build --release
./target/release/kobayashi serve
```

- Open `http://localhost:3000` after the server starts.
- Start the server from the root of the repository. The server finds `frontend/dist` and
  `data/` from there.
- For the quick start for players, refer to [README.md](README.md#quick-start).

## Local verification

Use a disposable checkout or a runtime with synthetic profiles for tests that write
files. Do not run those tests beside real player data. Follow the isolation steps
in [AGENTS.md](AGENTS.md#filesystem-test-isolation).

Run these commands from the root of the repository:

```bash
chmod +x scripts/local-ci.sh   # first time only
./scripts/local-ci.sh
```

This script runs the main Rust and frontend checks. It does not run every CI job
or every data gate. It skips the Rust audit if `cargo-audit` is absent. Report that
check as skipped. Run Python and Playwright tests separately when needed:

```bash
pip install -r tools/combat_engine/requirements-test.txt
python -m pytest tools/combat_engine/tests/ -v

npm ci && npx playwright install --with-deps chromium
cargo build --release && (cd frontend && npm ci && npm run build) && npm run test:e2e
```

You can also run `npm run verify` from the root of the repository. This command runs the
combined verification pipeline. For more data, refer to [scripts/README.md](scripts/README.md).

## The CI workflow

Each pull request runs [.github/workflows/ci.yml](.github/workflows/ci.yml). The name of
the workflow is **CI**. It contains these jobs:

| Job                        | What it runs                                                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| **Rust**                   | Formatting, Clippy, nextest, doctests, research and script tests, calibration, generated-data checks, and the data report. |
| **Rust release build + audit** | `cargo build --release` and `cargo audit`. |
| **Frontend**               | `npm ci`, `npm audit --audit-level=high`, `npm run lint`, `npm run typecheck`, `npm run test:coverage`, `npm run build` (in `frontend/`). |
| **Combat engine (Python)** | `pytest` under `tools/combat_engine/tests/`                                                                                     |
| **E2E smoke (Playwright)** | The release build, the frontend build, and `npm run test:e2e` (health plus the workspace simulate, optimize, preset, and results flows). A failed run uploads the `playwright-report` and `test-results` artifacts. |

### Branch protection (in the repository settings)

To stop a merge when CI fails, a maintainer must turn on branch protection for `main` or
`master`. The maintainer must then make these **status checks** mandatory. The names are
the names in the GitHub picker "Require status checks to pass":

- `CI / Rust`
- `CI / Rust release build + audit`
- `CI / Frontend`
- `CI / Combat engine (Python)`
- `CI / E2E smoke (Playwright)`

> CI also has **`CI / Rust coverage`** and **`CI / Upstream summary drift`** jobs.
> Coverage runs on selected events and has no threshold gate. Upstream drift has
> event-specific enforcement. Refer to *Upstream summary drift* below.

The workflow file defines the current job behavior. Separate benchmark workflows
check performance and optimizer quality.

Make a pull request mandatory before a merge if this agrees with the project policy.

## Pre-commit hooks (recommended)

The project uses [pre-commit](https://pre-commit.com/) with
[.pre-commit-config.yaml](.pre-commit-config.yaml). Your local commits then run the same
Rust `fmt` checks and `clippy` checks before you push. The hooks also run Biome when you
change a file in `frontend/`.

```bash
pip install pre-commit   # or: brew install pre-commit
pre-commit install         # once per clone
```

To run all the hooks manually:

```bash
pre-commit run --all-files
```

## Maintenance tasks (`cargo xtask`)

Run `cargo xtask --help` from the root of the repository. The output lists the wrappers for
the ship refresh, the hostile refresh, the research refresh, `validate_data`,
`generate_lcars`, `npm run data:refresh`, and `npm run verify`. For the commands below
these wrappers, refer to [AGENTS.md](AGENTS.md#verification-and-maintenance) and to
[scripts/README.md](scripts/README.md).

## Scheduled data pull requests

The workflow [.github/workflows/data-refresh.yml](.github/workflows/data-refresh.yml) can
open a pull request with a large difference in `data/`. Treat it as a usual pull request.
Review the scope, run `cargo test` locally if you need it, and merge the pull request when
you are satisfied.

**Upstream summary drift.** The CI job `upstream_drift` compares the live data.stfc.space
catalogs for ships, hostiles, and research with the committed files in
`data/upstream/data-stfc-space/summary-*.json`. Drift is enforced on `main` and on
automated data-refresh pull requests. Other feature pull requests retain the
diagnostic without failing for unrelated catalog changes. Review the workflow
before interpreting the result. Use `cargo xtask check-upstream-drift` to inspect
drift locally. Review data refreshes before merging them.

## Convenience npm script (frontend only)

Run this command from the root of the repository:

```bash
npm run ci:frontend
```

It runs a clean-install frontend pipeline that agrees with the Frontend job in CI. For more
data, refer to the root [package.json](package.json).

## Documentation style

The user-facing documents follow ASD-STE100 Simplified Technical English. Before you change
one of these documents, read [docs/STYLE_STE100.md](docs/STYLE_STE100.md). Section 1 of that
guide lists the documents in scope.
